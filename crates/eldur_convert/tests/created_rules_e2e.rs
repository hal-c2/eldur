//! Elements a script creates behave like the same elements written in the HTML:
//! every kind of selector reaches them, and they lay out and position as those would.
//!
//! Most pages come twice: `closed_*`, whose script the analysis can follow, and
//! `open_*`, the same script after one `createElement` it cannot, so the two ways a
//! created element gets its style answer the same.
//!
//! Each page in `tests/created_rules/` is converted and run through
//! qmltestrunner; what its script logged (lines starting `CS `, box geometry and
//! computed styles) is compared with what Chromium logged, recorded in
//! `tests/created_rules/chromium.json` by
//! `node crates/eldur_convert/tests/created_rules/gen.mjs`. A QML warning or an
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
    name: "CreatedRules"
    when: windowShown

    // A page whose logging runs in timers says it has finished by making `#csdone`.
    function named(item, name) {
        if (item.objectName === name)
            return true
        for (const kid of item.children) {
            if (named(kid, name))
                return true
        }
        return false
    }

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        if (TIMED) tryVerify(() => named(page, "csdone"), 30000, "the page's timers ran")
        for (const [x, y] of CLICKS) {
            if (typeof x === "string") {
                const named = { Tab: Qt.Key_Tab, Enter: Qt.Key_Return, ArrowDown: Qt.Key_Down }
                keyClick(named[x] !== undefined ? named[x] : x)
            } else {
                mouseClick(page, x, y)
            }
            wait(50)
        }
        if (PROBES.length > 0) {
            wait(50)
            const image = grabImage(page)
            for (const probe of PROBES) {
                const got = image.pixel(probe.x, probe.y)
                verify(Qt.colorEqual(got, probe.color),
                    "painted " + probe.id + ": " + got + " is not " + probe.color)
            }
        }
    }
}
}
"##;

