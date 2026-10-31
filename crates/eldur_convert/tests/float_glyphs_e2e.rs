//! Text beside a float is drawn at the same height in its line boxes as
//! text that is not: a float shortens lines, it does not move glyphs
//! (CSS 2.2 §9.5). In Chromium the first ink of both paragraphs on
//! `27-blog-post` is 5.86px below the top of the paragraph.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r###"<!DOCTYPE html><html><head><style>
body{margin:0;font:18px/1.6 serif;background:#fff;color:#000}
.f{float:right;width:100px;height:100px;background:#fff}
p{margin:0 0 20px;width:500px}
</style></head><body>
<div><div class="f"></div><p id="beside">Hello there, a line beside a float.</p></div>
<p id="plain">Hello there, a line without one.</p>
<script>var named=[document.getElementById('beside'),document.getElementById('plain')];</script>
</body></html>"###;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "FloatGlyphs"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    // Rows from the top of paragraph `name`'s box to its first dark pixel.
    function inkOffset(image, name) {
        const item = find(name)
        const top = Math.round(item.parent.mapToItem(page, 0, item.y).y)
        for (let y = top; y < top + 40; ++y)
            for (let x = 0; x < 400; ++x)
                if (image.pixel(x, y).r < 0.5)
                    return y - top
        return -1
    }

    function test_glyphs_beside_a_float_sit_where_others_do() {
        wait(100)
        const image = grabImage(page)
        const plain = inkOffset(image, "plain")
        verify(plain > 0, "no ink in the plain paragraph")
        // Whole rows of glyphs placed at fractional positions.
        fuzzyCompare(inkOffset(image, "beside"), plain, 1)
    }
}
}
"##;

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
fn text_beside_a_float_is_drawn_where_its_lines_are() {
    let out = convert();
    let dir = std::env::temp_dir().join(format!("eldur-float-glyphs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_glyphs.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_glyphs.qml"))
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
