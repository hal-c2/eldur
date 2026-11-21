//! Inline elements (and a select's options) are real DOM nodes to a script.
//!
//! The block that holds them paints one `Text`, so they have no QML object;
//! see "Inline nodes" in `docs/spec/dom-api.md`. Each page in `tests/inline_dom/`
//! is converted and run through qmltestrunner, pressing the pixels Chromium's
//! glyphs were at; what its script logged (lines starting `LC `) is compared
//! with what Chromium logged, recorded in `tests/inline_dom/chromium.json` by
//! `node crates/eldur_convert/tests/inline_dom/gen.mjs`. A QML warning or an
//! uncaught script error fails the page. A page may also name texts it must
//! draw, in order: what each visible `Text` shows once the script and clicks
//! are done; and, for an element (by id), texts drawn inside it and texts
//! it must not draw.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, convert_str};

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

    function drawnIn(item, under) {
        if (!item.visible || item.opacity === 0) {
            return
        }
        const names = item.objectName ? under.concat([item.objectName]) : under
        if (item.lineCount !== undefined && typeof item.text === "string" && item.text !== "") {
            const text = item.text.replace(/<[^>]*>/g, "")
            console.log("DRAWN " + text)
            for (const name of names) {
                console.log("DRAWNIN " + name + " " + text)
            }
        }
        for (let i = 0; i < item.children.length; ++i) {
            drawnIn(item.children[i], names)
        }
    }

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        for (const [x, y] of CLICKS) {
            if (typeof x === "string") {
                const named = { Tab: Qt.Key_Tab, Enter: Qt.Key_Return, ArrowDown: Qt.Key_Down, ArrowUp: Qt.Key_Up }
                keyClick(named[x] !== undefined ? named[x] : x)
            } else {
                mouseClick(page, x, y)
            }
            wait(50)
        }
        waitForRendering(page)
        drawnIn(page, [])
    }
}
}
"##;

/// What one element draws: texts it must draw, in order, and texts it must not.
type Within<'a> = (&'a str, &'a [&'a str], &'a [&'a str]);

