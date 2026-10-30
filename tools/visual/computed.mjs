#!/usr/bin/env node
/**
 * Chromium oracle for computed styles: what a browser's getComputedStyle()
 * reports, so tests assert browser behaviour rather than a reading of a spec.
 *
 * Usage:
 *   node tools/visual/computed.mjs cases.json      (or JSON on stdin)
 *
 * Input is a JSON array of cases:
 *   [{ "name": "stray-brace",
 *      "html": "<style>p { color: red } } p { color: blue }</style><p id=t>x</p>",
 *      "select": "#t",
 *      "props": ["color", "margin-top"] }]
 *
 * `html` is loaded as the page (prefix `<!DOCTYPE html>` for standards mode,
 * which is added when absent). Output is one JSON object per line:
 *   {"name":"stray-brace","color":"rgb(255, 0, 0)","margin-top":"16px"}
 * A selector that matches nothing reports {"name":..., "error": "no match"}.
 */

import fs from "node:fs";

import { loadPlaywright } from "./playwright.mjs";

const input = process.argv[2] ? fs.readFileSync(process.argv[2], "utf8") : fs.readFileSync(0, "utf8");
const cases = JSON.parse(input);

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  for (const c of cases) {
    const html = /^\s*<!doctype/i.test(c.html) ? c.html : `<!DOCTYPE html>${c.html}`;
    await page.setContent(html);
    const values = await page.evaluate(({ select, props }) => {
      const el = document.querySelector(select);
      if (!el) return { error: "no match" };
      const s = getComputedStyle(el);
      return Object.fromEntries(props.map((p) => [p, s.getPropertyValue(p)]));
    }, { select: c.select, props: c.props });
    process.stdout.write(`${JSON.stringify({ name: c.name, ...values })}\n`);
  }
} finally {
  await browser.close();
}
