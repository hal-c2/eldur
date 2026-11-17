// Records what Chromium reports for the page `inline_states_e2e.rs` drives
// (`page.html` here: an inline `<a>` and `<em>` in a list item's line, with
// `:hover` and `:active` rules), after the same hovers and presses.
//
//   node crates/eldur_convert/tests/inline_states/gen.mjs
//
// writes `cases.json` (the computed.mjs input) and `chromium.json`: for each
// case name, the computed values (`tools/visual/computed.mjs`), and under
// "geometry" the boxes of each selector's matches (`tools/visual/geometry.mjs`).
// Regenerate it; never edit it by hand.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { requireAll, writeRecording } from "../../../../tools/visual/record.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const html = fs.readFileSync(path.join(here, "page.html"), "utf8");
const tools = path.join(root, "tools/visual");

const SETTLED = 100;

const computed = [
  ["initial link", [], ".c a"],
  ["initial em", [], ".c em"],
  ["hover link", [{ hover: ".c a" }], ".c a"],
  ["hover link em", [{ hover: ".c a" }], ".c em"],
  ["hover em", [{ hover: ".c em" }], ".c em"],
  ["hover em link", [{ hover: ".c em" }], ".c a"],
  ["left link", [{ hover: ".c a" }, { hover: "#out" }], ".c a"],
  ["press link", [{ press: ".c a" }], ".c a"],
];

const cases = computed.map(([name, run, select]) => ({
  name,
  html,
  select,
  props: ["color"],
  run,
  after: SETTLED,
}));
const casesFile = path.join(here, "cases.json");
writeRecording(casesFile, `${JSON.stringify(cases, null, 1)}\n`);

const out = {};
const lines = execFileSync("node", [path.join(tools, "computed.mjs"), casesFile], { encoding: "utf8" });
for (const line of lines.split("\n").filter(Boolean)) {
  const { name, ...values } = JSON.parse(line);
  out[name] = values;
}
requireAll(cases.map((c) => c.name), out, "computed.mjs");

const boxes = {};
const geometry = execFileSync(
  "node",
  [path.join(tools, "geometry.mjs"), "--after", String(SETTLED), "--html", html, ".c a", ".c em", "#out"],
  { encoding: "utf8" },
);
for (const line of geometry.split("\n").filter(Boolean)) {
  const { select, index, ...rect } = JSON.parse(line);
  (boxes[select] ??= [])[index] = rect;
}
out.geometry = boxes;
writeRecording(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 1)}\n`);
