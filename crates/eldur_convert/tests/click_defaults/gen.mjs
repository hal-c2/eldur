// Records Chromium for `click_defaults_e2e.rs`: presses on page A (`page.html`
// titled A) whose listeners make the host swap in page Next, and in some
// Next's make page C.
//
//   node crates/eldur_convert/tests/click_defaults/gen.mjs
//
// writes `chromium.json`, each step with what the pages logged. Regenerate
// it; never edit it by hand.
//
// The Qt host makes Next inside the listener that set A's title, and C inside
// the one that set Next's. Chromium's own navigation is a later task, which
// would leave the old page running the rest of the press, so the host here is
// a page of frames instead: C at the bottom, Next over it, A on top. A
// mutation observer on each page's title takes its frame away at the first
// microtask checkpoint after that listener, inside the same press.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../..");
const { loadPlaywright } = await import(path.join(root, "tools/visual/playwright.mjs"));
const { writeRecording, drainConsole, isMark, catchUp } = await import(path.join(root, "tools/visual/record.mjs"));

const html = fs.readFileSync(path.join(here, "page.html"), "utf8");
const HOST = `<!doctype html><html><head><meta charset="utf-8"><style>
body { margin: 0 } iframe { position: absolute; left: 0; top: 0; width: 800px; height: 600px; border: 0 }
</style></head><body>
<iframe id="c" src="c.html"></iframe>
<iframe id="next" src="next.html"></iframe>
<iframe id="a" src="a.html"></iframe>
<script>
// A page that names another in its title goes, and the one under it is the
// document.
function goes(id, name) {
  var frame = document.getElementById(id);
  return new Promise(function(done) {
    frame.addEventListener("load", function() {
      var doc = frame.contentDocument;
      new MutationObserver(function() {
        if (doc.title !== name && frame.isConnected) { frame.remove(); done(); }
      }).observe(doc.head, { subtree: true, childList: true, characterData: true });
    });
  });
}
window.swapped = goes("a", "A");
goes("next", "Next");
</script></body></html>`;

const STEPS = [
  { name: "label in a link", click: "#lab" },
  { name: "double click whose second press swaps", dbl: "#lab2" },
  { name: "double click whose first press swaps", dbl: "#lab3" },
  { name: "double click whose second press swaps from a promise", dbl: "#lab4" },
  { name: "double click whose second press comes before the first click ran", dbl: "#lab5" },
  { name: "double click whose second press a move hands on to a third page", onward: "#lab7" },
  { name: "double click whose second press is lost to a third page", dbl: "#lab6" },
  { name: "press whose mousedown swaps", click: "#dn" },
  { name: "press whose move swaps", move: "#mv", click: "#mv" },
  { name: "move whose pointerover swaps", hover: "#ov" },
  { name: "double click whose first press swaps, on a button's text", dbl: "#in9" },
  { name: "press whose lostpointercapture swaps", click: "#cap" },
  { name: "press in a link whose lostpointercapture swaps", click: "#cap2" },
  { name: "press in a link whose mouseup swaps", click: "#up2" },
  // No "press whose light-dismissed popover swaps" (#pp): Chromium hangs when
  // a frame goes inside a light dismiss, so click_defaults_e2e.rs gives it.
  { name: "capture passed on whose lostpointercapture swaps", drag: "#xc" },
  { name: "label whose field's focus swaps", click: "#lab8" },
  { name: "double click whose second press comes before the first click ran, on a label for a div", dbl: "#lab10" },
  { name: "press in a paragraph of links whose mousedown swaps", click: "#tp" },
  { name: "press in a link whose mousedown swaps", click: "#dn2" },
];

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const out = [];
for (const step of STEPS) {
  const tab = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 1 });
  const log = [];
  tab.on("console", (m) => {
    if (!isMark(m.text())) log.push(m.text());
  });
  tab.on("pageerror", (e) => {
    throw new Error(`page threw: ${e.message}`);
  });
  await tab.route("http://eldur.test/**", (route) => {
    const file = new URL(route.request().url()).pathname.slice(1);
    const body = file === "host.html" ? HOST : html.replace("NAME", { "a.html": "A", "next.html": "Next", "c.html": "C" }[file]);
    return route.fulfill({ contentType: "text/html", body });
  });
  await tab.goto("http://eldur.test/host.html", { waitUntil: "load" });
  const a = tab.frame({ url: /a\.html$/ });
  const r = await a.evaluate((s) => {
    const b = document.querySelector(s).getBoundingClientRect();
    return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
  }, step.click || step.dbl || step.hover || step.onward || step.drag);
  if (step.onward) {
    // Chromium has no move inside a press: a move of its own between the
    // presses, a pixel on, which still counts the second press as 2.
    await tab.mouse.click(r.x, r.y);
    await tab.evaluate(() => window.swapped);
    await tab.mouse.move(r.x + 1, r.y);
    await tab.mouse.down({ clickCount: 2 });
    await tab.mouse.up({ clickCount: 2 });
  }
  if (step.move) {
    // A move of its own: its listener makes Next, and the press after it is Next's.
    await tab.mouse.move(r.x, r.y);
    await tab.evaluate(() => window.swapped);
  }
  if (step.hover) {
    // Onto the element, a little further on it, then off it to nothing.
    await tab.mouse.move(r.x, r.y);
    await tab.evaluate(() => window.swapped);
    await tab.mouse.move(r.x + 5, r.y);
    await tab.mouse.move(400, r.y);
  } else if (step.drag) {
    // A move inside the press, a pixel on, between its press and release.
    await tab.mouse.move(r.x, r.y);
    await tab.mouse.down();
    await tab.mouse.move(r.x + 1, r.y);
    await tab.mouse.up();
  } else if (step.click) await tab.mouse.click(r.x, r.y);
  else if (step.dbl) await tab.mouse.dblclick(r.x, r.y);
  await tab.waitForTimeout(200);
  await catchUp(tab);
  await drainConsole(tab);
  const swapped = await tab.evaluate(() => !document.getElementById("a"));
  if (!swapped) throw new Error(`${step.name}: A was never swapped out`);
  out.push({ name: step.name, log });
  await tab.close();
}
await browser.close();
writeRecording(path.join(here, "chromium.json"), JSON.stringify(out, null, 1) + "\n");
