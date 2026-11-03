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
 * Keeps what the page has asked to run later (timeouts, intervals not yet
 * cleared, animation frames), each with the page-clock time it is due.
 * Installed before the page's own scripts, so a recorder can wait for the page
 * rather than guess how long that takes on a loaded machine.
 *
 * A due time is fixed when the callback is scheduled, so "everything due by T
 * has run" means the same on an idle and on a loaded machine: only how long
 * the recorder waits for it differs.
 */
function countPending() {
  if (window.__eldurPending) return;
  const live = new Map();
  const now = () => performance.now();
  const track = (set, clear, kind) => {
    const original = window[set];
    const cancel = window[clear];
    window[set] = function (handler, ...rest) {
      const run = typeof handler === "function" ? handler : () => (0, eval)(String(handler));
      // A frame is due at once: it runs on the next one the page draws.
      const delay = kind === "frame" ? 0 : Math.max(0, Number(rest[0]) || 0);
      let key;
      const wrapped = (...args) => {
        try {
          return run.apply(this, args);
        } finally {
          if (kind === "repeat") {
            if (live.has(key)) live.get(key).due = now() + delay;
          } else live.delete(key);
        }
      };
      const id = original.call(window, wrapped, ...rest);
      key = `${set}${id}`;
      live.set(key, { due: now() + delay });
      return id;
    };
    window[clear] = function (id) {
      live.delete(`${set}${id}`);
      return cancel.call(window, id);
    };
  };
  track("setTimeout", "clearTimeout", "once");
  track("setInterval", "clearInterval", "repeat");
  track("requestAnimationFrame", "cancelAnimationFrame", "frame");
  // [everything pending, what of it is due by `until`].
  const pending = (until) => {
    let due = 0;
    for (const p of live.values()) if (p.due <= until) due++;
    return [live.size, due];
  };
  Object.defineProperty(window, "__eldurPending", { value: pending, enumerable: false, configurable: true });
}

/**
 * Makes `settle` and `catchUp` work on every document `page` loads from now on.
 * With `current`, on the one it shows now as well: `setContent` writes into
 * that document rather than loading a new one, so init scripts never see it.
 * Only then, because running a script in a fresh tab's blank document changes
 * which pointer events the page loaded into it gets.
 */
export async function trackPending(page, { current = false } = {}) {
  await page.addInitScript(countPending);
  if (current) await page.evaluate(`(${countPending})()`);
}

const pageNow = (page) => page.evaluate(() => performance.now());
const pending = (page, until) =>
  page.evaluate((t) => (window.__eldurPending ? window.__eldurPending(t) : [0, 0]), until);
const pause = () => new Promise((r) => setTimeout(r, 10));

/**
 * Resolves once every callback the page had due by now has run and what it
 * logged has arrived. For a recorder whose steps are timed (a step is what
 * happened in its window, as the Qt side waits the same window): a loaded
 * machine runs a page's timers late, and this waits for them rather than read
 * the step early. Work due later, a frame loop's next frame among it, is left
 * to the next step.
 */
export async function catchUp(page, timeout = 60000) {
  const until = await pageNow(page);
  const start = Date.now();
  for (;;) {
    const [, due] = await pending(page, until);
    if (due === 0) break;
    if (Date.now() - start > timeout) throw new Error(`page fell behind: ${due} timers or frames overdue after ${timeout}ms`);
    await pause();
  }
  await drainConsole(page, timeout);
}

/**
 * Resolves once the page has nothing left to run later and every line it
 * logged has arrived: when a page that logs from timers has said all it will.
 *
 * A page that never stops (a frame loop, an interval left running, a timer far
 * off) would hold this forever, so it waits only for what is due within `cap`
 * ms of page time from the call: everything due by then runs, however loaded
 * the machine. If work is still pending after that, the page's log depends on
 * when it is read, and the recording fails unless `running` says the caller
 * expects that; then it resolves to how many callbacks are still pending.
 */
export async function settle(page, { cap = 2000, running = false, timeout = 60000 } = {}) {
  const until = (await pageNow(page)) + cap;
  const start = Date.now();
  let quiet = 0;
  // Twice in a row: an event the page answers (a load, a transition's end)
  // can start new timers between two reads.
  while (quiet < 2) {
    const [left, due] = await pending(page, until);
    if (left === 0) {
      quiet++;
      await drainConsole(page, timeout);
      continue;
    }
    quiet = 0;
    if (due === 0 && (await pageNow(page)) >= until) {
      if (!running) throw new Error(`page never finished: ${left} timers or frames still pending ${cap}ms on`);
      await drainConsole(page, timeout);
      return left;
    }
    if (Date.now() - start > timeout) throw new Error(`page fell behind: ${due} timers or frames overdue after ${timeout}ms`);
    await pause();
  }
  return 0;
}

/**
 * Fails the recording of `name` when the page means to log lines starting
 * `prefix` and none were recorded, or when it logs from timers and its end
 * marker is missing: a page read before it finished must not become a
 * recording.
 */
export function requireLogged(name, html, log, prefix, end = `${prefix}done`) {
  const logs = ["'", '"', "`"].some((q) => html.includes(`${q}${prefix}`));
  if (!logs) return;
  if (log.length === 0) throw new Error(`${name}: the page logs "${prefix}" lines but none were recorded`);
  if (end === null) return;
  const timed = /setTimeout|setInterval|requestAnimationFrame/.test(html);
  if (timed && !log.includes(end)) throw new Error(`${name}: logs from timers but its end marker "${end}" was not recorded`);
}
