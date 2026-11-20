//! A block the converter showed that a script hides takes its margins with it:
//! the margins either side of it collapse as if it had never been there, also
//! through the top of its parent once the block after it is the first one, and
//! showing it again puts them back (hal-c2/eldur#51). A block hidden as loaded
//! that a script shows is the same thing the other way round. Positions are
//! run in qmltestrunner against Chromium's `getBoundingClientRect().top` at
//! 800×600, as loaded, after the toggle, and after toggling back.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, convert_str};

/// What the toggle does unless a case says otherwise; the issue's own page
/// is the first one.
const TOGGLE: &str = "const s = document.getElementById('shy');
  s.style.display = s.style.display === 'none' ? '' : 'none';";

/// What a class toggle does.
const CLASS_TOGGLE: &str = "document.getElementById('shy').classList.toggle('off');";

struct Case {
    name: &'static str,
    body: &'static str,
    /// Element ids measured, a script names each one so it has an
    /// `objectName`.
    ids: &'static [&'static str],
    /// Chromium's tops: as loaded, then with `#shy` toggled, of the first
    /// as many ids as it has.
    load: &'static [f64],
    toggled: &'static [f64],
    /// What the button runs, [`TOGGLE`] when empty.
    toggle: &'static str,
    /// Chromium's tops toggled back, when they aren't `load`: a box whose
    /// `display` was a script's `none` the first time round is shown by `''`.
    back: &'static [f64],
}

