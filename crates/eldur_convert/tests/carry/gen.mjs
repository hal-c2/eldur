// Records what Chromium reports for the page `carry_e2e.rs` drives
// (`page.html` here: blocks of fractional height whose positions must not
// drift, a hidden paragraph a button shows, and paragraphs a script appends
// and inserts), after the same clicks.
//
//   node crates/eldur_convert/tests/carry/gen.mjs
//
// writes `chromium.json`: for each step, the boxes of every `.b` in document
// order and of `#c` (`tools/visual/geometry.mjs`). Regenerate it; never edit
// it by hand.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { writeRecording } from "../../../../tools/visual/record.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const page = path.join(here, "page.html");
const tools = path.join(root, "tools/visual");

const SETTLED = 100;

const click = (id) => ({ click: `#${id}` });
const steps = {
  initial: [],
  show: [click("show")],
  append: [click("append")],
  insert: [click("insert")],
};

const html = fs.readFileSync(page, "utf8");
const out = {};
for (const [name, run] of Object.entries(steps)) {
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
      "#c",
      "#c .b",
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
