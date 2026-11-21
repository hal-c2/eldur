/**
 * Records, in headless Chromium, whether each of a set of `style` attribute
 * texts declares `display`, and writes them as the Rust table that
 * `eldur_css`'s `style_display_chromium` test checks the converter against.
 *
 *   node tools/visual/style_display_cases.mjs > crates/eldur_css/src/cascade/style_display_chromium.rs
 *
 * The named cases are the tokenizer's edges: strings, escapes, comments,
 * `url(` tokens and the names they can continue. The generated ones join
 * a fixed seed's random run of those pieces around one `display:none`, so
 * the table is the same on every run with the same Chromium.
 */

import { loadPlaywright } from "./playwright.mjs";

const named = [
  // Escaped quotes, inside a string and out.
  String.raw`--a:"a\"/*";display:none;--b:*/;color:red`,
  String.raw`--a:\"/*;display:none;--b:*/;color:red`,
  String.raw`--a:'a\'/*';display:none;--b:*/;color:red`,
  String.raw`--a:\'/*;display:none;--b:*/;color:red`,
  // A backslash-newline inside a string continues it; a bare newline ends it.
  '--a:"a\\\n/*";display:none;--b:*/;color:red',
  '--a:"a\n;display:none;--b:"',
  '--a:"a\n/*;display:none;--b:*/;color:red',
  "--a:'a\r\n;display:none;--b:'",
  "--a:'a\f;display:none;--b:'",
  // A backslash at the end of the text.
  "display:none;--a:\\",
  "--a:/*;display:none\\",
  '--a:"\\',
  // A non-hex escape against a hex one.
  String.raw`--a:\url(/*);display:none;--b:*/;color:red`,
  String.raw`--a:\75rl(/*);display:none;--b:*/;color:red`,
  String.raw`--a:\u\72 l(/*);display:none;--b:*/;color:red`,
  // Six hex digits, then one followed by a space, a tab, CR LF.
  String.raw`--a:\000075rl(/*);display:none;--b:*/;color:red`,
  String.raw`--a:\000075 rl(/*);display:none;--b:*/;color:red`,
  String.raw`--a:\0000755rl(/*);display:none;--b:*/;color:red`,
  "--a:a\\75\turl(/*);display:none;--b:*/;color:red",
  "--a:a\\75\r\nurl(/*);display:none;--b:*/;color:red",
  "--a:a\\75\r\n url(/*);display:none;--b:*/;color:red",
  // An escaped slash or star opens no comment.
  String.raw`--a:\/*;display:none;--b:*/;color:red`,
  String.raw`--a:/\*;display:none;--b:*/;color:red`,
  String.raw`--a:/\2a;display:none;--b:*/;color:red`,
  // `url(` inside a string is text.
  `--a:"url(/*";display:none;--b:*/;color:red`,
  `--a:'url(';display:none;--b:/*;color:red`,
  // Whitespace after `url(`, and a quoted argument holding `/*`.
  "--a:url(   /*);display:none;--b:*/;color:red",
  "--a:url(\n/*);display:none;--b:*/;color:red",
  `--a:url("/*");display:none;--b:*/;color:red`,
  `--a:url( '/*' );display:none;--b:*/;color:red`,
  `--a:url( "/*" /* x */);display:none;--b:*/;color:red`,
  // Unclosed at the end.
  "--a:url(/*;display:none",
  "--a:url(a;display:none",
  `--a:"/*;display:none`,
  `--a:'a;display:none`,
  "--a:/*;display:none",
  // A comment inside `url(` after a quote: a bad url, read to its `)`.
  `--a:url(a"/*);display:none;--b:*/;color:red`,
  `--a:url(a'b/*);display:none;--b:*/;color:red`,
  `--a:url(a b/*);display:none;--b:*/;color:red`,
  `--a:url(a"\\)/*);display:none;--b:*/;color:red`,
  // Digit-led names are dimensions; `-` and `--` lead idents.
  "--a:5url(/*);display:none;--b:*/;color:red",
  "--a:5 url(/*);display:none;--b:*/;color:red",
  "--a:5.url(/*);display:none;--b:*/;color:red",
  "--a:5%url(/*);display:none;--b:*/;color:red",
  "--a:-url(/*);display:none;--b:*/;color:red",
  "--a:--url(/*);display:none;--b:*/;color:red",
  "--a:- url(/*);display:none;--b:*/;color:red",
  "--a:-5url(/*);display:none;--b:*/;color:red",
  "--a:+url(/*);display:none;--b:*/;color:red",
  "--a:#url(/*);display:none;--b:*/;color:red",
  "--a:# url(/*);display:none;--b:*/;color:red",
  "--a:@url(/*);display:none;--b:*/;color:red",
  "--a:_url(/*);display:none;--b:*/;color:red",
  "--a:éurl(/*);display:none;--b:*/;color:red",
  "--a:)url(/*);display:none;--b:*/;color:red",
  `--a:"x"url(/*);display:none;--b:*/;color:red`,
  String.raw`--a:-\75rl(/*);display:none;--b:*/;color:red`,
  // Blocks hold the `;` that would end a declaration.
  "--a:(;display:none;--b:);color:red",
  "--a:[;display:none;--b:];color:red",
  "--a:{;display:none;--b:};color:red",
  "--a:(;display:none",
  "--a:[;display:none",
  "--a:{;display:none",
  "--a:);display:none",
  "--a:];display:none",
  "--a:};display:none",
  "--a:[(];display:none;--b:)];color:red",
  // Comment edges.
  "--a:/*/;display:none;--b:*/;color:red",
  "--a:/**/;display:none",
  "--a:/***/;display:none",
  "--a:/* */;display/**/:none",
  "display/**/:/**/none",
];

