// Records what Chromium reports for the page `inline_templates_e2e.rs`
// drives (`page.html` here: block `<li>`s whose inline `<span>`, `<strong>`
// and `<a>` a script creates), after the same clicks and hovers.
//
//   node crates/eldur_convert/tests/inline_templates/gen.mjs
//
// writes `cases.json` (the computed.mjs input) and `chromium.json`: for each
// case name, the computed values (`tools/visual/computed.mjs`) or the boxes of
// every match of each selector, in document order (`tools/visual/geometry.mjs`).
// Regenerate it; never edit it by hand.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const html = fs.readFileSync(path.join(here, "page.html"), "utf8");
const tools = path.join(root, "tools/visual");

const SETTLED = 100;

const warn = [{ click: "#warn" }];
const done = [{ click: "#done" }];
const hover = [{ hover: "#list li:nth-child(1)" }];
const warnHover = [{ click: "#warn" }, { hover: "#list li:nth-child(1)" }];
const TEXT = ["color", "font-weight", "font-style", "text-decoration-line", "background-color"];
const label = (n) => `#list li:nth-child(${n}) .label`;
const strong = (n) => `#list li:nth-child(${n}) strong`;
const item = (n) => `#list li:nth-child(${n})`;

const computed = [
  ["initial label1", [], label(1), TEXT],
  ["initial strong1", [], strong(1), TEXT],
  ["initial item1", [], item(1), TEXT],
  ["initial tag", [], "#list .tag", TEXT],
  ["initial link", [], "#list a", TEXT],
  ["warn label1", warn, label(1), TEXT],
  ["warn label2", warn, label(2), TEXT],
  ["hover label1", hover, label(1), TEXT],
  ["hover label2", hover, label(2), TEXT],
  ["warn hover label1", warnHover, label(1), TEXT],
  ["done item2", done, item(2), TEXT],
  ["done label2", done, label(2), TEXT],
  ["done strong2", done, strong(2), TEXT],
];

const SELECTORS = ["#list", "#list li", "#list .label", "#list strong", "#list .tag", "#list a"];
const geometry = [
  ["initial geometry", [], SELECTORS],
  ["warn geometry", warn, SELECTORS],
];

const cases = computed.map(([name, run, select, props]) => ({
  name,
  html,
  select,
  props,
  run,
  after: SETTLED,
}));
const casesFile = path.join(here, "cases.json");
fs.writeFileSync(casesFile, `${JSON.stringify(cases, null, 1)}\n`);

const out = {};
const lines = execFileSync("node", [path.join(tools, "computed.mjs"), casesFile], { encoding: "utf8" });
for (const line of lines.split("\n").filter(Boolean)) {
  const { name, ...values } = JSON.parse(line);
  out[name] = values;
}
for (const [name, run, selectors] of geometry) {
  const lines = execFileSync(
    "node",
    [
      path.join(tools, "geometry.mjs"),
      "--run",
      JSON.stringify(run),
      "--after",
      String(SETTLED),
      "--html",
      html,
      ...selectors,
    ],
    { encoding: "utf8" },
  );
  const boxes = {};
  for (const line of lines.split("\n").filter(Boolean)) {
    const { select, index, ...rect } = JSON.parse(line);
    (boxes[select] ??= [])[index] = rect;
  }
  out[name] = boxes;
}
fs.writeFileSync(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 1)}\n`);
