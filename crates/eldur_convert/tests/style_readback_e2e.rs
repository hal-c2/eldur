//! What `el.style` reads back: `cssText` holds a property the `style`
//! attribute repeats once, where its last valid declaration is, with the value
//! that wins and its priority; a `display` reads as CSSOM serializes it (`flow`
//! as `block`, `inline flow-root` as `inline-block`, `ruby` and `math` as
//! themselves) (hal-c2/eldur#51).
//!
//! `style_readback/page.html` logs lines starting `RB `, converted and run in
//! qmltestrunner; [`CHROMIUM`] is what Chromium 158 logged for it, through
//! headless Playwright at 800×600.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const PAGE: &str = include_str!("style_readback/page.html");

const CHROMIUM: &[&str] = &[
    "RB a color: blue;",
    "RB b width: 5px; color: blue;",
    "RB c width: 5px; color: red !important;",
    "RB d color: red; display: block;",
    "RB e display: none;",
    "RB f display: block;",
    "RB g --x: 2;",
    "RB c written width: 5px; color: green;",
    "RB h color: blue; width: 7px;",
    "RB f display block",
    "RB k [flow] block",
    "RB k [block flow] block",
    "RB k [inline flow] inline",
    "RB k [block flow-root] flow-root",
    "RB k [inline flow-root] inline-block",
    "RB k [ruby] ruby",
    "RB k [inline ruby] ruby",
    "RB k [block ruby] block ruby",
    "RB k [ruby block] block ruby",
    "RB k [math] math",
    "RB k [inline math] math",
    "RB k [block math] block math",
    "RB k [ruby-text] ruby-text",
    "RB k [list-item block] list-item",
    "RB k [flow list-item] list-item",
    "RB k [inline list-item] inline list-item",
    "RB k [list-item flow-root] flow-root list-item",
    "RB k [inline flow-root list-item] inline flow-root list-item",
    "RB k [block table] table",
    "RB k [inline flex] inline-flex",
    "RB k [grid block] grid",
    "RB k [FLOW] block",
    "RB k [ Flow ] block",
    "RB sp display block",
];

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "StyleReadback"
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
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        tryVerify(() => named(page, "rbdone"), 10000, "the page's script ran")
    }
}
}
"##;

#[test]
fn style_reads_back_as_chromium_serializes_it() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-style-readback-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_readback.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_readback.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("RB ").map(|at| &l[at..]))
        .collect();
    let mut problems = Vec::new();
    for i in 0..logged.len().max(CHROMIUM.len()) {
        let (got, chromium) = (logged.get(i), CHROMIUM.get(i));
        if got != chromium {
            problems.push(format!(
                "line {i}\n    got      {}\n    Chromium {}",
                got.unwrap_or(&"(nothing)"),
                chromium.unwrap_or(&"(nothing)"),
            ));
        }
    }
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
    assert!(problems.is_empty(), "{}\n{text}", problems.join("\n"));
}
