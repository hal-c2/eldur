//! Block children of fractional height in a `ColumnLayout` (layout.md §1.1):
//! Qt Quick Layouts round every cell up to a whole pixel, so without the
//! carry each `27.2px` paragraph moved the rest of the column down by
//! 0.8 px. Each item hands its rounding to the next, so positions stay
//! exact; a hidden box passes it on; boxes a script appends or inserts are
//! laid out as before (at most a pixel off), with no warnings.
//!
//! The page is `tests/carry/page.html`, driven with real clicks through
//! qmltestrunner. The expected boxes are Chromium's after the same steps,
//! recorded in `tests/carry/chromium.json` by
//! `node crates/eldur_convert/tests/carry/gen.mjs`.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = include_str!("carry/page.html");
const CHROMIUM: &str = include_str!("carry/chromium.json");

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Carry"
    when: windowShown

    readonly property var chromium: (@CHROMIUM@)
    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function items(item, out) {
        out.push(item)
        for (let i = 0; i < item.children.length; i++)
            items(item.children[i], out)
        return out
    }
    function named(name) {
        const found = findChild(page, name)
        verify(found !== null, "no item named " + name)
        return found
    }
    // The box, without the half-leading `Translate` of a `Text`.
    function pageY(item) { return item.parent.mapToItem(page, item.x, item.y).y }
    // The shown blocks in `#c`, top to bottom, as Chromium's `#c .b`.
    function blocks() {
        const c = named("c")
        return items(c, []).filter(i => i !== c && i.visible
                && ["p", "div", "section"].indexOf(i.eldurTag) >= 0)
            .sort((a, b) => pageY(a) - pageY(b) || b.height - a.height)
    }
    function click(name) {
        const item = named(name)
        const p = item.mapToItem(page, item.width / 2, item.height / 2)
        mouseClick(page, p.x, p.y)
        wait(100)
    }
    function near(got, expected, tolerance, what) {
        verify(Math.abs(got - expected) <= tolerance, what + ": " + got + " is not " + expected)
    }
    // Static blocks are placed within 0.5 px of Chromium (its boxes are in
    // 1/64 px units, ours are the CSS values); created ones, and the static
    // ones after an inserted one, within a pixel. A `Text` keeps the height
    // Qt rounds it to.
    function sameBlocks(step) {
        const want = chromium[step]["#c .b"].filter(b => b.height > 0)
        const got = blocks()
        compare(got.length, want.length, step + " count")
        let created = false
        for (let i = 0; i < want.length; i++) {
            const item = got[i]
            created = created || item.objectName === ""
            const what = step + " " + (item.objectName || "created " + item.eldurTag)
            near(pageY(item), want[i].y, created ? 1 : 0.5, what + " y")
            near(item.height, want[i].height, 1, what + " height")
        }
        near(named("c").height, chromium[step]["#c"][0].height, 1, step + " #c height")
    }

    function test_initial() { sameBlocks("initial") }
    function test_show() { click("show"); sameBlocks("show") }
    function test_append() { click("append"); sameBlocks("append") }
    function test_insert() { click("insert"); sameBlocks("insert") }
}
}
"##;

#[test]
fn fractional_blocks_keep_chromium_positions() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "carry/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-carry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_carry.qml"),
        TEST.replace("@CHROMIUM@", CHROMIUM),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_carry.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("eldur:"), "{text}");
    assert!(!text.contains("Warning:"), "{text}");
}