/// `(page, Chromium's line, ours, why)`: what is known to differ, in full, so a
/// changed difference and one that went away both fail.
const KNOWN: &[(&str, &str, &str, &str)] = &[
    (
        "rule_probe_crt2",
        "CS a2 143.328125,8,129.328125,56",
        "CS a2 143,8,130,56",
        "a static grid no script fills is a GridLayout, which gives its 1fr tracks whole \
     pixels (129, 129, 130 of 388); it was so before created elements were laid out \
     by the runtime, and a page with no script keeps its converted QML",
    ),
    (
        "rule_table_widths",
        "CS t6c00 9,324.5,225.078125,21",
        "CS t6c00 9,325,226,21",
        "the table solver (`tableColumnWidths`, `tableRowHeights`) gives columns and rows \
     whole pixels so cell borders are drawn sharp: Chromium's 225.08px column becomes \
     226, and the 21.5px head row above it 22",
    ),
    (
        "rule_fallback_lines",
        "CS g 8,110,1264,22",
        "CS g 8,110,1264,25",
        "fontconfig gives Qt and Chromium different faces for a serif line's tick: \
     Qt draws it from a Noto CJK face (ascent 1.16em), Chromium from a monospace \
     one (1.02em); the line is as tall as the face Qt draws, and QML can't name \
     the fallback (no `font.families`)",
    ),
    (
        "rule_fallback_lines",
        "CS end 8,132,1264,4",
        "CS end 8,135,1264,4",
        "below the serif line above, which is 3px taller",
    ),
    (
        "rule_pseudo_boxes",
        "CS b1 8,44,58.25,20",
        "CS b1 8,44,59,20",
        "a text box is as wide as `Math.ceil` of its implicit width, so a wrapping Text \
     keeps its last word on the line: the inline-block's 34.25px text is 35",
    ),
    (
        "rule_pseudo_boxes",
        "CS b3 70.140625,46,23.359375,16",
        "CS b3 70.875,45.999999,23.267578125,16",
        "follows the inline-block before it on the line, 0.75px wider (above)",
    ),
    (
        "rule_table_row_added",
        "CS hq 207.5,8.5,120,23",
        "CS hq 207,8,121,23",
        "a written collapsed table's second-column cell holds the whole inner border it \
     draws, on the pixel a browser paints it, and the table's whole outer border: a \
     browser's cell box runs from the middle of each, half a pixel in on both sides",
    ),
    (
        "rule_table_row_added",
        "CS r0b 207.5,31.5,120,23",
        "CS r0b 207,31,121,23",
        "a written collapsed table's second-column cell holds the whole inner border it \
     draws, on the pixel a browser paints it, and the table's whole outer border: a \
     browser's cell box runs from the middle of each, half a pixel in on both sides",
    ),
    (
        "rule_table_row_added",
        "CS r1b 207.5,54.5,120,23",
        "CS r1b 207,54,121,23",
        "a written collapsed table's second-column cell holds the whole inner border it \
     draws, on the pixel a browser paints it, and the table's whole outer border: a \
     browser's cell box runs from the middle of each, half a pixel in on both sides",
    ),
    (
        "rule_table_row_added",
        "CS r2b 207.5,77.5,120,23",
        "CS r2b 207,77,121,23",
        "a written collapsed table's second-column cell holds the whole inner border it \
     draws, on the pixel a browser paints it, and the table's whole outer border: a \
     browser's cell box runs from the middle of each, half a pixel in on both sides",
    ),
    (
        "rule_table_row_added",
        "CS tot 207.5,77.5,120,23",
        "CS tot 207,77,121,24",
        "as the second-column cells above, and a written collapsed table keeps its rows on \
     whole pixels: its last row holds the table's outer half border as well",
    ),
    (
        "rule_table_row_added",
        "CS tot 207.5,100.5,120,23",
        "CS tot 207,100,121,24",
        "as the second-column cells above, and a written collapsed table keeps its rows on \
     whole pixels: its last row holds the table's outer half border as well",
    ),
    (
        "made_lines_chat",
        "CS H div8 15,148.96875,360,31.59375",
        "CS H div8 15,147.9687,360,31.5938",
        "on a page the analysis follows, the made bubbles are the converter's templates \
     in the log's ColumnLayout, and Qt's layouts put their items on whole pixels: \
     the third bubble sits at 133 where the sum is 134.0 (4 + 51.2 + 4 + 70.8 + 4); \
     the open page lays the same bubbles out with the rules and matches",
    ),
    (
        "made_lines_chat",
        "CS H b9 25,155.96875,17.90625,16",
        "CS H b9 25,154.968748,17.875,16",
        "in the third bubble, which is 1px high (above)",
    ),
    (
        "made_lines_chat",
        "CS H a10 65.484375,155.96875,28.03125,16",
        "CS H a10 65.5,154.968748,27.875,16",
        "in the third bubble, which is 1px high (above)",
    ),
    (
        "made_lines_chat",
        "CS H code11 124.65625,156.96875,13.203125,16",
        "CS H code11 124.515625,155.968748,13.1875,16",
        "in the third bubble, which is 1px high (above)",
    ),
    (
        "made_lines_log",
        "CS H span1 14,31,397.8125,34",
        "CS H span1 14,31,304,34",
        "Qt breaks a `pre-wrap` line after the URL's `/` (UAX #14 allows it), Chromium \
     keeps the URL whole on the first line; written spans in the same pane break \
     the same way (measured on the written copy of this page)",
    ),
    (
        "made_lines_log_open",
        "CS H span1 14,31,397.8125,34",
        "CS H span1 14,31,304,34",
        "Qt breaks a `pre-wrap` line after the URL's `/` (UAX #14 allows it), Chromium \
     keeps the URL whole on the first line; written spans in the same pane break \
     the same way (measured on the written copy of this page)",
    ),
    (
        "made_lines_indent",
        "CS H b1 38,11,37.5,17",
        "CS H b1 8,11,37.375,17",
        "`text-indent` is not converted, for written paragraphs either (the converter \
     reports it): the first line starts 30px left of Chromium's, and the second \
     line's break moves with it",
    ),
    (
        "made_lines_indent",
        "CS H a2 166.421875,59,35.03125,17",
        "CS H a2 141.25,59,34.875,17",
        "on the second line, which breaks with the first line's missing indent",
    ),
    (
        "made_lines_indent_open",
        "CS H b1 38,11,37.5,17",
        "CS H b1 8,11,37.375,17",
        "`text-indent` is not converted, for written paragraphs either (the converter \
     reports it): the first line starts 30px left of Chromium's, and the second \
     line's break moves with it",
    ),
    (
        "made_lines_indent_open",
        "CS H a2 166.421875,59,35.03125,17",
        "CS H a2 141.25,59,34.875,17",
        "on the second line, which breaks with the first line's missing indent",
    ),
];

