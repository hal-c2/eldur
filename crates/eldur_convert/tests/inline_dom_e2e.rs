//! Inline elements (and a select's options) are real DOM nodes to a script.
//!
//! The block that holds them paints one `Text`, so they have no QML object;
//! see "Inline nodes" in `docs/spec/dom-api.md`. Each page in `tests/inline_dom/`
//! is converted and run through qmltestrunner, pressing the pixels Chromium's
//! glyphs were at; what its script logged (lines starting `LC `) is compared
//! with what Chromium logged, recorded in `tests/inline_dom/chromium.json` by
//! `node crates/eldur_convert/tests/inline_dom/gen.mjs`. A QML warning or an
//! uncaught script error fails the page.

use std::path::Path;
use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineDom"
    when: windowShown

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        for (const [x, y] of CLICKS) {
            if (typeof x === "string") {
                const named = { Tab: Qt.Key_Tab, Enter: Qt.Key_Return, ArrowDown: Qt.Key_Down }
                keyClick(named[x] !== undefined ? named[x] : x)
            } else {
                mouseClick(page, x, y)
            }
            wait(50)
        }
    }
}
}
"##;

/// Runs one page; `Err` has what went wrong.
fn run(name: &str, html: &str, want: &[&str], clicks: &str) -> Result<(), String> {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-idom-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_inline.qml"), TEST.replace("CLICKS", clicks)).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_inline.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let mut problems: Vec<String> = text
        .lines()
        .filter(|l| {
            l.contains("FAIL!")
                || l.contains("Warning:")
                || l.contains("QWARN")
                || l.contains("Error:")
        })
        .map(str::to_owned)
        .collect();
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("LC ").map(|at| &l[at..]))
        .collect();
    for i in 0..logged.len().max(want.len()) {
        let (got, chromium) = (logged.get(i), want.get(i));
        if got != chromium {
            problems.push(format!(
                "line {i}\n    got      {}\n    Chromium {}",
                got.unwrap_or(&"(nothing)"),
                chromium.unwrap_or(&"(nothing)"),
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{name}:\n  {}", problems.join("\n  ")))
    }
}

fn page(name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/inline_dom");
    let chromium: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("chromium.json")).expect("json"))
            .expect("chromium.json");
    let html = std::fs::read_to_string(dir.join(format!("{name}.html"))).expect("page");
    let want: Vec<&str> = chromium[name]["log"]
        .as_array()
        .expect("a log for every page")
        .iter()
        .map(|l| l.as_str().expect("a line"))
        .collect();
    let clicks: Vec<String> = chromium[name]["clicks"]
        .as_array()
        .expect("clicks for every page")
        .iter()
        .map(|c| match c.get("key") {
            Some(key) => format!("[{key}]"),
            None => format!("[{},{}]", c["x"], c["y"]),
        })
        .collect();
    let clicks = format!("[{}]", clicks.join(","));
    if let Err(e) = run(name, &html, &want, &clicks) {
        panic!("{e}");
    }
}

#[test]
fn lookups_and_tree_reads_match_chromium() {
    page("lookups");
}

#[test]
fn text_class_style_and_attribute_writes_match_chromium() {
    page("writes");
}

#[test]
fn clicks_on_glyphs_match_chromium() {
    page("clicks");
}

#[test]
fn created_and_removed_inline_nodes_match_chromium() {
    page("mutate");
}

#[test]
fn options_are_nodes_of_their_select() {
    page("options");
}

#[test]
fn bubbling_runs_from_the_target_up_and_capture_from_the_root_down() {
    page("ev_order");
}

#[test]
fn stop_propagation_and_prevent_default_at_each_level() {
    page("ev_stop");
}

#[test]
fn delegation_through_closest_reaches_controls_with_no_listener() {
    page("ev_delegate");
}

#[test]
fn attribute_property_and_listener_handlers_share_one_dispatch() {
    page("ev_static");
}

#[test]
fn a_page_with_only_attributes_bubbles_through_ancestors() {
    page("ev_static_only");
}

#[test]
fn form_events_bubble() {
    page("ev_forms");
}

fn notes(html: &str) -> Vec<String> {
    convert_str(html, &Options::default())
        .diagnostics
        .into_iter()
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_class_on_an_inline_node_is_styled_so_no_note_says_otherwise() {
    let html = r#"<!doctype html><style>.done{color:red}</style>
<p>one <span id="s">two</span></p><button id="b">b</button>
<script>document.getElementById("b").onclick=function(){
document.getElementById("s").classList.add("done")}</script>"#;
    let notes = notes(html);
    assert!(
        !notes.iter().any(|n| n.contains("styles under class")),
        "{notes:?}"
    );
}

#[test]
fn a_class_with_no_node_to_style_keeps_its_note() {
    let html = r#"<!doctype html><style>a.on{color:red}a:hover{color:blue}</style>
<p>one <a class="on" href="/x">two</a></p>"#;
    let notes = notes(html);
    assert!(notes.iter().any(|n| n.contains("`:hover`")), "{notes:?}");
}

#[test]
fn a_created_span_in_a_line_with_rules_is_a_styled_node() {
    let html = r#"<!doctype html><style>.n{color:red}</style>
<p id="line">one</p><button id="b">b</button>
<script>document.getElementById("b").onclick=function(){
var s=document.createElement("span");s.className="n";s.textContent="x";
document.getElementById("line").appendChild(s)}</script>"#;
    let notes = notes(html);
    assert!(
        !notes.iter().any(|n| n.contains("a created <span>")),
        "{notes:?}"
    );
}

#[test]
fn a_created_element_that_stays_an_object_keeps_its_note() {
    // `display` makes it inline for the probe; the runtime goes by the tag.
    let html = r#"<!doctype html><style>section{display:inline;color:red}</style>
<p id="line">one</p><button id="b">b</button>
<script>document.getElementById("b").onclick=function(){
var s=document.createElement("section");s.textContent="x";
document.getElementById("line").appendChild(s)}</script>"#;
    // A template now stands apart from the line its probe sat on, so this one
    // has an item and nothing to note.
    assert!(
        !notes(html)
            .iter()
            .any(|n| n.contains("has no item of its own")),
    );
    // A control has no place for an element's item, so there is still no template.
    let html = html.replace(
        r#"<p id="line">one</p>"#,
        r#"<button id="line">one</button>"#,
    );
    let notes = notes(&html);
    assert!(
        notes.iter().any(|n| n.contains("a created <section>")),
        "{notes:?}"
    );
}

#[test]
fn submit_reaches_ancestors_of_a_form_with_no_listener() {
    page("ev_submit");
}

#[test]
fn change_comes_only_when_the_text_changed() {
    page("ev_change");
}

#[test]
fn a_scripted_click_bubbles() {
    page("ev_scripted");
}
