//! A wrapping flex container whose row gap and column gap differ (px, or a
//! percentage row gap that resolves to 0 against an auto height while the
//! column gap resolves, CSS Box Alignment 3 §8.1) keeps each gap on its own
//! axis, and the box ends at its last line. Run in qmltestrunner against
//! Chromium's boxes at 900 and 450 px wide; one page converted at 900 and
//! resized live.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, Viewport, convert_str};

const PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div div { height: 20px }
</style></head><body>
<div style="display:flex;flex-wrap:wrap;width:400px;gap:10%"><div id=a1 style="width:100%"></div><div id=a2 style="width:100%"></div></div>
<div id=s1 style="height:10px"></div>
<div style="display:flex;flex-wrap:wrap;width:400px;row-gap:5px;column-gap:30px"><div id=b1 style="width:150px"></div><div id=b2 style="width:150px"></div><div id=b3 style="width:150px"></div></div>
<div id=s2 style="height:10px"></div>
<div style="display:flex;flex-wrap:wrap;width:400px;row-gap:30px;column-gap:5px"><div id=c1 style="width:150px"></div><div id=c2 style="width:150px"></div><div id=c3 style="width:150px"></div></div>
<div id=s3 style="height:10px"></div>
<div style="display:flex;flex-direction:row-reverse;flex-wrap:wrap;width:400px;row-gap:5px;column-gap:30px"><div id=d1 style="width:150px"></div><div id=d2 style="width:150px"></div><div id=d3 style="width:150px"></div></div>
<div id=s4 style="height:10px"></div>
<div style="display:flex;flex-wrap:wrap;width:400px;row-gap:5px;column-gap:30px"><div id=e1 style="width:500px"></div><div id=e2 style="width:100px"></div></div>
<div id=s5 style="height:10px"></div>
<div style="display:flex;flex-wrap:wrap;row-gap:10px;column-gap:20px"><div id=f1 style="width:300px"></div><div id=f2 style="width:300px"></div><div id=f3 style="width:300px"></div></div>
<div id=s6 style="height:10px"></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('a1'), document.getElementById('a2'), document.getElementById('s1'), document.getElementById('b1'), document.getElementById('b2'), document.getElementById('b3'), document.getElementById('s2'), document.getElementById('c1'), document.getElementById('c2'), document.getElementById('c3'), document.getElementById('s3'), document.getElementById('d1'), document.getElementById('d2'), document.getElementById('d3'), document.getElementById('s4'), document.getElementById('e1'), document.getElementById('e2'), document.getElementById('s5'), document.getElementById('f1'), document.getElementById('f2'), document.getElementById('f3'), document.getElementById('s6')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 900×800 and 450×800: `[x, y,
/// width, height]`.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 800

TestCase {
    name: "FlowGaps"
    when: windowShown

    property var page
    function check(name, x, y, width, height) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0)
        verify(Math.abs(at.x - x) <= 0.5, name + " x " + at.x + ", Chromium " + x)
        verify(Math.abs(at.y - y) <= 1, name + " y " + at.y + ", Chromium " + y)
        verify(Math.abs(item.width - width) <= 0.5, name + " width " + item.width + ", Chromium " + width)
        verify(Math.abs(item.height - height) <= 1, name + " height " + item.height + ", Chromium " + height)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 900, height: 800 })
        waitForRendering(page)
    }

    function test_gaps_at_900() {
        check("a1", 0, 0, 400, 20)
        check("a2", 0, 20, 400, 20)
        check("s1", 0, 40, 900, 10)
        check("b1", 0, 50, 150, 20)
        check("b2", 180, 50, 150, 20)
        check("b3", 0, 75, 150, 20)
        check("s2", 0, 95, 900, 10)
        check("c1", 0, 105, 150, 20)
        check("c2", 155, 105, 150, 20)
        check("c3", 0, 155, 150, 20)
        check("s3", 0, 175, 900, 10)
        check("d1", 250, 185, 150, 20)
        check("d2", 70, 185, 150, 20)
        check("d3", 250, 210, 150, 20)
        check("s4", 0, 230, 900, 10)
        check("e1", 0, 240, 400, 20)
        check("e2", 0, 265, 100, 20)
        check("s5", 0, 285, 900, 10)
        check("f1", 0, 295, 300, 20)
        check("f2", 320, 295, 300, 20)
        check("f3", 0, 325, 300, 20)
        check("s6", 0, 345, 900, 10)
    }

    function test_gaps_follow_the_width() {
        page.width = 450
        waitForRendering(page)
        check("a1", 0, 0, 400, 20)
        check("a2", 0, 20, 400, 20)
        check("s1", 0, 40, 450, 10)
        check("b1", 0, 50, 150, 20)
        check("b2", 180, 50, 150, 20)
        check("b3", 0, 75, 150, 20)
        check("s2", 0, 95, 450, 10)
        check("c1", 0, 105, 150, 20)
        check("c2", 155, 105, 150, 20)
        check("c3", 0, 155, 150, 20)
        check("s3", 0, 175, 450, 10)
        check("d1", 250, 185, 150, 20)
        check("d2", 70, 185, 150, 20)
        check("d3", 250, 210, 150, 20)
        check("s4", 0, 230, 450, 10)
        check("e1", 0, 240, 400, 20)
        check("e2", 0, 265, 100, 20)
        check("s5", 0, 285, 450, 10)
        check("f1", 0, 295, 300, 20)
        check("f2", 0, 325, 300, 20)
        check("f3", 0, 355, 300, 20)
        check("s6", 0, 375, 450, 10)
    }
}
}
"##;

#[test]
fn wrapping_gaps_match_chromium() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        viewport: Viewport {
            width: 900.0,
            height: 800.0,
            ..Viewport::default()
        },
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-flow-gaps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_flow_gaps.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_flow_gaps.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = format!(
        "{}\n{stderr}\n{}",
        String::from_utf8_lossy(&output.stdout),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    // The gaps and the box's height must not feed back into the layout.
    assert!(!stderr.contains("Binding loop"), "{text}");
    assert!(!stderr.contains("recursive rearrange"), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
