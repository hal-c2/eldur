// Records what Chromium reports for the page `table_widths_e2e.rs` drives
// (`page.html` here: auto-layout tables of one to five columns, with and
// without specified widths in px and %, spans, spacing, padding and
// collapsed borders of several widths).
//
//   node crates/eldur_convert/tests/table_widths/gen.mjs
//
// writes `chromium.json`: for each table, by id, whether its width is auto,
// whether it collapses its
// borders and how wide they are between cells, its box, and the boxes of the
// cells of its `tr.m` row (`tools/visual/geometry.mjs`). Regenerate it;
// never edit it by hand.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { writeRecording } from "../../../../tools/visual/record.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const page = path.join(here, "page.html");
const tools = path.join(root, "tools/visual");

const html = fs.readFileSync(page, "utf8");
// Classes `c` and `t5` collapse borders, and give the edges between cells
// the widths here (see the page's style sheet).
const INNER = { c: 1, c2: 2, c3: 3, t5: 0 };
// A table with no `width` in its `style` is `width: auto`.
const tables = [...html.matchAll(/<table id="([^"]+)"(?: class="([^"]*)")?(?: style="([^"]*)")?/g)].map((m) => {
  const classes = (m[2] ?? "").split(/\s+/);
  const widths = classes.filter((c) => c in INNER).map((c) => INNER[c]);
  return {
    id: m[1],
    auto: !/(^|;)\s*width:/.test(m[3] ?? ""),
    collapse: widths.length > 0,
    inner: widths.length ? widths.at(-1) : 0,
  };
});
const ids = tables.map((t) => t.id);
const selectors = ids.flatMap((id) => [`#${id}`, `#${id} tr.m > td`]);
const lines = execFileSync(
  "node",
  [path.join(tools, "geometry.mjs"), "--html", html, ...selectors],
  { encoding: "utf8", maxBuffer: 1 << 24 },
);
const boxes = {};
for (const line of lines.split("\n").filter(Boolean)) {
  const { select, index, ...rect } = JSON.parse(line);
  (boxes[select] ??= [])[index] = rect;
}
const out = tables.map(({ id, auto, collapse, inner }) => ({
  id,
  auto,
  collapse,
  inner,
  table: boxes[`#${id}`][0],
  cells: boxes[`#${id} tr.m > td`],
}));
writeRecording(path.join(here, "chromium.json"), `${JSON.stringify(out, null, 1)}\n`);
