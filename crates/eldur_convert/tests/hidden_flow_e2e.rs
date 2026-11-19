//! A `display: none` box a script can show stays in its parent's flow,
//! invisible. The margins around it must be right both while it is hidden
//! and once shown; positions are run in qmltestrunner against Chromium's.
//!
//! A box shown at load that a script hides is the same box the other way
//! round: the margins either side of it collapse together while it is gone,
//! through its parent's top edge when it was the first.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, convert_str};

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

/// The page of #51, with a box hidden in the middle of a column and one at
/// the end of a column with padding.
const SCRIPTED: &str = r#"<!DOCTYPE html>
<style>body { font: 16px/20px sans-serif; }</style>
<p id="shy"><em>Shy</em></p>
<p id="after">After</p>
<div id="w" style="padding:10px"><p id="a" style="margin:0 0 16px">a</p><p id="mid" style="margin:4px 0 30px">mid</p><p id="b" style="margin:8px 0 0">b</p></div>
<div id="v" style="padding:10px"><p id="c" style="margin:0 0 16px">c</p><p id="end" style="margin:0 0 40px">end</p></div>
<button id="t">Toggle</button>
<script>
// Named so that they get an objectName the test can find.
var named = ['after', 'w', 'a', 'b', 'v', 'c'].map(function(id) { return document.getElementById(id); });
var shy = document.getElementById('shy');
var mid = document.getElementById('mid');
var end = document.getElementById('end');
document.getElementById('t').addEventListener('click', function() {
  shy.style.display = shy.style.display === 'none' ? '' : 'none';
  mid.style.display = mid.style.display === 'none' ? '' : 'none';
  end.style.display = end.style.display === 'none' ? '' : 'none';
});
</script>"#;

/// Chromium, `getBoundingClientRect()` at 800×600 after each click: `top`.
const SCRIPTED_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "ScriptHidden"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }
    function posY(name) {
        const item = find(name)
        return item.parent.mapToItem(page, item.x, item.y).y
    }
    function toggle() {
        mouseClick(find("t"))
        wait(50)
        waitForRendering(page)
    }
    function shown() {
        verify(find("shy").visible)
        compare(posY("shy"), 16)
        compare(posY("after"), 52)
        compare(posY("w"), 88)
        compare(posY("b"), 184)
        compare(find("w").height, 126)
        compare(posY("v"), 214)
        compare(find("v").height, 116)
        compare(posY("t"), 330)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
    }

    function test_margins_close_over_a_box_a_script_hides() {
        shown()
        toggle()
        verify(!find("shy").visible)
        verify(!find("mid").visible)
        verify(!find("end").visible)
        // #51: its 16 px margin collapses into the body's once it is first.
        compare(posY("after"), 16)
        compare(posY("w"), 52)
        compare(posY("a"), 62)
        compare(posY("b"), 98)
        compare(find("w").height, 76)
        compare(posY("v"), 128)
        compare(posY("c"), 138)
        compare(find("v").height, 56)
        compare(posY("t"), 184)
        toggle()
        shown()
    }
}
}
"##;

#[test]
fn hidden_box_margins_match_chromium() {
    run(PAGE, TEST, "hidden");
}

#[test]
fn a_box_a_script_hides_lets_the_margins_around_it_collapse() {
    run(SCRIPTED, SCRIPTED_TEST, "script-hidden");
}

fn run(page: &str, test: &str, name: &str) {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(page, &options);
    let dir = std::env::temp_dir().join(format!("eldur-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_hidden.qml"), test).expect("write test");
    let output = support::qmltestrunner()
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

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