/// Pixels a logged number may be off: glyph widths differ a little between the two
/// text engines. Whole numbers rounded by a page's helper are held exactly.
const NUMBER_TOLERANCE: f64 = 0.5;

/// Same text between the numbers, and each number within [`NUMBER_TOLERANCE`].
fn same_line(got: &str, chromium: &str) -> bool {
    fn parts(line: &str) -> Vec<&str> {
        let number = |c: char| c.is_ascii_digit() || c == '.' || c == '-';
        let mut out = Vec::new();
        let mut rest = line;
        while let Some(c) = rest.chars().next() {
            let run = rest
                .find(|d: char| number(d) != number(c))
                .unwrap_or(rest.len());
            out.push(&rest[..run]);
            rest = &rest[run..];
        }
        out
    }
    let (got, chromium) = (parts(got), parts(chromium));
    got.len() == chromium.len()
        && got.iter().zip(&chromium).all(|(a, b)| {
            a == b
                || matches!((a.parse::<f64>(), b.parse::<f64>()),
                    (Ok(x), Ok(y)) if (x - y).abs() <= NUMBER_TOLERANCE
                        && (a.contains('.') || b.contains('.')))
        })
}

/// Runs one page; `Err` has what went wrong.
fn run(name: &str, html: &str, want: &[&str], clicks: &str, probes: &str) -> Result<(), String> {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-rules-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_rules.qml"),
        TEST.replace("CLICKS", clicks)
            .replace("PROBES", probes)
            .replace(
                "TIMED",
                if html.contains("csdone") {
                    "true"
                } else {
                    "false"
                },
            ),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_rules.qml"))
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
        .filter_map(|l| l.find("CS ").map(|at| &l[at..]))
        .collect();
    let known: Vec<_> = KNOWN.iter().filter(|k| k.0 == name).collect();
    for k in &known {
        if !want.contains(&k.1) || !logged.contains(&k.2) {
            problems.push(format!("known difference is gone: {}", k.2));
        }
    }
    for i in 0..logged.len().max(want.len()) {
        let (got, chromium) = (logged.get(i), want.get(i));
        let listed = known
            .iter()
            .any(|k| chromium == Some(&k.1) && got == Some(&k.2));
        let same = matches!((got, chromium), (Some(a), Some(b)) if same_line(a, b));
        if !same && !listed {
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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/created_rules");
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
    let probes = chromium[name]["probes"].to_string();
    if let Err(e) = run(name, &html, &want, &clicks, &probes) {
        panic!("{e}");
    }
}

#[test]
fn closed_abs_host_abs() {
    page("closed_abs_host_abs");
}

#[test]
fn closed_abs_in_created_host() {
    page("closed_abs_in_created_host");
}

#[test]
fn closed_abs_in_static_host() {
    page("closed_abs_in_static_host");
}

#[test]
fn closed_abs_inner_html() {
    page("closed_abs_inner_html");
}

#[test]
fn closed_attr() {
    page("closed_attr");
}

#[test]
fn closed_attr_toggle() {
    page("closed_attr_toggle");
}

#[test]
fn closed_card_inner_id() {
    page("closed_card_inner_id");
}

#[test]
fn closed_control_pos() {
    page("closed_control_pos");
}

#[test]
fn closed_control_pos_created() {
    page("closed_control_pos_created");
}

#[test]
fn closed_control_pos_rule() {
    page("closed_control_pos_rule");
}

#[test]
fn closed_descendant() {
    page("closed_descendant");
}

#[test]
fn closed_fixed_relative() {
    page("closed_fixed_relative");
}

#[test]
fn closed_flex_stretch() {
    page("closed_flex_stretch");
}

#[test]
fn closed_grid_stretch() {
    page("closed_grid_stretch");
}

#[test]
fn closed_id_after() {
    page("closed_id_after");
}

#[test]
fn closed_id_before() {
    page("closed_id_before");
}

#[test]
fn closed_id_change() {
    page("closed_id_change");
}

#[test]
fn closed_id_descendant() {
    page("closed_id_descendant");
}

#[test]
fn closed_id_dynamic() {
    page("closed_id_dynamic");
}

#[test]
fn closed_id_set_attribute() {
    page("closed_id_set_attribute");
}

#[test]
fn closed_id_static_ancestor() {
    page("closed_id_static_ancestor");
}

#[test]
fn closed_moved_between() {
    page("closed_moved_between");
}

#[test]
fn closed_sibling() {
    page("closed_sibling");
}

#[test]
fn closed_structural() {
    page("closed_structural");
}

#[test]
fn closed_text_inherit() {
    page("closed_text_inherit");
}

#[test]
fn grid_stretch_markup() {
    page("grid_stretch_markup");
}

#[test]
fn open_abs_host_abs() {
    page("open_abs_host_abs");
}

#[test]
fn open_abs_in_created_host() {
    page("open_abs_in_created_host");
}

#[test]
fn open_abs_in_static_host() {
    page("open_abs_in_static_host");
}

#[test]
fn open_abs_inner_html() {
    page("open_abs_inner_html");
}

#[test]
fn open_attr() {
    page("open_attr");
}

#[test]
fn open_attr_toggle() {
    page("open_attr_toggle");
}

#[test]
fn open_card_inner_id() {
    page("open_card_inner_id");
}

#[test]
fn open_control_pos() {
    page("open_control_pos");
}

#[test]
fn open_control_pos_created() {
    page("open_control_pos_created");
}

#[test]
fn open_control_pos_rule() {
    page("open_control_pos_rule");
}

#[test]
fn open_descendant() {
    page("open_descendant");
}

#[test]
fn open_fixed_relative() {
    page("open_fixed_relative");
}

#[test]
fn open_flex_stretch() {
    page("open_flex_stretch");
}

#[test]
fn open_grid_stretch() {
    page("open_grid_stretch");
}

#[test]
fn open_id_after() {
    page("open_id_after");
}

#[test]
fn open_id_before() {
    page("open_id_before");
}

#[test]
fn open_id_change() {
    page("open_id_change");
}

#[test]
fn open_id_descendant() {
    page("open_id_descendant");
}

#[test]
fn open_id_dynamic() {
    page("open_id_dynamic");
}

#[test]
fn open_id_set_attribute() {
    page("open_id_set_attribute");
}

#[test]
fn open_id_static_ancestor() {
    page("open_id_static_ancestor");
}

#[test]
fn open_moved_between() {
    page("open_moved_between");
}

#[test]
fn open_sibling() {
    page("open_sibling");
}

#[test]
fn open_structural() {
    page("open_structural");
}

#[test]
fn open_text_inherit() {
    page("open_text_inherit");
}

#[test]
fn sib_static_style_pos() {
    page("sib_static_style_pos");
}

#[test]
fn sib_static_class_abs() {
    page("sib_static_class_abs");
}

#[test]
fn sib_grid_align_center() {
    page("sib_grid_align_center");
}

#[test]
fn sib_grid_align_end() {
    page("sib_grid_align_end");
}

#[test]
fn sib_grid_align_self() {
    page("sib_grid_align_self");
}

#[test]
fn sib_grid_mixed() {
    page("sib_grid_mixed");
}

#[test]
fn sib_box_paint() {
    page("sib_box_paint");
}

#[test]
fn sib_item_margin() {
    page("sib_item_margin");
}

#[test]
fn sib_flex_mixed() {
    page("sib_flex_mixed");
}

#[test]
fn sib_flex_column_stretch() {
    page("sib_flex_column_stretch");
}

#[test]
fn sib_id_on_static() {
    page("sib_id_on_static");
}

#[test]
fn sib_inner_span_id() {
    page("sib_inner_span_id");
}

#[test]
fn sib_inner_span_class() {
    page("sib_inner_span_class");
}

#[test]
fn sib_inner_span_after() {
    page("sib_inner_span_after");
}

#[test]
fn sib_literal_grid() {
    page("sib_literal_grid");
}

#[test]
fn sib_literal_flex() {
    page("sib_literal_flex");
}

#[test]
fn sib_literal_abs() {
    page("sib_literal_abs");
}

#[test]
fn sib_literal_fixed() {
    page("sib_literal_fixed");
}

#[test]
fn sib_literal_child() {
    page("sib_literal_child");
}

#[test]
fn rule_probe_crt() {
    page("rule_probe_crt");
}

#[test]
fn rule_probe_crt2() {
    page("rule_probe_crt2");
}

#[test]
fn rule_bare_ids() {
    page("rule_bare_ids");
}

#[test]
fn rule_helper_selectors() {
    page("rule_helper_selectors");
}

#[test]
fn rule_created_floats() {
    page("rule_created_floats");
}

#[test]
fn rule_created_float_wrap() {
    page("rule_created_float_wrap");
}

#[test]
fn rule_hidden_nav() {
    page("rule_hidden_nav");
}

#[test]
fn rule_literal_helpers() {
    page("rule_literal_helpers");
}

#[test]
fn rule_computed_static() {
    page("rule_computed_static");
}

#[test]
fn rule_computed_mixed() {
    page("rule_computed_mixed");
}

#[test]
fn rule_empty_found() {
    page("rule_empty_found");
}

#[test]
fn rule_float_margins_created() {
    page("rule_float_margins_created");
}

#[test]
fn rule_float_margins_static() {
    page("rule_float_margins_static");
}

#[test]
fn rule_probe_crt4() {
    page("rule_probe_crt4");
}

#[test]
fn rule_table_widths() {
    page("rule_table_widths");
}

#[test]
fn rule_css_text() {
    page("rule_css_text");
}

#[test]
fn rule_badge_in_text() {
    page("rule_badge_in_text");
}

#[test]
fn rule_child_nodes() {
    page("rule_child_nodes");
}

/// `counter(list-item)` from the lists' own numbering (an `<ol start>` and a
/// nested `<ol>` with an unrelated `counter-reset` included), and counters a
/// `::before` changes itself, as items are added and moved.
#[test]
fn rule_counters_own() {
    page("rule_counters_own");
}

/// `::before` and `::after` that are boxes: an inline-block icon on the line, an
/// absolutely positioned badge (which gives its line no baseline), a block
/// heading and a clearfix table.
#[test]
fn rule_pseudo_boxes() {
    page("rule_pseudo_boxes");
}

/// A form adds a row to a written collapsed table and the rows are sorted: the
/// made row's cells, which draw all four borders, measure as the written ones.
#[test]
fn rule_table_row_added() {
    page("rule_table_row_added");
}

/// Classes give and take `::before`/`::after` boxes at run time: an icon goes
/// from a static span, a block heading comes and goes on a box, a tooltip and a
/// required-field asterisk appear, and a created span keeps its icon on its line.
#[test]
fn rule_pseudo_class_change() {
    page("rule_pseudo_class_change");
}

/// The same class changes on a page whose script makes nothing the templates
/// cannot: the boxes the classes give still come and go.
#[test]
fn rule_pseudo_class_closed() {
    page("rule_pseudo_class_closed");
}

/// Floats a script makes in blocks that are no formatting context of their own
/// go on past them, beside the floats and lines after; a clearfix the script
/// makes holds its floats side by side.
#[test]
fn rule_floats_escape() {
    page("rule_floats_escape");
}

/// Space beside inline content: a span's margins and a separator's `::after`
/// margins move what follows, a button's `::before` icon sits in its label,
/// and an absolute box that may not wrap keeps its text on one line however
/// narrow what holds it.
#[test]
fn rule_inline_space() {
    page("rule_inline_space");
}

/// Separators a structural selector gives created links (`a+a::before`) keep
/// their margins as links come and go, a checklist's `::before` tick moves
/// between created labels, and a written label gains a required mark.
#[test]
fn rule_run_time_separators() {
    page("rule_run_time_separators");
}

/// A line whose text, written or generated, falls back on another face for a
/// tick or a cross is as tall as that face makes it under `line-height: normal`.
#[test]
fn rule_fallback_lines() {
    page("rule_fallback_lines");
}

/// Created inline content gets the whole cascade: pills with padding, border
/// and radius, `.j span` padding, `display: inline` list items whose
/// `:last-child::after` and `:nth-child` follow appends and removals, a button
/// whose `::before` icon sits beside its label, and empty `::after` boxes whose
/// margins take room.
#[test]
fn rule_inline_lists_buttons() {
    page("rule_inline_lists_buttons");
}

/// `content: ""` with margins, or padding and a background, makes a box on the
/// line, on written spans and on spans a script appends to a line of runs.
#[test]
fn rule_empty_generated_boxes() {
    page("rule_empty_generated_boxes");
}

/// Class changes, removals and appends on inline elements in lines of runs: a
/// separator `::after` leaves with its link, a required mark with its class, a
/// `::before` badge moves to the span that gains it, structural rules re-match
/// a list's new last item, and a created button gets its pseudo box once.
#[test]
fn rule_inline_class_changes() {
    page("rule_inline_class_changes");
}

/// Chat messages: a `<b>` name, text, an `<a>` link and padded `<code>` in each made bubble.
#[test]
fn made_lines_chat() {
    page("made_lines_chat");
}

/// Chat messages: a `<b>` name, text, an `<a>` link and padded `<code>` in each made bubble. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_chat_open() {
    page("made_lines_chat_open");
}

/// Search results: `<mark>` in wrapped snippets, and a made `<li>`'s link on its line.
#[test]
fn made_lines_search() {
    page("made_lines_search");
}

/// Search results: `<mark>` in wrapped snippets, and a made `<li>`'s link on its line. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_search_open() {
    page("made_lines_search_open");
}

