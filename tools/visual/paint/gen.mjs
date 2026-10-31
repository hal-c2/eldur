// Records how Chromium paints the page `test_visual.py` (PaintSnapTests)
// compares a conversion against: boxes at fractional positions, boxes around
// fractional content, and text lines at fractions of a pixel. Qt's own
// baselines come from qml6, so that the shifts are measurements on both sides
// (docs/spec/layout.md §1.2).
//
//   node tools/visual/paint/gen.mjs
//
// writes `rich.txt` (the same three numbers for RichText lines), `page.html`,
// `chromium.json` (for each probe its box and the colours
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
  round: "height:20px;background:#f00;border:1px solid #000;box-sizing:border-box;border-radius:3px",
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

// Inline runs the converter writes as RichText, at the same fractions: the
// run's font, background or padding is what makes a line RichText, and the
// line's baseline depends on the tallest run.
const CODE = "font-family:monospace;background:#eee";
const RICH = {
  r1: { css: "font:16px/1.6 sans-serif", html: `Hx<span style="${CODE};font-size:14px;padding:2px 5px">H</span>`, qt: ["sans-serif", 16, 25.6, "monospace", 14] },
  r2: { css: "font:14px/20px sans-serif", html: `Hx<span style="${CODE};font-size:12px">H</span>`, qt: ["sans-serif", 14, 20, "monospace", 12] },
  r3: { css: "font:16px/1.5 sans-serif", html: `Hx<span style="${CODE};font-size:16px">H</span>`, qt: ["sans-serif", 16, 24, "monospace", 16] },
  r4: { css: "font:16px/1.5 sans-serif", html: `Hx<span style="${CODE};font-size:24px">H</span>`, qt: ["sans-serif", 16, 24, "monospace", 24] },
  r5: { css: "font:15px/1.5 sans-serif", html: `Hx<span style="background:#eee;font-family:serif">H</span>`, qt: ["sans-serif", 15, 22.5, "serif", 15] },
  r6: { css: "font:16px/1.5 serif", html: `Hx<span style="${CODE};font-size:14px">H</span>`, qt: ["serif", 16, 24, "monospace", 14] },
  r7: { css: "font:16px/1.6 sans-serif", html: `Hx<span style="background:#eee">H</span>`, qt: ["sans-serif", 16, 25.6, "sans-serif", 16] },
  r8: { css: "font:14px/1.5 sans-serif", html: `Hx<span style="background:#eee;font-size:24px">H</span>`, qt: ["sans-serif", 14, 21, "sans-serif", 24] },
  r9: { css: "font:16px/1.6 sans-serif", html: `Hx<br>H<span style="${CODE};font-size:14px">H</span>`, qt: ["sans-serif", 16, 25.6, "monospace", 14] },
};
const RICH_FRACTIONS = [0, 0.25, 0.5, 0.75];
// A plain line of the monospace font, which RICH's runs mix in.
const MONO = { m1: { css: "font:14px/1.5 monospace", html: "Hx" }, m2: { css: "font:16px/1.6 monospace", html: "Hx" } };

// Table cells around text: `vertical-align`, a row a neighbour makes taller,
// and insets that differ, which a collapsed border or padding gives.
const CELLS = {
  c1: { table: "border-collapse:collapse", cell: "padding:4px 8px;border:1px solid #ccc;font:14px/20px sans-serif", other: "" },
  c2: { table: "border-collapse:collapse", cell: "padding:4px 8px;border:1px solid #ccc;font:14px/20px sans-serif", other: "height:60px" },
  c3: { table: "border-collapse:collapse", cell: "padding:4px 8px;border:1px solid #ccc;font:14px/20px sans-serif;vertical-align:top", other: "height:60px" },
  c4: { table: "border-collapse:collapse", cell: "padding:4px 8px;border:1px solid #ccc;font:14px/20px sans-serif;vertical-align:bottom", other: "height:60px" },
  c5: { table: "border-collapse:collapse", cell: "padding:4px 8px;border:3px solid #ccc;font:14px/20px sans-serif", other: "height:61px" },
  c6: { table: "border-collapse:collapse", cell: "padding:4px 8px;border:1px solid #ccc;font:15px/1.5 sans-serif", other: "height:50px" },
  c7: { table: "border-collapse:separate;border-spacing:0", cell: "padding:5px 8px 4px;font:14px/20px sans-serif", other: "height:41px" },
  c8: { table: "border-collapse:separate;border-spacing:0", cell: "padding:6px 8px 3px;font:15px/1.5 sans-serif", other: "height:40px" },
  c9: { table: "border-collapse:separate;border-spacing:0", cell: "padding:10px 8px;border-bottom:1px solid #ccc;font:14px/1.6 sans-serif", other: "" },
};
const CELL_FRACTIONS = [0, 0.5];

