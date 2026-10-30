/**
 * Geometry oracle: what Chromium's getBoundingClientRect() gives.
 *
 *   node tools/visual/geometry.mjs page.html 'selector' ['selector' ...]
 *   node tools/visual/geometry.mjs --html '<p>x</p>' 'p'
 *
 * Prints one JSON line per matched element: {select, index, x, y, width,
 * height}. The viewport is 1280×800 at deviceScaleFactor 1, as in
 * render_html.mjs, so test expectations measure what the parity harness
 * compares against.
 */

import path from "node:path";
import { pathToFileURL } from "node:url";

import { loadPlaywright } from "./playwright.mjs";

const args = process.argv.slice(2);
let html = null;
let file = null;
if (args[0] === "--html") {
  args.shift();
  html = args.shift();
} else {
  file = args.shift();
}
if ((html === null && !file) || args.length === 0) {
  console.error("usage: geometry.mjs (page.html | --html '<markup>') selector...");
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
