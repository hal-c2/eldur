// Records what Chromium's console says for the pages `created_rules_e2e.rs`
// converts (`*.html` here): the lines starting "CS ", logged at load and for
// each click named by `<body data-clicks>`, which lands on the middle of the
// named element. Each point is recorded, so the test presses the same pixels.
//
//   node crates/eldur_convert/tests/created_rules/gen.mjs
//
// writes `chromium.json`. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));
const { writeRecording, drainConsole } = await import(path.join(root, "tools/visual/record.mjs"));

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1,
});
let log = [];
page.on("console", (m) => {
  if (m.text().startsWith("CS ")) log.push(m.text());
});
page.on("pageerror", (e) => {
  throw new Error(`page threw: ${e.message}`);
});
const out = {};
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  log = [];
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  // A page whose logging runs in timers says it has finished by making `#csdone`.
  if (fs.readFileSync(path.join(here, file), "utf8").includes("csdone")) {
    await page.waitForSelector("#csdone", { state: "attached", timeout: 30000 });
  }
  const names = (await page.getAttribute("body", "data-clicks")) || "";
  const clicks = [];
  for (const name of names.split(/\s+/).filter(Boolean)) {
    const point = await page.evaluate((name) => {
      const r = document.querySelector(name).getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }, name);
    await page.mouse.click(point.x, point.y);
    clicks.push({ name, x: point.x, y: point.y });
    // Console events arrive asynchronously; the next page must not steal these.
    await page.waitForTimeout(30);
  }
  await page.waitForTimeout(30);
  // What shows at each `data-probe="dx,dy"`: the first opaque background going
  // up the tree, or the border's colour with `data-border`.
  const probes = await page.evaluate(() =>
    [...document.querySelectorAll("[data-probe]")].map((e) => {
      const [dx, dy] = e.dataset.probe.split(",").map(Number);
      const r = e.getBoundingClientRect();
      let color = e.dataset.border ? getComputedStyle(e).borderTopColor : null;
      for (let at = e; !color && at; at = at.parentElement) {
        const bg = getComputedStyle(at).backgroundColor;
        if (bg !== "rgba(0, 0, 0, 0)") color = bg;
      }
      // An element's own opacity or visibility, over the white page.
      const own = getComputedStyle(e);
      const alpha = own.visibility === "hidden" ? 0 : Number(own.opacity);
      const hex = (color || "rgb(255, 255, 255)").match(/\d+/g).slice(0, 3)
        .map((v) => Math.round(Number(v) * alpha + 255 * (1 - alpha)).toString(16).padStart(2, "0")).join("");
      return { id: e.id, x: r.left + dx, y: r.top + dy, color: "#" + hex };
    }));
  await drainConsole(page);
  out[file.slice(0, -5)] = { log, clicks, probes };
}
await browser.close();
writeRecording(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