function page() {
  const css = [
    "html,body{margin:0;background:#fff}",
    "div,p{margin:0}",
    ".stage{position:relative;width:1280px;height:800px;overflow:hidden}",
    ".abs{position:absolute;width:20px}",
    ".line{position:absolute;width:44px;color:#000;white-space:nowrap}",
    ".flow{position:absolute;left:20px;top:370px;width:330px}",
    ".card{background:#eee;border:1px solid #000;padding:10px;margin-bottom:9.3px}",
    ".card p{font:16px/27.2px sans-serif;color:#000}",
    ...Object.entries(BOXES).map(([k, v]) => `.${k}{${v}}`),
    ...Object.entries(FONTS).map(([k, v]) => `.${k}{${v.css}}`),
    ...Object.entries({ ...RICH, ...MONO }).map(([k, v]) => `.${k}{${v.css}}`),
    ".cells{position:absolute}",
    ".cells td{width:44px;color:#000;white-space:nowrap}",
    ".cells td+td{width:12px;padding:0;font:0/0 sans-serif}",
    ...Object.entries(CELLS).flatMap(([k, v]) => [`.t${k}{${v.table}}`, `.cells .${k}{${v.cell}}`, `.cells .${k}+td{border:${v.cell.match(/border:([^;]+)/)?.[1] ?? "0"}}`]),
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
  Object.entries({ ...MONO, ...RICH }).forEach(([font, rich], r) => {
    RICH_FRACTIONS.forEach((f, i) => {
      body.push(
        `<p class="line ${font}" data-line style="left:${1000 + i * 70}px;top:${40 + r * 50 + f}px">${rich.html}</p>`,
      );
    });
  });
  Object.entries(CELLS).forEach(([name, cell], i) => {
    CELL_FRACTIONS.forEach((f, r) => {
      body.push(
        `<table class="cells t${name}" style="left:${400 + i * 88}px;top:${600 + r * 95 + f}px">` +
          `<tr><td class="cell ${name}" data-line>Hx</td><td style="${cell.other}"></td></tr></table>`,
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

// The row Qt paints a RichText line's baseline on, as the converter writes
// such a line without a shift: from the pixels, since QML reports no baseline
// for a RichText line.
function qtRichProbe() {
  const items = Object.entries(RICH).map(([, r], i) => {
    const [family, size, line, runFamily, runSize] = r.qt;
    const run = `<span style=\\"font-family:'${runFamily}';font-size:${runSize}px;background-color:#eee\\">H</span>`;
    return (
      `Text { x: 10; y: ${10 + i * 70}; width: 60; textFormat: Text.RichText; color: "#000000"; ` +
      `font.family: "${family}"; font.pixelSize: ${size}; ` +
      `text: "<div style=\\"line-height:${line}px\\">H${run}</div>" }`
    );
  });
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "eldur-paint-"));
  fs.writeFileSync(path.join(dir, "Main.qml"), `import QtQuick\nItem {\n${items.join("\n")}\n}\n`);
  const png = path.join(dir, "rich.png");
  execFileSync("python3", [path.join(here, "..", "render_qml.py"), dir, png], { stdio: "ignore" });
  const shot = fs.readFileSync(png).toString("base64");
  fs.rmSync(dir, { recursive: true });
  return shot;
}

// Runs in the page: the last inked row, plus one, of each probed Text.
function qtRichBaselines({ png, count }) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onerror = reject;
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = image.width;
      canvas.height = image.height;
      const g = canvas.getContext("2d", { willReadFrequently: true });
      g.drawImage(image, 0, 0);
      const out = [];
      for (let i = 0; i < count; ++i) {
        const y0 = 10 + i * 70;
        let last = -1;
        for (let y = y0; y < y0 + 66; ++y) {
          for (let x = 10; x < 70; ++x) {
            if (g.getImageData(x, y, 1, 1).data[1] < 128) {
              last = y - y0;
              break;
            }
          }
        }
        out.push(last + 1);
      }
      resolve(out);
    };
    image.src = png;
  });
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
// A line box's baseline, on the page itself: its first line, whatever runs it holds.
const richBaselines = await tab.evaluate((fonts) => {
  const out = {};
  for (const font of fonts) {
    const line = document.querySelector(`.${font}`);
    const mark = document.createElement("span");
    mark.style.cssText = "display:inline-block;width:0;height:0";
    line.prepend(mark);
    out[font] = mark.getBoundingClientRect().bottom - line.getBoundingClientRect().top;
    mark.remove();
  }
  return out;
}, Object.keys(RICH));
const richQt = await tab.evaluate(qtRichBaselines, {
  png: `data:image/png;base64,${qtRichProbe()}`,
  count: Object.keys(RICH).length,
});
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

const rich = ["# font chromium-baseline qt-painted-baseline shift (gen.mjs; never edit by hand)"];
Object.keys(RICH).forEach((k, i) => {
  rich.push(`${k} ${richBaselines[k]} ${richQt[i]} ${richBaselines[k] - richQt[i]}`);
});
fs.writeFileSync(path.join(here, "rich.txt"), `${rich.join("\n")}\n`);
