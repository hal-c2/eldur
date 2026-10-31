// Records what Chromium reports for the pages `absolute_e2e.rs` converts
// (`*.html` here): the box of every element with an id, the page's scroll
// height and, for `scrolled`, the boxes after scrolling.
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
const SCROLL = { scrolled: 300 };

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1,
});
const out = {};
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  const name = file.slice(0, -5);
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
}
await browser.close();
fs.writeFileSync(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
