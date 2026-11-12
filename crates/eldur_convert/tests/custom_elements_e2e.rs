//! Autonomous custom elements: `customElements.define` upgrades the elements
//! of its name already in the page, `createElement` and `new` make new ones,
//! and the lifecycle callbacks run when Chromium runs them.
//!
//! Each page in `tests/custom_elements/` is converted and run through
//! qmltestrunner; what its script logged (lines starting `LC `) is compared
//! with what Chromium logged, recorded in `tests/custom_elements/chromium.json`
//! by `node crates/eldur_convert/tests/custom_elements/gen.mjs`. A QML warning
//! or an uncaught script error fails the page.

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
    name: "CustomElements"
    when: windowShown

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        const go = findChild(page, "go")
        if (go) {
            mouseClick(go)
            wait(50)
        }
    }
}
}
"##;

/// Runs one page; `Err` has what went wrong.
fn run(name: &str, html: &str, want: &[&str]) -> Result<(), String> {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!(
        "eldur-custom-elements-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_custom_elements.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_custom_elements.qml"))
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
        // The page's own lines name the errors it caught; they are compared below.
        .filter(|l| !l.contains("LC "))
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

#[test]
fn custom_elements_match_chromium() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/custom_elements");
    let chromium: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("chromium.json")).expect("json"))
            .expect("chromium.json");
    let mut failures = Vec::new();
    let mut pages: Vec<_> = std::fs::read_dir(&dir)
        .expect("pages")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect();
    pages.sort();
    assert!(!pages.is_empty());
    for page in pages {
        let name = page
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .into_owned();
        let html = std::fs::read_to_string(&page).expect("page");
        let want: Vec<&str> = chromium[name.as_str()]
            .as_array()
            .expect("a log for every page")
            .iter()
            .map(|l| l.as_str().expect("a line"))
            .collect();
        if let Err(e) = run(&name, &html, &want) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
