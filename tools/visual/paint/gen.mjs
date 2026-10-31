// Records how Chromium paints the page `test_visual.py` (PaintSnapTests)
// compares a conversion against: boxes at fractional positions, boxes around
// fractional content, and text lines at fractions of a pixel. Qt's own
// baselines come from qml6, so that the shifts are measurements on both sides
// (docs/spec/layout.md §1.2).
//
//   node tools/visual/paint/gen.mjs
//
// writes `page.html`, `chromium.json` (for each probe its box and the colours
// along a line through it; for each text line its inked rows) and
// `shifts.txt` (for each font: Chromium's baseline below the line top, Qt's,
// and the shift between them, which eldur_convert's unit test reads).
// Regenerate them; never edit them by hand.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { loadPlaywright } from "../playwright.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));

const FRACTIONS = [0, 0.125, 0.25, 0.375, 0.49, 0.5, 0.51, 0.625, 0.75, 0.875];

// Painted boxes. Their heights and borders are what §1.2 is about.
const BOXES = {
  fill: "height:20px;background:#00f",
  tall: "height:20.5px;background:#00f",
  short: "height:20.25px;background:#00f",
  border: "height:20px;background:#f00;border:1px solid #000;box-sizing:border-box",
  bordertall: "height:20.5px;background:#f00;border:1px solid #000;box-sizing:border-box",
  thick: "height:20px;background:#f00;border:2px solid #000;box-sizing:border-box",
  bottom: "height:20.25px;background:#f00;border-bottom:1px solid #000;box-sizing:border-box",
};

// `.a` … `.f` are the fonts of the unit test that reads `shifts.txt`.
const FONTS = {
  a: { css: "font:15px/1.5 sans-serif", family: "sans-serif", size: 15, weight: 400, line: 22.5 },
  b: { css: "font:17px/1.5 sans-serif", family: "sans-serif", size: 17, weight: 400, line: 25.5 },
  c: { css: "font:bold 28px/1.5 sans-serif", family: "sans-serif", size: 28, weight: 700, line: 42 },
  d: { css: "font:16px/1.5 serif", family: "serif", size: 16, weight: 400, line: 24 },
  e: { css: "font:16px/1.1 sans-serif", family: "sans-serif", size: 16, weight: 400, line: 17.6 },
  f: { css: "font:13px/1 sans-serif", family: "sans-serif", size: 13, weight: 400, line: 13 },
};

