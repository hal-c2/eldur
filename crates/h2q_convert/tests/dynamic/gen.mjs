// Records what Chromium reports for the corpus pages that `dynamic_styles_e2e.rs`
// drives, after the same clicks, hovers and focus changes, so the tests compare
// the converted pages against a browser.
//
//   node crates/h2q_convert/tests/dynamic/gen.mjs
//
// writes `cases.json` (the computed.mjs input) and `chromium.json`: for each case
// name, the computed values (`tools/visual/computed.mjs`) or the element boxes
// (`tools/visual/geometry.mjs`). Regenerate it; never edit it by hand.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const corpus = (name) => path.join(root, "tests/corpus", name, "index.html");
const tools = path.join(root, "tools/visual");

// Past every transition on these pages (0.2s).
const SETTLED = 400;

const computed = [
  // 29-class-toggles: the DOMContentLoaded handler selects tab1 and marks item1 done.
  ["29 initial item1", "29-class-toggles", [], "#item1", ["opacity"]],
  ["29 initial tab1", "29-class-toggles", [], "#tab1", ["color", "background-color"]],
  ["29 initial tab2", "29-class-toggles", [], "#tab2", ["color", "background-color"]],
  ["29 initial tab3", "29-class-toggles", [], "#tab3", ["color"]],
  ["29 initial status", "29-class-toggles", [], "#status", ["color"]],
  ["29 initial panel", "29-class-toggles", [], "#panel", ["display"]],
  ["29 tab2 tab1", "29-class-toggles", [{ click: "#tab2" }], "#tab1", ["color", "background-color"]],
  ["29 tab2 tab2", "29-class-toggles", [{ click: "#tab2" }], "#tab2", ["color", "background-color"]],
  ["29 tab2 tab3", "29-class-toggles", [{ click: "#tab2" }], "#tab3", ["color"]],
  ["29 ok", "29-class-toggles", [{ click: "#ok" }], "#status", ["color"]],
  ["29 warn", "29-class-toggles", [{ click: "#warn" }], "#status", ["color"]],
  ["29 ok warn", "29-class-toggles", [{ click: "#ok" }, { click: "#warn" }], "#status", ["color"]],
  ["29 ok twice", "29-class-toggles", [{ click: "#ok" }, { click: "#ok" }], "#status", ["color"]],
  ["29 more", "29-class-toggles", [{ click: "#more" }], "#panel", ["display"]],
  ["29 hover item2", "29-class-toggles", [{ hover: "#item2" }], "#item2", ["opacity", "background-color"]],
  ["29 hover item1", "29-class-toggles", [{ hover: "#item1" }], "#item1", ["opacity"]],
  ["29 click item2", "29-class-toggles", [{ click: "#item2" }], "#item2", ["opacity"]],
  ["29 click item2 leave", "29-class-toggles", [{ click: "#item2" }, { hover: "h1" }], "#item2", ["opacity"]],
  // 12-hover-states: :hover, :focus and :disabled on buttons, :hover on cards and on an ancestor.
  ["12 hover primary", "12-hover-states", [{ hover: ".btn--primary" }], ".btn--primary", ["background-color"]],
  ["12 hover secondary", "12-hover-states", [{ hover: ".btn--secondary" }], ".btn--secondary", ["background-color"]],
  ["12 hover disabled", "12-hover-states", [{ hover: ".btn:disabled" }], ".btn:disabled", ["background-color"]],
  ["12 focus primary", "12-hover-states", [{ focus: ".btn--primary" }], ".btn--primary", ["box-shadow"]],
  ["12 hover card", "12-hover-states", [{ hover: ".lift-card" }], ".lift-card", ["transform"]],
  ["12 hover item", "12-hover-states", [{ hover: ".item" }], ".item", ["background-color"]],
  ["12 hover item icon", "12-hover-states", [{ hover: ".item" }], ".item .icon", ["background-color"]],
  ["12 initial item icon", "12-hover-states", [], ".item .icon", ["background-color"]],
  // 15-script-counter: a class written by the page script, :hover and :focus.
  ["15 dec", "15-script-counter", [{ click: "#dec" }], "#count", ["color"]],
  ["15 dec inc", "15-script-counter", [{ click: "#dec" }, { click: "#inc" }], "#count", ["color"]],
  ["15 hover inc", "15-script-counter", [{ hover: "#inc" }], "#inc", ["background-color"]],
  ["15 hover dec", "15-script-counter", [{ hover: "#dec" }], "#dec", ["background-color"]],
  ["15 focus input", "15-script-counter", [{ focus: "#mirror-input" }], "#mirror-input", ["border-top-color"]],
];

const geometry = [
  ["29 initial geometry", "29-class-toggles", [], ["#list", "#item2"]],
  ["29 compact geometry", "29-class-toggles", [{ click: "#compact" }], ["#list", "#item2"]],
  ["29 more geometry", "29-class-toggles", [{ click: "#more" }], ["#panel"]],
  ["12 card geometry", "12-hover-states", [{ hover: "h1" }], [".lift-card"]],
  ["12 hover card geometry", "12-hover-states", [{ hover: ".lift-card" }], [".lift-card"]],
];

const cases = computed.map(([name, page, run, select, props]) => ({
  name,
  html: fs.readFileSync(corpus(page), "utf8"),
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
for (const [name, page, run, selectors] of geometry) {
  const lines = execFileSync(
    "node",
    [path.join(tools, "geometry.mjs"), "--run", JSON.stringify(run), "--after", String(SETTLED), corpus(page), ...selectors],
    { encoding: "utf8" },
  );
  const boxes = {};
  for (const line of lines.split("\n").filter(Boolean)) {
    const { select, index, ...rect } = JSON.parse(line);
    if (index === 0) boxes[select] = rect;
  }
  out[name] = boxes;
}
fs.writeFileSync(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 1)}\n`);
