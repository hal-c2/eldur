//! A `double` border is two lines with a gap between them. The pixel
//! patterns across each border are Chromium's rendering of the same page
//! (each line is round(width / 3) wide).

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r###"<!DOCTYPE html><html><head><style>
body{margin:0;background:#fff}
#box{width:50px;height:30px;border-bottom:5px double #000;border-left:3px double #000}
</style></head><body>
<div id="box"></div>
<script>var named=[document.getElementById('box')];</script>
</body></html>"###;

const TEST: &str = r###"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "DoubleBorder"
    when: windowShown

    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function ink(image, x, y) { return image.pixel(x, y).r < 0.5 ? "#" : "." }

    function test_a_double_border_is_two_lines() {
        wait(100)
        const image = grabImage(page)
        let bottom = ""
        for (let y = 30; y < 35; ++y)
            bottom += ink(image, 25, y)
        let left = ""
        for (let x = 0; x < 3; ++x)
            left += ink(image, x, 15)
        compare(bottom, "##.##")
        compare(left, "#.#")
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
fn double_borders_are_two_lines() {
    let out = convert();
    let dir = std::env::temp_dir().join(format!("eldur-double-border-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_double.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_double.qml"))
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
