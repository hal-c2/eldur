// Records Chromium for `svg_invalid_root_e2e.rs`: each element's box and the
// real pixels of a screenshot at its `data-pixel="dx,dy dx,dy"`, relative to
// the box's top left.
//
//   node crates/eldur_convert/tests/svg_invalid_root/gen.mjs [pictures-dir]
//
// writes `chromium.json` (and, given a directory, a picture of each element).
// Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const options = { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 };
const page = await browser.newPage(options);
const decoder = await browser.newPage(options);
await page.goto(pathToFileURL(path.join(here, "page.html")).href);
await page.waitForLoadState("load");

const elements = await page.evaluate(() =>
  [...document.querySelectorAll("[data-pixel]")].map((e) => {
    const r = e.getBoundingClientRect();
    return {
      id: e.id,
      box: [r.x, r.y, r.width, r.height],
      points: e.dataset.pixel.split(" ").map((p) => {
        const [dx, dy] = p.split(",").map(Number);
        return { at: p, x: r.x + dx, y: r.y + dy };
      }),
    };
  }),
);
const png = (await page.screenshot({ fullPage: true })).toString("base64");
const colors = await decoder.evaluate(
  async ({ png, elements }) => {
    const blob = await (await fetch("data:image/png;base64," + png)).blob();
    const bitmap = await createImageBitmap(blob);
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const cx = canvas.getContext("2d");
    cx.drawImage(bitmap, 0, 0);
    return elements.map((e) =>
      e.points.map((p) => [...cx.getImageData(p.x, p.y, 1, 1).data].slice(0, 3)),
    );
  },
  { png, elements },
);
const out = Object.fromEntries(
  elements.map((e, i) => [
    e.id,
    { box: e.box, pixels: e.points.map((p, j) => ({ ...p, rgb: colors[i][j] })) },
  ]),
);
fs.writeFileSync(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
const pictures = process.argv[2];
if (pictures) {
  fs.mkdirSync(pictures, { recursive: true });
  for (const e of elements) {
    await page.locator("#" + e.id).screenshot({ path: path.join(pictures, `chromium-${e.id}.png`) });
  }
}
await browser.close();