/// `innerHTML`: a centred paragraph with a padded, rounded inline-block badge, `<em>`, and a long word under `overflow-wrap: anywhere`.
#[test]
fn made_lines_card() {
    page("made_lines_card");
}

/// `innerHTML`: a centred paragraph with a padded, rounded inline-block badge, `<em>`, and a long word under `overflow-wrap: anywhere`. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_card_open() {
    page("made_lines_card_open");
}

/// A written `white-space: pre-wrap` pane takes made spans one line each, and a text node after them.
#[test]
fn made_lines_log() {
    page("made_lines_log");
}

/// A written `white-space: pre-wrap` pane takes made spans one line each, and a text node after them. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_log_open() {
    page("made_lines_log_open");
}

/// A made paragraph's text node set longer, then shorter: its lines are cut again each time.
#[test]
fn made_lines_swap() {
    page("made_lines_swap");
}

/// A made paragraph's text node set longer, then shorter: its lines are cut again each time. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_swap_open() {
    page("made_lines_swap_open");
}

/// `vertical-align` `middle`, `super` and `sub` on made icons, badges and spans in wrapped text.
#[test]
fn made_lines_valign() {
    page("made_lines_valign");
}

/// `vertical-align` `middle`, `super` and `sub` on made icons, badges and spans in wrapped text. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_valign_open() {
    page("made_lines_valign_open");
}

