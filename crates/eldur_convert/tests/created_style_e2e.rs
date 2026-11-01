//! Elements a script creates take the page's CSS at run time, however the script
//! names their tag and class.
//!
//! Each page in `tests/created_style/` is converted and run through
//! qmltestrunner; what its script logged (lines starting `CS `, box geometry and
//! computed styles) is compared with what Chromium logged, recorded in
//! `tests/created_style/chromium.json` by
//! `node crates/eldur_convert/tests/created_style/gen.mjs`. A QML warning or an
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
    name: "CreatedStyle"
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

const OVERFLOW: &str = "a `height: 100%` item with a sibling takes what the sibling leaves: \
    overflowing its column makes Qt's layouts warn of a recursive rearrange";

const BLOCK_IN_LINE: &str = "an inline control a script makes `display: block` stays on its line: \
    a line of text is one item, which it can't split";

const ROW_TOPS: &str = "a collapsed table starts half its border in, on Chromium's rounded \
    rows 0.5px higher than ours, so every other row's top rounds 1px apart";

const COLLAPSED_COLUMNS: &str = "a collapsed table's cells start on whole pixels where Chromium's \
    start half a border in, and a column's spare width rounds differently: one column is 1px over";

/// `(page, Chromium's line, ours, why)`: what is known to differ, in full, so a
/// changed difference and one that went away both fail.
const KNOWN: &[(&str, &str, &str, &str)] = &[
    (
        "stretch_flex",
        "CS c6:500,0,100,150 c6a:500,0,100,150 c6b:500,150,100,18",
        "CS c6:500,0,100,150 c6a:500,0,100,132 c6b:500,132,100,18",
        OVERFLOW,
    ),
    (
        "hidden_reads",
        "CS sh-5 62 451 \"a\\nb k c\"",
        "CS sh-5 21 410 \"a  b k c\"",
        BLOCK_IN_LINE,
    ),
    (
        "created_rows",
        "CS rows 33,66,98,131",
        "CS rows 34,66,99,131",
        ROW_TOPS,
    ),
    (
        "created_rows",
        "CS cells h:1,115 h2:115,92 h3:207,92 a0:1,115 b0:115,92 c0:207,92 a3:1,115 b3:115,92 c3:207,92",
        "CS cells h:0,115 h2:115,93 h3:208,92 a0:0,115 b0:115,93 c0:208,92 a3:0,115 b3:115,93 c3:208,92",
        COLLAPSED_COLUMNS,
    ),
];

/// Runs one page; `Err` has what went wrong.
fn run(name: &str, html: &str, want: &[&str], clicks: &str, probes: &str) -> Result<(), String> {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-created-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_created.qml"),
        TEST.replace("CLICKS", clicks).replace("PROBES", probes),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_created.qml"))
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
        if got != chromium && !listed {
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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/created_style");
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
fn paint() {
    page("paint");
}

#[test]
fn cascade() {
    page("cascade");
}

#[test]
fn context() {
    page("context");
}

#[test]
fn controls() {
    page("controls");
}

#[test]
fn display() {
    page("display");
}

#[test]
fn helper() {
    page("helper");
}

#[test]
fn markup() {
    page("markup");
}

#[test]
fn names() {
    page("names");
}

#[test]
fn position() {
    page("position");
}

#[test]
fn props() {
    page("props");
}

#[test]
fn statics() {
    page("statics");
}

/// An inline `display` or `hidden` over a class binding.
#[test]
fn inline_display() {
    page("inline_display");
}

/// The same with no computed read, so no rule table on the page.
#[test]
fn inline_display_plain() {
    page("inline_display_plain");
}

/// Inline paint wins, clearing it gives what the rules now say.
#[test]
fn inline_paint() {
    page("inline_paint");
}

/// Inline box properties on a static element, with class changes around them.
#[test]
fn inline_box() {
    page("inline_box");
}

/// Inline box properties on containers and on flex and grid items.
#[test]
fn inline_children() {
    page("inline_children");
}

#[test]
fn hidden_items() {
    page("hidden_items");
}

#[test]
fn hidden_reads() {
    page("hidden_reads");
}

#[test]
fn span_line_height() {
    page("span_line_height");
}

#[test]
fn moved_fixed() {
    page("moved_fixed");
}

#[test]
fn created_rows() {
    page("created_rows");
}

#[test]
fn flex_no_grow() {
    page("flex_no_grow");
}

#[test]
fn grid_fractions() {
    page("grid_fractions");
}

#[test]
fn row_fractions() {
    page("row_fractions");
}

/// A grid given its height: by a flex row, a grid, `height` or `min-height`.
#[test]
fn stretch_grid() {
    page("stretch_grid");
}

/// `align-content`, `align-items`, tracks and own heights in a stretched grid.
#[test]
fn stretch_grid_align() {
    page("stretch_grid_align");
}

/// `body` a grid at `height: 100%` of a definite `html`.
#[test]
fn stretch_grid_body() {
    page("stretch_grid_body");
}

/// The same with `html` auto high, where the percentage is auto too.
#[test]
fn stretch_grid_body_auto() {
    page("stretch_grid_body_auto");
}

/// Flex columns stretched by rows and grids, grown children, percentages of them.
#[test]
fn stretch_flex() {
    page("stretch_flex");
}
