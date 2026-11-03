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
 *
 * Optional `run` is a list of steps executed in order before measuring, with
 * Playwright's real input (so :hover, :focus and transitions behave as for a
 * user):
 *   {"eval": "<js>"}  {"click": "<sel>"}  {"hover": "<sel>"}
 *   {"focus": "<sel>"}  {"type": ["<sel>", "<text>"]}  {"wait": <ms>}
 * Optional `after` is milliseconds to wait once the steps are done, to read a
 * value mid-transition or after it. The page's own timers and frames due by
 * the end of that wait have run before reading, however loaded the machine:
 *   {"name": "t", "html": "...", "select": "#t", "props": ["color"],
 *    "run": [{"hover": "#t"}], "after": 100}
 * Cases without `run`/`after` behave exactly as before.
 *
 * A selector that matches nothing reports {"name":..., "error": "no match"}.
 */

import fs from "node:fs";

import { loadPlaywright } from "./playwright.mjs";
import { catchUp, trackPending } from "./record.mjs";
import { runSteps } from "./run_steps.mjs";

const input = process.argv[2] ? fs.readFileSync(process.argv[2], "utf8") : fs.readFileSync(0, "utf8");
const cases = JSON.parse(input);

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
try {
  for (const c of cases) {
    // A page per case: the mouse position and focus of one case's `run`
    // steps must not carry into the next.
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    const html = /^\s*<!doctype/i.test(c.html) ? c.html : `<!DOCTYPE html>${c.html}`;
    await trackPending(page, { current: true });
    await page.setContent(html);
    if (c.run) await runSteps(page, c.run);
    if (c.after) {
      await page.waitForTimeout(c.after);
      await catchUp(page);
    }
    const values = await page.evaluate(({ select, props }) => {
      const el = document.querySelector(select);
      if (!el) return { error: "no match" };
      const s = getComputedStyle(el);
      return Object.fromEntries(props.map((p) => [p, s.getPropertyValue(p)]));
    }, { select: c.select, props: c.props });
    process.stdout.write(`${JSON.stringify({ name: c.name, ...values })}\n`);
    await page.close();
  }
} finally {
  await browser.close();
}
