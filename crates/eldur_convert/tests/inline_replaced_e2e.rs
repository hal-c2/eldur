//! An inline replaced element laid out as a block still sits in a line
//! box: the line is at least `line-height` tall and the element's bottom
//! margin edge is on the baseline, raised by `vertical-align`. Positions
//! are Chromium's `getBoundingClientRect()` for the same page.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r###"<!DOCTYPE html><html><head><style>
body{margin:0}
#d1{font:14px/1.5 sans-serif}
#d1 label{display:block;margin-bottom:4px}
#bar{width:100%;height:10px}
p{margin:0;font:16px/18px serif}
#d2{font:16px/2 serif;margin-bottom:10px}
#pic{width:50px;height:40px;margin-top:5px}
</style></head><body>
<div id="d1"><label>Only 7 left</label><progress id="bar" value="1"></progress></div>
<p id="p1">after</p>
<div id="d2"><canvas id="pic" width="50" height="40"></canvas></div>
<p id="p2">after</p>
<script>
var named = [document.getElementById('bar'), document.getElementById('p1'),
  document.getElementById('pic'), document.getElementById('p2')];
</script></body></html>"###;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineReplacedLine"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function topOf(name) { return find(name).mapToItem(page, 0, 0).y }

    // `vertical-align: -0.2em` in a 21px line: 7.8px below the line's top,
    // 3.2px of the line below it.
    function test_a_progress_bar_sits_on_the_baseline() {
        fuzzyCompare(topOf("bar"), 32.8, 0.5)
        fuzzyCompare(topOf("p1"), 46, 0.5)
    }

    // A 45px margin box on the baseline of a 32px line: the descent below
    // it, and the line's margin after the block.
    function test_a_canvas_keeps_the_descent_below_it() {
        fuzzyCompare(topOf("pic"), 69, 0.5)
        fuzzyCompare(topOf("p2"), 130, 0.5)
    }
}
}
"##;

fn convert() -> eldur_convert::Output {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    )
}

#[test]
fn inline_replaced_elements_keep_their_line_box() {
    let out = convert();
    let dir = std::env::temp_dir().join(format!("eldur-inline-replaced-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_line.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_line.qml"))
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
