//! A grid item whose width is a percentage of its area, here an image at
//! `width: min(100%, 25rem)`, takes its `fr` share like the rest of its track.
//! Its intrinsic width or its own width must not widen the track, or the `fr`
//! column beside it is squeezed to its narrowest and its text wraps one word
//! per line. Positions are run in qmltestrunner against Chromium's.
//!
//! A percentage is of the item's area, not of the grid: half of a `1fr` of
//! two is a quarter of the grid. A definite width with no percentage in it
//! is no share of its area, and is sized as before.

use std::process::Command;

use eldur_convert::{Input, Options, Resource, Resources, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

/// Wider than it is tall, so that its `height` attribute gives an intrinsic
/// width (896) past the grid's.
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="498" height="222" viewBox="0 0 498 222"><rect width="498" height="222" fill="#c60"/></svg>"##;

const PAGE: &str = r#"<!DOCTYPE html><html><body style="margin:0;font:20px sans-serif">
<div style="display:grid;grid-template-columns:7fr 4fr;gap:27px;align-items:center">
<div id="t" style="display:flex;flex-direction:column;align-items:flex-start">
<div id="p" style="max-width:400px">A paragraph of ordinary text that is longer than its maximum width, so it wraps onto a few lines.</div>
</div>
<img id="i" src="wide.svg" width="400" height="400" alt="Wide" style="width:min(100%,25rem);height:auto">
</div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('t'), document.getElementById('p'), document.getElementById('i')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 900×600: the two columns share the
/// 873 px beside the gap 7 : 4. Qt rounds layout margins to whole pixels, so
/// within a pixel.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 600

TestCase {
    name: "GridImage"
    when: windowShown

    property var page
    function check(name, x, width) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0).x
        verify(Math.abs(at - x) <= 1, name + " x " + at + ", Chromium " + x)
        verify(Math.abs(item.width - width) <= 1, name + " width " + item.width + ", Chromium " + width)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 900, height: 600 })
        waitForRendering(page)
    }

    function test_the_image_takes_its_fr_share() {
        check("t", 0, 555.53)
        check("p", 0, 400)
        check("i", 582.53, 317.47)
    }
}
}
"##;

/// Two `1fr` columns, then `3fr 1fr` (split into sub-columns, its gap in
/// the items' margins), each 20 px apart, with an item at half its area;
/// then a `calc()` width with no percentage.
const AREAS: &str = r#"<!DOCTYPE html><html><body style="margin:0;font:20px sans-serif">
<div style="display:grid;grid-template-columns:1fr 1fr;gap:20px">
<div id="h" style="width:50%;height:20px"></div><div id="h2" style="height:20px"></div>
</div>
<div style="display:grid;grid-template-columns:3fr 1fr;gap:20px">
<div id="q" style="width:50%;height:20px"></div><div id="q2" style="height:20px"></div>
</div>
<script>
var named = ['h', 'h2', 'q', 'q2'].map(function (id) { return document.getElementById(id); });
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 900×600.
const AREAS_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 600

TestCase {
    name: "GridAreas"
    when: windowShown

    property var page
    function check(name, x, width) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0).x
        verify(Math.abs(at - x) <= 1, name + " x " + at + ", Chromium " + x)
        verify(Math.abs(item.width - width) <= 1, name + " width " + item.width + ", Chromium " + width)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 900, height: 600 })
        waitForRendering(page)
    }

    function test_a_percentage_is_of_the_area() {
        check("h", 0, 220)
        check("h2", 460, 440)
        check("q", 0, 330)
        check("q2", 680, 220)
    }
}
}
"##;

struct Files;

impl Resources for Files {
    fn read(&mut self, url: &str, _base: &str) -> Option<Resource> {
        (url == "wide.svg").then(|| Resource {
            resolved: "/page/wide.svg".to_owned(),
            bytes: SVG.as_bytes().to_vec(),
        })
    }
}

