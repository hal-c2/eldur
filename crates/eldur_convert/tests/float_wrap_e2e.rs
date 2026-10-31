//! Lines beside a float are shortened by the float's margin box, and the
//! blocks after it start as if it were not there (CSS 2.2 §9.5). Each line
//! here is one `<br>`-separated word, so where a line starts or ends does
//! not depend on the fonts: the expected edges are Chromium's layout text
//! (DOMSnapshot) for the same page.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r#"<!DOCTYPE html><html><head><style>
body{margin:0;font:16px/20px sans-serif}
.c{width:400px}
.l{float:left;width:100px;height:30px;margin:0 10px 25px 0;background:#ccc}
.r{float:right;width:100px;height:30px;margin:0 0 0 20px;background:#ccc}
p{margin:0}
.end{text-align:right}
</style></head><body>
<div class="c"><div class="l"></div><p id="a">aa<br>bb</p><p id="b">cc<br>dd</p></div>
<div class="c"><div class="r"></div><p id="e" class="end">ee<br>ff<br>gg</p></div>
<script>
var named = [document.getElementById('a'),
  document.getElementById('b'),
  document.getElementById('e')];
</script></body></html>"#;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "FloatWrap"
    when: windowShown

    property var page
    // The page edges of paragraph `name`'s line boxes, as Qt lays them
    // out: this handler runs after the page's own.
    function lines(name) {
        const text = findChild(page, name)
        const out = []
        const record = line => out.push({ x: line.x, width: line.width, y: line.y })
        text.lineLaidOut.connect(record)
        text.forceLayout()
        text.lineLaidOut.disconnect(record)
        const p = text.mapToItem(page, 0, 0)
        // The first line box starts at the top of the text; Qt's `line.y`
        // is offset within a fixed line height.
        return out.map(l => ({ left: p.x + l.x, right: p.x + l.x + l.width, top: p.y }))
    }
    function near(a, b) { return Math.abs(a - b) < 1 }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_lines_beside_a_left_float_start_after_it() {
        tryVerify(() => {
            const l = lines("a")
            return l.length === 2 && near(l[0].left, 110) && near(l[1].left, 110)
        }, 2000, JSON.stringify(lines("a")))
    }

    function test_the_next_block_wraps_until_the_margin_box_ends() {
        tryVerify(() => {
            const l = lines("b")
            return l.length === 2 && near(l[0].top, 40) && near(l[0].left, 110)
                && near(l[1].left, 0)
        }, 2000, JSON.stringify(lines("b")))
    }

    function test_lines_beside_a_right_float_end_before_it() {
        tryVerify(() => {
            const l = lines("e")
            return l.length === 3 && near(l[0].top, 80) && near(l[0].right, 280)
                && near(l[1].right, 280) && near(l[2].right, 400)
        }, 2000, JSON.stringify(lines("e")))
    }
}
}
"##;

#[test]
fn lines_wrap_beside_floats_as_in_chromium() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-float-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
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
    assert!(!text.contains("recursive"), "{text}");
}
