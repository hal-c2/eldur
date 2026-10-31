//! `position: absolute` and `fixed` boxes at every depth: the containing
//! block is the nearest positioned or transformed ancestor or, with none,
//! the initial containing block, which scrolls with the page and has the
//! viewport's size (`docs/spec/layout.md` §5.2).
//!
//! Each page in `tests/absolute/` is converted and every element with an id
//! is compared, through qmltestrunner, with the box Chromium gives it,
//! recorded in `tests/absolute/chromium.json` by
//! `node crates/eldur_convert/tests/absolute/gen.mjs`, with what the page's
//! script logged about the DOM (lines starting `LC `). A QML warning fails the
//! page, and an element Chromium does not have must not be showing.

use std::path::Path;
use std::process::Command;

use eldur_convert::{Options, RootKind, Severity, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("absolute/chromium.json");

/// Runs one page; `Err` has what went wrong.
fn run(name: &str, html: &str) -> Result<(), String> {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let warned: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning && d.message.contains("position"))
        .collect();
    if !warned.is_empty() {
        return Err(format!("{name}: {warned:?}"));
    }
    let dir = std::env::temp_dir().join(format!("eldur-absolute-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let test = TEST.replace("@CHROMIUM@", CHROMIUM).replace("@CASE@", name);
    std::fs::write(dir.join("tst_absolute.qml"), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_absolute.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    if !output.status.success() || text.contains("Warning:") || text.contains("QWARN") {
        let lines: Vec<_> = text
            .lines()
            .filter(|l| {
                l.contains("FAIL!")
                    || l.contains("Actual")
                    || l.contains("Warning:")
                    || l.contains("QWARN")
            })
            .collect();
        return Err(format!("{name}: {}", lines.join("\n  ")));
    }
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("LC ").map(|at| &l[at..]))
        .collect();
    let chromium: serde_json::Value = serde_json::from_str(CHROMIUM).expect("chromium.json");
    let want: Vec<_> = chromium[name]["log"]
        .as_array()
        .expect("a log for every page")
        .iter()
        .map(|l| l.as_str().expect("a line"))
        .collect();
    if logged != want {
        let mut lines = vec![format!(
            "{name}: {} lines logged, Chromium {}",
            logged.len(),
            want.len()
        )];
        for (got, want) in logged.iter().zip(&want).filter(|(g, w)| g != w) {
            lines.push(format!("got      {got}\n  Chromium {want}"));
        }
        return Err(lines.join("\n  "));
    }
    Ok(())
}

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Absolute"
    when: windowShown

    readonly property var chromium: (@CHROMIUM@)["@CASE@"]
    property var page

    function flickable(item) {
        if (item.contentHeight !== undefined) return item
        for (let i = 0; i < item.children.length; i++) {
            const found = flickable(item.children[i])
            if (found) return found
        }
        return null
    }

    function test_boxes() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        const flick = flickable(page)
        // Chromium's scroll height is never under the viewport's.
        compare(Math.max(flick.contentHeight, 800), chromium.scrollHeight, "scroll height")
        flick.contentY = chromium.scroll
        wait(50)
        compareBoxes(chromium.boxes, "")
        compareHits()
        if (!chromium.after) return
        const go = findChild(page, "go")
        verify(go !== null, "no button")
        mouseClick(go)
        wait(50)
        compareBoxes(chromium.after, "pressed: ")
    }

    function compareBoxes(boxes, when) {
        noOtherElements(page, boxes, when)
        for (const id in boxes) {
            const want = boxes[id]
            const item = findById(page, id)
            verify(item !== null, when + "no item named " + id)
            if (want[2] === 0 && want[3] === 0 && !item.visible) continue
            verify(item.visible, when + id + " is hidden")
            const r = item.mapToItem(page, Qt.rect(0, 0, item.width, item.height))
            const got = [r.x, r.y, r.width, r.height]
            // Text is measured by two engines; boxes agree to the pixel.
            for (let i = 0; i < 4; i++)
                verify(Math.abs(got[i] - want[i]) <= 1.5,
                       when + id + ": got " + got + ", Chromium " + want)
        }
    }

    // Paint order: the item on top at a point, as Chromium's hit test gives it.
    function compareHits() {
        for (const hit of chromium.hits || [])
            compare(topAt(page, hit[0], hit[1]), hit[2], "on top at " + hit[0] + "," + hit[1])
    }

    function topAt(item, x, y) {
        if (!item.visible) return ""
        const kids = []
        for (let i = 0; i < item.children.length; i++) kids.push(item.children[i])
        // Stable: of equal `z`, the later is on top.
        kids.reverse().sort((a, b) => b.z - a.z)
        for (const kid of kids) {
            const found = topAt(kid, x, y)
            if (found !== "") return found
        }
        const r = item.mapToItem(page, Qt.rect(0, 0, item.width, item.height))
        const inside = x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
        return inside && item.objectName ? item.objectName : ""
    }

    // What the script removed is gone from the screen, not only from the DOM.
    function noOtherElements(item, boxes, when) {
        const id = item.eldurId || item.objectName
        verify(!id || !item.visible || boxes[id] !== undefined,
               when + id + " is showing, and is not in Chromium's page")
        for (let i = 0; i < item.children.length; i++)
            noOtherElements(item.children[i], boxes, when)
    }

    // A script's element has its `id` in the runtime, not as an objectName.
    function findById(item, id) {
        const named = findChild(item, id)
        if (named) return named
        return findCreated(item, id)
    }

    function findCreated(item, id) {
        if (item.eldurId === id) return item
        for (let i = 0; i < item.children.length; i++) {
            const found = findCreated(item.children[i], id)
            if (found) return found
        }
        return null
    }
}
}
"##;

#[test]
fn boxes_match_chromium() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/absolute");
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
        if let Err(e) = run(&name, &html) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