/// `text-indent` and `line-height: 1.6` on a made paragraph.
#[test]
fn made_lines_indent() {
    page("made_lines_indent");
}

/// `text-indent` and `line-height: 1.6` on a made paragraph. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_indent_open() {
    page("made_lines_indent_open");
}

/// `innerHTML` straight into a centred block, no paragraph: text, an inline-block badge, `<em>`, a long word under `overflow-wrap: anywhere`, a coloured span.
#[test]
fn made_lines_textcard() {
    page("made_lines_textcard");
}

/// `innerHTML` straight into a centred block, no paragraph: text, an inline-block badge, `<em>`, a long word under `overflow-wrap: anywhere`, a coloured span. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_textcard_open() {
    page("made_lines_textcard_open");
}

/// Made inline-blocks with `vertical-align: middle`, and a made `<sup>`, on written paragraphs' lines.
#[test]
fn made_lines_icon() {
    page("made_lines_icon");
}

/// Made inline-blocks with `vertical-align: middle`, and a made `<sup>`, on written paragraphs' lines. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_icon_open() {
    page("made_lines_icon_open");
}

/// A paragraph filled before it is in the page takes the font the rules give it there, in every run its boxes cut it into.
#[test]
fn made_lines_late_font() {
    page("made_lines_late_font");
}

