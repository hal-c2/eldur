// Records what Chromium reports for the pages `absolute_e2e.rs` converts
// (`*.html` here): the box of every element with an id, the page's scroll
// height, for `scrolled` the boxes after scrolling, and what the page's script
// logged with the prefix "LC ". For a `paint_` page, which id's element is on top
// at each point of a grid: what paints over what.
//
//   node crates/eldur_convert/tests/absolute/gen.mjs
//
// writes `chromium.json`. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));

// How far each page is scrolled before measuring.
const SCROLL = { scrolled: 300, dyn_created_rel_scrolled: 300 };

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1,
});
const out = {};
let log = [];
page.on("console", (m) => {
  if (m.text().startsWith("LC ")) log.push(m.text());
});
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  const name = file.slice(0, -5);
  log = [];
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  out[name] = await page.evaluate((scroll) => {
    window.scrollTo(0, scroll);
    const boxes = {};
    for (const e of document.querySelectorAll("[id]")) {
      const r = e.getBoundingClientRect();
      boxes[e.id] = [r.x, r.y, r.width, r.height];
    }
    return { scroll, scrollHeight: document.documentElement.scrollHeight, boxes };
  }, SCROLL[name] ?? 0);
  // A page with a button is measured again once it has been pressed.
  if (await page.$("#go")) {
    await page.click("#go");
    out[name].after = await page.evaluate(() => {
      const boxes = {};
      for (const e of document.querySelectorAll("[id]")) {
        const r = e.getBoundingClientRect();
        boxes[e.id] = [r.x, r.y, r.width, r.height];
      }
      return boxes;
    });
  }
  out[name].log = log;
  if (name.startsWith("paint_")) {
    out[name].hits = await page.evaluate(() => {
      const hits = [];
      for (let y = 5; y < 300; y += 10) {
        for (let x = 5; x < 300; x += 10) {
          let e = document.elementFromPoint(x, y);
          while (e && !e.id) e = e.parentElement;
          hits.push([x, y, e ? e.id : ""]);
        }
      }
      return hits;
    });
  }
}
await browser.close();
fs.writeFileSync(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
