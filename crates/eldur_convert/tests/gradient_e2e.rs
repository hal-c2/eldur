//! Gradients whose geometry depends on the box: a linear gradient to a
//! corner, whose angle follows the aspect ratio, and conic gradients. The
//! page is rendered with the software backend in qmltestrunner and sampled
//! against Chromium's pixels.

use std::path::Path;
use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r#"<!doctype html><html><body style="margin:0;background:#fff">
<div style="width:400px;height:100px;background:linear-gradient(to top right,#000,#fff)"></div>
<div style="width:400px;height:200px;background:conic-gradient(#f00,#00f)"></div>
<div style="width:200px;height:200px;background:conic-gradient(from 90deg at 25% 50%,#000,#fff)"></div>
<div style="width:300px;height:100px;background:linear-gradient(to bottom left,#000 20px,#fff 80%)"></div>
</body></html>"#;

/// Chromium, a screenshot at 800×600: `[x, y, r, g, b]`.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "Gradients"
    when: windowShown

    property var page
    property var image

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        image = grabImage(page)
    }

    function near(x, y, r, g, b) {
        const c = image.pixel(x, y)
        const got = [Math.round(c.r * 255), Math.round(c.g * 255), Math.round(c.b * 255)]
        const want = [r, g, b]
        for (let i = 0; i < 3; ++i)
            verify(Math.abs(got[i] - want[i]) <= 6,
                   `(${x}, ${y}): got ${got}, Chromium ${want}`)
    }

    // The 50% line joins the top left and bottom right corners.
    function test_to_top_right_follows_the_aspect_ratio() {
        near(1, 1, 126, 126, 126)
        near(200, 50, 127, 127, 127)
        near(398, 98, 129, 129, 129)
        near(200, 1, 190, 190, 190)
        near(100, 50, 95, 95, 95)
        near(2, 97, 4, 4, 4)
        near(397, 2, 251, 251, 251)
    }

    // A px stop is a distance along the line, whose length follows the box.
    function test_corner_stops_in_px_and_percent() {
        near(298, 502, 0, 0, 0)
        near(250, 520, 29, 29, 29)
        near(100, 510, 102, 102, 102)
        near(150, 550, 145, 145, 145)
        near(60, 580, 255, 255, 255)
    }

    // 0deg is up, turning clockwise.
    function test_conic_turns_clockwise_from_the_top() {
        near(390, 200, 191, 0, 63)
        near(200, 290, 127, 0, 127)
        near(10, 200, 64, 0, 191)
    }

    function test_conic_from_an_angle_at_a_position() {
        near(190, 400, 0, 0, 0)
        near(50, 490, 63, 63, 63)
        near(5, 400, 127, 127, 127)
        near(50, 310, 191, 191, 191)
    }
}
}
"##;

#[test]
fn gradients_match_chromium() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-gradient-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_gradient.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_gradient.qml"))
        .env("QT_QUICK_BACKEND", "software")
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
