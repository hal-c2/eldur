//! A growing flex item stops at its `max-width`; the space it leaves is
//! free space that `justify-content` places (Flexbox 1 §9.7, §8.2), not
//! space that drifts into the cells. Positions are run in qmltestrunner
//! against Chromium's.

use std::process::Command;

use h2q_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.row { display: flex; width: 1200px; height: 20px }
.side { flex: 0 0 240px; background: #ccc }
.main { flex: 1; min-width: 0; max-width: 820px; background: #888 }
</style></head><body>
<div class=row><div class=side id=s1></div><div class=main id=m1></div></div>
<div class=row style="width: 900px"><div class=side id=s2></div><div class=main id=m2></div></div>
<div class=row style="justify-content: center"><div class=side id=s3></div><div class=main id=m3></div></div>
<div class=row><div class=side id=s4></div><div class=main id=m4></div><div class=main id=n4></div></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('s1'),
  document.getElementById('m1'),
  document.getElementById('s2'),
  document.getElementById('m2'),
  document.getElementById('s3'),
  document.getElementById('m3'),
  document.getElementById('s4'),
  document.getElementById('m4'),
  document.getElementById('n4')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800: `[x, width]`.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "FlexMax"
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

    function test_free_space_goes_to_the_end() {
        check("s1", 0, 240)
        check("m1", 240, 820)
    }

    function test_below_the_maximum_the_item_fills() {
        check("s2", 0, 240)
        check("m2", 240, 660)
    }

    function test_free_space_is_centred() {
        check("s3", 70, 240)
        check("m3", 310, 820)
    }

    function test_equal_items_share_the_space() {
        check("s4", 0, 240)
        check("m4", 240, 480)
        check("n4", 720, 480)
    }
}
}
"##;

#[test]
fn capped_flex_items_match_chromium() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("h2q-flex-max-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_flex_max.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_flex_max.qml"))
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
