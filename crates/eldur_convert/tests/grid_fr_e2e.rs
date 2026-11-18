//! Unequal `fr` columns are split into equal sub-columns; the column gap
//! must fall only between the real tracks, not between the sub-columns of
//! one. `fr` tracks with a minimum keep it when their share is less.
//! Positions are run in qmltestrunner against Chromium's.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, convert_str};

const PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.g { display: grid; grid-template-columns: 2fr 1fr; gap: 20px }
.h { display: grid; grid-template-columns: 1fr 2fr 1fr; column-gap: 20px; row-gap: 10px }
.k { display: grid; grid-template-columns: 2fr 1fr 1fr 1fr; gap: 40px }
.g div, .h div, .k div { height: 30px; background: #ccc }
</style></head><body>
<div class=g><div id=a></div><div id=b></div></div>
<div class=h><div id=c></div><div id=d></div><div id=e></div><div id=f style="grid-column: 2 / 4"></div></div>
<div class=k><div id=p></div><div id=q></div><div id=r></div><div id=s></div></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('a'), document.getElementById('b'),
  document.getElementById('c'), document.getElementById('d'), document.getElementById('e'),
  document.getElementById('f'), document.getElementById('p'), document.getElementById('q'),
  document.getElementById('r'), document.getElementById('s')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800: `[x, width]`. Qt
/// rounds layout margins to whole pixels, so within half a pixel.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "GridFr"
    when: windowShown

    property var page
    function check(name, x, width) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0).x
        verify(Math.abs(at - x) <= 0.5, name + " x " + at + ", Chromium " + x)
        verify(Math.abs(item.width - width) <= 0.5, name + " width " + item.width + ", Chromium " + width)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_gaps_fall_between_tracks() {
        check("a", 0, 840)
        check("b", 860, 420)
        check("c", 0, 310)
        check("d", 330, 620)
        check("e", 970, 310)
        check("f", 330, 950)
        check("p", 0, 464)
        check("q", 504, 232)
        check("r", 776, 232)
        check("s", 1048, 232)
    }
}
}
"##;

#[test]
fn unequal_fr_columns_match_chromium() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-grid-fr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_grid_fr.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_grid_fr.qml"))
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

/// `fr` tracks with a `minmax()` minimum: a track whose share is under its
/// minimum keeps the minimum and the others share what is left (CSS Grid 2
/// §12.7.1), as the grid resizes too.
const MIN_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div div { height: 20px }
.g { display: grid }
</style></head><body>
<div class=g style="width:300px;grid-template-columns:minmax(200px,1fr) 1fr"><div id=p></div><div id=q></div></div>
<div class=g style="width:500px;grid-template-columns:minmax(200px,1fr) 1fr"><div id=p5></div><div id=q5></div></div>
<div class=g style="width:300px;grid-template-columns:minmax(120px,1fr) minmax(120px,1fr) 1fr"><div id=r></div><div id=s></div><div id=t></div></div>
<div class=g style="width:300px;grid-template-columns:minmax(200px,2fr) 1fr;gap:20px"><div id=u></div><div id=v></div></div>
<div class=g style="width:400px;grid-template-columns:100px minmax(200px,1fr) 1fr"><div id=y1></div><div id=y2></div><div id=y3></div><div id=y4 style="grid-column:2 / 4"></div></div>
<div class=g style="grid-template-columns:minmax(300px,1fr) 1fr"><div id=w></div><div id=x></div></div>
<div class=g style="grid-template-columns:minmax(250px,1fr) 1fr;gap:10%"><div id=z1></div><div id=z2></div></div>
<div class=g style="grid-template-columns:minmax(0,1fr) 1fr"><div id=o1></div><div id=o2></div></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('p'), document.getElementById('q'), document.getElementById('p5'), document.getElementById('q5'), document.getElementById('r'), document.getElementById('s'), document.getElementById('t'), document.getElementById('u'), document.getElementById('v'), document.getElementById('y1'), document.getElementById('y2'), document.getElementById('y3'), document.getElementById('y4'), document.getElementById('w'), document.getElementById('x'), document.getElementById('z1'), document.getElementById('z2'), document.getElementById('o1'), document.getElementById('o2')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800 and 500×800: `[x,
/// width]`.
const MIN_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "GridFrMin"
    when: windowShown

    property var page
    function check(name, x, width) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0).x
        verify(Math.abs(at - x) <= 0.5, name + " x " + at + ", Chromium " + x)
        verify(Math.abs(item.width - width) <= 0.5, name + " width " + item.width + ", Chromium " + width)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_a_share_under_the_minimum_keeps_the_minimum() {
        check("p", 0, 200)
        check("q", 200, 100)
        check("p5", 0, 250)
        check("q5", 250, 250)
        check("r", 0, 120)
        check("s", 120, 120)
        check("t", 240, 60)
        check("u", 0, 200)
        check("v", 220, 80)
        check("y1", 0, 100)
        check("y2", 100, 200)
        check("y3", 300, 100)
        check("y4", 100, 300)
        check("w", 0, 640)
        check("x", 640, 640)
        check("z1", 0, 576)
        check("z2", 704, 576)
        check("o1", 0, 640)
        check("o2", 640, 640)
    }

    function test_the_shares_follow_the_width() {
        page.width = 500
        waitForRendering(page)
        check("w", 0, 300)
        check("x", 300, 200)
        check("z1", 0, 250)
        check("z2", 300, 200)
        check("o1", 0, 250)
        check("o2", 250, 250)
    }
}
}
"##;

#[test]
fn fr_columns_with_a_minimum_match_chromium() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(MIN_PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-grid-fr-min-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_grid_fr_min.qml"), MIN_TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_grid_fr_min.qml"))
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
    assert!(!stderr.contains("Binding loop"), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
