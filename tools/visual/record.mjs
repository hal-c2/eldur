/**
 * What the `gen.mjs` recorders share: a recording is written whole or not at
 * all, and what a page said is read once all of it has arrived.
 */

import fs from "node:fs";

/**
 * Writes `text` to `file` by renaming a finished copy over it: a recorder that
 * dies halfway leaves the last recording, never part of a new one.
 */
export function writeRecording(file, text) {
  const partial = `${file}.partial-${process.pid}`;
  fs.writeFileSync(partial, text);
  fs.renameSync(partial, file);
}

let marks = 0;
const MARK = "eldur-recorder-mark ";

/** Whether a console line is one `drainConsole` logged, which no page said. */
export function isMark(text) {
  return text.startsWith(MARK);
}

/**
 * Resolves once every console line the page logged before the call has reached
 * the recorder. Console events come over the wire after the call that caused
 * them returns, in order, so a line logged now arrives after all of them.
 */
export async function drainConsole(page, timeout = 30000) {
  const mark = `${MARK}${process.pid} ${++marks}`;
  const seen = page.waitForEvent("console", { predicate: (m) => m.text() === mark, timeout });
  await page.evaluate((m) => console.log(m), mark);
  await seen;
}

/**
 * Resolves once the page logs `line`, which a page logging from timers writes
 * last. A page that never says it has finished fails the recording rather than
 * giving it what it said so far.
 */
export async function waitForLine(page, line, timeout = 30000) {
  await page.waitForEvent("console", { predicate: (m) => m.text() === line, timeout });
}

/** Fails the recording when any of `names` has nothing recorded in `out`. */
export function requireAll(names, out, what) {
  const missing = names.filter((n) => out[n] === undefined);
  if (missing.length > 0) throw new Error(`${what}: nothing recorded for ${missing.join(", ")}`);
}
