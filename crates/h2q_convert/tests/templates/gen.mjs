// Records what Chromium reports for the pages `templates_e2e.rs` drives (the
// focused `page.html` here and corpus 16-script-todo), after the same clicks
// and typing, so the tests compare the elements the converted scripts create
// against a browser's.
//
//   node crates/h2q_convert/tests/templates/gen.mjs
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
const pages = {
  T: path.join(here, "page.html"),
  16: path.join(root, "tests/corpus/16-script-todo/index.html"),
};
const tools = path.join(root, "tools/visual");

const SETTLED = 100;

// The created items' texts in document order, as a CSS string on <html>, so
// computed.mjs can read them as `--h2q-order`.
const order = (sel) => ({
  eval: `document.documentElement.style.setProperty("--h2q-order", JSON.stringify([...document.querySelectorAll(${JSON.stringify(sel)})].map((e) => e.textContent).join("|")))`,
});

const T = {
  add: [{ click: "#add" }],
  check: [{ click: "#list li:nth-child(1) input" }],
  label: [{ click: "#list li:nth-child(1) .label" }],
  labelAway: [{ click: "#list li:nth-child(1) .label" }, { hover: "#compact" }],
  remove: [{ click: "#list li:nth-child(2) .remove" }],
  compact: [{ click: "#compact" }],
  hover: [{ hover: "#list li:nth-child(1)" }],
};
const S16 = {
  add: [{ type: ["#todo-input", "Buy milk"] }, { click: "#add-btn" }],
  check: [{ click: "#todo-list li:nth-child(1) input" }],
  remove: [{ click: "#todo-list li:nth-child(2) .delete" }],
};
const label = (n) => `#list li:nth-child(${n}) .label`;
const item = (n) => `#list li:nth-child(${n})`;
const text = (n) => `#todo-list li:nth-child(${n}) .todo-text`;
const LABEL = ["color", "text-decoration-line"];

const computed = [
  ["T initial order", "T", [order("#list .label")], "html", ["--h2q-order"]],
  ["T initial label1", "T", [], label(1), LABEL],
  ["T initial label2", "T", [], label(2), LABEL],
  ["T initial item1", "T", [], item(1), ["background-color"]],
  ["T initial item2", "T", [], item(2), ["background-color"]],
  ["T add order", "T", [...T.add, order("#list .label")], "html", ["--h2q-order"]],
  ["T add label3", "T", T.add, label(3), LABEL],
  ["T check label1", "T", T.check, label(1), LABEL],
  ["T check item1", "T", T.check, item(1), ["background-color"]],
  ["T label label1", "T", T.label, label(1), LABEL],
  ["T label item1", "T", T.label, item(1), ["background-color"]],
  ["T label away label1", "T", T.labelAway, label(1), LABEL],
  ["T remove order", "T", [...T.remove, order("#list .label")], "html", ["--h2q-order"]],
  ["T hover label1", "T", T.hover, label(1), LABEL],
  ["T hover label2", "T", T.hover, label(2), LABEL],
  ["16 initial order", "16", [order("#todo-list .todo-text")], "html", ["--h2q-order"]],
  ["16 initial text1", "16", [], text(1), LABEL],
  ["16 initial text3", "16", [], text(3), LABEL],
  ["16 add order", "16", [...S16.add, order("#todo-list .todo-text")], "html", ["--h2q-order"]],
  ["16 add text4", "16", S16.add, text(4), LABEL],
  ["16 check text1", "16", S16.check, text(1), LABEL],
  ["16 check text3", "16", S16.check, text(3), LABEL],
  ["16 remove order", "16", [...S16.remove, order("#todo-list .todo-text")], "html", ["--h2q-order"]],
];

const geometry = [
  ["T initial geometry", "T", [], ["#list", "#list li", "#list .label", "#list input", "#list .remove"]],
  ["T add geometry", "T", T.add, ["#list", "#list li"]],
  ["T compact geometry", "T", T.compact, ["#list", "#list li"]],
  ["T remove geometry", "T", T.remove, ["#list", "#list li"]],
  ["16 initial geometry", "16", [], ["#todo-list", "#todo-list li", "#todo-list .todo-text", "#todo-list input", "#todo-list .delete"]],
  ["16 add geometry", "16", S16.add, ["#todo-list", "#todo-list li"]],
  ["16 remove geometry", "16", S16.remove, ["#todo-list", "#todo-list li"]],
];

const cases = computed.map(([name, page, run, select, props]) => ({
  name,
  html: fs.readFileSync(pages[page], "utf8"),
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
    [
      path.join(tools, "geometry.mjs"),
      "--run",
      JSON.stringify(run),
      "--after",
      String(SETTLED),
      "--html",
      fs.readFileSync(pages[page], "utf8"),
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
