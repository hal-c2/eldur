//! A percentage gap (`gap`, `column-gap`, `row-gap`, and `calc()` mixing px
//! and %) is of its container's content box (CSS Box Alignment 3 §8), and
//! follows it as the container resizes. Run in qmltestrunner against
//! Chromium's boxes at 900 and 450 px wide; one page converted at 900 and
//! resized live.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, Viewport, convert_str};

const PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div div { height: 20px }
</style></head><body>
<div style="display:grid;grid-template-columns:7fr 4fr;gap:3%"><div id=a></div><div id=b></div></div>
<div style="display:grid;grid-template-columns:1fr 1fr;column-gap:calc(10px + 2%);row-gap:5%;height:200px"><div id=c style="height:auto"></div><div id=d style="height:auto"></div><div id=e style="height:auto"></div></div>
<div style="display:grid;grid-template-columns:repeat(3,1fr);gap:2%"><div id=h></div><div id=i></div><div id=j></div></div>
<div style="display:grid;grid-template-columns:1fr 3fr;column-gap:4%"><div id=k></div><div id=l></div><div id=m style="grid-column:1 / 3"></div></div>
<div style="display:flex;gap:4%"><div id=f style="width:100px"></div><div id=g style="flex:1"></div></div>
<div style="display:flex;justify-content:space-between;column-gap:10%;width:600px"><div id=n style="width:20px"></div><div id=o style="width:30px"></div><div id=p style="width:10px"></div></div>
<div style="display:flex;flex-direction:column;row-gap:10%;height:200px"><div id=q></div><div id=r></div></div>
<div style="display:flex;flex-direction:column;row-gap:10%"><div id=s></div><div id=t></div></div>
<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(100px,1fr));gap:2%"><div id=w></div><div id=x></div></div>
<div style="display:flex;flex-direction:column;row-gap:10%;min-height:200px"><div id=u1></div><div id=u2></div></div>
<div style="display:flex;flex-direction:column;row-gap:10%;height:50%"><div id=v1></div><div id=v2></div></div>
<div style="display:flex;flex-wrap:wrap;align-content:start;row-gap:10%;min-height:200px"><div id=z1 style="width:100%"></div><div id=z2 style="width:100%"></div></div>
<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(100px,1fr));grid-auto-rows:100px;width:400px"><div id=y1 style="height:50px"></div><div id=y2 style="height:auto"></div></div>
<div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(100px,1fr));grid-auto-rows:100px;width:400px"><div id=y3 style="height:50px"></div><div id=y4 style="height:150px"></div><div id=y5 style="height:50px"></div><div id=y6 style="height:50px"></div><div id=y7 style="height:50px"></div></div>
<div><div id=y8></div></div>
<div style="display:flex;width:400px;max-width:200px;column-gap:10%"><div id=x1 style="width:50px"></div><div id=x2 style="width:50px"></div></div>
<div style="display:flex;width:100px;min-width:300px;column-gap:10%"><div id=x3 style="width:50px"></div><div id=x4 style="width:50px"></div></div>
<div style="display:grid;grid-template-columns:50px 50px;width:400px;max-width:200px;column-gap:10%"><div id=x5></div><div id=x6></div></div>
<div style="display:flex;box-sizing:border-box;padding:0 20px;width:400px;max-width:240px;column-gap:10%"><div id=x7 style="width:50px"></div><div id=x8 style="width:50px"></div></div>
<div style="display:flex;flex-direction:column;height:400px;max-height:200px;row-gap:10%"><div id=x9></div><div id=x10></div></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('a'), document.getElementById('b'), document.getElementById('c'), document.getElementById('d'), document.getElementById('e'), document.getElementById('h'), document.getElementById('i'), document.getElementById('j'), document.getElementById('k'), document.getElementById('l'), document.getElementById('m'), document.getElementById('f'), document.getElementById('g'), document.getElementById('n'), document.getElementById('o'), document.getElementById('p'), document.getElementById('q'), document.getElementById('r'), document.getElementById('s'), document.getElementById('t'), document.getElementById('w'), document.getElementById('x'), document.getElementById('u1'), document.getElementById('u2'), document.getElementById('v1'), document.getElementById('v2'), document.getElementById('z1'), document.getElementById('z2'), document.getElementById('y1'), document.getElementById('y2'), document.getElementById('y3'), document.getElementById('y4'), document.getElementById('y5'), document.getElementById('y6'), document.getElementById('y7'), document.getElementById('y8'), document.getElementById('x1'), document.getElementById('x2'), document.getElementById('x3'), document.getElementById('x4'), document.getElementById('x5'), document.getElementById('x6'), document.getElementById('x7'), document.getElementById('x8'), document.getElementById('x9'), document.getElementById('x10')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 900×800 and 450×800: `[x, y,
/// width, height]`. Chromium keeps sizes to 1/64 px and Qt rounds layout
/// cells, so within half a pixel; y within a pixel.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 800

