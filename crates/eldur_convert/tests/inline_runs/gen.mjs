// Records what Chromium does with a paragraph whose inline runs differ in font
// size, family, line height or vertical alignment, for `inline_runs_e2e.rs` and
// for the rule in layout.md §1.3: the box of each paragraph and of each inline
// element in it, on a page with no script, on the same page with one that reads
// them, and after a script changes a run.
//
//   node crates/eldur_convert/tests/inline_runs/gen.mjs
//
// writes `closed.html`, `open.html`, `changes.html` and `chromium.json`.
// Regenerate them; never edit them by hand.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { loadPlaywright } from "../../../../tools/visual/playwright.mjs";
import { writeRecording, waitForLine } from "../../../../tools/visual/record.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));

const LONG = "that goes on long enough to wrap around the page because the sentence just keeps going and going past the edge of the page until it wraps at last";
const WORDS = "several words in this run keep on going so that the run itself has to break across the lines of the paragraph and then carries on again a good deal further than one line of it";

// [block style, inner HTML with RUN marking the run's id]. A run is `id=@`.
const CASES = [
  // Sizes.
  ...[8, 10, 11.67, 13, 15, 18, 20, 24, 30, 48].map((s) => ["", `Text <span id=@ style="font-size:${s}px">sized run</span> after`]),
  ["font-size:20px", `Text <span id=@ style="font-size:10px">small run</span> after`],
  ["font-size:13.333px", `Text <span id=@ style="font-size:17.6px">sized run</span> after`],
  // Families.
  ...["serif", "monospace", "'Liberation Serif'", "'Liberation Mono'", "'Nimbus Sans'", "'Noto Sans'", "'Noto Serif'"].map((f) => ["", `Text <span id=@ style="font-family:${f}">family run</span> after`]),
  ["font-family:serif;font-size:16px", `Text <span id=@ style="font-family:sans-serif">family run</span> after`],
  ["font-family:'Noto Sans'", `Text <span id=@ style="font-family:'Liberation Sans'">family run</span> after`],
  ["", `Text <span id=@ style="font-family:serif;font-size:18px">serif run</span> after`],
  // Weight and slant at one size: do the faces change the line?
  ["", `Text <b id=@>bold run</b> after`],
  ["", `Text <i id=@>italic run</i> after`],
  ["font-family:'Noto Sans'", `Text <b id=@>bold run</b> after`],
  ["font-family:serif", `Text <i id=@>italic run</i> after`],
  ["font-family:monospace", `Text <b id=@><i>bold italic</i></b> after`],
  ["", `Text <a id=@ href="#x">link run</a> after`],
  // Line heights: inherited as a number, a length and a percentage.
  ...["1", "1.2", "1.5", "2", "0.8", "21px", "30px", "10px", "150%", "2em"].map((l) => [`line-height:${l}`, `Own <span id=@ style="font-size:24px">large run</span> after`]),
  ...["1.5", "21px", "150%"].map((l) => [`line-height:${l}`, `Own <span id=@ style="font-size:9px">small run</span> after`]),
  ...["1.5", "21px"].map((l) => [`line-height:${l}`, `Own <span id=@ style="font-family:serif">serif run</span> after`]),
  // Lines of a run's own.
  ...["1", "1.5", "0.5", "3", "40px", "5px", "200%", "normal"].map((l) => ["", `Own <span id=@ style="line-height:${l}">spaced run</span> after`]),
  ...["1", "1.5", "0.5", "40px", "normal"].map((l) => ["", `Own <span id=@ style="font-size:30px;line-height:${l}">large run</span> after`]),
  ["line-height:2", `Own <span id=@ style="font-size:30px;line-height:1">large run</span> after`],
  ["line-height:30px", `Own <span id=@ style="line-height:normal">normal run</span> after`],
  // Vertical alignment.
  ...["sub", "super", "top", "bottom", "middle", "text-top", "text-bottom", "baseline", "3px", "-3px", "50%", "-50%", "1em"].map((v) => ["", `Text <span id=@ style="vertical-align:${v}">moved run</span> after`]),
  ...["sub", "super", "top", "bottom", "middle", "text-top", "text-bottom", "3px", "50%"].map((v) => ["", `Text <span id=@ style="vertical-align:${v};font-size:24px">moved large</span> after`]),
  ...["sub", "super", "top", "bottom", "middle", "text-top", "text-bottom", "3px", "50%"].map((v) => ["line-height:1.5;font-size:16px", `Text <span id=@ style="vertical-align:${v};font-size:10px">moved small</span> after`]),
  // The elements' own defaults.
  ...["small", "big", "sub", "sup", "code", "kbd", "samp", "tt", "mark", "abbr", "q", "var", "cite", "dfn", "em", "strong", "u", "s", "ins", "del"].map((t) => ["", `Text <${t} id=@>some words</${t}> after`]),
  ...["small", "big", "sub", "sup", "code", "kbd"].map((t) => ["font-size:16px;line-height:1.5", `Text <${t} id=@>some words</${t}> after`]),
  ...["1", "2", "3", "4", "5", "6", "7", "+1", "-1"].map((s) => ["", `Text <font id=@ size="${s}">font words</font> after`]),
  ["", `Text <font id=@ face="serif" color="#900">font words</font> after`],
  // Units.
  ...["0.75em", "1.5em", "50%", "120%", "1.25rem", "smaller", "larger", "x-small", "xx-large", "medium", "2ex", "1vw"].map((s) => ["", `Text <span id=@ style="font-size:${s}">unit run</span> after`]),
  // Nesting.
  ["", `Text <small id=@><small>small small</small></small> after`],
  ["", `Text <big id=@><big>big big</big></big> after`],
  ["", `Text <sub id=@><sub>sub sub</sub></sub> after`],
  ["", `Text <sup id=@><sup>sup sup</sup></sup> after`],
  ["", `Text <sup id=@><sub>sup sub</sub></sup> after`],
  ["", `Text <code id=@><b>bold code</b></code> after`],
  ["", `Text <b id=@><code>code bold</code></b> after`],
  ["", `Text <sup id=@><code>sup code</code></sup> after`],
  ["", `Text <small id=@><code>small code</code></small> after`],
  ["", `Text <span id=@ style="font-size:24px"><b style="font-size:10px">inner small</b></span> after`],
  ["", `Text <span id=@ style="font-size:24px">outer <b style="font-size:10px">inner</b> outer</span> after`],
  ["", `Text <span id=@ style="font-size:10px">outer <b style="font-size:24px">inner</b> outer</span> after`],
  ["", `Text <span id=@ style="line-height:40px"><b style="font-size:10px;line-height:1">inner</b></span> after`],
  ["", `Text <span id=@ style="vertical-align:5px">out <b style="vertical-align:5px">in</b> out</span> after`],
  ["", `Text <span id=@ style="vertical-align:top"><b style="vertical-align:5px;font-size:20px">in</b> out</span> after`],
  // Empty boxes and the root's font.
  ["", `Text <span id=@ style="font-size:50px"></span> after`],
  ["", `Text <span id=@ style="font-size:50px"> </span> after`],
  ["font-size:30px", `<span id=@ style="font-size:10px">Whole paragraph in one span</span>`],
  ["", `<span id=@ style="font-size:20px">Whole paragraph in one span</span>`],
  ["line-height:1.5", `<span id=@ style="font-size:20px">Whole paragraph in one span</span>`],
  ["", `<span id=@ style="vertical-align:super">Whole paragraph raised</span>`],
  // More than one run.
  ["", `H<sub id=@>2</sub>O and x<sup>2</sup> plain`],
  ["", `<b style="font-size:30px">T</b>ext <span id=@ style="font-size:9px">x</span><i style="font-size:18px">y</i>z`],
  ["line-height:1.5", `One <small>two</small> <big>three</big> <span id=@ style="font-size:30px">four</span> <code>five</code>`],
  // Wrapping: inside a run, around a tall word, per-line heights.
  ["", `Text with <span id=@ style="font-family:serif;font-size:18px">serif run</span> ${LONG}`],
  ["", `Lead <span id=@ style="font-size:20px">${WORDS}</span> tail ${LONG}`],
  ["line-height:1.5", `Lead <span id=@ style="font-size:20px">${WORDS}</span> tail ${LONG}`],
  ["", `Some ${LONG} <span id=@ style="font-size:30px">tall</span> ${LONG}`],
  ["", `Lead <code id=@>${WORDS}</code> tail ${LONG}`],
  ["", `Lead <sub id=@>${WORDS}</sub> tail ${LONG}`],
  ["", `Lead <small id=@>${WORDS}</small> tail`],
  ["width:300px", `Narrow <big id=@>big words wrap</big> here and <small>small</small> and on and on and on with more`],
  ["width:60px", `Long<span id=@ style="font-size:20px">unbroken</span>word and<b style="font-size:9px">no</b>break`],
  ["width:120px", `A foo<span id=@ style="font-size:20px">bar</span>baz stays whole as one word here`],
  ["width:120px;text-align:center", `Centred <span id=@ style="font-size:20px">words go</span> over the lines here`],
  ["width:120px;text-align:right", `Right <span id=@ style="font-size:20px">words go</span> over the lines here`],
  ["text-align:justify;width:300px", `Justified <span id=@ style="font-size:20px">words go</span> ${LONG}`],
  ["white-space:nowrap", `Kept <span id=@ style="font-size:20px">on one</span> ${LONG}`],
  ["white-space:pre", `Kept  <span id=@ style="font-size:20px">pre  run</span>\nnext line`],
  ["", `One<br><span id=@ style="font-size:20px">two<br>three</span><br>four`],
  // `middle` sits on the x-height of the face its parent is in.
  ["font-family:'Nimbus Roman';font-weight:bold", `Text <span id=@ style="vertical-align:middle;font-size:30px">middle run</span> after`],
  ["font-family:'Nimbus Roman';font-style:italic", `Text <span id=@ style="vertical-align:middle;font-size:30px">middle run</span> after`],
  ["font-family:'Noto Sans';font-weight:bold", `Text <span id=@ style="vertical-align:middle;font-size:30px">middle run</span> after`],
  ["font-family:'Nimbus Sans';font-weight:bold;font-style:italic", `Text <span id=@ style="vertical-align:middle;font-size:30px">middle run</span> after`],
  // A run's box: padding and border are in its rect, on the lines it opens and closes.
  ["", `Use <span id=@ style="padding:3px 5px">padding only</span> after`],
  ["", `Use <span id=@ style="padding:2px 6px;border:1px solid #999;border-radius:3px;background:#eee;font-size:12px">tag</span> after`],
  ["", `Use <span id=@ style="background:#ff0;padding:1px">square</span> after`],
  ["", `Use <span id=@ style="background:#ff0;padding:4px 0">tall only</span> after`],
  ["", `Use <kbd id=@ style="border:1px solid #aaa;padding:0 3px">Ctrl</kbd>+<kbd style="border:1px solid #aaa;padding:0 3px">C</kbd> after`],
  ["width:150px", `Use <span id=@ style="padding:2px 6px;border:1px solid #999">a box that goes over lines</span> after`],
  // Generated content is in the element's box.
  ["", `Text <span id=@ class="bef">before</span> after`],
  ["", `Text <span id=@ class="aft">after it</span> after`],
  ["", `Text <span id=@ class="bef aft">both sides</span> after`],
  ["", `Text <q id=@>quoted words</q> after`],
  // \`position: relative\`, normalize.css's sub and sup among them, moves and does not grow the line.
  ["", `Rel <span id=@ style="position:relative;top:5px">moved run</span> after`],
  ["", `Rel <span id=@ style="position:relative;bottom:5px;left:3px;font-size:20px">moved run</span> after`],
  ["", `<span id=@ style="position:relative;top:4px">Whole paragraph moved</span>`],
  ["", `Rel <span id=@ style="position:relative;left:2px">out <b style="position:relative;top:3px">in</b> out</span> after`],
  ["", `x<sup id=@ style="position:relative;top:-0.5em;vertical-align:baseline;font-size:75%;line-height:0">2</sup> and H<sub style="position:relative;bottom:-0.25em;vertical-align:baseline;font-size:75%;line-height:0">2</sub>O`],
].map(([block, inner], i) => ({ id: `p${i}`, block, inner: inner.replaceAll("id=@", `id=p${i}r`) }));