/// A paragraph filled before it is in the page takes the font the rules give it there, in every run its boxes cut it into. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_late_font_open() {
    page("made_lines_late_font_open");
}

/// A text node set while out of a `pre-wrap` line keeps its spaces when put back; one moved to a `normal` line collapses them.
#[test]
fn made_lines_respace() {
    page("made_lines_respace");
}

/// A text node set while out of a `pre-wrap` line keeps its spaces when put back; one moved to a `normal` line collapses them. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_respace_open() {
    page("made_lines_respace_open");
}

/// `vertical-align` in `rem`, `ex`, `ch` and `vh` on made spans and inline-blocks, on lines tall enough that no shift grows them.
#[test]
fn made_lines_valign_units() {
    page("made_lines_valign_units");
}

/// `vertical-align` in `rem`, `ex`, `ch` and `vh` on made spans and inline-blocks, on lines tall enough that no shift grows them. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_valign_units_open() {
    page("made_lines_valign_units_open");
}

/// A made paragraph that sets only how it wraps keeps the spaces of the `pre-wrap` block around it; `text-wrap-mode: nowrap` keeps them on one line.
#[test]
fn made_lines_wrap_style() {
    page("made_lines_wrap_style");
}

/// A made paragraph that sets only how it wraps keeps the spaces of the `pre-wrap` block around it; `text-wrap-mode: nowrap` keeps them on one line. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_wrap_style_open() {
    page("made_lines_wrap_style_open");
}

/// A made paragraph whose `white-space` comes from a `var()` holding no keyword is `unset` and takes its `pre-wrap` block's; a script's unreadable `whiteSpace` or `textWrap` leaves the value it wrote before.
#[test]
fn made_lines_wrap_invalid() {
    page("made_lines_wrap_invalid");
}

/// A made paragraph whose `white-space` comes from a `var()` holding no keyword is `unset` and takes its `pre-wrap` block's; a script's unreadable `whiteSpace` or `textWrap` leaves the value it wrote before. The same after a `createElement` the analysis cannot follow.
#[test]
fn made_lines_wrap_invalid_open() {
    page("made_lines_wrap_invalid_open");
}
