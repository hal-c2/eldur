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

/// The toggle every page has; the issue's own page is the first one.
const TOGGLE: &str = r#"<button id="t">Toggle</button>
<script>
document.getElementById('t').addEventListener('click', () => {
  const s = document.getElementById('shy');
  s.style.display = s.style.display === 'none' ? '' : 'none';
});
</script>"#;

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
    },
    // Hidden as loaded, then shown: the same page the other way round.
    Case {
        name: "first_shows",
        body: r#"<p id="shy" style="display:none"><em>Shy</em></p>
<p id="after">After</p>"#,
        ids: &["after", "t"],
        load: &[16.0, 50.0],
        toggled: &[50.0, 84.0],
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
    },
    // The margins escape through `#w` too, where its own 20 px waits for them.
    Case {
        name: "escapes_through_two",
        body: r#"<body style="margin:0"><div id="w" style="margin-top:20px"><p id="shy" style="margin:30px 0"><em>Shy</em></p>
<p id="after" style="margin:10px 0">After</p></div>"#,
        ids: &["w", "after"],
        load: &[30.0, 78.0],
        toggled: &[20.0, 20.0],
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
    },
    // An empty block after it: hidden or not, the block's margins and the
    // empty one's collapse with the next one's.
    Case {
        name: "empty_after",
        body: r#"<div style="padding:8px 0"><p id="shy" style="margin:0 0 16px">S</p><div id="e" style="margin:30px 0"></div><p id="n" style="margin:8px 0">N</p></div>"#,
        ids: &["n"],
        load: &[64.0],
        toggled: &[46.0],
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
    },
];

fn page(case: &Case) -> String {
    let named = case
        .ids
        .iter()
        .map(|id| format!("document.getElementById('{id}')"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "<!DOCTYPE html>\n{}\n{TOGGLE}\n<script>\n// Named so that they get an objectName the test can find.\nvar named = [{named}];\n</script>\n",
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
        expect([{ids}], {load}, "toggled back")
        button.clicked()
        expect([{toggled_ids}], {toggled}, "toggled again")
    }}
}}
}}
"#,
        name = case.name,
        load = list(case.load),
        toggled = list(case.toggled),
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
