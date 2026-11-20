// Records what Chromium shows on the pages `hidden_siblings_e2e.rs` drives:
// elements a `:hover` rule shows, and what they do to the siblings after
// them while hidden and while shown. None of the hidden elements has an id.
// - `numbered.html`: an `<ol>` item that counts only while shown;
// - `reversed.html`: the same in an `<ol reversed>`, which counts down from
//   the number of items shown;
// - `counted.html`: a `<p>` that increments a `counter()` its siblings'
//   `::before` show, and shows its own, only while shown;
// - `joint.html`: two items shown by different states, so the items after
//   both are numbered by how many of the two show at once;
// - `nested.html`: the same where one of the two is a counted `<p>` inside a
//   hidden block, which hides and shows it with itself;
// - `margin.html`: a block whose margins collapse with the next one's only
//   while shown.
// - `empty.html`: the same with an empty block, whose margins collapse
//   through it into the next one's only while shown.
//
//   node crates/eldur_convert/tests/hidden_siblings/gen.mjs
//
// writes `chromium.json`: for each page and step, the numbers shown (list
// markers and `::before` text, from the accessibility tree, which is where
// Chromium exposes both) and the top of every `li` and `p`, at 1280×800.
// Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { loadPlaywright } from "../../../../tools/visual/playwright.mjs";
import { writeRecording } from "../../../../tools/visual/record.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
// Each step puts the pointer on the item with that text, or outside every
// element; the e2e takes the same steps in the same order.
const steps = JSON.parse(fs.readFileSync(path.join(here, "steps.json"), "utf8"));

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const out = {};
try {
  for (const [name, sequence] of Object.entries(steps)) {
    const html = fs.readFileSync(path.join(here, `${name}.html`), "utf8");
    for (const [i, [step]] of sequence.entries()) {
      const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
      await page.setContent(html, { waitUntil: "load" });
      await page.mouse.move(1279, 799);
      for (const [, over] of sequence.slice(0, i + 1)) {
        if (over === null) await page.mouse.move(1279, 799);
        else await page.locator("li, p").filter({ hasText: new RegExp(`^${over}$`) }).hover();
      }
      await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      const cdp = await page.context().newCDPSession(page);
      const { nodes } = await cdp.send("Accessibility.getFullAXTree");
      // A list marker has its text as its name; a `::before` is an unnamed
      // node whose static texts ("1", ". ") make up its text.
      const text = new Map();
      for (const n of nodes) {
        if (n.role?.value === "StaticText" && n.parentId !== undefined)
          text.set(n.parentId, (text.get(n.parentId) ?? "") + (n.name?.value ?? ""));
      }
      const markers = nodes
        .map((n) => (n.role?.value === "ListMarker" ? n.name?.value : n.role?.value === "none" ? text.get(n.nodeId) : undefined))
        .map((t) => (t ?? "").trim())
        .filter((t) => /^\d+\.$/.test(t));
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
