// Records Chromium for `click_defaults_e2e.rs`: presses on page A (`page.html`
// titled A) whose label default makes the host swap in page Next.
//
//   node crates/eldur_convert/tests/click_defaults/gen.mjs
//
// writes `chromium.json`, each step with what the two pages logged. Regenerate
// it; never edit it by hand.
//
// The Qt host makes Next inside the listener that set A's title. Chromium's
// own navigation is a later task, which would leave A running the rest of the
// press, so the host here is a page of two frames instead: Next, made first,
// lies under A, and a mutation observer on A's title takes A's frame away at
// the first microtask checkpoint after that listener, inside the same press.

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
<iframe id="next" src="next.html"></iframe>
<iframe id="a" src="a.html"></iframe>
<script>
window.swapped = new Promise(function(done) {
  var a = document.getElementById("a");
  a.addEventListener("load", function() {
    var doc = a.contentDocument;
    new MutationObserver(function() {
      if (doc.title === "Next" && a.isConnected) { a.remove(); done(); }
    }).observe(doc.head, { subtree: true, childList: true, characterData: true });
  });
});
</script></body></html>`;

const STEPS = [
  { name: "label in a link", click: "#lab" },
  { name: "double click whose second press swaps", dbl: "#lab2" },
  { name: "double click whose first press swaps", dbl: "#lab3" },
  { name: "double click whose second press swaps from a promise", dbl: "#lab4" },
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
    const body = file === "host.html" ? HOST : html.replace("NAME", file === "a.html" ? "A" : "Next");
    return route.fulfill({ contentType: "text/html", body });
  });
  await tab.goto("http://eldur.test/host.html", { waitUntil: "load" });
  const a = tab.frame({ url: /a\.html$/ });
  const r = await a.evaluate((s) => {
    const b = document.querySelector(s).getBoundingClientRect();
    return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
  }, step.click || step.dbl);
  if (step.click) await tab.mouse.click(r.x, r.y);
  else await tab.mouse.dblclick(r.x, r.y);
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
