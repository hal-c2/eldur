// Records Chromium for the pages `apps_e2e.rs` converts (`*.html` here, each with
// its `*.json`: `steps` to run with real input, and what to `watch`). After load
// and each step it keeps the console lines since the last step and what
// `snapshot.js` reads.
//
//   node crates/eldur_convert/tests/apps/gen.mjs [--shots DIR] [--only a,b]
//
// writes `chromium.json`, and with `--shots` each page's last state as
// DIR/<page>.png. `--only` rerecords those pages and keeps the rest. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));
const { writeRecording, drainConsole, isMark, trackPending, catchUp } = await import(path.join(root, "tools/visual/record.mjs"));
const shots = process.argv.includes("--shots") ? process.argv[process.argv.indexOf("--shots") + 1] : null;

const only = process.argv.includes("--only") ? process.argv[process.argv.indexOf("--only") + 1].split(",") : null;

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
let log = [];
async function openTab() {
  const tab = await browser.newPage({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  tab.on("console", (m) => {
    if (!isMark(m.text())) log.push(m.text());
  });
  tab.on("pageerror", (e) => {
    throw new Error(`page threw: ${e.message}`);
  });
  await trackPending(tab);
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
  if (step.clickAt) await page.mouse.click(step.clickAt[0], step.clickAt[1], { button: step.button ?? "left" });
  if (step.dblAt) await page.mouse.dblclick(step.dblAt[0], step.dblAt[1]);
  if (step.move) await page.mouse.move(step.move[0], step.move[1]);
  if (step.down) await page.mouse.down({ button: step.down });
  if (step.up) await page.mouse.up({ button: step.up });
  if (step.hold) await page.keyboard.down(step.hold);
  if (step.unhold) await page.keyboard.up(step.unhold);
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
  if (step.keys) await page.keyboard.type(step.keys);
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
  // A step is what happened in its window, as the Qt side waits the same
  // window. Timers due in it may still be waiting on a loaded machine: they
  // run first. What is due later (a frame loop's next frame among it) belongs
  // to the next step, so a page that never stops does not hold this.
  await catchUp(page);
  // A bitmap of what is on screen, for the page to read `paint` watches from.
  let canvas = "null";
  if (watch.some((w) => w.paint)) {
    const shot = (await page.screenshot()).toString("base64");
    await page.evaluate(async (data) => {
      // One canvas, and each bitmap released: a page's captures add up, and after
      // twenty or so Chromium's GPU process dies of them.
      const blob = await (await fetch("data:image/png;base64," + data)).blob();
      const image = await createImageBitmap(blob);
      if (!window.__shot) {
        const c = document.createElement("canvas");
        c.width = image.width;
        c.height = image.height;
        window.__shot = c.getContext("2d", { willReadFrequently: true });
      }
      window.__shot.drawImage(image, 0, 0);
      image.close();
    }, shot);
    canvas = "(x, y) => { const d = window.__shot.getImageData(x, y, 1, 1).data; return `rgb(${d[0]}, ${d[1]}, ${d[2]})`; }";
  }
  const read = await page.evaluate(
    `(() => { ${snapshot}; return eldurSnapshot(document, getComputedStyle, ${JSON.stringify(watch)}, ${canvas}); })()`,
  );
  await drainConsole(page);
  const lines = log;
  log = [];
  return { name, log: lines, state: read };
}

const recorded = path.join(here, "chromium.json");
const kept = only ? JSON.parse(fs.readFileSync(recorded, "utf8")) : {};
const out = {};
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  const name = file.slice(0, -5);
  if (only && !only.includes(name)) {
    out[name] = kept[name];
    continue;
  }
  const spec = JSON.parse(fs.readFileSync(path.join(here, `${name}.json`), "utf8"));
  log = [];
  // A tab that scrolled to its end can no longer take a screenshot on the next page.
  await page.close().catch(() => {});
  page = await openTab();
  await page.mouse.move(1200, 790);
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  // A page caught mid-flight at load has nothing to record there that a rerun would repeat.
  const steps = [await state("load", spec.loadMoves ? [] : spec.watch)];
  for (const step of spec.steps) {
    await act(step);
    if (spec.bubble) {
      // Chromium's validation bubble kills the headless renderer, sooner or later:
      // what the page said before that is the record, and its state is not read.
      await page.waitForTimeout(200);
      steps.push({ name: step.name, log, state: {} });
      log = [];
      break;
    }
    steps.push(await state(step.name, spec.watch));
  }
  // A page that logs and has nothing recorded was read before it said anything.
  const html = fs.readFileSync(path.join(here, file), "utf8");
  if (/console\.(log|info|warn|error)/.test(html) && steps.every((s) => s.log.length === 0)) {
    throw new Error(`${name}: the page logs but no step recorded a line`);
  }
  out[name] = steps;
  if (shots) {
    fs.mkdirSync(shots, { recursive: true });
    await page.screenshot({ path: path.join(shots, `${name}.png`) });
  }
}
await browser.close();
writeRecording(recorded, JSON.stringify(out, null, 1) + "\n");
