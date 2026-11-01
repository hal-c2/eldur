// Records what Chromium does with fonts, for `fonts_e2e.rs` and for the table
// in `eldur_fonts.js`: the face each CSS family stack reaches
// (`CSS.getPlatformFontsForNode`), each installed face's metrics, and the line
// and baseline of text across families, sizes and weights.
//
//   node crates/eldur_convert/tests/fonts/gen.mjs
//
// writes `page.html` (the sweep) and `chromium.json`. Regenerate them; never
// edit them by hand. The faces are this machine's, so a face it does not have
// is recorded as what Chromium fell back to.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { loadPlaywright } from "../../../../tools/visual/playwright.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));

const FAMILIES = [
  "Liberation Sans", "Liberation Serif", "Liberation Mono",
  "Nimbus Sans", "Nimbus Roman", "Nimbus Mono PS",
  "Noto Sans", "Noto Serif", "Noto Sans Mono",
  "Helvetica", "Arial", "Times New Roman", "Courier New",
  "sans-serif", "serif", "monospace", "system-ui",
];
const SIZES = [8, 10, 11, 12, 13, 13.333, 14, 14.4, 15, 16, 17.6, 18, 20, 21.6, 24, 28.8, 32, 36, 48, 64, 96];
const WEIGHTS = [400, 700];
const STACKS = [
  "", "Helvetica", "Arial", "-apple-system", "BlinkMacSystemFont", "system-ui", "sans-serif",
  "Times", "Times New Roman", "serif", "Courier", "Courier New", "monospace",
  "cursive", "fantasy", "ui-sans-serif", "ui-serif", "ui-monospace", "Georgia", "Foo",
  "Foo, Arial, sans-serif", "Foo, monospace", "Foo, serif", "Foo, Foo2", "Foo, Helvetica",
  "Georgia, Foo, monospace", "'Helvetica Neue', Helvetica, Arial, sans-serif",
  "-apple-system, monospace", "Consolas, 'Courier New', monospace",
  "SFMono-Regular, Menlo, Consolas, monospace", "'Nimbus Sans', Arial", "Foo, 'Nimbus Sans'",
  "Verdana, Geneva, sans-serif", "Inter, system-ui, sans-serif",
  "'Liberation Sans'", "'Liberation Serif'", "'Liberation Mono'", "'Noto Sans'",
];
const FACES = [
  "Liberation Sans", "Liberation Serif", "Liberation Mono",
  "Nimbus Sans", "Nimbus Roman", "Nimbus Mono PS", "Nimbus Sans Narrow",
  "Noto Sans", "Noto Serif", "Noto Sans Mono", "JetBrainsMono Nerd Font",
];

const quote = (f) => (/^(sans-serif|serif|monospace|system-ui)$/.test(f) ? f : `'${f}'`);

