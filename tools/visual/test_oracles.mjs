#!/usr/bin/env node
/**
 * Checks `run`/`after` support in computed.mjs and geometry.mjs, and the
 * recorders' waiting in record.mjs, against Chromium.
 * Run: node tools/visual/test_oracles.mjs   (exit 1 on failure)
 */
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { loadPlaywright } from "./playwright.mjs";
import { catchUp, isMark, requireLogged, settle, trackPending } from "./record.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const node = (script, args, input) => {
  const r = spawnSync(process.execPath, [path.join(here, script), ...args], { input, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`${script} failed: ${r.stderr}`);
  return r.stdout.trim().split("\n").map((l) => JSON.parse(l));
};
let failed = false;
const check = (name, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : `: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`}`);
  if (!ok) failed = true;
};

const q = (s) => `document.querySelector("${s}")`;
const cases = [
  { name: "plain", html: '<p id=t style="color:red">x</p>', select: "#t", props: ["color"] },
  {
    name: "click-class",
    html: "<style>#t{color:red}#t.on{color:blue}</style><p id=t onclick=\"this.classList.toggle('on')\">x</p>",
    select: "#t", props: ["color"], run: [{ click: "#t" }],
  },
  {
    name: "hover",
    html: "<style>#t{background:red;width:50px;height:50px}#t:hover{background:blue}</style><div id=t></div>",
    select: "#t", props: ["background-color"], run: [{ hover: "#t" }],
  },
  {
    // Same page, no steps: the previous case's mouse must not hover it.
    name: "after-hover",
    html: "<style>#t{background:red;width:50px;height:50px}#t:hover{background:blue}</style><div id=t></div>",
    select: "#t", props: ["background-color"],
  },
  {
    name: "focus",
    html: "<style>#t{color:red}#t:focus{color:blue}</style><button id=t>x</button>",
    select: "#t", props: ["color"], run: [{ focus: "#t" }],
  },
  {
    name: "type",
    html: "<input id=t><i id=o></i>",
    select: "#o", props: ["display"],
    run: [{ type: ["#t", "hi"] }, { eval: `${q("#o")}.style.display = ${q("#t")}.value === "hi" ? "block" : "none"` }],
  },
];
const transition = (ms) => ({
  html: `<style>#t{width:100px;height:10px;transition:width ${ms}ms linear}#t.b{width:300px}</style><div id=t onclick="this.classList.add('b')"></div>`,
  select: "#t", props: ["width"],
});
cases.push(
  { name: "mid", ...transition(1000), run: [{ click: "#t" }], after: 500 },
  { name: "end", ...transition(200), run: [{ click: "#t" }, { wait: 100 }], after: 400 },
);

const out = node("computed.mjs", [], JSON.stringify(cases));
const by = Object.fromEntries(out.map((o) => [o.name, o]));
check("no-run output format unchanged", out[0], { name: "plain", color: "rgb(255, 0, 0)" });
check("click toggles class: colour", by["click-class"].color, "rgb(0, 0, 255)");
check("hover: background", by.hover["background-color"], "rgb(0, 0, 255)");
check("no hover left over from the previous case", by["after-hover"]["background-color"], "rgb(255, 0, 0)");
check("focus: colour", by.focus.color, "rgb(0, 0, 255)");
check("type: value reached the input", by.type.display, "block");
const mid = parseFloat(by.mid.width);
check(`transition mid-way (${by.mid.width} strictly between 100px and 300px)`, mid > 120 && mid < 280, true);
check("transition after end", by.end.width, "300px");

