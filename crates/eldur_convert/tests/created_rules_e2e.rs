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