/// Runs one page; `Err` has what went wrong.
fn run(
    name: &str,
    html: &str,
    want: &[&str],
    clicks: &str,
    drawn: &[&str],
    within: &[Within],
) -> Result<(), String> {
    if support::no_qt() {
        return Ok(());
    }
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
    let output = support::qmltestrunner()
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
    let mut problems: Vec<String> = support::problem_lines(&text, |l| {
        l.contains("FAIL!") || l.contains("Warning:") || l.contains("QWARN") || l.contains("Error:")
    });
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
    let drew: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("DRAWN ").map(|at| &l[at + 6..]))
        .collect();
    let mut from = 0;
    for want in drawn {
        match drew[from..].iter().position(|d| d == want) {
            Some(at) => from += at + 1,
            None => {
                problems.push(format!(
                    "draws no {want:?} after what came before; drew {drew:?}"
                ));
                break;
            }
        }
    }
    for (id, must, never) in within {
        let prefix = format!("DRAWNIN {id} ");
        let drew_in: Vec<_> = text
            .lines()
            .filter_map(|l| l.find(&prefix).map(|at| &l[at + prefix.len()..]))
            .collect();
        let mut from = 0;
        for want in *must {
            match drew_in[from..].iter().position(|d| d == want) {
                Some(at) => from += at + 1,
                None => {
                    problems.push(format!(
                        "#{id} draws no {want:?} in order; drew {drew_in:?}"
                    ));
                    break;
                }
            }
        }
        for not in *never {
            if drew_in.iter().any(|d| d.contains(not)) {
                problems.push(format!("#{id} draws {not:?}; drew {drew_in:?}"));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{name}:\n  {}", problems.join("\n  ")))
    }
}

fn page(name: &str) {
    page_drawing(name, &[]);
}

/// `page`, and the page draws `drawn` in that order, others between allowed.
fn page_drawing(name: &str, drawn: &[&str]) {
    page_drawing_within(name, drawn, &[]);
}

/// `page_drawing`, and each element named in `within` draws what it says
/// inside it, and nothing it rules out.
fn page_drawing_within(name: &str, drawn: &[&str], within: &[Within]) {
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
    if let Err(e) = run(name, &html, &want, &clicks, drawn, within) {
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
fn a_block_put_beside_a_line_of_nodes_is_in_its_elements_text_and_markup() {
    page("beside_model");
}

// #18: the text the converter wrote on the element is one of its children.
#[test]
fn replace_children_replaces_the_text_the_page_was_written_with() {
    page("replace");
}

#[test]
fn a_control_in_an_inline_element_is_its_child() {
    page("cut_boxes");
}

#[test]
fn a_buttons_text_content_is_its_dom_text_not_what_it_draws() {
    page("button_text");
}

// #8: a text-only button holds its text as a Text node. Its text written
// whole, a write through a node it had leaves the label ("New") alone.
#[test]
fn a_text_only_buttons_text_is_a_node_it_draws() {
    page_drawing(
        "button_nodes",
        &[
            "Saved", "Hi Save", "Back", "Load?", "New", "Save", "Save!", "x", "A+B", "AB",
        ],
    );
}

// #6: a control or block the page made goes into the inline element.
#[test]
fn a_box_the_page_appends_to_an_inline_element_goes_into_it() {
    page("boxes_into_inline");
}

// #6: appended with no reference node to a box with boxes after its text or
// its lines, text, an inline element and a block go below those boxes; with
// none after them, an inline element goes on the line.
#[test]
fn what_is_appended_after_a_box_s_boxes_goes_below_them() {
    page("append_after_boxes");
}

// #6: a page Text node a script moves into a column of boxes leaves the
// element it came from, and the column reads it, and a box put before its own
// text, in DOM order.
#[test]
fn text_moved_among_a_column_s_boxes_reads_in_dom_order() {
    page("text_into_boxes");
}

// #6: a removed element's boxes come back with it, and a write to a text
// beside a box keeps the box between its texts.
#[test]
fn boxes_come_back_with_the_element_that_held_them() {
    page_drawing("reinsert_boxes", &["x zz", "K", "b y"]);
}

// #23: a created span's text is its own Text node.
#[test]
fn a_created_spans_text_is_its_own_node() {
    page("created_span_text");
}

// #23: markup read on an element holding a created span and a box has both.
#[test]
fn inner_html_has_created_spans_and_boxes() {
    page("model_inner_html");
}

#[test]
fn a_custom_elements_constructor_finds_the_select_inside_its_label() {
    page("ce_select");
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

// #23: what the drive drew differently from Chromium: a span before a
// button's text, an emptied button, whitespace runs, markup in a button, a
// disabled fieldset's legend, a line after a paragraph, a span moved to the
// body; then a button given markup and text in turn, a button's own Text
// appended to it again, and long text put in a legend.
#[test]
fn the_drives_drawn_gaps_match_chromium() {
    let words = "word ".repeat(30);
    let after_box = words.as_str();
    let after_text = format!("w {}", words.trim_end());
    page_drawing(
        "drawn_gaps",
        &[
            "GoSave",
            "g bold ",
            "Go",
            " now",
            "l ",
            "one",
            "yz",
            "DDD",
            "B2",
            "Save",
            &after_text,
            "v ",
            after_box,
            "end",
            "moved",
        ],
    );
}

// A legend's own width, min-width and max-width hold in a narrower fieldset;
// with none it wraps at the fieldset's content width.
#[test]
fn a_legend_keeps_its_width_bounds_as_chromium_does() {
    page_drawing(
        "legend_sizes",
        &[
            "Wide",
            "Min",
            "Max legend text",
            "Both",
            "a long legend that wraps",
        ],
    );
}

#[test]
fn a_button_holding_lines_keeps_them_as_chromium_does() {
    // What a button holds but does not draw stays undrawn once it holds lines.
    page_drawing_within(
        "button_lines",
        &["A", "x", "Go", " many words that wrap", "Go", " on"],
        &[("bh", &["x"], &["hidden"]), ("bm", &["B", "A"], &["H"])],
    );
}
