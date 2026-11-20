// Records Chromium for the pages `module_graph_e2e.rs` converts. Each page is
// served from this directory over http (a module script never runs from a
// file: URL), a missing file is a 404, and `/` is this directory, as
// `--site-root` makes it for the converter.
//
//   node crates/eldur_convert/tests/module_graph/gen.mjs
//
// writes `chromium.json`: per page, the `LC ` console lines in order and the
// text of the elements the test reads. Regenerate it; never edit it by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { loadPlaywright } from "../../../../tools/visual/playwright.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const ORIGIN = "http://eldur.test";

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();

async function record(name, read, click) {
  const page = await browser.newPage();
  const lines = [];
  const errors = [];
  page.on("console", (m) => {
    if (m.text().startsWith("LC ")) lines.push(m.text().slice(3));
  });
  page.on("pageerror", (e) => errors.push(`${e.name}: ${e.message}`));
  await page.route("**/*", (route) => {
    const url = new URL(route.request().url());
    const file = path.join(here, decodeURIComponent(url.pathname));
    if (url.origin !== ORIGIN || !file.startsWith(here) || !fs.existsSync(file)) {
      return route.fulfill({ status: 404, body: "" });
    }
    const type = file.endsWith(".html") ? "text/html" : "text/javascript";
    return route.fulfill({ status: 200, contentType: type, body: fs.readFileSync(file) });
  });
  await page.goto(`${ORIGIN}/${name}/index.html`);
  await page.waitForTimeout(500);
  if (click) {
    await page.click(click);
    await page.waitForTimeout(300);
  }
  const text = {};
  for (const id of read) {
    text[id] = await page.evaluate((i) => document.getElementById(i).textContent, id);
  }
  await page.close();
  return { lines, text, errors };
}

const out = {
  repro: await record("repro", ["a", "b"]),
  order: await record("order", ["out"], "#go"),
  rooted: await record("rooted", ["a", "b"]),
  lazy: await record("lazy", ["out"]),
  nested: await record("nested/page", ["a", "b"]),
  eager: await record("eager", ["out"]),
  element: await record("element", ["out", "label"]),
  cycle: await record("cycle", ["out"]),
  skip: await record("skip", ["out"]),
  cross: await record("cross", ["out"]),
  hoist: await record("hoist", ["out"]),
  cyclewait: await record("cyclewait", ["out"]),
  cycleentry: await record("cycleentry", ["out"]),
  listenthrow: await record("listenthrow", ["out"], "#go"),
  onclickthrow: await record("onclickthrow", ["out"], "#go"),
  onloadthrow: await record("onloadthrow", ["out"]),
  onloadnull: await record("onloadnull", ["out"]),
  onloadplace: await record("onloadplace", ["out"]),
  onloadreset: await record("onloadreset", ["out"]),
  loadvalue: await record("loadvalue", ["out"]),
  loadtwice: await record("loadtwice", ["out"]),
  loadoptions: await record("loadoptions", ["out"]),
  readlater: await record("readlater", ["out"]),
};
await browser.close();
fs.writeFileSync(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 2)}\n`);