const sweep = [];
for (const family of FAMILIES) {
  for (const size of SIZES) {
    for (const weight of WEIGHTS) sweep.push({ family, size, weight, italic: false });
  }
  sweep.push({ family, size: 16, weight: 400, italic: true });
}
// Each row is alone in its block; the fixed line-heights give the half-leading.
// Rows are grouped by family: one column of 800 rows would be a binding chain
// of 800 for the rounding each hands the next (layout.md §1.1). The page the
// converter gets has plain text, as most text is; Chromium's copy has a
// zero-size inline-block on the baseline to read it from.
const PROBE = '<b style="display:inline-block;width:0;height:0"></b>';
const fixed = [];
for (const family of ["Liberation Sans", "Liberation Serif", "Nimbus Sans"]) {
  for (const [size, line] of [[14, 21], [14.4, 21.6], [16, 24], [16, 19.2], [24, 28.8], [32, 30], [96, 96], [16, 16]]) {
    fixed.push({ family, size, weight: 400, italic: false, line, id: `f${fixed.length}` });
  }
}
sweep.forEach((c, i) => {
  c.id = `n${i}`;
  c.css = `font:${c.italic ? "italic " : ""}${c.weight} ${c.size}px ${quote(c.family)}`;
});
for (const c of fixed) c.css = `font:400 ${c.size}px/${c.line}px ${quote(c.family)}`;
// Text a flex container places: a line centred or at the end of a taller box,
// and lines of different fonts on one baseline. A probe in these would be a
// flex item of its own, so the baseline is the text's box plus the font's ascent.
const PLACED = [
  (t) => `<div style="display:flex;align-items:center;justify-content:center;height:40px;font:600 15.2px/24.32px sans-serif">${t("Py0")}</div>`,
  (t) => `<div style="display:flex;align-items:center;height:52px;font:600 16px/25.6px sans-serif">${t("Py1")}</div>`,
  (t) => `<div style="display:flex;align-items:center;height:52px;font:16px sans-serif">${t("Py2")}</div>`,
  (t) => `<div style="display:flex;align-items:flex-end;height:50px;font:16px/24px sans-serif">${t("Py3")}</div>`,
  (t) => `<div style="display:flex;align-items:baseline;justify-content:space-between;font:15.2px/24.32px sans-serif"><span>${t("Py4")}</span><strong style="font-size:24px;line-height:38.4px">${t("Py5")}</strong></div>`,
  (t) => `<div style="display:flex;align-items:baseline;gap:8px;font:13px sans-serif"><span>${t("Py6")}</span><span style="font-size:24px">${t("Py7")}</span></div>`,
];
const document_ = (probe) => {
  const sections = new Map();
  for (const c of [...sweep, ...fixed]) {
    const row = `<div id="${c.id}" style="${c.css};white-space:nowrap">Hx${probe}</div>`;
    sections.set(c.family, [...(sections.get(c.family) ?? []), row]);
  }
  const body = [...sections.values()].map((rows) => `<section>\n${rows.join("\n")}\n</section>`);
  const placed = PLACED.map((row) => row((t) => t));
  body.push(`<section>\n${placed.join("\n")}\n</section>`);
  return `<!DOCTYPE html>\n<html><body style="margin:0">\n${body.join("\n")}\n</body></html>\n`;
};
fs.writeFileSync(path.join(here, "page.html"), document_(""));
const html = document_(PROBE);

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.setContent(html);
const measure = (id) => page.evaluate((id) => {
  const d = document.getElementById(id);
  const top = d.getBoundingClientRect().top;
  return { top, height: d.getBoundingClientRect().height, baseline: d.querySelector("b").getBoundingClientRect().bottom - top };
}, id);
const lines = [];
// In document order, which is how the test walks the page.
const order = [...sweep, ...fixed];
const families = [...new Set(order.map((c) => c.family))];
for (const c of families.flatMap((f) => order.filter((c) => c.family === f))) {
  lines.push({ id: c.id, family: c.family, size: c.size, weight: c.weight, italic: c.italic, line: c.line ?? null, ...(await measure(c.id)) });
}

const placed = await page.evaluate(() => {
  const walker = document.createTreeWalker(document.querySelector("section:last-of-type"), NodeFilter.SHOW_TEXT);
  const context = document.createElement("canvas").getContext("2d");
  const out = [];
  while (walker.nextNode()) {
    const node = walker.currentNode;
    if (!node.textContent.startsWith("Py")) continue;
    const range = document.createRange();
    range.selectNodeContents(node);
    const style = getComputedStyle(node.parentElement);
    context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    out.push({ text: node.textContent, baseline: range.getBoundingClientRect().top + context.measureText("x").fontBoundingBoxAscent });
  }
  return out;
});

const cdp = await page.context().newCDPSession(page);
await cdp.send("DOM.enable");
await cdp.send("CSS.enable");
const platformFace = async () => {
  const { root } = await cdp.send("DOM.getDocument");
  const { nodeId } = await cdp.send("DOM.querySelector", { nodeId: root.nodeId, selector: "#t" });
  const { fonts } = await cdp.send("CSS.getPlatformFontsForNode", { nodeId });
  return fonts.map((f) => f.familyName);
};
const stacks = [];
for (const stack of STACKS) {
  await page.setContent(`<!DOCTYPE html><body><div id=t style="font-family:${stack.replaceAll('"', "")}">Hx</div>`);
  stacks.push({ stack, faces: await platformFace() });
}

// At 1000px rounding says nothing, so the numbers are the font's, in 1/1000 em.
const faces = [];
for (const family of FACES) {
  for (const [weight, style] of [[400, "normal"], [700, "normal"], [400, "italic"], [700, "italic"]]) {
    await page.setContent(`<!DOCTYPE html><body style=margin:0><div id=t style="font:${style} ${weight} 1000px '${family}';white-space:nowrap"><span id=s>H</span><b style="display:inline-block;width:0;height:0"></b></div>`);
    const [line, box, base] = await page.evaluate(() => {
      const d = document.getElementById("t").getBoundingClientRect();
      const b = document.querySelector("b").getBoundingClientRect();
      return [d.height, document.getElementById("s").getBoundingClientRect().height, b.bottom - d.top];
    });
    const gap = line - box;
    const ascent = base - Math.floor(gap / 2);
    faces.push({ family, weight, italic: style === "italic", ascent, descent: box - ascent, gap, reached: await platformFace() });
  }
}
await browser.close();
fs.writeFileSync(path.join(here, "chromium.json"), `${JSON.stringify({ lines, placed, stacks, faces }, null, 0).replaceAll('},{', '},\n{')}\n`);
