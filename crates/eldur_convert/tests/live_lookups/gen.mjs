// Records what Chromium's console says for the pages `live_lookups_e2e.rs`
// converts (`*.html` here): the lines starting "LC ", logged at load and, on a
// page with a `#go` button, once it has been pressed.
//
//   node crates/eldur_convert/tests/live_lookups/gen.mjs
//
// writes `chromium.json`. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));
const { writeRecording, trackPending, settle, requireLogged } = await import(path.join(root, "tools/visual/record.mjs"));

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
// A page that throws in Chromium would pin a broken expectation.
page.on("pageerror", (e) => {
  throw new Error(`page threw: ${e.message}`);
});
const out = {};
for (const file of fs.readdirSync(here).filter((f) => f.endsWith(".html")).sort()) {
  log = [];
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  if (await page.$("#go")) await page.click("#go");
  // The log is all the page says, however late its timers run.
  await settle(page);
  requireLogged(file, fs.readFileSync(path.join(here, file), "utf8"), log, "LC ");
  out[file.slice(0, -5)] = log;
}
await browser.close();
writeRecording(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
