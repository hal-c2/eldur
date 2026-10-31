// Records Chromium for the pages `created_paint_e2e.rs` converts (`*.html`
// here): the console lines starting "CS ", and the real pixels of a screenshot
// at each `data-pixel="dx,dy dx,dy"`, relative to the element's untransformed
// layout place. They are read at load and again after each of
// `<body data-steps>`: `hover SEL`, `press SEL`, `release SEL`, `click SEL`,
// `away`.
//
//   node crates/eldur_convert/tests/created_paint/gen.mjs
//
// writes `chromium.json`. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const options = { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 };
const page = await browser.newPage(options);
const decoder = await browser.newPage(options);
let log = [];
page.on("console", (m) => {
  if (m.text().startsWith("CS ")) log.push(m.text());
});
page.on("pageerror", (e) => {
  throw new Error(`page threw: ${e.message}`);
});

// Layout places ignore transforms, as the test's probes do.
const places = () =>
  page.evaluate(() =>
    [...document.querySelectorAll("[data-pixel]")].flatMap((e) => {
      let x = 0;
      let y = 0;
      for (let at = e; at; at = at.offsetParent) {
        x += at.offsetLeft;
        y += at.offsetTop;
      }
      return e.dataset.pixel.split(" ").map((p) => {
        const [dx, dy] = p.split(",").map(Number);
        return { id: e.id, at: p, x: x + dx, y: y + dy };
      });
    }),
  );

async function pixels(points) {
  const png = (await page.screenshot()).toString("base64");
  const colors = await decoder.evaluate(
    async ({ png, points }) => {
      const blob = await (await fetch("data:image/png;base64," + png)).blob();
      const bitmap = await createImageBitmap(blob);
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const cx = canvas.getContext("2d");
      cx.drawImage(bitmap, 0, 0);
      return points.map((p) => [...cx.getImageData(p.x, p.y, 1, 1).data].slice(0, 3));
    },
    { png, points },
  );
  return points.map((p, i) => ({ ...p, rgb: colors[i] }));
}

async function step(text) {
  const [verb, selector] = text.split(" ");
  if (verb === "away") {
    await page.mouse.move(1200, 780);
    return { verb, x: 1200, y: 780 };
  }
  const box = await page.evaluate((s) => {
    const r = document.querySelector(s).getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }, selector);
  if (verb === "hover") await page.mouse.move(box.x, box.y);
  if (verb === "press") {
    await page.mouse.move(box.x, box.y);
    await page.mouse.down();
  }
  if (verb === "release") await page.mouse.up();
  if (verb === "click") await page.mouse.click(box.x, box.y);
  return { verb, ...box };
}

const out = {};
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  log = [];
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  await page.mouse.move(1200, 780);
  await page.waitForTimeout(30);
  const points = await places();
  const first = await pixels(points);
  const names = (await page.getAttribute("body", "data-steps")) || "";
  const steps = [];
  for (const name of names.split("|").filter(Boolean)) {
    const done = await step(name);
    // Past every transition on the pages.
    await page.waitForTimeout(250);
    steps.push({ name, ...done, pixels: await pixels(points) });
  }
  await page.waitForTimeout(30);
  out[file.slice(0, -5)] = { log, pixels: first, steps };
}
await browser.close();
fs.writeFileSync(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