TestCase {
    name: "PercentGap"
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
        check("a", 0, 0, 555.531, 20)
        check("b", 582.531, 0, 317.469, 20)
        check("c", 0, 20, 436, 95)
        check("d", 464, 20, 436, 95)
        check("e", 0, 125, 436, 95)
        check("h", 0, 220, 288, 20)
        check("i", 306, 220, 288, 20)
        check("j", 612, 220, 288, 20)
        check("k", 0, 240, 216, 20)
        check("l", 252, 240, 648, 20)
        check("m", 0, 260, 900, 20)
        check("f", 0, 280, 100, 20)
        check("g", 136, 280, 764, 20)
        check("n", 0, 300, 20, 20)
        check("o", 290, 300, 30, 20)
        check("p", 590, 300, 10, 20)
        check("q", 0, 320, 900, 20)
        check("r", 0, 360, 900, 20)
        check("s", 0, 520, 900, 20)
        check("t", 0, 540, 900, 20)
        check("w", 0, 560, 441, 20)
        check("x", 459, 560, 441, 20)
        // Against a min-height or a percentage of an auto height, a flex
        // container's percentage row gap is 0.
        check("u1", 0, 580, 900, 20)
        check("u2", 0, 600, 900, 20)
        check("v1", 0, 780, 900, 20)
        check("v2", 0, 800, 900, 20)
        check("z1", 0, 820, 900, 20)
        check("z2", 0, 840, 900, 20)
        // A set height keeps its own in a px auto row; the row stays 100.
        check("y1", 0, 1020, 200, 50)
        check("y2", 200, 1020, 200, 100)
        // Every item of a px auto row with a set height: the row stays 100,
        // a taller item overflows it.
        check("y3", 0, 1120, 100, 50)
        check("y4", 100, 1120, 100, 150)
        check("y5", 200, 1120, 100, 50)
        check("y6", 300, 1120, 100, 50)
        check("y7", 0, 1220, 100, 50)
        check("y8", 0, 1320, 900, 20)
        // A px max-/min- size bounds the box the percentage is of.
        check("x1", 0, 1340, 50, 20)
        check("x2", 70, 1340, 50, 20)
        check("x3", 0, 1360, 50, 20)
        check("x4", 80, 1360, 50, 20)
        check("x5", 0, 1380, 50, 20)
        check("x6", 70, 1380, 50, 20)
        check("x7", 20, 1400, 50, 20)
        check("x8", 90, 1400, 50, 20)
        check("x9", 0, 1420, 900, 20)
        check("x10", 0, 1460, 900, 20)
    }

    function test_gaps_follow_the_width() {
        page.width = 450
        waitForRendering(page)
        check("a", 0, 0, 277.766, 20)
        check("b", 291.266, 0, 158.734, 20)
        check("c", 0, 20, 215.5, 95)
        check("d", 234.5, 20, 215.5, 95)
        check("e", 0, 125, 215.5, 95)
        check("h", 0, 220, 144, 20)
        check("i", 153, 220, 144, 20)
        check("j", 306, 220, 144, 20)
        check("k", 0, 240, 108, 20)
        check("l", 126, 240, 324, 20)
        check("m", 0, 260, 450, 20)
        check("f", 0, 280, 100, 20)
        check("g", 118, 280, 332, 20)
        check("n", 0, 300, 20, 20)
        check("o", 290, 300, 30, 20)
        check("p", 590, 300, 10, 20)
        check("q", 0, 320, 450, 20)
        check("r", 0, 360, 450, 20)
        check("s", 0, 520, 450, 20)
        check("t", 0, 540, 450, 20)
        check("w", 0, 560, 220.5, 20)
        check("x", 229.5, 560, 220.5, 20)
        // Against a min-height or a percentage of an auto height, a flex
        // container's percentage row gap is 0.
        check("u1", 0, 580, 450, 20)
        check("u2", 0, 600, 450, 20)
        check("v1", 0, 780, 450, 20)
        check("v2", 0, 800, 450, 20)
        check("z1", 0, 820, 450, 20)
        check("z2", 0, 840, 450, 20)
        // A set height keeps its own in a px auto row; the row stays 100.
        check("y1", 0, 1020, 200, 50)
        check("y2", 200, 1020, 200, 100)
        // Every item of a px auto row with a set height: the row stays 100,
        // a taller item overflows it.
        check("y3", 0, 1120, 100, 50)
        check("y4", 100, 1120, 100, 150)
        check("y5", 200, 1120, 100, 50)
        check("y6", 300, 1120, 100, 50)
        check("y7", 0, 1220, 100, 50)
        check("y8", 0, 1320, 450, 20)
        // A px max-/min- size bounds the box the percentage is of.
        check("x1", 0, 1340, 50, 20)
        check("x2", 70, 1340, 50, 20)
        check("x3", 0, 1360, 50, 20)
        check("x4", 80, 1360, 50, 20)
        check("x5", 0, 1380, 50, 20)
        check("x6", 70, 1380, 50, 20)
        check("x7", 20, 1400, 50, 20)
        check("x8", 90, 1400, 50, 20)
        check("x9", 0, 1420, 450, 20)
        check("x10", 0, 1460, 450, 20)
    }
}
}
"##;

#[test]
fn percentage_gaps_match_chromium() {
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
    let dir = std::env::temp_dir().join(format!("eldur-percent-gap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_percent_gap.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_percent_gap.qml"))
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
    // A gap bound to the container's size must not feed back into it.
    assert!(!stderr.contains("Binding loop"), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
