/**
 * Geometry oracle: what Chromium's getBoundingClientRect() gives.
 *
 *   node tools/visual/geometry.mjs page.html 'selector' ['selector' ...]
 *   node tools/visual/geometry.mjs --html '<p>x</p>' 'p'
 *   node tools/visual/geometry.mjs --click 'a[href="#f"]' page.html 'h2'
 *
 * Prints one JSON line per matched element: {select, index, x, y, width,
 * height}. The viewport is 1280×800 at deviceScaleFactor 1, as in
 * render_html.mjs, so test expectations measure what the parity harness
 * compares against.
 *
 * `--click selector` clicks the first match before measuring and prints
 * {click, scrollX, scrollY} after it: what following a link does.
 *
 * `--run '<json>'` (or `--run @steps.json`) takes the same `run` step list as
 * computed.mjs, executed before measuring (after --click):
 *   --run '[{"eval":"document.body.append(document.createElement(\"p\"))"}]'
 * Steps: eval, click, hover, focus, type [sel, text], wait ms.
 * `--after ms` waits that long after the steps, before measuring.
 *
 * `--elements` (no selectors) prints every element that generates a box
 * other than an inline one, in document order, as {describe, x, y, width,
 * height}; `describe` is the `<tag id class>` text the converter writes in
 * its source comments, which drift.py matches against.
 */

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { loadPlaywright } from "./playwright.mjs";
import { runSteps } from "./run_steps.mjs";

const args = process.argv.slice(2);
let html = null;
let file = null;
let click = null;
let run = null;
let after = 0;
let elements = false;
if (args[0] === "--elements") {
  args.shift();
  elements = true;
}
while (args[0] === "--run" || args[0] === "--after") {
  const flag = args.shift();
  const v = args.shift();
  if (flag === "--after") after = Number(v);
  else run = JSON.parse(v.startsWith("@") ? fs.readFileSync(v.slice(1), "utf8") : v);
}
if (args[0] === "--click") {
  args.shift();
  click = args.shift();
}
if (args[0] === "--html") {
  args.shift();
  html = args.shift();
} else {
  file = args.shift();
}
if ((html === null && !file) || (args.length === 0) === !elements) {
  console.error(
    "usage: geometry.mjs [--elements] [--run json] [--after ms] [--click selector] (page.html | --html '<markup>') [selector...]",
  );
  process.exit(2);
}

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
try {
  const page = await browser.newPage({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
  });
  if (html !== null) {
    await page.setContent(html, { waitUntil: "load" });
  } else {
    await page.goto(pathToFileURL(path.resolve(file)).href, { waitUntil: "load" });
  }
  await page.evaluate(() => document.fonts.ready);
  if (click !== null) {
    await page.click(click);
    // Following a fragment scrolls synchronously, but let a frame pass.
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => r())));
    const scroll = await page.evaluate(() => ({ scrollX: window.scrollX, scrollY: window.scrollY }));
    console.log(JSON.stringify({ click, ...scroll }));
  }
  if (run) await runSteps(page, run);
  if (after) await page.waitForTimeout(after);
  if (elements) {
    const rows = await page.evaluate(() => {
      const describe = (e) => {
        let out = `<${e.localName}`;
        if (e.id) out += ` id="${e.id}"`;
        const cls = (e.getAttribute("class") || "").trim().split(/\s+/).filter(Boolean).join(" ");
        if (cls) out += ` class="${cls}"`;
        return `${out}>`;
      };
      const rows = [];
      for (const e of document.querySelectorAll("*")) {
        const display = getComputedStyle(e).display;
        if (display === "inline" || display === "contents" || e.getClientRects().length === 0) continue;
        const r = e.getBoundingClientRect();
        rows.push({ describe: describe(e), x: r.x, y: r.y, width: r.width, height: r.height });
      }
      return rows;
    });
    rows.forEach((r) => console.log(JSON.stringify(r)));
  }
  for (const select of args) {
    const rects = await page.$$eval(select, (els) =>
      els.map((e) => {
        const r = e.getBoundingClientRect();
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      }),
    );
    rects.forEach((r, index) => console.log(JSON.stringify({ select, index, ...r })));
  }
} finally {
  await browser.close();
}
