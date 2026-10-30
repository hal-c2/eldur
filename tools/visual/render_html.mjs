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
 * Playwright resolution:
 *   When Playwright is installed globally via mise rather than in a local
 *   package.json, bare `import("playwright")` fails in ESM. We try:
 *     1. Standard `import("playwright")` (local node_modules / NODE_PATH)
 *     2. `PLAYWRIGHT_MODULE_PATH` environment variable (if set)
 *     3. `<npm root -g>/playwright` via `createRequire`
 *     4. `~/.local/share/mise/installs/npm-playwright/latest/node_modules/playwright`
 *     5. Sibling `../playwright` relative to any `playwright` executable on `PATH`
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);

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

async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch {
    // Fall back to global npm / mise installation paths.
  }

  const candidates = [];

  if (process.env.PLAYWRIGHT_MODULE_PATH) {
    candidates.push(process.env.PLAYWRIGHT_MODULE_PATH);
  }

  try {
    const npmRoot = execFileSync("npm", ["root", "-g"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
    if (npmRoot) {
      candidates.push(path.join(npmRoot, "playwright"));
    }
  } catch {
    // npm may not be on PATH in minimal environments; continue checking mise paths.
  }

  candidates.push(
    path.join(
      os.homedir(),
      ".local/share/mise/installs/npm-playwright/latest/node_modules/playwright"
    )
  );

  for (const dir of (process.env.PATH || "").split(path.delimiter)) {
    if (!dir) continue;
    const binCandidate = path.join(dir, "playwright");
    if (fs.existsSync(binCandidate)) {
      try {
        const realBin = fs.realpathSync(binCandidate);
        candidates.push(path.resolve(path.dirname(realBin), "..", "playwright"));
      } catch {
        // Ignore unreadable or broken symlink entries.
      }
    }
  }

  const tried = [];
  for (const candidate of candidates) {
    if (tried.includes(candidate)) continue;
    tried.push(candidate);
    if (fs.existsSync(candidate)) {
      return require(candidate);
    }
  }

  throw new Error(
    `Cannot resolve 'playwright' module. Checked standard import and: ${tried.join(", ")}`
  );
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
