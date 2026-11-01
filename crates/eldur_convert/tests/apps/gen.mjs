// Records Chromium for the pages `apps_e2e.rs` converts (`*.html` here, each with
// its `*.json`: `steps` to run with real input, and what to `watch`). After load
// and each step it keeps the console lines since the last step and what
// `snapshot.js` reads.
//
//   node crates/eldur_convert/tests/apps/gen.mjs [--shots DIR]
//
// writes `chromium.json`, and with `--shots` each page's last state as
// DIR/<page>.png. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));
const shots = process.argv.includes("--shots") ? process.argv[process.argv.indexOf("--shots") + 1] : null;

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
let log = [];
async function openTab() {
  const tab = await browser.newPage({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  tab.on("console", (m) => log.push(m.text()));
  tab.on("pageerror", (e) => {
    throw new Error(`page threw: ${e.message}`);
  });
  return tab;
}
let page = await openTab();
const snapshot = fs.readFileSync(path.join(here, "snapshot.js"), "utf8");

async function center(selector) {
  return page.evaluate((s) => {
    const r = document.querySelector(s).getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }, selector);
}

async function act(step) {
  if (step.viewport) await page.setViewportSize({ width: step.viewport[0], height: step.viewport[1] });
  if (step.click) {
    const { x, y } = await center(step.click);
    await page.mouse.click(x, y);
  }
  if (step.clickAt) await page.mouse.click(step.clickAt[0], step.clickAt[1]);
  if (step.hover) {
    const { x, y } = await center(step.hover);
    await page.mouse.move(x, y);
  }
  if (step.type) {
    const { x, y } = await center(step.type[0]);
    await page.mouse.click(x, y);
    await page.keyboard.press("End");
    await page.keyboard.type(step.type[1]);
  }
  if (step.key) await page.keyboard.press(step.key);
  if (step.wheel) {
    const { x, y } = step.wheel[0] === "page" ? { x: 1200, y: 400 } : await center(step.wheel[0]);
    await page.mouse.move(x, y);
    await page.mouse.wheel(step.wheel[1], step.wheel[2]);
    // Chromium answers a wheel on a later frame, and may ease to its end.
    await page.waitForTimeout(400);
  }
  // Keys and fragment links ease to where they end up.
  if (step.settle) await page.waitForTimeout(step.settle);
}

async function state(name, watch) {
  await page.waitForTimeout(50);
  // A bitmap of what is on screen, for the page to read `paint` watches from.
  let canvas = "null";
  if (watch.some((w) => w.paint)) {
    const shot = (await page.screenshot()).toString("base64");
    await page.evaluate(async (data) => {
      const image = new Image();
      image.src = "data:image/png;base64," + data;
      await image.decode();
      const c = document.createElement("canvas");
      c.width = image.width;
      c.height = image.height;
      window.__shot = c.getContext("2d", { willReadFrequently: true });
      window.__shot.drawImage(image, 0, 0);
    }, shot);
    canvas = "(x, y) => { const d = window.__shot.getImageData(x, y, 1, 1).data; return `rgb(${d[0]}, ${d[1]}, ${d[2]})`; }";
  }
  const read = await page.evaluate(
    `(() => { ${snapshot}; return eldurSnapshot(document, getComputedStyle, ${JSON.stringify(watch)}, ${canvas}); })()`,
  );
  const lines = log;
  log = [];
  return { name, log: lines, state: read };
}

const out = {};
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  const name = file.slice(0, -5);
  const spec = JSON.parse(fs.readFileSync(path.join(here, `${name}.json`), "utf8"));
  log = [];
  // A tab that scrolled to its end can no longer take a screenshot on the next page.
  await page.close();
  page = await openTab();
  await page.mouse.move(1200, 790);
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  const steps = [await state("load", spec.watch)];
  for (const step of spec.steps) {
    await act(step);
    steps.push(await state(step.name, spec.watch));
  }
  out[name] = steps;
  if (shots) {
    fs.mkdirSync(shots, { recursive: true });
    await page.screenshot({ path: path.join(shots, `${name}.png`) });
  }
}
await browser.close();
fs.writeFileSync(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
