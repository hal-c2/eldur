// Records what Chromium answers for the probe pages next to this file, so
// `metadata_e2e.rs` compares the converted pages against a browser rather
// than against a reading of the specification.
//
//   node crates/eldur_convert/tests/metadata/chromium.mjs
//
// writes `chromium.json`. Pages are served at an `https:` URL (the base the
// converter is given), and once from a local file, whose directory is
// replaced by `{dir}` because it differs per machine.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { loadPlaywright } from "../../../../tools/visual/playwright.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const URL_HTTPS = "https://example.com/docs/guide/index.html";

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage();

async function probe(file, url) {
  const html = fs.readFileSync(path.join(here, file), "utf8");
  await page.route("**/*", (route) =>
    route.request().url() === url
      ? route.fulfill({ status: 200, contentType: "text/html", body: html })
      : route.fulfill({ status: 404, body: "" }),
  );
  await page.goto(url);
  const result = JSON.parse(await page.evaluate("probe()"));
  await page.unrouteAll();
  return result;
}

const out = {};
for (const name of ["probe", "base", "literal"]) {
  out[`${name}-https`] = await probe(`${name}.html`, URL_HTTPS);
}

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "eldur-metadata-"));
fs.copyFileSync(path.join(here, "probe.html"), path.join(dir, "index.html"));
const dirUrl = pathToFileURL(dir).href;
await page.goto(`${dirUrl}/index.html`);
const local = JSON.parse(await page.evaluate("probe()"));
for (const key of Object.keys(local)) {
  if (typeof local[key] === "string") {
    local[key] = local[key].split(dirUrl).join("{dir}");
  }
}
// `../up.html` leaves the temporary directory, so it names a different
// file on every machine.
delete local.parentHref;
out["probe-local"] = local;
fs.rmSync(dir, { recursive: true });

await browser.close();
fs.writeFileSync(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 2)}\n`);