const STYLE = 'body{margin:8px;font:14px Arial}p{margin:0 0 10px}.bef::before{content:"» ";font-size:24px}.aft::after{content:" «";font-size:10px}';
const body = CASES.map((c) => `<p id="${c.id}"${c.block ? ` style="${c.block}"` : ""}>${c.inner}</p>`).join("\n");

// One decimal-free line per element, the way the test reads them back.
const READER = `function rect(n){const r=n.getBoundingClientRect();return [r.x,r.y,r.width,r.height]}
function rects(n){return Array.from(n.getClientRects()).map(r=>[r.x,r.y,r.width,r.height])}
function report(tag,ids){for(const id of ids){const n=document.getElementById(id);if(n)console.log("LC "+tag+" "+id+" "+JSON.stringify({b:rect(n),l:rects(n)}))}}`;
// The last line a page logs: what it said is recorded only once this arrives.
const END = "LE end";
const OPEN_SCRIPT = `<script>${READER}
setTimeout(function(){const all=document.querySelectorAll("[id]");console.log("LC count "+all.length);console.log("LC order "+Array.from(all).slice(0,8).map(n=>n.id).join(","));report("open",Array.from(all).map(n=>n.id));console.log("${END}")},100)</script>`;

writeRecording(path.join(here, "closed.html"), `<!DOCTYPE html>\n<html><head><style>${STYLE}</style></head><body>\n${body}\n</body></html>\n`);
writeRecording(path.join(here, "open.html"), `<!DOCTYPE html>\n<html><head><style>${STYLE}</style></head><body>\n${body}\n${OPEN_SCRIPT}\n</body></html>\n`);

