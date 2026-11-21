//! A button the page leaves to the UA sheet is drawn with Chromium's theme
//! corner. `CHROMIUM` is the grey of the top-left 4x4 pixels of both buttons
//! below in Chromium's rendering of the same page (both buttons read the same).
//! A created button is drawn as a page one is.

mod support;

use eldur_convert::{Input, NoResources, Options, RootKind};

const PAGE: &str = r###"<!DOCTYPE html><html><head><style>
body{margin:0;background:#fff;font:13.333px Arial,sans-serif}
div{height:30px;padding:4px}
</style></head><body>
<div><button id="p">Page</button></div>
<div id="d"></div>
<script>
var made = document.createElement("button");
made.textContent = "Made";
document.getElementById("d").appendChild(made);
</script>
</body></html>"###;

const TEST: &str = r###"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "ButtonCorner"
    when: windowShown

    property var page
    readonly property var chromium: [
        [255, 162, 151, 118],
        [162, 171, 239, 239],
        [151, 239, 239, 239],
        [118, 239, 239, 239]
    ]

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function grey(image, x, y) { return Math.round(image.pixel(x, y).r * 255) }

    // The button's top-left corner: its top line, then its left line.
    function corner(image, from) {
        let top = from
        while (top < from + 40 && grey(image, 20, top) > 150) ++top
        let left = 0
        while (left < 20 && grey(image, left, top + 10) > 150) ++left
        return [left, top]
    }

    function check(image, from, what) {
        const [x, y] = corner(image, from)
        const got = []
        let worst = 0
        for (let r = 0; r < 4; ++r) {
            got.push([])
            for (let c = 0; c < 4; ++c) {
                const g = grey(image, x + c, y + r)
                got[r].push(g)
                worst = Math.max(worst, Math.abs(g - chromium[r][c]))
            }
        }
        verify(worst <= 24, what + " corner at " + x + "," + y + ": " + JSON.stringify(got))
        return y
    }

    function test_buttons_draw_chromium_s_corner() {
        wait(100)
        const image = grabImage(page)
        const first = check(image, 0, "page button")
        check(image, first + 25, "created button")
    }
}
}
"###;

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
fn buttons_draw_chromium_s_corner() {
    if support::no_qt() {
        return;
    }
    let out = convert();
    let dir = std::env::temp_dir().join(format!("eldur-button-corner-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_corner.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_corner.qml"))
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
