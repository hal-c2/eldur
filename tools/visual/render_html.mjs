#!/usr/bin/env node
/**
 * Render a local HTML file to a viewport PNG screenshot using Playwright Chromium.
 *
 * Usage:
 *   node tools/visual/render_html.mjs <page.html> <out.png> [--width 1280] [--height 800]
 *
 * Behaviour:
 *   - Launches headless Chromium with viewport WxH and deviceScaleFactor = 1.
 *   - Navigates to the file:// URL of <page.html> and waits for the `load` event.
 *   - Injects CSS disabling animations, transitions, and text carets.
 *   - Waits for `document.fonts.ready` followed by 2 `requestAnimationFrame` ticks.
 *   - Captures a viewport-only PNG screenshot (not fullPage).
 *
 * Playwright is resolved by `playwright.mjs`.
 */

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { loadPlaywright } from "./playwright.mjs";

const DISABLE_MOTION_CSS =
  "*{animation:none!important;transition:none!important;caret-color:transparent!important}" +
  "*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}";

const USAGE = `Usage: render_html.mjs <page.html> <out.png> [--width 1280] [--height 800]

Options:
  --width <px>   Viewport width in CSS pixels (default: 1280)
  --height <px>  Viewport height in CSS pixels (default: 800)
  -h, --help     Show this help message`;

function fail(message, exitCode = 1) {
  process.stderr.write(`render_html.mjs: error: ${message}\n`);
  process.exit(exitCode);
}

function parsePositiveInt(raw, flagName) {
  if (!/^\d+$/.test(String(raw))) {
    fail(`${flagName} expects a positive integer, got '${raw}'`, 2);
  }
  const value = Number.parseInt(String(raw), 10);
  if (!Number.isSafeInteger(value) || value <= 0) {
    fail(`${flagName} must be > 0, got '${raw}'`, 2);
  }
  return value;
}

function parseArgs(argv) {
  let width = 1280;
  let height = 800;
  const positional = [];

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "-h" || arg === "--help") {
      process.stdout.write(`${USAGE}\n`);
      process.exit(0);
    } else if (arg === "--width") {
      if (i + 1 >= argv.length) {
        fail("--width requires a value", 2);
      }
      i += 1;
      width = parsePositiveInt(argv[i], "--width");
    } else if (arg.startsWith("--width=")) {
      width = parsePositiveInt(arg.slice("--width=".length), "--width");
    } else if (arg === "--height") {
      if (i + 1 >= argv.length) {
        fail("--height requires a value", 2);
      }
      i += 1;
      height = parsePositiveInt(argv[i], "--height");
    } else if (arg.startsWith("--height=")) {
      height = parsePositiveInt(arg.slice("--height=".length), "--height");
    } else if (arg.startsWith("-")) {
      fail(`unknown option '${arg}'\n\n${USAGE}`, 2);
    } else {
      positional.push(arg);
    }
  }

  if (positional.length !== 2) {
    fail(`expected <page.html> and <out.png>, got ${positional.length} positional argument(s)\n\n${USAGE}`, 2);
  }

  return {
    htmlPath: positional[0],
    outPath: positional[1],
    width,
    height,
  };
}

async function main() {
  const { htmlPath, outPath, width, height } = parseArgs(process.argv.slice(2));

  const htmlAbs = path.resolve(htmlPath);
  if (!fs.existsSync(htmlAbs)) {
    fail(`HTML file does not exist: ${htmlAbs}`);
  }
  const stat = fs.statSync(htmlAbs);
  if (!stat.isFile()) {
    fail(`HTML path is not a regular file: ${htmlAbs}`);
  }

  const outAbs = path.resolve(outPath);
  fs.mkdirSync(path.dirname(outAbs), { recursive: true });

  const playwright = await loadPlaywright();
  const browser = await playwright.chromium.launch({ headless: true });

  try {
    const context = await browser.newContext({
      viewport: { width, height },
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();

    // Disable animations/transitions as early as possible on document creation,
    // and reinforce via addStyleTag once load finishes so late stylesheets cannot override it.
    await page.addInitScript((cssText) => {
      const inject = () => {
        const root = document.head || document.documentElement;
        if (!root) return;
        const style = document.createElement("style");
        style.setAttribute("data-h2q-visual", "disable-motion");
        style.textContent = cssText;
        root.appendChild(style);
      };
      if (document.documentElement) {
        inject();
      } else {
        document.addEventListener("DOMContentLoaded", inject, { once: true });
      }
    }, DISABLE_MOTION_CSS);

    const fileUrl = pathToFileURL(htmlAbs).href;
    await page.goto(fileUrl, { waitUntil: "load", timeout: 15000 });
    await page.addStyleTag({ content: DISABLE_MOTION_CSS });

    await page.evaluate(async () => {
      if (document.fonts && document.fonts.ready) {
        await document.fonts.ready;
      }
      await new Promise((resolve) => {
        requestAnimationFrame(() => {
          requestAnimationFrame(resolve);
        });
      });
    });

    await page.screenshot({
      path: outAbs,
      fullPage: false,
      type: "png",
    });
  } finally {
    await browser.close();
  }
}

main().catch((err) => {
  fail(err && err.stack ? err.stack : String(err));
});
