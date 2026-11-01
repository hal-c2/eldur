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