/// Converts `page` and runs `test` against it in qmltestrunner.
fn run(page: &str, test: &str, name: &str) -> String {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: page.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut Files,
    );
    let dir = std::env::temp_dir().join(format!("eldur-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        let path = dir.join(&file.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("asset dir");
        std::fs::write(path, &file.contents).expect("write output");
    }
    let file = format!("tst_{name}.qml");
    std::fs::write(dir.join(&file), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join(&file))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let main = out.main_qml().unwrap_or_default().to_owned();
    let text = format!(
        "{}\n{}\n{main}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    main
}

#[test]
fn a_percentage_width_image_keeps_the_fr_columns() {
    run(PAGE, TEST, "grid_image");
}

#[test]
fn a_percentage_width_is_of_its_grid_area() {
    run(AREAS, AREAS_TEST, "grid_areas");
}

/// `calc(30vw - 10px)` has no percentage: it is a definite width, not a
/// share of its area capped at a percentage.
#[test]
fn a_definite_calc_width_is_not_a_share() {
    let page = r#"<!DOCTYPE html><html><body style="margin:0">
<div style="display:grid;grid-template-columns:1fr 1fr"><div></div><div style="width:calc(30vw - 10px);height:20px"></div></div>
</body></html>"#;
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert_str(page, &options);
    let main = out.main_qml().unwrap_or_default();
    // It sizes its track as a px width does, with no share and no keeper.
    assert!(
        main.contains("Layout.preferredWidth: Math.max(0, root.width * 0.3 - 10)"),
        "{main}"
    );
    // The layouts around it say they take any width; the item has no cap.
    assert!(
        main.lines()
            .filter(|l| l.contains("Layout.maximumWidth"))
            .all(|l| l.trim() == "Layout.maximumWidth: Number.POSITIVE_INFINITY"),
        "{main}"
    );
    assert!(!main.contains("gridArea"), "{main}");
}

/// A test that checks each named item's place and width against
/// Chromium's (`getBoundingClientRect()` at 900×600), once the page's
/// scripts have run.
fn places(name: &str, checks: &str) -> String {
    format!(
        r##"
import QtQuick
import QtTest

Item {{
    id: host
    width: 900
    height: 600

TestCase {{
    name: "{name}"
    when: windowShown

    property var page
    function check(name, x, y, width) {{
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0)
        // Where the layout put it, and the grid's other children.
        const cell = c => (c.objectName || "?") + " at " + c.x + "," + c.y
            + " " + c.width + "×" + c.height + (c.visible ? "" : " hidden")
        const why = " (" + Array.from(item.parent.children, cell).join("; ") + ")"
        verify(Math.abs(at.x - x) <= 1, name + " x " + at.x + ", Chromium " + x + why)
        verify(Math.abs(at.y - y) <= 1, name + " y " + at.y + ", Chromium " + y + why)
        verify(Math.abs(item.width - width) <= 1, name + " width " + item.width + ", Chromium " + width + why)
    }}

    function init() {{
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, {{ width: 900, height: 600 }})
        waitForRendering(page)
    }}

    function test_places() {{
{checks}
    }}
}}
}}
"##
    )
}

/// `display: none` takes no cell (CSS Grid 2 §8): with the first item
/// hidden, the item at half its area moves into the first column and the
/// third into the second, in the first row, which has no gap above it. The
/// keeper's row adds no gap below the grid either.
#[test]
fn a_hidden_item_leaves_no_cell_before_a_share() {
    let page = r#"<!DOCTYPE html><html><body style="margin:0">
<div style="display:grid;grid-template-columns:1fr 1fr;gap:20px">
<div id="x" style="height:20px"></div><div id="h" style="width:50%;height:20px"></div><div id="y" style="height:20px"></div>
</div>
<div id="after" style="height:10px"></div>
<script>
var named = ['h', 'y', 'after'].map(function (id) { return document.getElementById(id); });
document.getElementById('x').style.display = 'none';
</script></body></html>"#;
    let checks = r#"        tryVerify(() => findChild(page, "y").mapToItem(page, 0, 0).y < 1, 2000, "y moves up")
        check("h", 0, 0, 220)
        check("y", 460, 0, 440)
        check("after", 0, 20, 900)"#;
    run(page, &places("HiddenShare", checks), "grid_hidden_share");
}

/// A class that gives an item half its area keeps the grid's form in both
/// states: the keeper is there whether or not the class is, so the states
/// differ only in the item's cap, and the class still applies.
#[test]
fn a_share_under_a_class_keeps_the_grid_form() {
    let page = r#"<!DOCTYPE html><html><body style="margin:0">
<style>.half { width: 50% }</style>
<div style="display:grid;grid-template-columns:1fr 1fr;gap:20px">
<div id="h" style="height:20px"></div><div id="h2" style="height:20px"></div>
</div>
<script>
var named = ['h', 'h2'].map(function (id) { return document.getElementById(id); });
document.getElementById('h').classList.toggle('half');
</script></body></html>"#;
    let out = eldur_convert::convert_str(
        page,
        &Options {
            root: RootKind::Item,
            ..Options::default()
        },
    );
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.to_string().contains("dynamic-form")),
        "{:?}",
        out.diagnostics
    );
    let checks = r#"        tryVerify(() => findChild(page, "h").width < 300, 2000, "the class applies")
        check("h", 0, 0, 220)
        check("h2", 460, 0, 440)"#;
    run(page, &places("ClassShare", checks), "grid_class_share");
}
