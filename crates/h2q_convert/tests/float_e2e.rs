//! Floats that share a line: a run of left (or right) floats lines up
//! along the line and wraps when one does not fit. Positions are run in
//! qmltestrunner against Chromium's.

use std::path::Path;
use std::process::Command;

use h2q_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r#"<!doctype html><html><body style="margin:0;font:16px/20px sans-serif">
<div id=row style="overflow:hidden;width:500px;padding:4px"><div id=a style="float:left;width:150px;height:40px;margin:5px">a</div><div id=b style="float:left;width:30%;height:60px;margin:0 10px">b</div><div id=c style="float:left;width:200px;height:20px">c</div></div>
<div id=right style="overflow:hidden;width:400px"><div id=d style="float:right;width:100px;height:30px;margin-left:8px">d</div><div id=e style="float:right;width:25%;height:30px">e</div></div>
<p id=after style="margin:0">after</p>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('row'),
  document.getElementById('a'),
  document.getElementById('b'),
  document.getElementById('c'),
  document.getElementById('right'),
  document.getElementById('d'),
  document.getElementById('e'),
  document.getElementById('after')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 800×600.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "FloatRun"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }
    // The item's box, without the half-leading `Translate` of its glyphs.
    function box(name) {
        const item = find(name)
        const p = item.parent.mapToItem(page, item.x, item.y)
        return [p.x, p.y, item.width, item.height]
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
    }

    // Left floats line up from the left, and one that does not fit moves
    // down below them.
    function test_left_floats_share_a_line() {
        compare(box("a"), [9, 9, 150, 40])
        compare(box("b"), [174, 4, 150, 60])
        compare(box("c"), [4, 64, 200, 20])
        compare(box("row"), [0, 0, 508, 88])
    }

    // The first right float is the rightmost.
    function test_right_floats_line_up_from_the_right() {
        compare(box("right"), [0, 88, 400, 30])
        compare(box("d"), [300, 88, 100, 30])
        compare(box("e"), [192, 88, 100, 30])
        compare(box("after")[1], 118)
    }
}
}
"##;

#[test]
fn float_runs_match_chromium() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("h2q-float-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_float.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_float.qml"))
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
}

fn write(dir: &Path, out: &h2q_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