// geometry.mjs: element created by an eval.
const geo = node("geometry.mjs", [
  "--run",
  JSON.stringify([{ eval: 'const d=document.createElement("div");d.id="n";d.style.cssText="width:40px;height:30px";document.body.append(d)' }]),
  "--html", "<body style=margin:0><p style=margin:0;height:20px>x</p></body>", "#n",
]);
check("geometry: created element", geo, [{ select: "#n", index: 0, x: 0, y: 20, width: 40, height: 30 }]);
const geoNone = node("geometry.mjs", ["--html", "<body style=margin:0><p style=margin:0;height:20px>x</p></body>", "p"]);
check("geometry: no-run output unchanged", geoNone, [{ select: "p", index: 0, x: 0, y: 0, width: 1280, height: 20 }]);
const geoAll = node("geometry.mjs", [
  "--elements", "--html",
  "<!DOCTYPE html><body style=margin:0><p id=a class=' x  y ' style=margin:0;height:10.5px>x<b>y</b></p><i style=display:none></i><div style=height:5px></div></body>",
]);
check("geometry --elements: boxes that are not inline, described", geoAll.map((r) => [r.describe, r.y, r.height]), [
  ["<html>", 0, 15.5], ["<body>", 0, 15.5], ['<p id="a" class="x y">', 0, 10.5], ["<div>", 10.5, 5],
]);

// record.mjs: what a recorder waits for, and what it refuses.
const throws = async (f) => {
  try {
    await f();
    return null;
  } catch (e) {
    return e.message;
  }
};
const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
try {
  // A page per case: a document `setContent` replaces keeps its window, and
  // with it the last case's timers.
  let page;
  let log = [];
  const open = async (html) => {
    await page?.close();
    page = await browser.newPage();
    await trackPending(page, { current: true });
    log = [];
    page.on("console", (m) => {
      if (!isMark(m.text())) log.push(m.text());
    });
    await page.setContent(html);
  };
  const record = async (html, opts) => {
    await open(html);
    const left = await settle(page, opts);
    return { log, left };
  };

  const late = "<script>setTimeout(()=>{console.log('CS late');console.log('CS done')},500)</script>";
  check("settle: a page that only logs from a 500ms timer", await record(late), { log: ["CS late", "CS done"], left: 0 });
  check("requireLogged: its log is accepted", await throws(() => requireLogged("late", late, ["CS late", "CS done"], "CS ")), null);
  const chain = "<script>setTimeout(()=>{console.log('CS a');requestAnimationFrame(()=>setTimeout(()=>console.log('CS done'),200))},100)</script>";
  check("settle: timers a timer and a frame start", (await record(chain)).log, ["CS a", "CS done"]);
  const cleared = "<script>const t=setTimeout(()=>console.log('CS never'),5000);clearTimeout(t);console.log('CS now')</script>";
  check("settle: a cleared timer is not waited for", (await record(cleared)).log, ["CS now"]);

  const silent = "<script>if(false)console.log('CS '+1)</script>";
  check("requireLogged: a page that logs nothing is refused", await throws(() => requireLogged("silent", silent, [], "CS ")),
    'silent: the page logs "CS " lines but none were recorded');
  check("requireLogged: a timed page without its end marker is refused",
    await throws(() => requireLogged("late", late, ["CS late"], "CS ")), 'late: logs from timers but its end marker "CS done" was not recorded');
  check("requireLogged: a page that does not log is not checked", await throws(() => requireLogged("plain", "<p>x</p>", [], "CS ")), null);

  // A frame loop and an interval never finish: refused, or with `running`
  // reported after the cap rather than waited for.
  const loop = "<script>(function f(){requestAnimationFrame(f)})();setInterval(()=>{},30)</script>";
  check("settle: a page that never stops is refused", await throws(() => record(loop, { cap: 300 })),
    "page never finished: 2 timers or frames still pending 300ms on");
  check("settle: with running, a page that never stops is left running", (await record(loop, { cap: 300, running: true })).left, 2);

  // catchUp: a busy page runs a timer late; what was due by the call is read.
  await open("<script>setTimeout(()=>{const s=performance.now();while(performance.now()-s<400);},0);setTimeout(()=>console.log('CS t50'),50);setTimeout(()=>console.log('CS t5000'),5000)</script>");
  await page.waitForTimeout(100);
  await catchUp(page);
  check("catchUp: a timer due by now has run, a later one is not waited for", log, ["CS t50"]);
} finally {
  await browser.close();
}

process.exit(failed ? 1 : 0);
