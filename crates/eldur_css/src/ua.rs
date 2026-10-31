//! User-agent default stylesheet transcribed from WHATWG HTML Living Standard
//! §15 "Rendering" (<https://html.spec.whatwg.org/multipage/rendering.html>),
//! mapped to physical CSS properties understood by the converter.

use std::sync::OnceLock;

use crate::{Origin, Stylesheet};

static UA_STYLESHEET: OnceLock<Stylesheet> = OnceLock::new();

const UA_CSS: &str = r#"
/* §15.3.1 Hidden elements */
area, base, basefont, datalist, head, link, meta, noembed,
noframes, noscript, param, rp, script, style, template, title {
    display: none;
}

[hidden]:not([hidden="until-found" i]) {
    display: none;
}

dialog:not([open]) {
    display: none;
}

input[type="hidden" i] {
    display: none;
}

/* §15.3.2 The page & §15.3.6 Flow / sectioning blocks */
html, body {
    display: block;
}

body {
    margin: 8px;
}

address, article, aside, blockquote, center, dd, details, dir, div,
dl, dt, fieldset, figcaption, figure, footer, form, header, hgroup,
hr, legend, listing, main, menu, nav, ol, p, plaintext, pre, search,
section, summary, ul, xmp {
    display: block;
}

/* §15.3.6 Headings */
h1 {
    display: block;
    font-size: 2em;
    margin-top: 0.67em;
    margin-bottom: 0.67em;
    margin-left: 0;
    margin-right: 0;
    font-weight: bold;
}

h2 {
    display: block;
    font-size: 1.5em;
    margin-top: 0.83em;
    margin-bottom: 0.83em;
    margin-left: 0;
    margin-right: 0;
    font-weight: bold;
}

h3 {
    display: block;
    font-size: 1.17em;
    margin-top: 1em;
    margin-bottom: 1em;
    margin-left: 0;
    margin-right: 0;
    font-weight: bold;
}

h4 {
    display: block;
    font-size: 1em;
    margin-top: 1.33em;
    margin-bottom: 1.33em;
    margin-left: 0;
    margin-right: 0;
    font-weight: bold;
}

h5 {
    display: block;
    font-size: 0.83em;
    margin-top: 1.67em;
    margin-bottom: 1.67em;
    margin-left: 0;
    margin-right: 0;
    font-weight: bold;
}

h6 {
    display: block;
    font-size: 0.67em;
    margin-top: 2.33em;
    margin-bottom: 2.33em;
    margin-left: 0;
    margin-right: 0;
    font-weight: bold;
}

/* §15.3.5 Flow content & §15.3.8 Lists */
p, dl {
    margin-top: 1em;
    margin-bottom: 1em;
}

dd {
    margin-left: 40px;
}

blockquote, figure {
    margin-top: 1em;
    margin-bottom: 1em;
    margin-left: 40px;
    margin-right: 40px;
}

pre, listing, plaintext, xmp {
    margin-top: 1em;
    margin-bottom: 1em;
    font-family: monospace;
    white-space: pre;
}

ol, ul, menu, dir {
    margin-top: 1em;
    margin-bottom: 1em;
    padding-left: 40px;
}

ul, menu, dir {
    list-style-type: disc;
}

ol {
    list-style-type: decimal;
}

ol[type="1"], li[type="1"] {
    list-style-type: decimal;
}

ol[type="a" s], li[type="a" s] {
    list-style-type: lower-alpha;
}

ol[type="A" s], li[type="A" s] {
    list-style-type: upper-alpha;
}

ol[type="i" s], li[type="i" s] {
    list-style-type: lower-roman;
}

ol[type="I" s], li[type="I" s] {
    list-style-type: upper-roman;
}

li {
    display: list-item;
}

:is(dir, dl, menu, ol, ul) :is(dir, menu, ol, ul) {
    margin-top: 0;
    margin-bottom: 0;
}

:is(dir, menu, ol, ul) :is(dir, menu, ul) {
    list-style-type: circle;
}

:is(dir, menu, ol, ul) :is(dir, menu, ol, ul) :is(dir, menu, ul) {
    list-style-type: square;
}

