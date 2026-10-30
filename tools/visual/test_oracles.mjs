#!/usr/bin/env node
/**
 * Checks `run`/`after` support in computed.mjs and geometry.mjs against
 * Chromium. Run: node tools/visual/test_oracles.mjs   (exit 1 on failure)
 */
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

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

process.exit(failed ? 1 : 0);