function page() {
  const css = [
    "html,body{margin:0;background:#fff}",
    "div,p{margin:0}",
    ".stage{position:relative;width:1280px;height:800px;overflow:hidden}",
    ".abs{position:absolute;width:20px}",
    ".line{position:absolute;width:44px;color:#000;white-space:nowrap}",
    ".flow{position:absolute;left:20px;top:330px;width:330px}",
    ".card{background:#eee;border:1px solid #000;padding:10px;margin-bottom:9.3px}",
    ".card p{font:16px/27.2px sans-serif;color:#000}",
    ...Object.entries(BOXES).map(([k, v]) => `.${k}{${v}}`),
    ...Object.entries(FONTS).map(([k, v]) => `.${k}{${v.css}}`),
  ];
  const body = [];
  let row = 0;
  for (const name of Object.keys(BOXES)) {
    FRACTIONS.forEach((f, i) => {
      const left = 20 + i * 30;
      const top = 10 + row * 45 + f;
      body.push(
        `<div class="abs ${name}" data-box style="left:${left}px;top:${top}px"></div>`,
      );
    });
    row += 1;
  }
  // Fractional x, on the same whole y.
  FRACTIONS.forEach((f, i) => {
    body.push(
      `<div class="abs border" data-box style="left:${360 + i * 30 + f}px;top:10px"></div>`,
    );
  });
  // Boxes around content of fractional height, in block flow.
  body.push('<div class="flow">');
  body.push(
    '<div class="card" data-box><p>One</p><p>Two</p></div>',
    '<div class="card" data-box><p>Three</p></div>',
    '<div class="card" data-box><div class="card" data-box><p>Four</p></div><p>Five</p></div>',
    '<div class="card" data-box><p>Six</p></div>',
  );
  body.push("</div>");
  Object.keys(FONTS).forEach((font, r) => {
    FRACTIONS.forEach((f, i) => {
      body.push(
        `<p class="line ${font}" data-line style="left:${400 + i * 60}px;top:${100 + r * 80 + f}px">Hx</p>`,
      );
    });
  });
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>paint snapping</title>
<style>
${css.join("\n")}
</style></head>
<body><div class="stage">
${body.join("\n")}
</div></body></html>
`;
}

// Runs in the page: the colours of the screenshot, and the probes.
function read(png) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onerror = reject;
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = image.width;
      canvas.height = image.height;
      const g = canvas.getContext("2d", { willReadFrequently: true });
      g.drawImage(image, 0, 0);
      const at = (x, y) => {
        const d = g.getImageData(x, y, 1, 1).data;
        return `${d[0]},${d[1]},${d[2]}`;
      };
      const boxes = [...document.querySelectorAll("[data-box]")].map((e) => {
        const r = e.getBoundingClientRect();
        const x = Math.floor(r.x) + 5;
        const y0 = Math.floor(r.y) - 3;
        const rows = [];
        for (let y = y0; y < Math.ceil(r.bottom) + 3; ++y) rows.push(at(x, y));
        const out = { x: r.x, y: r.y, width: r.width, height: r.height, column: x, from: y0, rows };
        if (r.width < 100) {
          const ym = Math.floor(r.y) + 5;
          const x0 = Math.floor(r.x) - 3;
          const cols = [];
          for (let c = x0; c < Math.ceil(r.right) + 3; ++c) cols.push(at(c, ym));
          Object.assign(out, { row: ym, fromX: x0, cols });
        }
        return out;
      });
      const lines = [...document.querySelectorAll("[data-line]")].map((e) => {
        const r = e.getBoundingClientRect();
        const y0 = Math.floor(r.y) - 4;
        const y1 = Math.ceil(r.bottom) + 4;
        const inked = [];
        for (let y = y0; y < y1; ++y) {
          for (let x = Math.floor(r.x); x < r.right + 6; ++x) {
            const d = g.getImageData(x, y, 1, 1).data;
            if (d[1] < 128) {
              inked.push(y);
              break;
            }
          }
        }
        return {
          font: e.classList[1],
          x: r.x,
          y: r.y,
          height: r.height,
          ink: inked.length ? [inked[0], inked[inked.length - 1]] : null,
        };
      });
      resolve({ boxes, lines });
    };
    image.src = png;
  });
}

// Runs in the page: each font's baseline below its line's top.
function baselines(fonts) {
  const out = {};
  for (const font of fonts) {
    const p = document.createElement("p");
    p.className = font;
    p.style.cssText = "position:absolute;left:0;top:0";
    p.innerHTML = 'x<span style="display:inline-block;width:0;height:0"></span>';
    document.body.append(p);
    out[font] = p.lastChild.getBoundingClientRect().bottom - p.getBoundingClientRect().top;
    p.remove();
  }
  return out;
}

// Qt's baseline in a fixed-height line of the same font and line height.
function qtBaselines() {
  const items = Object.entries(FONTS)
    .map(
      ([k, f]) =>
        `Text { objectName: "${k}"; text: "x"; font.family: "${f.family}"; font.pixelSize: ${f.size}; ` +
        `font.weight: ${f.weight}; lineHeight: ${f.line}; lineHeightMode: Text.FixedHeight }`,
    )
    .join("\n  ");
  const qml = `import QtQuick
Item {
  ${items}
  Component.onCompleted: {
    for (const t of children)
      console.warn("baseline " + t.objectName + " " + t.baselineOffset)
    Qt.quit()
  }
}
`;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "eldur-paint-"));
  const file = path.join(dir, "probe.qml");
  fs.writeFileSync(file, qml);
  const out = execFileSync("sh", ["-c", `qml6 --software "${file}" 2>&1`], {
    encoding: "utf8",
    env: {
      ...process.env,
      QT_QPA_PLATFORM: "offscreen",
      QT_QUICK_BACKEND: "software",
      QT_FORCE_STDERR_LOGGING: "1",
      QT_FONT_DPI: "96",
      QT_SCALE_FACTOR: "1",
    },
  });
  fs.rmSync(dir, { recursive: true });
  const found = {};
  for (const m of out.matchAll(/baseline (\w) ([\d.]+)/g)) found[m[1]] = Number(m[2]);
  return found;
}

const html = page();
const pagePath = path.join(here, "page.html");
fs.writeFileSync(pagePath, html);

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1,
});
const tab = await context.newPage();
await tab.goto(pathToFileURL(pagePath).href);
await tab.evaluate(() => document.fonts.ready);
const shot = (await tab.screenshot({ fullPage: false })).toString("base64");
const measured = await tab.evaluate(read, `data:image/png;base64,${shot}`);
const chromiumBaselines = await tab.evaluate(baselines, Object.keys(FONTS));
await browser.close();

fs.writeFileSync(path.join(here, "chromium.json"), `${JSON.stringify(measured, null, 0)}\n`);

const qt = qtBaselines();
const lines = ["# font chromium-baseline qt-painted-baseline shift (gen.mjs; never edit by hand)"];
for (const k of Object.keys(FONTS)) {
  // Qt reports the font's own ascent and paints on it rounded.
  const painted = Math.round(qt[k]);
  lines.push(`${k} ${chromiumBaselines[k]} ${painted} ${chromiumBaselines[k] - painted}`);
}
fs.writeFileSync(path.join(here, "shifts.txt"), `${lines.join("\n")}\n`);
