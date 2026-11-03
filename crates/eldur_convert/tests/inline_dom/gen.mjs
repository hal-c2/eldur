// Records what Chromium's console says for the pages `inline_dom_e2e.rs`
// converts (`*.html` here): the lines starting "LC ", logged at load and for
// each click named by `<body data-clicks>`. A click names a selector, and
// lands on the middle of that element's first glyph run; `#id:tail` aims at the
// element's last text node, `#id:start` at the first child's text and `#id:right`
// at the empty end of the element's line. Each point is recorded, so the test
// presses the same pixels. `key:Name` presses a key instead.
//
//   node crates/eldur_convert/tests/inline_dom/gen.mjs
//
// writes `chromium.json`. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));
const { writeRecording, trackPending, catchUp, settle, requireLogged } = await import(path.join(root, "tools/visual/record.mjs"));

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1,
});
await trackPending(page);
let log = [];
page.on("console", (m) => {
  if (m.text().startsWith("LC ")) log.push(m.text());
});
page.on("pageerror", (e) => {
  throw new Error(`page threw: ${e.message}`);
});
const out = {};
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  log = [];
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  const names = (await page.getAttribute("body", "data-clicks")) || "";
  const clicks = [];
  for (const name of names.split(/\s+/).filter(Boolean)) {
    if (name.startsWith("key:")) {
      await page.keyboard.press(name.slice(4));
      clicks.push({ name, key: name.slice(4) });
      await page.waitForTimeout(30);
      await catchUp(page);
      continue;
    }
    const point = await page.evaluate((name) => {
      const [sel, part] = name.split(":");
      const el = document.querySelector(sel);
      let node = el;
      if (part === "tail") node = el.lastChild;
      else if (part === "start") node = el.firstChild;
      const range = document.createRange();
      range.selectNodeContents(node);
      if (part === "tail") {
        // Skip the run's leading space: it may hang outside the glyphs.
        range.setStart(node, 1);
      }
      const r = range.getClientRects()[0] || el.getBoundingClientRect();
      if (part === "right") {
        return { x: el.getBoundingClientRect().right - 20, y: r.top + r.height / 2 };
      }
      return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }, name);
    await page.mouse.click(point.x, point.y);
    clicks.push({ name, x: point.x, y: point.y });
    await page.waitForTimeout(30);
    // What the click set off and is due by now has run, however loaded the machine.
    await catchUp(page);
  }
  // The log is all the page says, however late its timers run.
  await settle(page);
  requireLogged(file, fs.readFileSync(path.join(here, file), "utf8"), log, "LC ");
  out[file.slice(0, -5)] = { log, clicks };
}
await browser.close();
writeRecording(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
