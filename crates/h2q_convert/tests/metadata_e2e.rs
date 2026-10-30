//! Runtime DOM access on converted pages (`docs/design/dynamic-styles.md`
//! §7): selectors built at run time match the emitted `h2qTag` and
//! `h2qClasses`, and `location` and relative URLs use `root.baseUrl`.
//!
//! Each page in `tests/metadata/` has a `probe()` that returns what its
//! scripts see as JSON. The expected answers are Chromium's for the same
//! page, recorded in `tests/metadata/chromium.json` by
//! `node crates/h2q_convert/tests/metadata/chromium.mjs`.

use std::process::Command;

use h2q_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("metadata/chromium.json");
const HTTPS: &str = "https://example.com/docs/guide/index.html";

/// Converts `html` from `url`, runs `probe()` in qml6 and compares every key
/// of Chromium's answer for `case`. In the answers, `{dir}` stands for the
/// directory of the page; `rename` maps Chromium values that differ by
/// design to the converted app's.
fn check(case: &str, html: &str, url: &str, rename: &[(&str, &str)]) {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = h2q_convert::convert(
        &Input {
            html: html.to_owned(),
            url: url.to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("h2q-metadata-{case}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    // A test can only reach the page's functions through the page's root.
    let main = dir.join("Main.qml");
    let qml = std::fs::read_to_string(&main).expect("Main.qml");
    let at = qml.rfind('}').expect("root object");
    let patched = format!(
        "{}    function probeForTest() {{ return Page.probe(); }}\n{}",
        &qml[..at],
        &qml[at..]
    );
    std::fs::write(&main, patched).expect("write");
    let renames: Vec<String> = rename
        .iter()
        .map(|(from, to)| format!("[{from:?}, {to:?}]"))
        .collect();
    let test = TEST
        .replace("@CHROMIUM@", CHROMIUM)
        .replace("@CASE@", case)
        .replace("@RENAME@", &renames.join(", "));
    std::fs::write(dir.join("tst_metadata.qml"), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_metadata.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("H2Q:"), "{text}");
    assert!(!text.contains("Warning:"), "{text}");
}

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "Metadata"
    when: windowShown

    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
    }

    function test_probe_answers_as_chromium() {
        const want = (@CHROMIUM@)["@CASE@"]
        verify(want !== undefined, "no Chromium answers for @CASE@")
        // The directory of Main.qml, without the trailing slash.
        const dir = String(Qt.resolvedUrl(".")).replace(/\/$/, "")
        const renames = [@RENAME@]
        const got = JSON.parse(page.probeForTest())
        for (const key in want) {
            let expected = want[key]
            if (typeof expected === "string") {
                for (const [from, to] of renames)
                    expected = expected.split(from).join(to)
                expected = expected.split("{dir}").join(dir)
            }
            compare(got[key], expected, key)
        }
    }
}
}
"##;

#[test]
fn runtime_selectors_and_urls_match_chromium() {
    check(
        "probe-https",
        include_str!("metadata/probe.html"),
        HTTPS,
        &[],
    );
}

#[test]
fn base_href_resolves_against_the_source_url() {
    check("base-https", include_str!("metadata/base.html"), HTTPS, &[]);
}

#[test]
fn a_page_with_only_literal_queries_still_has_its_location() {
    check(
        "literal-https",
        include_str!("metadata/literal.html"),
        HTTPS,
        &[],
    );
}

/// A local page's base is the converted `Main.qml`, next to which its assets
/// are copied (dynamic-styles.md §14, decision 2), where Chromium shows the
/// source file.
#[test]
fn a_local_page_resolves_next_to_the_converted_app() {
    check(
        "probe-local",
        include_str!("metadata/probe.html"),
        "index.html",
        &[("{dir}/index.html", "{dir}/Main.qml")],
    );
}
