//! A `display: none` box a script can show stays in its parent's flow,
//! invisible. The margins around it must be right both while it is hidden
//! and once shown; positions are run in qmltestrunner against Chromium's.

use std::path::Path;
use std::process::Command;

use h2q_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r#"<!doctype html><html><body style="margin:0;font:16px/20px sans-serif">
<div id=w1 style="padding:10px"><p id=a style="margin:0 0 16px">a</p><p id=x hidden style="margin:4px 0 30px">x</p><p id=b style="margin:8px 0 0">b</p></div>
<div id=w2 style="padding:10px"><p id=d style="margin:0 0 16px">d</p><p id=y hidden style="margin:0">y</p></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('a'), document.getElementById('b'),
  document.getElementById('d'), document.getElementById('w1'), document.getElementById('w2')];
function show() {
  document.getElementById('x').hidden = false;
  document.getElementById('y').hidden = false;
}
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 800×600: `[y, height]`.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "HiddenFlow"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }
    // The item's box, without the half-leading `Translate` of its glyphs.
    function posY(name) {
        const item = find(name)
        return item.parent.mapToItem(page, item.x, item.y).y
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
    }

    function test_margins_skip_a_hidden_box() {
        compare(posY("a"), 10)
        compare(posY("b"), 46)
        compare(find("w1").height, 76)
        compare(posY("d"), 86)
        compare(find("w2").height, 56)
    }

    function test_margins_around_a_shown_box() {
        find("x").visible = true
        find("y").visible = true
        wait(50)
        compare(posY("x"), 46)
        compare(posY("b"), 96)
        compare(find("w1").height, 126)
        compare(posY("d"), 136)
        compare(posY("y"), 172)
        compare(find("w2").height, 76)
    }
}
}
"##;

#[test]
fn hidden_box_margins_match_chromium() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("h2q-hidden-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_hidden.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_hidden.qml"))
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
