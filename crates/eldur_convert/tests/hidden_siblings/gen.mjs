// Records what Chromium shows on the pages `hidden_siblings_e2e.rs` drives:
// an element a `:hover` rule shows, and what it does to the siblings after it
// while hidden and while shown. `numbered.html`: an `<ol>` item that counts
// only while shown; `margin.html`: a block whose margins collapse with the
// next one's only while shown. Neither hidden element has an id.
//
//   node crates/eldur_convert/tests/hidden_siblings/gen.mjs
//
// writes `chromium.json`: for each page and step, the list markers (from the
// accessibility tree, which is where Chromium exposes them) and the top of
// every `li` and `p`, at 1280×800. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadPlaywright } from "../../../../tools/visual/playwright.mjs";
import { writeRecording } from "../../../../tools/visual/record.mjs";
import { runSteps } from "../../../../tools/visual/run_steps.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const steps = { initial: [], hover: [{ hover: ".a" }], left: [{ hover: ".a" }, { hover: "body" }] };

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const out = {};
try {
  for (const name of ["numbered", "margin"]) {
    const html = fs.readFileSync(path.join(here, `${name}.html`), "utf8");
    for (const [step, run] of Object.entries(steps)) {
      const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
      await page.setContent(html, { waitUntil: "load" });
      await page.mouse.move(1279, 799);
      await runSteps(page, run);
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const cdp = await page.context().newCDPSession(page);
      const { nodes } = await cdp.send("Accessibility.getFullAXTree");
      const markers = nodes
        .filter((n) => n.role?.value === "ListMarker")
        .map((n) => n.name?.value ?? "");
      const tops = await page.$$eval("li, p", (els) =>
        els.map((e) => ({ text: e.textContent, shown: e.getClientRects().length > 0, top: e.getBoundingClientRect().top })),
      );
      out[`${name} ${step}`] = { markers, tops };
      await page.close();
    }
  }
} finally {
  await browser.close();
}
writeRecording(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 1)}\n`);
