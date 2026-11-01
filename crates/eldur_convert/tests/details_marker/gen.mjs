// Records what Chromium paints for the disclosure markers of `page.html`, which
// `details_marker_e2e.rs` compares a converted page's pixels with, after the
// same presses on each marker.
//
//   node crates/eldur_convert/tests/details_marker/gen.mjs
//
// writes `chromium.json`: for each state and summary, the ink of the rows of
// the marker's square, and the first inked column of each line of text past it.
// Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));
const pw = await loadPlaywright();
const chromium = pw.chromium || pw;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
await page.setContent(fs.readFileSync(path.join(here, "page.html"), "utf8"));

const summaries = await page.evaluate(() => [...document.querySelectorAll("summary")].map((s) => {
  const b = s.getBoundingClientRect();
  const c = getComputedStyle(s);
  const em = parseFloat(c.fontSize);
  const [cx, cy] = s.dataset.click.split(",").map(Number);
  const pad = Number(s.dataset.pad || 0);
  const out = "out" in s.dataset;
  const plain = "plain" in s.dataset;
  // The marker takes about an em; what is further right is text.
  const side = plain ? 0 : Math.ceil(em * 1.06);
  return { id: s.id, x: b.x + pad - (out ? Math.ceil(em * 1.25) : 0), y: b.y + pad, click: [b.x + cx, b.y + cy],
           side, rows: Number(s.dataset.rows || Math.ceil(em * 1.2)), height: Math.ceil(em * 1.2) };
}));

const decode = async () => {
  const png = await page.screenshot({ type: "png" });
  return page.evaluate(async (b64) => {
    const img = new Image();
    img.src = "data:image/png;base64," + b64;
    await img.decode();
    const c = new OffscreenCanvas(img.width, img.height);
    const g = c.getContext("2d");
    g.drawImage(img, 0, 0);
    const d = g.getImageData(0, 0, img.width, img.height).data;
    return { w: img.width, data: Array.from(d) };
  }, png.toString("base64"));
};

const state = async () => {
  const shot = await decode();
  const ink = (x, y) => {
    const i = (y * shot.w + x) * 4;
    return Math.min(shot.data[i], shot.data[i + 1], shot.data[i + 2]) < 128;
  };
  const out = {};
  for (const s of summaries) {
    const x = Math.round(s.x);
    const y = Math.round(s.y);
    const rows = [];
    for (let r = 0; r < s.height; r++) {
      let row = "";
      for (let col = 0; col < s.side; col++) {
        row += ink(x + col, y + r) ? "#" : ".";
      }
      rows.push(row);
    }
    const starts = [];
    for (let r = 0; r < s.rows; r++) {
      let first = -1;
      // A marker has the first line; the lines below go back to the edge.
      for (let col = r < s.height ? s.side : 0; col < 120; col++) {
        if (ink(x + col, y + r)) {
          first = col;
          break;
        }
      }
      starts.push(first);
    }
    // The middle of the ink: inside the triangle whichever way it points.
    const inked = [];
    rows.forEach((row, r) => [...row].forEach((c, col) => c === "#" && inked.push([col, r])));
    let paint = null;
    if (inked.length > 0) {
      const col = Math.round(inked.reduce((a, p) => a + p[0], 0) / inked.length);
      const r = Math.round(inked.reduce((a, p) => a + p[1], 0) / inked.length);
      const i = ((y + r) * shot.w + x + col) * 4;
      paint = [col, r, shot.data[i], shot.data[i + 1], shot.data[i + 2]];
    }
    out[s.id] = { rows, starts, paint };
  }
  return out;
};

const out = { summaries, states: {} };
out.states.initial = await state();
const press = async () => {
  for (const s of summaries) {
    await page.mouse.click(s.click[0], s.click[1]);
  }
  await page.mouse.move(790, 590);
  await page.waitForTimeout(50);
};
await press();
out.states.pressed = await state();
await press();
out.states.again = await state();
await browser.close();

fs.writeFileSync(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