/* §15.3.4 Phrasing content */
a:link, a:visited {
    color: #0000ee;
    text-decoration: underline;
    cursor: pointer;
}

b, strong {
    font-weight: bolder;
}

i, cite, em, var, dfn, address {
    font-style: italic;
}

tt, code, kbd, samp {
    font-family: monospace;
}

mark {
    background-color: yellow;
    color: black;
}

big {
    font-size: larger;
}

small {
    font-size: smaller;
}

sub {
    vertical-align: sub;
    font-size: smaller;
}

sup {
    vertical-align: super;
    font-size: smaller;
}

u, ins {
    text-decoration: underline;
}

s, strike, del {
    text-decoration: line-through;
}

center {
    text-align: center;
}

abbr[title], acronym[title] {
    text-decoration: dotted underline;
}

nobr {
    white-space: nowrap;
}

/* §15.3.9 Tables */
table {
    display: table;
    box-sizing: border-box;
    border-spacing: 2px;
    border-collapse: separate;
    text-indent: 0;
}

thead {
    display: table-header-group;
    vertical-align: middle;
}

tbody {
    display: table-row-group;
    vertical-align: middle;
}

tfoot {
    display: table-footer-group;
    vertical-align: middle;
}

tr {
    display: table-row;
    vertical-align: inherit;
}

colgroup {
    display: table-column-group;
}

col {
    display: table-column;
}

td, th {
    display: table-cell;
    vertical-align: inherit;
    padding: 1px;
}

th {
    font-weight: bold;
    text-align: center;
}

caption {
    display: table-caption;
    text-align: center;
}

/* §15.3.7 & §15.3.10 HR, Fieldset, Legend, Form controls, Replaced & Interactive */
hr {
    color: gray;
    border-style: inset;
    border-width: 1px;
    margin-top: 0.5em;
    margin-bottom: 0.5em;
    margin-left: auto;
    margin-right: auto;
    overflow: hidden;
}

fieldset {
    margin-left: 2px;
    margin-right: 2px;
    border: 2px groove #c0c0c0;
    padding-top: 0.35em;
    padding-bottom: 0.625em;
    padding-left: 0.75em;
    padding-right: 0.75em;
    min-width: min-content;
}

legend {
    padding-left: 2px;
    padding-right: 2px;
}

input, button, select, textarea {
    display: inline-block;
}

button,
input[type="button" i],
input[type="submit" i],
input[type="reset" i] {
    text-align: center;
    box-sizing: border-box;
}

input[type="checkbox" i],
input[type="radio" i] {
    box-sizing: border-box;
    margin: 3px;
}

select,
input[type="color" i],
progress,
meter {
    box-sizing: border-box;
}

textarea {
    white-space: pre-wrap;
}

iframe {
    border: 2px inset;
}

img, video, canvas, svg {
    display: inline;
}

ruby {
    display: inline;
}

rt {
    font-size: 50%;
}

dialog[open] {
    display: block;
    position: absolute;
    left: 0;
    right: 0;
    width: fit-content;
    height: fit-content;
    margin: auto;
    border: solid;
    padding: 1em;
    background-color: white;
    color: black;
}
"#;

pub(crate) fn stylesheet() -> &'static Stylesheet {
    UA_STYLESHEET.get_or_init(|| {
        let (sheet, diags) = Stylesheet::parse(UA_CSS, Origin::UserAgent, "<user-agent>", None);
        debug_assert!(
            diags.is_empty(),
            "UA-origin stylesheets must never emit user-visible diagnostics"
        );
        sheet
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ua_stylesheet_parses_cleanly_without_diagnostics() {
        let (sheet, diags) = Stylesheet::parse(UA_CSS, Origin::UserAgent, "<user-agent>", None);
        assert!(diags.is_empty(), "UA sheet emitted diagnostics: {diags:?}");
        assert!(!sheet.inner.rules.is_empty());

        let cached = stylesheet();
        assert_eq!(cached.origin, Origin::UserAgent);
        assert_eq!(cached.inner.rules.len(), sheet.inner.rules.len());
    }
}
