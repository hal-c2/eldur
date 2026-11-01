//! A line of `EldurLines` that starts on a fractional pixel, with a run at a
//! fractional font size. The row and the widths are Chromium's for the same
//! page: `.a` puts the line's top at 0.8, which Chromium rounds up with the
//! text's baseline; `code` is 85% of 16, 13.6px, where Qt can only draw 14.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!DOCTYPE html><html><head><style>
body{margin:0;font-family:sans-serif;font-size:16px;line-height:1.6}
.a{height:.8px}
code{background:#eee;border-radius:4px;padding:2px 5px;font-size:85%;font-family:monospace}
</style></head><body><div class=a></div>
<p id=p1 style="margin:0">Tidewater reads <code>tidewater.toml</code> in the</p>
<script>var p = document.getElementById('p1');</script>
</body></html>"##;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineRow"
    when: windowShown

    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    // Chromium's screenshot: "T" takes rows 7 to 18.
    function test_a_fractional_line_top_rounds_with_the_baseline() {
        const image = grabImage(page)
        let first = -1
        for (let y = 0; y < 26 && first < 0; y++)
            for (let x = 0; x < 8; x++)
                if (image.pixel(x, y).r < 0.47) { first = y; break }
        compare(first, 7)
    }

    // Chromium: the line, as one span with the `code` in it, is 286.36 wide;
    // drawn at 14px with no correction it comes to 290.
    function test_a_fractional_font_size_keeps_chromiums_advance() {
        fuzzyCompare(findChild(page, "p1").implicitWidth, 286.36, 1.5)
    }
}
}
"##;

#[test]
fn a_fractional_line_and_font_size_come_out_as_in_chromium() {
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &Options {
            root: RootKind::Item,
            ..Options::default()
        },
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-row-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_row.qml"), TEST).expect("write test");
    let run = Command::new(RUNNER)
        .args(["-input", "tst_row.qml"])
        .current_dir(&dir)
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_FONT_DPI", "96")
        .output()
        .expect("qmltestrunner");
    let log = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{log}");
}
