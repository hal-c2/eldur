//! A wrapping flex container whose row gap and column gap differ (px, or a
//! percentage row gap that resolves to 0 against an auto height while the
//! column gap resolves, CSS Box Alignment 3 §8.1) keeps each gap on its own
//! axis, and the box ends at its last line. Run in qmltestrunner against
//! Chromium's boxes at 900 and 450 px wide; one page converted at 900 and
//! resized live. A second page holds scripts: the items stay the box's
//! children in the DOM, so reading, appending and removing them work as in
//! Chromium; and an `inline-flex` box with a width keeps it.

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
<div style="display:flex;flex-wrap:wrap;width:300px;row-gap:10px;column-gap:40px"><span id=g1>Alpha</span><span id=g2>Beta</span></div>
<div style="display:flex;flex-wrap:wrap;width:400px;row-gap:5px;column-gap:30px"><div id=h1 style="width:150px"></div><div id=h2 style="width:150px"></div><div id=h3 style="width:150px"></div></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('a1'), document.getElementById('a2'), document.getElementById('s1'), document.getElementById('b1'), document.getElementById('b2'), document.getElementById('b3'), document.getElementById('s2'), document.getElementById('c1'), document.getElementById('c2'), document.getElementById('c3'), document.getElementById('s3'), document.getElementById('d1'), document.getElementById('d2'), document.getElementById('d3'), document.getElementById('s4'), document.getElementById('e1'), document.getElementById('e2'), document.getElementById('s5'), document.getElementById('f1'), document.getElementById('f2'), document.getElementById('f3'), document.getElementById('s6'), document.getElementById('g1'), document.getElementById('g2'), document.getElementById('h1'), document.getElementById('h2'), document.getElementById('h3')];
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

    function near(what, got, want) {
        verify(Math.abs(got - want) <= 0.5, what + " " + got + ", want " + want)
    }

    // Text widths are the font's, so these are Chromium's relations, not
    // its numbers: an auto-width item keeps its natural width, and the
    // next one sits one column gap past it.
    function test_auto_width_items_keep_their_width() {
        const g1 = findChild(page, "g1")
        const g2 = findChild(page, "g2")
        verify(g1 && g2, "g1, g2")
        verify(g1.width > 0, "g1 width " + g1.width)
        near("g1 width", g1.width, Math.ceil(g1.implicitWidth))
        near("g2 width", g2.width, Math.ceil(g2.implicitWidth))
        const a = g1.mapToItem(page, 0, 0)
        const b = g2.mapToItem(page, 0, 0)
        near("g2 y", b.y, a.y)
        near("g2 x", b.x, a.x + g1.width + 40)
    }

    // An item a script hides takes no slot: Chromium, with `#h1` at
    // `display: none`, puts `#h2` at the line's start and `#h3` one column
    // gap after it, on the same line. Shown again, `#h3` wraps.
    function test_a_hidden_item_takes_no_slot() {
        const h1 = findChild(page, "h1")
        const h2 = findChild(page, "h2")
        const h3 = findChild(page, "h3")
        verify(h1 && h2 && h3, "h1, h2, h3")
        const top = h1.mapToItem(page, 0, 0)
        near("h2 x", h2.mapToItem(page, 0, 0).x, top.x + 180)
        near("h3 y", h3.mapToItem(page, 0, 0).y, top.y + 25)
        h1.visible = false
        waitForRendering(page)
        near("hidden: h2 x", h2.mapToItem(page, 0, 0).x, top.x)
        near("hidden: h2 y", h2.mapToItem(page, 0, 0).y, top.y)
        near("hidden: h3 x", h3.mapToItem(page, 0, 0).x, top.x + 180)
        near("hidden: h3 y", h3.mapToItem(page, 0, 0).y, top.y)
        h1.visible = true
        waitForRendering(page)
        near("shown: h2 x", h2.mapToItem(page, 0, 0).x, top.x + 180)
        near("shown: h3 x", h3.mapToItem(page, 0, 0).x, top.x)
        near("shown: h3 y", h3.mapToItem(page, 0, 0).y, top.y + 25)
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

const DOM_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.w > div { height: 20px; width: 150px }
</style></head><body>
<div id=k style="display:flex;flex-wrap:wrap;width:400px;row-gap:5px;column-gap:30px" class=w><div id=k1></div><div id=k2></div><div id=k3></div></div>
<div id=s1 style="height:10px"></div>
<div id=m style="display:flex;flex-wrap:wrap;width:400px;row-gap:5px;column-gap:30px" class=w><div id=m1></div><div id=m2></div><div id=m3></div></div>
<div id=s2 style="height:10px"></div>
<div><span id=i style="display:inline-flex;flex-wrap:wrap;width:400px;row-gap:5px;column-gap:30px" class=w><div id=i1></div><div id=i2></div><div id=i3></div></span></div>
<div id=s3 style="height:10px"></div>
<div><span id=j style="display:inline-flex;flex-wrap:wrap;width:400px;gap:10px" class=w><div id=j1></div><div id=j2></div><div id=j3></div></span></div>
<div id=s4 style="height:10px"></div>
<script>
var k = document.getElementById('k');
var m = document.getElementById('m');
var log = [];
log.push('kids=' + k.children.length);
log.push('parent=' + (document.getElementById('k1').parentNode === k));
log.push('first=' + (k.firstElementChild && k.firstElementChild.id));
var n = document.createElement('div'); n.id = 'k4'; k.appendChild(n);
log.push('kids=' + k.children.length);
log.push('last=' + k.lastElementChild.id);
m.removeChild(document.getElementById('m2'));
log.push('kids=' + m.children.length);
console.log('DOM ' + log.join(' '));
var named = [document.getElementById('k1'), document.getElementById('k2'), document.getElementById('k3'), document.getElementById('s1'), document.getElementById('m1'), document.getElementById('m3'), document.getElementById('s2'), document.getElementById('i'), document.getElementById('i1'), document.getElementById('i2'), document.getElementById('i3'), document.getElementById('s3'), document.getElementById('j'), document.getElementById('j1'), document.getElementById('j2'), document.getElementById('j3'), document.getElementById('s4')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 900×800, after the page's script
/// appended `k4` and removed `m2`. It logged `DOM kids=3 parent=true
/// first=k1 kids=4 last=k4 kids=2`.
const DOM_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 800

TestCase {
    name: "FlowGapsDom"
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

    function test_scripted_items_at_900() {
        check("k1", 0, 0, 150, 20)
        check("k2", 180, 0, 150, 20)
        check("k3", 0, 25, 150, 20)
        check("k4", 180, 25, 150, 20)
        check("s1", 0, 45, 900, 10)
        check("m1", 0, 55, 150, 20)
        check("m3", 180, 55, 150, 20)
        check("s2", 0, 75, 900, 10)
        check("i", 0, 85, 400, 45)
        check("i1", 0, 85, 150, 20)
        check("i2", 180, 85, 150, 20)
        check("i3", 0, 110, 150, 20)
        check("s3", 0, 130, 900, 10)
        check("j", 0, 140, 400, 50)
        check("j1", 0, 140, 150, 20)
        check("j2", 160, 140, 150, 20)
        check("j3", 0, 170, 150, 20)
        check("s4", 0, 190, 900, 10)
    }
}
}
"##;

#[test]
fn scripts_see_the_wrapping_items_as_in_chromium() {
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
    let out = convert_str(DOM_PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-flow-gaps-dom-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_flow_gaps_dom.qml"), DOM_TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_flow_gaps_dom.qml"))
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
    // What the page's script read, as Chromium logged it; qmltestrunner
    // writes a test's console.log to stdout.
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("DOM kids=3 parent=true first=k1 kids=4 last=k4 kids=2"),
        "{text}"
    );
    assert!(!stderr.contains("Binding loop"), "{text}");
    assert!(!stderr.contains("recursive rearrange"), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
