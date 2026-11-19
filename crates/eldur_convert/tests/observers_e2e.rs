//! `IntersectionObserver`, `ResizeObserver` and `MutationObserver`: what each
//! delivers, in what order, and when, measured against Chromium.
//!
//! Each page in `tests/observers/` is converted and run through qmltestrunner;
//! what its script logged (lines starting `OB `) is compared with what Chromium
//! logged, recorded in `tests/observers/chromium.json` by
//! `node crates/eldur_convert/tests/observers/gen.mjs`. Every page steps through
//! frames and timers and says it has finished by making `#obdone`. A QML warning
//! or an uncaught script error fails the page.

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
    name: "Observers"
    when: windowShown

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
        tryVerify(() => named(page, "obdone"), 30000, "the page's steps ran")
    }
}
}
"##;

/// Runs one page; `Ok` has its log, `Err` what went wrong.
fn run(name: &str, html: &str, want: &[&str]) -> Result<String, String> {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-observers-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_observers.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_observers.qml"))
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
        !l.contains("OB ")
            && (l.contains("FAIL!")
                || l.contains("Warning:")
                || l.contains("QWARN")
                || l.contains("Error:"))
    });
    // Every line matching and then a crash is still a failed run.
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("OB ").map(|at| &l[at..]))
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
        Ok(text)
    } else {
        Err(format!("{name}:\n  {}", problems.join("\n  ")))
    }
}

fn pages() -> (Vec<std::path::PathBuf>, serde_json::Value) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/observers");
    let chromium: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("chromium.json")).expect("json"))
            .expect("chromium.json");
    let mut pages: Vec<_> = std::fs::read_dir(&dir)
        .expect("pages")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect();
    pages.sort();
    assert!(!pages.is_empty());
    (pages, chromium)
}

#[test]
fn observers_match_chromium() {
    if support::no_qt() {
        return;
    }
    let (pages, chromium) = pages();
    let mut failures = Vec::new();
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

/// A mutation callback whose every run makes a new record never ends in
/// Chromium, so this page has no recording: Eldur stops the loop after a
/// thousand notifications in a row, says so, and the page goes on, with its
/// observer still hearing later changes. A callback that awaits before it
/// writes is the same loop and stops the same way.
#[test]
fn a_runaway_mutation_loop_stops() {
    if support::no_qt() {
        return;
    }
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/observers/runaway/runaway.html");
    let html = std::fs::read_to_string(page).expect("page");
    let log = run(
        "runaway",
        &html,
        &[
            "OB calls 1000 1000",
            "OB later true",
            "OB async 1000 1000",
            "OB done",
        ],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        log.contains("MutationObserver callbacks made new records for 1000 notifications in a row"),
        "the stop is said:\n{log}"
    );
}

/// The converter no longer calls the three unsupported: they run.
#[test]
fn observers_convert_without_warnings() {
    let (pages, _) = pages();
    for page in pages {
        let html = std::fs::read_to_string(&page).expect("page");
        let out = convert_str(&html, &Options::default());
        let said: Vec<String> = out
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .filter(|m| m.contains("Observer"))
            .collect();
        assert!(said.is_empty(), "{}: {said:?}", page.display());
    }
}