const CASES: &[Case] = &[
    // The issue's page: `#shy`'s margin escapes through the body's top, and
    // once it is hidden, `#after`'s does.
    Case {
        name: "first_hides",
        body: r#"<p id="shy"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // Hidden as loaded, then shown: the same page the other way round.
    Case {
        name: "first_shows",
        body: r#"<p id="shy" style="display:none"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // What escapes once `#shy` is hidden is smaller than what escaped with it:
    // `#after` goes above where its own margin alone would put it.
    Case {
        name: "smaller_margin_escapes",
        body: r#"<body style="margin:0"><p id="shy" style="margin:30px 0"><em>Shy</em></p>
<p id="after" style="margin:10px 0">After</p>"#,
        ids: &["after", "shy"],
        load: &[78.0, 30.0],
        toggled: &[10.0],
        toggle: "",
        back: &[],
    },
    // The margins escape through `#w` too, where its own 20 px waits for them.
    Case {
        name: "escapes_through_two",
        body: r#"<body style="margin:0"><div id="w" style="margin-top:20px"><p id="shy" style="margin:30px 0"><em>Shy</em></p>
<p id="after" style="margin:10px 0">After</p></div>"#,
        ids: &["w", "after"],
        load: &[30.0, 78.0],
        toggled: &[20.0, 20.0],
        toggle: "",
        back: &[],
    },
    // Between two blocks: hidden, the margins either side of it collapse
    // with each other.
    Case {
        name: "between_two",
        body: r#"<p id="a" style="margin:0 0 40px">A</p><p id="shy"><em>Shy</em></p>
<p id="b" style="margin:10px 0">B</p>"#,
        ids: &["a", "b"],
        load: &[8.0, 100.0],
        toggled: &[8.0, 66.0],
        toggle: "",
        back: &[],
    },
    // An empty block after it: hidden or not, the block's margins and the
    // empty one's collapse with the next one's.
    Case {
        name: "empty_after",
        body: r#"<div style="padding:8px 0"><p id="shy" style="margin:0 0 16px">S</p><div id="e" style="margin:30px 0"></div><p id="n" style="margin:8px 0">N</p></div>"#,
        ids: &["n"],
        load: &[64.0],
        toggled: &[46.0],
        toggle: "",
        back: &[],
    },
    // A block a script hides earlier in the column does not decide what the
    // column's last, hidden one passes on to the block after the column.
    Case {
        name: "earlier_block_hides",
        body: r#"<div id="outer"><p id="r" style="margin:0 0 40px">r</p><p id="x">x</p><div id="holder"><p id="shy" style="display:none">h</p></div></div>
<p id="next">next</p>
<script>document.getElementById('r').style.display = '';</script>"#,
        ids: &["x", "next"],
        load: &[66.0, 100.0],
        toggled: &[66.0, 134.0],
        toggle: "",
        back: &[],
    },
    // After a block a script can hide, a wrapper whose own first block
    // hides: the gap above the wrapper is what its next block escapes then.
    Case {
        name: "wrapper_after_hideable",
        body: r#"<body style="margin:0"><p id="p" style="margin:0">P</p><div id="w"><p id="shy" style="margin:40px 0 0"><em>Shy</em></p><p id="g" style="margin:10px 0 0">G</p></div>
<script>document.getElementById('p').style.display = '';</script>"#,
        ids: &["w", "g", "t"],
        load: &[58.0, 86.0, 104.0],
        toggled: &[28.0, 28.0, 46.0],
        toggle: "",
        back: &[],
    },
    // Hidden by its `style` attribute and by `hidden`: clearing its display
    // leaves the `[hidden]` rule hiding it.
    Case {
        name: "hidden_attribute_stays",
        body: r#"<p id="shy" style="display:none" hidden><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // The same with `hidden` set by a script before the toggle.
    Case {
        name: "hidden_set_stays",
        body: r#"<p id="shy" style="display:none"><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').hidden = true;</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // Its `hidden` attribute removed as loaded: its `style` attribute still
    // hides it, and clearing its display then shows it.
    Case {
        name: "hidden_removed",
        body: r#"<p id="shy" style="display:none" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // `hidden` set by a script and the attribute then removed: it no longer
    // keeps the box hidden once its display is cleared.
    Case {
        name: "hidden_set_removed",
        body: r#"<p id="shy" style="display:none"><em>Shy</em></p>
<p id="after">After</p>
<script>const h = document.getElementById('shy'); h.hidden = true; h.removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // Its `style` attribute's display removed as loaded: `hidden` set and
    // cleared again does not bring that declaration back.
    Case {
        name: "removed_display_stays_removed",
        body: r#"<p id="shy" style="display:none"><em>Shy</em></p>
<p id="after">After</p>
<script>const h = document.getElementById('shy'); h.style.removeProperty('display'); h.hidden = true; h.hidden = false;</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // A `display: none` inside a string of its `style` attribute is no
    // declaration: `hidden` set and cleared shows it again.
    Case {
        name: "display_in_a_string",
        body: r#"<p id="shy" style='display:block;font-family:"x;display:none;"'><em>Shy</em></p>
<p id="after">After</p>
<script>const h = document.getElementById('shy'); h.hidden = true; h.hidden = false;</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // An invalid later `display` is dropped: with `hidden` removed, the
    // valid `display: none` before it still hides the box.
    Case {
        name: "invalid_display_dropped",
        body: r#"<p id="shy" style="display:none;display:bogus" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // `el.style.display` reads the important declaration, which wins over
    // the later one: the toggle clears it.
    Case {
        name: "important_display_read",
        body: r#"<p id="shy" style="display:none!important;display:block"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // `flow` alone is a valid display, a block: it wins over the `none` before it.
    Case {
        name: "flow_display",
        body: r#"<p id="shy" style="display:none;display:flow"><em>Shy</em></p>
<p id="after">After</p>
<script>const h = document.getElementById('shy'); h.hidden = true; h.hidden = false;</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // An escaped `(` opens no bracket: the `display` after it is its own
    // declaration.
    Case {
        name: "escaped_bracket",
        body: r#"<p id="shy" style="font-family:a\(b;display:none"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // Two outside keywords are no display: the `none` before them still hides
    // the box once `hidden` is removed.
    Case {
        name: "two_outside_keywords_dropped",
        body: r#"<p id="shy" style="display:none;display:block inline" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // `run-in` is no display Chromium or eldur_css takes: the `none` before it
    // still hides the box.
    Case {
        name: "run_in_dropped",
        body: r#"<p id="shy" style="display:none;display:run-in" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // A quote in a comment opens no string: the `display` after it is its own
    // declaration.
    Case {
        name: "quote_in_a_comment",
        body: r#"<p id="shy" style="display:none;color:red/* ' */;display:block" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // An escaped keyword is the keyword: `\62 lock` is `block`.
    Case {
        name: "escaped_display_keyword",
        body: r#"<p id="shy" style="display:none;display:\62 lock" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // A `var()` display wins over the `none` before it.
    Case {
        name: "display_from_a_variable",
        body: r#"<p id="shy" style="display:none;display:var(--missing, block)" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // A comment ends a token: `block/**/flow` is `block flow`.
    Case {
        name: "comment_between_keywords",
        body: r#"<p id="shy" style="display:none;display:block/**/flow" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // A comment ends a token: `bl/**/ock` is two words and no display.
    Case {
        name: "comment_inside_keyword",
        body: r#"<p id="shy" style="display:none;display:bl/**/ock" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // A quote in an unquoted `url(` opens no string: the `display` after it is
    // its own declaration, which keeps the box hidden once `hidden` is removed.
    Case {
        name: "quote_in_an_unquoted_url",
        body: r#"<p id="shy" style="background:url(a'b);display:none" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // A `var()` display that resolves to `none` hides the block as its own
    // `style` attribute's, so clearing what a script wrote over it shows it.
    Case {
        name: "variable_display_cleared",
        body: r#"<div style="--d:none"><p id="shy" style="display:var(--d)"><em>Shy</em></p></div>
<p id="after">After</p>
<script>document.getElementById('shy').style.display = 'none';</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // The same with the custom property from a stylesheet rule.
    Case {
        name: "variable_from_a_rule_cleared",
        body: r#"<style>:root { --d: none; }</style>
<p id="shy" style="display:var(--d)"><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').style.display = 'none';</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    // The `var()` display still hides a block whose `hidden` is removed.
    Case {
        name: "variable_hides_hidden_removed",
        body: r#"<p id="shy" style="--d:none;display:var(--d)" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[50.0, 84.0],
    },
    Case {
        name: "rule_variable_hides_hidden_removed",
        body: r#"<style>:root { --d: none; }</style>
<p id="shy" style="display:var(--d)" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[50.0, 84.0],
    },
    // A script writing a `var()` display that resolves to `none` hides it.
    Case {
        name: "variable_written",
        body: r#"<p id="shy" style="--d:none"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display === '' ? 'var(--d)' : '';",
        back: &[],
    },
    // A bottom margin that varies meets a top margin that varies too: the
    // gap takes both cases.
    Case {
        name: "varying_edges_meet",
        body: r#"<div id="box"><p id="a" style="margin:0">A</p><p id="shy" style="margin:0 0 40px">Shy</p></div>
<div id="r"><p id="shy2" style="margin:30px 0 0">S2</p><p id="after" style="margin:10px 0 0">After</p></div>"#,
        ids: &["shy2", "after", "t"],
        load: &[84.0, 112.0, 130.0],
        toggled: &[56.0, 84.0, 102.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display === '' ? 'none' : '';
  if (s.dataset.never) document.getElementById('shy2').style.display = 'none';",
        back: &[],
    },
    // A bottom margin that varies, past a box hidden as loaded, still
    // varies at the block after.
    Case {
        name: "varying_edge_past_hidden",
        body: r#"<div id="box"><p id="a" style="margin:0">A</p><p id="shy" style="margin:0 0 40px">Shy</p></div>
<div id="mid" style="display:none;margin:0">Mid</div>
<p id="after" style="margin:10px 0 0">After</p>"#,
        ids: &["after", "t"],
        load: &[84.0, 102.0],
        toggled: &[36.0, 54.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display === '' ? 'none' : '';
  if (s.dataset.never) document.getElementById('mid').style.display = '';",
        back: &[],
    },
    // A bottom margin that varies stays inside a parent with bottom padding
    // only while the block giving it shows.
    Case {
        name: "varying_edge_closed",
        body: r#"<div id="wrap" style="padding-bottom:1px"><div id="box"><p id="a" style="margin:0">A</p><p id="shy" style="margin:0 0 40px">Shy</p></div></div>
<p id="after" style="margin:0">After</p>"#,
        ids: &["after", "t"],
        load: &[85.0, 103.0],
        toggled: &[27.0, 45.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display === '' ? 'none' : '';",
        back: &[],
    },
    // A custom property a script changes shows the box whose display
    // reads it, with nothing else written.
    Case {
        name: "variable_changed",
        body: r#"<div id="host" style="--d:none"><p id="shy"><em>Shy</em></p></div>
<p id="after">After</p>
<script>document.getElementById('shy').style.display = 'var(--d)';</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: "const h = document.getElementById('host');
  h.style.setProperty('--d', h.style.getPropertyValue('--d') === 'none' ? 'block' : 'none');",
        back: &[],
    },
    // An inherited custom property is resolved where it is declared: the
    // child's `--base` does not change what the parent's `--display` is.
    Case {
        name: "variable_resolved_first",
        body: r#"<div id="host" style="--base:none;--display:var(--base)"><p id="shy" style="--base:block"><em>Shy</em></p></div>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display === '' ? 'var(--display)' : '';",
        back: &[],
    },
    // A custom property on an inline ancestor is one a box's `var()`
    // display reads.
    Case {
        name: "variable_on_inline",
        body: r#"<p id="line"><span style="--d:none">Text <button id="shy" style="font-size:30px">Shy</button></span></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[72.0, 106.0],
        toggled: &[50.0, 84.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display === '' ? 'var(--d)' : '';",
        back: &[],
    },
    // A class a script takes away no longer hides the block: clearing its
    // `display` after that shows it, as the class world it is in says.
    Case {
        name: "class_gone_display_cleared",
        body: r#"<style>p{margin:0;height:20px}.off{display:none}</style>
<p id="a">A</p><p id="shy" class="off">B</p><p id="c">C</p>"#,
        ids: &["a", "c"],
        load: &[8.0, 28.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy');
  if (s.classList.contains('off')) { s.classList.remove('off'); s.style.display = 'none'; } else { s.style.display = s.style.display ? '' : 'none'; }",
        back: &[8.0, 48.0],
    },
    // The same through a class taken off an ancestor.
    Case {
        name: "ancestor_class_gone_display_cleared",
        body: r#"<style>p{margin:0;height:20px}.off #shy{display:none}</style>
<div id="w" class="off"><p id="a">A</p><p id="shy">B</p><p id="c">C</p></div>"#,
        ids: &["a", "c"],
        load: &[8.0, 28.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy'); const w = document.getElementById('w');
  if (w.classList.contains('off')) { w.classList.remove('off'); s.style.display = 'none'; } else { s.style.display = s.style.display ? '' : 'none'; }",
        back: &[8.0, 48.0],
    },
    // A class a script adds hides the block once its own `display: none`
    // is cleared, though no rule hid it as loaded.
    Case {
        name: "class_added_display_cleared",
        body: r#"<style>p{margin:0;height:20px}.off{display:none}</style>
<p id="a">A</p><p id="shy" style="display:none">B</p><p id="c">C</p>"#,
        ids: &["a", "c"],
        load: &[8.0, 28.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy');
  if (!s.classList.contains('off')) { s.classList.add('off'); } else { s.style.display = s.style.display ? '' : 'none'; }",
        back: &[],
    },
    // A `var()` display a script writes reads a custom property a
    // stylesheet declares.
    Case {
        name: "sheet_variable",
        body: r#"<style>p{margin:0;height:20px}:root{--d:none}</style>
<p id="a">A</p><p id="shy">B</p><p id="c">C</p>"#,
        ids: &["a", "c"],
        load: &[8.0, 48.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display ? '' : 'var(--d)';",
        back: &[],
    },
    // A script's custom property on an ancestor's `style` wins over the
    // one its stylesheet declares, until the script takes it away again.
    Case {
        name: "sheet_variable_overridden",
        body: r#"<style>p{margin:0;height:20px}#w{--d:none}</style>
<div id="w"><p id="a">A</p><p id="shy">B</p><p id="c">C</p></div>"#,
        ids: &["a", "c"],
        load: &[8.0, 48.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy'); const w = document.getElementById('w');
  if (!s.style.display) { s.style.display = 'var(--d)'; } else { w.style.setProperty('--d', w.style.getPropertyValue('--d') ? '' : 'block'); }",
        back: &[],
    },
    // `initial` makes a custom property guaranteed-invalid, so one that
    // reads it takes its fallback (#85).
    Case {
        name: "variable_initial_fallback",
        body: r#"<style>p{margin:0;height:20px}</style>
<div id="w" style="--a:initial;--b:var(--a, none)"><p id="a">A</p><p id="shy">B</p><p id="c">C</p></div>"#,
        ids: &["a", "c"],
        load: &[8.0, 48.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display ? '' : 'var(--b, block)';",
        back: &[],
    },
    // A rule declaring the custom property on the child holds against an
    // ancestor's later write, though both had the same value as loaded.
    Case {
        name: "variable_declared_on_child",
        body: r#"<style>p{margin:0;height:20px}#w{--d:block}#in{--d:block}</style>
<div id="w"><div id="in"><p id="a">A</p><p id="shy">B</p><p id="c">C</p></div></div>"#,
        ids: &["a", "c"],
        load: &[8.0, 48.0],
        toggled: &[8.0, 48.0],
        toggle: "const s = document.getElementById('shy'); const w = document.getElementById('w');
  if (!s.style.display) { s.style.display = 'var(--d)'; w.style.setProperty('--d', 'none'); } else { s.style.display = ''; w.style.removeProperty('--d'); }",
        back: &[],
    },
    // The stylesheet's value comes back once a script removes the `style`
    // attribute's.
    Case {
        name: "sheet_variable_under_attribute",
        body: r#"<style>p{margin:0;height:20px}#w{--d:none}</style>
<div id="w" style="--d:block"><p id="a">A</p><p id="shy">B</p><p id="c">C</p></div>"#,
        ids: &["a", "c"],
        load: &[8.0, 48.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy'); const w = document.getElementById('w');
  if (!s.style.display) { s.style.display = 'var(--d)'; w.style.removeProperty('--d'); } else { s.style.display = ''; w.style.setProperty('--d', 'block'); }",
        back: &[],
    },
    // A `;` and a name inside a string in the `style` attribute are part of
    // a value, not a declaration of their own.
    Case {
        name: "attribute_string_holds_a_name",
        body: r#"<style>p{margin:0;height:20px}#w{--d:none}</style>
<div id="w" style='--note:"x;--d:1"'><p id="a">A</p><p id="shy">B</p><p id="c">C</p></div>"#,
        ids: &["a", "c"],
        load: &[8.0, 48.0],
        toggled: &[8.0, 28.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display ? '' : 'var(--d)';",
        back: &[],
    },
    // `ruby` and `math` are valid displays that win over the `none` before
    // them: an inline box once `hidden` is removed. A script hiding an
    // inline box alone in its line isn't followed yet (as with `inline`),
    // so these pin the page as loaded.
    Case {
        name: "ruby_display",
        body: r#"<p id="shy" style="display:none;display:ruby" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[42.0, 76.0],
        toggled: &[42.0, 76.0],
        toggle: "void 0;",
        back: &[],
    },
    Case {
        name: "math_display",
        body: r#"<p id="shy" style="display:none;display:math" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[42.0, 76.0],
        toggled: &[42.0, 76.0],
        toggle: "void 0;",
        back: &[],
    },
    Case {
        name: "block_ruby_display",
        body: r#"<p id="shy" style="display:none;display:block ruby" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // The converter shows a `ruby` block as loaded.
    Case {
        name: "ruby_shown",
        body: r#"<p id="shy" style="display:none;display:ruby"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[42.0, 76.0],
        toggled: &[42.0, 76.0],
        toggle: "void 0;",
        back: &[],
    },
    // `flow` reads back as `block`, which the toggle tests for.
    Case {
        name: "flow_read_back",
        body: r#"<p id="shy"><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').style.display = 'flow';</script>"#,
        ids: &["after", "t"],
        load: &[50.0, 84.0],
        toggled: &[16.0, 50.0],
        toggle: "const s = document.getElementById('shy');
  s.style.display = s.style.display === 'block' ? 'none' : 'block';",
        back: &[],
    },
    // A column with no bottom padding or border passes its last block's
    // bottom margin on; with that block hidden, its earlier one's is what
    // goes on, collapsing with the next block's outside the column.
    Case {
        name: "unpadded_column_bottom",
        body: r#"<div id="col"><p id="a">A</p><p id="shy" style="margin:0 0 40px"><em>Shy</em></p></div>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[108.0, 142.0],
        toggled: &[50.0, 84.0],
        toggle: "",
        back: &[],
    },
    Case {
        name: "unpadded_column_bottom_nested",
        body: r#"<div id="outer"><div id="col"><p id="a">A</p><p id="shy" style="margin:0 0 40px"><em>Shy</em></p></div></div>
<p id="after" style="margin:10px 0">After</p>"#,
        ids: &["after", "t"],
        load: &[108.0, 136.0],
        toggled: &[50.0, 78.0],
        toggle: "",
        back: &[],
    },
    // A block a class hides takes its margins with it too.
    Case {
        name: "class_hides",
        body: r#"<style>.off { display: none; }</style>
<p id="a" style="margin:0 0 40px">A</p><p id="shy">Shy</p>
<p id="after" style="margin:10px 0">After</p>"#,
        ids: &["a", "after", "t"],
        load: &[8.0, 100.0, 128.0],
        toggled: &[8.0, 66.0, 94.0],
        toggle: CLASS_TOGGLE,
        back: &[],
    },
    Case {
        name: "class_hides_first",
        body: r#"<style>.off { display: none; }</style>
<p id="shy" class="off"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
        toggle: CLASS_TOGGLE,
        back: &[],
    },
    // A stylesheet rule that hides the block too keeps it hidden once a
    // script clears its own `display`.
    Case {
        name: "rule_hides_too",
        body: r#"<style>#shy { display: none; }</style>
<p id="shy" style="display:none"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    Case {
        name: "rule_variable_hides_too",
        body: r#"<style>:root { --d: none; } #shy { display: var(--d); }</style>
<p id="shy" style="display:none"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    Case {
        name: "class_rule_hides_too",
        body: r#"<style>.off { display: none; }</style>
<p id="shy" class="off" style="display:none"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
    // And once a script removes its `hidden` attribute.
    Case {
        name: "rule_hides_hidden_removed",
        body: r#"<style>#shy { display: none; }</style>
<p id="shy" hidden><em>Shy</em></p>
<p id="after">After</p>
<script>document.getElementById('shy').removeAttribute('hidden');</script>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[16.0, 50.0],
        toggle: "",
        back: &[],
    },
];

fn page(case: &Case) -> String {
    let named = case
        .ids
        .iter()
        .map(|id| format!("document.getElementById('{id}')"))
        .collect::<Vec<_>>()
        .join(", ");
    let toggle = if case.toggle.is_empty() {
        TOGGLE
    } else {
        case.toggle
    };
    format!(
        "<!DOCTYPE html>\n{}\n<button id=\"t\">Toggle</button>\n<script>\ndocument.getElementById('t').addEventListener('click', () => {{\n  {toggle}\n}});\n</script>\n<script>\n// Named so that they get an objectName the test can find.\nvar named = [{named}];\n</script>\n",
        case.body
    )
}

fn list(values: &[f64]) -> String {
    let parts = values.iter().map(f64::to_string).collect::<Vec<_>>();
    format!("[{}]", parts.join(", "))
}

fn test(case: &Case) -> String {
    let ids = case
        .ids
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let toggled_ids = &case.ids[..case.toggled.len()];
    let toggled_ids = toggled_ids
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"
import QtQuick
import QtTest

Item {{
    id: host
    width: 800
    height: 600

TestCase {{
    name: "HidingShown_{name}"
    when: windowShown

    property var page
    function find(name) {{ return findChild(page, name) }}
    // The item's box, without the half-leading `Translate` of its glyphs.
    function posY(name) {{
        const item = find(name)
        verify(item !== null, "no #" + name)
        return item.parent.mapToItem(page, item.x, item.y).y
    }}

    function expect(names, tops, step) {{
        // A layout is polished on the next frame: wait a bounded time for
        // it, then say where everything is.
        // A carry lands blocks on N.999999: compare to a hundredth.
        const at = () => names.map(n => Math.round(posY(n) * 100) / 100)
        for (let waited = 0; waited < 40 && JSON.stringify(at()) !== JSON.stringify(tops); ++waited)
            wait(25)
        compare(JSON.stringify(at()), JSON.stringify(tops), step + ": " + JSON.stringify(names))
    }}

    function test_toggle() {{
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, {{ width: 800, height: 600 }})
        waitForRendering(page)
        expect([{ids}], {load}, "as loaded")
        const button = find("t")
        button.clicked()
        expect([{toggled_ids}], {toggled}, "toggled")
        button.clicked()
        expect([{ids}], {back}, "toggled back")
        button.clicked()
        expect([{toggled_ids}], {toggled}, "toggled again")
    }}
}}
}}
"#,
        name = case.name,
        load = list(case.load),
        toggled = list(case.toggled),
        back = list(if case.back.is_empty() {
            case.load
        } else {
            case.back
        }),
    )
}

fn run(case: &Case) {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(&page(case), &options);
    let dir = std::env::temp_dir().join(format!(
        "eldur-hiding-shown-{}-{}",
        case.name,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_hiding.qml"), test(case)).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_hiding.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}

fn case(name: &str) -> &'static Case {
    CASES.iter().find(|c| c.name == name).expect("case")
}

#[test]
fn a_hidden_first_block_lets_the_next_ones_margin_escape() {
    run(case("first_hides"));
}

#[test]
fn a_shown_first_block_takes_the_escaping_margin_back() {
    run(case("first_shows"));
}

#[test]
fn a_smaller_margin_escapes_in_its_place() {
    run(case("smaller_margin_escapes"));
}

#[test]
fn the_margin_escapes_through_two_parents() {
    run(case("escapes_through_two"));
}

#[test]
fn the_margins_either_side_of_a_hidden_block_collapse() {
    run(case("between_two"));
}

#[test]
fn an_empty_block_after_a_hidden_one_keeps_its_margins() {
    run(case("empty_after"));
}

#[test]
fn an_earlier_hiding_block_does_not_decide_what_a_column_passes_on() {
    run(case("earlier_block_hides"));
}

#[test]
fn a_wrapper_after_a_hideable_block_takes_its_own_first_blocks_cases() {
    run(case("wrapper_after_hideable"));
}

#[test]
fn clearing_the_display_of_a_hidden_attributed_block_keeps_it_hidden() {
    run(case("hidden_attribute_stays"));
}

#[test]
fn clearing_the_display_of_a_block_a_script_set_hidden_keeps_it_hidden() {
    run(case("hidden_set_stays"));
}

#[test]
fn a_block_whose_hidden_attribute_is_removed_stays_hidden_by_its_style_until_cleared() {
    run(case("hidden_removed"));
}

#[test]
fn a_removed_hidden_attribute_no_longer_keeps_a_cleared_block_hidden() {
    run(case("hidden_set_removed"));
}

#[test]
fn a_removed_style_attribute_display_stays_removed() {
    run(case("removed_display_stays_removed"));
}

#[test]
fn a_display_inside_a_string_does_not_hide_the_block() {
    run(case("display_in_a_string"));
}

#[test]
fn an_invalid_later_display_does_not_show_the_block() {
    run(case("invalid_display_dropped"));
}

#[test]
fn the_important_display_is_the_one_a_script_reads() {
    run(case("important_display_read"));
}

#[test]
fn a_later_flow_display_shows_the_block() {
    run(case("flow_display"));
}

#[test]
fn an_escaped_bracket_does_not_swallow_the_display_after_it() {
    run(case("escaped_bracket"));
}

#[test]
fn a_display_with_two_outside_keywords_does_not_show_the_block() {
    run(case("two_outside_keywords_dropped"));
}

#[test]
fn a_display_chromium_drops_does_not_show_the_block() {
    run(case("run_in_dropped"));
}

#[test]
fn a_quote_in_a_comment_does_not_swallow_the_display_after_it() {
    run(case("quote_in_a_comment"));
}

#[test]
fn an_escaped_display_keyword_shows_the_block() {
    run(case("escaped_display_keyword"));
}

#[test]
fn a_display_from_a_variable_shows_the_block() {
    run(case("display_from_a_variable"));
}

#[test]
fn a_comment_between_display_keywords_separates_them() {
    run(case("comment_between_keywords"));
}

#[test]
fn a_comment_inside_a_display_keyword_splits_it() {
    run(case("comment_inside_keyword"));
}

#[test]
fn a_quote_in_an_unquoted_url_does_not_swallow_the_display_after_it() {
    run(case("quote_in_an_unquoted_url"));
}

#[test]
fn a_variable_display_that_hides_is_the_attributes_and_clearing_shows_the_block() {
    run(case("variable_display_cleared"));
}

#[test]
fn a_variable_display_from_a_rules_property_is_the_attributes_too() {
    run(case("variable_from_a_rule_cleared"));
}

#[test]
fn a_variable_display_keeps_a_block_whose_hidden_attribute_is_removed_hidden() {
    run(case("variable_hides_hidden_removed"));
}

#[test]
fn a_variable_display_from_a_rules_property_keeps_it_hidden_too() {
    run(case("rule_variable_hides_hidden_removed"));
}

#[test]
fn a_variable_display_a_script_writes_hides_the_block() {
    run(case("variable_written"));
}

#[test]
fn a_ruby_display_shows_an_inline_box() {
    run(case("ruby_display"));
}

#[test]
fn a_math_display_shows_an_inline_box() {
    run(case("math_display"));
}

#[test]
fn a_block_ruby_display_shows_a_block() {
    run(case("block_ruby_display"));
}

#[test]
fn the_converter_shows_a_ruby_display() {
    run(case("ruby_shown"));
}

#[test]
fn a_flow_display_reads_back_as_block() {
    run(case("flow_read_back"));
}

#[test]
fn an_unpadded_column_passes_on_the_bottom_margin_left_once_its_last_block_hides() {
    run(case("unpadded_column_bottom"));
}

#[test]
fn an_unpadded_column_passes_it_through_its_parent_too() {
    run(case("unpadded_column_bottom_nested"));
}

#[test]
fn a_block_a_class_hides_takes_its_margins_with_it() {
    run(case("class_hides"));
}

#[test]
fn a_first_block_a_class_shows_takes_the_escaping_margin_back() {
    run(case("class_hides_first"));
}

#[test]
fn a_rule_that_hides_the_block_too_keeps_it_hidden_once_its_display_is_cleared() {
    run(case("rule_hides_too"));
}

#[test]
fn a_rule_with_a_variable_display_keeps_it_hidden_too() {
    run(case("rule_variable_hides_too"));
}

#[test]
fn a_class_rule_keeps_it_hidden_too() {
    run(case("class_rule_hides_too"));
}

#[test]
fn a_rule_keeps_a_block_whose_hidden_attribute_is_removed_hidden() {
    run(case("rule_hides_hidden_removed"));
}

#[test]
fn a_varying_bottom_margin_meets_a_varying_top_margin() {
    run(case("varying_edges_meet"));
}

#[test]
fn a_varying_bottom_margin_passes_a_block_hidden_as_loaded() {
    run(case("varying_edge_past_hidden"));
}

#[test]
fn a_varying_bottom_margin_stays_inside_a_padded_parent() {
    run(case("varying_edge_closed"));
}

#[test]
fn a_changed_custom_property_shows_the_block_again() {
    run(case("variable_changed"));
}

#[test]
fn a_parents_custom_property_is_resolved_before_a_child_overrides_what_it_reads() {
    run(case("variable_resolved_first"));
}

#[test]
fn a_custom_property_on_an_inline_ancestor_is_read() {
    run(case("variable_on_inline"));
}

#[test]
fn a_block_whose_hiding_class_is_gone_shows_once_its_display_is_cleared() {
    run(case("class_gone_display_cleared"));
}

#[test]
fn a_variable_display_reads_a_stylesheet_custom_property() {
    run(case("sheet_variable"));
}

#[test]
fn a_scripts_custom_property_wins_over_the_stylesheets_until_removed() {
    run(case("sheet_variable_overridden"));
}

#[test]
fn an_initial_custom_property_gives_way_to_the_fallback() {
    run(case("variable_initial_fallback"));
}

#[test]
fn a_custom_property_a_rule_declares_on_the_child_holds_against_the_ancestors_write() {
    run(case("variable_declared_on_child"));
}

#[test]
fn the_stylesheets_custom_property_comes_back_once_the_attributes_is_removed() {
    run(case("sheet_variable_under_attribute"));
}

#[test]
fn a_name_inside_a_string_in_the_style_attribute_declares_nothing() {
    run(case("attribute_string_holds_a_name"));
}

#[test]
fn a_block_whose_ancestors_hiding_class_is_gone_shows_once_its_display_is_cleared() {
    run(case("ancestor_class_gone_display_cleared"));
}

#[test]
fn a_class_added_after_load_hides_the_block_once_its_display_is_cleared() {
    run(case("class_added_display_cleared"));
}
