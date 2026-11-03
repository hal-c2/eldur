/**
 * What the `gen.mjs` recorders share: a recording is written whole or not at
 * all, and what a page said is read once all of it has arrived.
 */

import fs from "node:fs";
import path from "node:path";

/**
 * Writes `text` to `file` by renaming a finished copy over it: a recorder that
 * dies halfway leaves the last recording, never part of a new one.
 */
export function writeRecording(file, text) {
  // A stress run of several recorders at once keeps each one's result apart.
  if (process.env.ELDUR_RECORDING_DIR) {
    const name = `${path.basename(path.dirname(file))}-${path.basename(file)}`;
    file = path.join(process.env.ELDUR_RECORDING_DIR, `${process.pid}-${name}`);
  }
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

/**
 * Counts what the page has asked to run later (timeouts, intervals not yet
 * cleared, animation frames). Installed before the page's own scripts, so a
 * recorder can wait for the page to finish rather than guess how long that
 * takes on a loaded machine.
 */
function countPending() {
  const live = new Set();
  const track = (set, clear, repeats) => {
    const original = window[set];
    const cancel = window[clear];
    window[set] = function (handler, ...rest) {
      const run = typeof handler === "function" ? handler : () => (0, eval)(String(handler));
      let id;
      const wrapped = (...args) => {
        try {
          return run.apply(this, args);
        } finally {
          if (!repeats) live.delete(`${set}${id}`);
        }
      };
      id = original.call(window, wrapped, ...rest);
      live.add(`${set}${id}`);
      return id;
    };
    window[clear] = function (id) {
      live.delete(`${set}${id}`);
      return cancel.call(window, id);
    };
  };
  track("setTimeout", "clearTimeout", false);
  track("setInterval", "clearInterval", true);
  track("requestAnimationFrame", "cancelAnimationFrame", false);
  Object.defineProperty(window, "__eldurPending", { value: () => live.size, enumerable: false });
}

/** Makes `settle` work on every document `page` loads from now on. */
export async function trackPending(page) {
  await page.addInitScript(countPending);
}

/**
 * Resolves once the page has nothing left to run later and every line it
 * logged has arrived: when a page that logs from timers has said all it will.
 * A page that never stops (a frame loop, an interval left running) fails the
 * recording instead of being read part way.
 */
export async function settle(page, timeout = 30000) {
  const start = Date.now();
  let quiet = 0;
  // Twice in a row: an event the page answers (a load, a transition's end)
  // can start new timers between two reads.
  while (quiet < 2) {
    const left = await page.evaluate(() => (window.__eldurPending ? window.__eldurPending() : 0));
    if (left === 0) {
      quiet++;
      await drainConsole(page, timeout);
    } else {
      quiet = 0;
      if (Date.now() - start > timeout) throw new Error(`page never finished: ${left} timers or frames pending`);
      await new Promise((r) => setTimeout(r, 10));
    }
  }
}

/**
 * Fails the recording of `name` when the page means to log lines starting
 * `prefix` and none were recorded, or when it logs from timers and its end
 * marker is missing: a page read before it finished must not become a
 * recording.
 */
export function requireLogged(name, html, log, prefix, end = `${prefix}done`) {
  const logs = html.includes(`'${prefix}`) || html.includes(`"${prefix}`);
  if (!logs) return;
  if (log.length === 0) throw new Error(`${name}: the page logs "${prefix}" lines but none were recorded`);
  if (end === null) return;
  const timed = /setTimeout|setInterval|requestAnimationFrame/.test(html);
  if (timed && !log.includes(end)) throw new Error(`${name}: logs from timers but its end marker "${end}" was not recorded`);
}
