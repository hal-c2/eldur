// Records what Chromium reports for the page `structural_e2e.rs` drives
// (`page.html` here: `li:first-child`, `li:last-child` and
// `li:nth-child(even)` on a list scripts add to, remove from and re-render),
// after the same clicks, so the test compares the converted page against a
// browser's.
//
//   node crates/eldur_convert/tests/structural/gen.mjs
//
// writes `cases.json` (the computed.mjs input) and `chromium.json`: for each
// case name, the computed values (`tools/visual/computed.mjs`) or the boxes of
// every match of each selector, in document order (`tools/visual/geometry.mjs`).
// Regenerate it; never edit it by hand.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { requireAll, writeRecording } from "../../../../tools/visual/record.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const page = path.join(here, "page.html");
const tools = path.join(root, "tools/visual");

const SETTLED = 100;

// The labels in document order, as a CSS string on <html>, so computed.mjs
// can read them as `--eldur-order`.
const order = {
  eval: `document.documentElement.style.setProperty("--eldur-order", JSON.stringify([...document.querySelectorAll("#list .label")].map((e) => e.textContent).join("|")))`,
};

const click = (id) => ({ click: `#${id}` });
const steps = {
  initial: [],
  add: [click("add")],
  addTwice: [click("add"), click("add")],
  prepend: [click("prepend")],
  shift: [click("shift")],
  shiftTwice: [click("shift"), click("shift")],
  addShift: [click("add"), click("shift")],
  reverse: [click("reverse")],
  addReverse: [click("add"), click("reverse")],
  reverseAdd: [click("reverse"), click("add")],
};
// The most items any of them has.
const ITEMS = 5;

const computed = [];
const geometry = [];
for (const [step, run] of Object.entries(steps)) {
  computed.push([`${step} order`, [...run, order], "html", ["--eldur-order"]]);
  for (let n = 1; n <= ITEMS; n++) {
    computed.push([`${step} item${n}`, run, `#list li:nth-child(${n})`, ["background-color", "border-top-width", "border-bottom-color"]]);
    computed.push([`${step} label${n}`, run, `#list li:nth-child(${n}) .label`, ["color"]]);
  }
  geometry.push([`${step} geometry`, run, ["#list", "#list li", "#list .label"]]);
}

const html = fs.readFileSync(page, "utf8");
const cases = computed.map(([name, run, select, props]) => ({
  name,
  html,
  select,
  props,
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
writeRecording(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 1)}\n`);
