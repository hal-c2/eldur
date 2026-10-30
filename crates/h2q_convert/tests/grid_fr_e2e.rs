//! Unequal `fr` columns are split into equal sub-columns; the column gap
//! must fall only between the real tracks, not between the sub-columns of
//! one. Positions are run in qmltestrunner against Chromium's.

use std::path::Path;
use std::process::Command;

use h2q_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

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
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("h2q-grid-fr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_grid_fr.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
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

fn write(dir: &Path, out: &h2q_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