// A script that changes a run: each step does one thing and every box is read back.
const CHANGES = [
  ["size", `$("s1r").style.fontSize="24px"`],
  ["back", `$("s1r").style.fontSize=""`],
  ["family", `$("s2r").style.fontFamily="serif"`],
  ["line", `$("s3r").style.lineHeight="40px"`],
  ["valign", `$("s4r").style.verticalAlign="super"`],
  ["class", `$("s5r").className="big"`],
  ["unclass", `$("s6r").classList.remove("big")`],
  ["text", `$("s7r").textContent="now holds rather more words than it did before so that this run has to go over the end of the first line and wrap all the way round to the next line of it and more"`],
  ["block", `$("s8").style.fontSize="20px"`],
  ["blockline", `$("s9").style.lineHeight="2"`],
  ["whole", `$("s10r").style.fontSize="30px"`],
  ["nested", `$("s11i").style.fontSize="30px"`],
  ["tall", `$("s12r").style.fontSize="14px"`],
  ["created", `const n=document.createElement("span");n.id="s13n";n.style.fontSize="24px";n.textContent="made";$("s13").appendChild(n)`],
  ["removed", `$("s14r").remove()`],
];
const CHANGE_PAGE = `<!DOCTYPE html>
<html><head><style>${STYLE}.big{font-size:22px}</style></head><body>
<p id="s1">Text with <span id="s1r">a run</span> after</p>
<p id="s2">Text with <span id="s2r">a run</span> after</p>
<p id="s3">Text with <span id="s3r">a run</span> after</p>
<p id="s4">Text with <span id="s4r">a run</span> after</p>
<p id="s5">Text with <span id="s5r">a run</span> after</p>
<p id="s6">Text with <span id="s6r" class="big">a run</span> after</p>
<p id="s7">Text with <b id="s7r" style="font-size:20px">a run</b> after</p>
<p id="s8">Text with <span id="s8r" style="font-size:14px">a run</span> after</p>
<p id="s9">Text with <span id="s9r" style="font-size:20px">a run</span> after</p>
<p id="s10"><span id="s10r">All one span</span></p>
<p id="s11">Text with <span id="s11r">a run <i id="s11i">inside</i></span> after</p>
<p id="s12">Text with <span id="s12r" style="font-size:24px">a run</span> after</p>
<p id="s13">Text with no run</p>
<p id="s14">Text with <span id="s14r" style="font-size:24px">a run</span> after</p>
<script>${READER}
const $=(id)=>document.getElementById(id);
const ids=[];for(let i=1;i<=14;i++){ids.push("s"+i,"s"+i+"r")}ids.push("s11i","s13n");
report("start",ids);
${CHANGES.map(([name, code]) => `{${code};report(${JSON.stringify(name)},ids)}`).join("\n")}
console.log("${END}");
</script>
</body></html>
`;
writeRecording(path.join(here, "changes.html"), CHANGE_PAGE);

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
const collect = async (file) => {
  const log = [];
  const listen = (m) => { if (m.text().startsWith("LC ")) log.push(m.text()); };
  page.on("console", listen);
  const done = waitForLine(page, END);
  await page.goto("file://" + path.join(here, file), { waitUntil: "load" });
  await done;
  page.off("console", listen);
  return log;
};
const openLog = await collect("open.html");
const changesLog = await collect("changes.html");
const cases = CASES.map((c) => ({ id: c.id, block: c.block, inner: c.inner }));
await browser.close();
const lines = (l) => l.map((s) => JSON.stringify(s)).join(",\n");
writeRecording(path.join(here, "chromium.json"), `{"cases":[\n${lines(cases)}\n],\n"open":[\n${lines(openLog)}\n],\n"changes":[\n${lines(changesLog)}\n]}\n`);
