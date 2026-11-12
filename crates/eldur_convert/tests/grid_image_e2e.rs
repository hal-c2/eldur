//! A grid item whose width is a percentage of its area, here an image at
//! `width: min(100%, 25rem)`, takes its `fr` share like the rest of its track.
//! Its intrinsic width or its own width must not widen the track, or the `fr`
//! column beside it is squeezed to its narrowest and its text wraps one word
//! per line. Positions are run in qmltestrunner against Chromium's.

use std::process::Command;

use eldur_convert::{Input, Options, Resource, Resources, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

/// Wider than it is tall, so that its `height` attribute gives an intrinsic
/// width (896) past the grid's.
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="498" height="222" viewBox="0 0 498 222"><rect width="498" height="222" fill="#c60"/></svg>"##;

const PAGE: &str = r#"<!DOCTYPE html><html><body style="margin:0;font:20px sans-serif">
<div style="display:grid;grid-template-columns:7fr 4fr;gap:27px;align-items:center">
<div id="t" style="display:flex;flex-direction:column;align-items:flex-start">
<div id="p" style="max-width:400px">A paragraph of ordinary text that is longer than its maximum width, so it wraps onto a few lines.</div>
</div>
<img id="i" src="wide.svg" width="400" height="400" alt="Wide" style="width:min(100%,25rem);height:auto">
</div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('t'), document.getElementById('p'), document.getElementById('i')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 900×600: the two columns share the
/// 873 px beside the gap 7 : 4. Qt rounds layout margins to whole pixels, so
/// within a pixel.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 600

TestCase {
    name: "GridImage"
    when: windowShown

    property var page
    function check(name, x, width) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0).x
        verify(Math.abs(at - x) <= 1, name + " x " + at + ", Chromium " + x)
        verify(Math.abs(item.width - width) <= 1, name + " width " + item.width + ", Chromium " + width)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 900, height: 600 })
        waitForRendering(page)
    }

    function test_the_image_takes_its_fr_share() {
        check("t", 0, 555.53)
        check("p", 0, 400)
        check("i", 582.53, 317.47)
    }
}
}
"##;

struct Files;

impl Resources for Files {
    fn read(&mut self, url: &str, _base: &str) -> Option<Resource> {
        (url == "wide.svg").then(|| Resource {
            resolved: "/page/wide.svg".to_owned(),
            bytes: SVG.as_bytes().to_vec(),
        })
    }
}

#[test]
fn a_percentage_width_image_keeps_the_fr_columns() {
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
        &mut Files,
    );
    let dir = std::env::temp_dir().join(format!("eldur-grid-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        let path = dir.join(&file.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("asset dir");
        std::fs::write(path, &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_grid_image.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_grid_image.qml"))
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