// Pieces the generated texts are made of.
const pieces = [
  '"', "'", "\\", '\\"', "\\'", "/*", "*/", "/", "*", "url(", "URL(", "u\\72 l(",
  "\\75rl(", "\\75 ", "\\000075", "\\000075 ", "\\u", ")", "(", "[", "]", "{", "}",
  " ", "\n", "\\\n", "a", "5", "-", "--", "#", "@", "\r\n", "\r", "\f", "\t", ";",
  "x;", ".", "%",
];
const tails = ["*/", '"', "'", ")", "]", "}", " ", "a", "\n", "/*"];

// mulberry32, so the generated cases are the same on every run.
function rng(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const random = rng(51);
const pick = (list) => list[Math.floor(random() * list.length)];
const generated = new Set();
while (generated.size < 600) {
  let head = "";
  for (let i = 1 + Math.floor(random() * 6); i > 0; --i) head += pick(pieces);
  let tail = "";
  for (let i = Math.floor(random() * 3); i > 0; --i) tail += pick(tails);
  generated.add(`--a:${head};display:none;--b:${tail};color:red`);
}

const { chromium } = await loadPlaywright();
const browser = await chromium.launch();
const page = await browser.newPage();
const texts = [...named, ...generated];
const declared = await page.evaluate((texts) => {
  const el = document.createElement("div");
  document.body.append(el);
  return texts.map((text) => {
    el.setAttribute("style", text);
    return el.style.getPropertyValue("display") !== "";
  });
}, texts);
const version = browser.version();
await browser.close();

function rust(text) {
  let out = "";
  for (const c of text) {
    const code = c.codePointAt(0);
    if (c === "\\") out += "\\\\";
    else if (c === '"') out += '\\"';
    else if (c === "\n") out += "\\n";
    else if (c === "\r") out += "\\r";
    else if (c === "\t") out += "\\t";
    else if (code < 0x20 || code === 0x7f) out += `\\u{${code.toString(16)}}`;
    else out += c;
  }
  return `"${out}"`;
}

const rows = (list, from) =>
  list.map((text, i) => `    (${rust(text)}, ${declared[from + i]}),`).join("\n");
process.stdout.write(`//! Whether a \`style\` attribute's text declares \`display\`, as Chromium
//! ${version} reads it, recorded by \`tools/visual/style_display_cases.mjs\`.
//! Regenerate with that script rather than editing by hand.

/// The tokenizer's edges, one case each.
pub(super) const NAMED: &[(&str, bool)] = &[
${rows(named, 0)}
];

/// Seed 51's run of strings, quotes, escapes, comments, \`url(\` and blocks
/// around one \`display:none\`.
pub(super) const GENERATED: &[(&str, bool)] = &[
${rows([...generated], named.length)}
];
`);
