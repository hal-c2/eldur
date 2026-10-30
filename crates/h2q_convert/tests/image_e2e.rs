//! `<img>` becomes an `Image` of a copy of the file under `assets/`, sized
//! as Chromium sizes it: intrinsic size, one given dimension and the
//! aspect ratio, percentage widths, and padding and borders around the
//! image. Sizes are run in qmltestrunner against Chromium's.

use std::path::Path;
use std::process::Command;

use h2q_convert::{Input, Options, Resource, Resources, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="220" viewBox="0 0 320 220"><rect width="320" height="220" fill="#9cc3d5"/></svg>"##;

const PAGE: &str = r#"<!DOCTYPE html><html><head><style>img{display:block}</style></head><body style="margin:0">
<img id="a" src="harbour.svg" alt="Harbour">
<div style="width:500px"><img id="b" src="harbour.svg" style="width:100%"></div>
<img id="c" src="harbour.svg" width="160">
<img id="d" src="harbour.svg" style="height:110px">
<img id="e" src="harbour.svg" style="width:200px;height:50px">
<div style="width:200px"><img id="f" src="harbour.svg" style="width:100%;border:1px solid red;border-radius:8px;box-sizing:border-box"></div>
<div style="width:300px"><img id="g" src="harbour.svg" style="max-width:100%;padding:4px"></div>
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('a'),
  document.getElementById('b'),
  document.getElementById('c'),
  document.getElementById('d'),
  document.getElementById('e'),
  document.getElementById('f'),
  document.getElementById('g')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Images"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }
    function box(name) {
        const item = find(name)
        const p = item.parent.mapToItem(page, item.x, item.y)
        return [p.x, p.y, item.width, item.height]
    }
    // Qt Layouts round each cell up to whole pixels, so the y of a box
    // below a fractional height drifts by up to a pixel per box above it;
    // the size itself is exact.
    function compareBox(name, expected) {
        const b = box(name)
        compare([b[0], b[2], b[3]], [expected[0], expected[2], expected[3]])
        fuzzyCompare(b[1], expected[1], 1.5)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_intrinsic_size() {
        compareBox("a", [0, 0, 320, 220])
    }

    function test_percentage_width_keeps_the_ratio() {
        compareBox("b", [0, 220, 500, 343.75])
    }

    function test_one_dimension_keeps_the_ratio() {
        compareBox("c", [0, 563.75, 160, 110])
        compareBox("d", [0, 673.75, 160, 110])
    }

    function test_both_dimensions_stretch() {
        compareBox("e", [0, 783.75, 200, 50])
    }

    function test_border_box_percentage() {
        compareBox("f", [0, 833.75, 200, 138.125])
    }

    function test_max_width_with_padding() {
        compareBox("g", [0, 971.875, 308, 214.25])
    }
}
}
"##;

struct Files;

impl Resources for Files {
    fn read(&mut self, url: &str, _base: &str) -> Option<Resource> {
        (url == "harbour.svg").then(|| Resource {
            resolved: "/page/harbour.svg".to_owned(),
            bytes: SVG.as_bytes().to_vec(),
        })
    }
}

fn convert() -> h2q_convert::Output {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    h2q_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut Files,
    )
}

#[test]
fn image_is_copied_to_assets() {
    let out = convert();
    let asset = out
        .files
        .iter()
        .find(|f| f.path == Path::new("assets/harbour.svg"))
        .expect("assets/harbour.svg is written");
    assert_eq!(asset.contents, SVG.as_bytes());
    // One copy however often the page uses it.
    assert_eq!(
        out.files
            .iter()
            .filter(|f| f.path.starts_with("assets"))
            .count(),
        1
    );
    let qml = out.main_qml().expect("Main.qml");
    assert!(qml.contains("source: \"assets/harbour.svg\""), "{qml}");
    assert!(qml.contains("Accessible.name: \"Harbour\""), "{qml}");
}

#[test]
fn image_sizes_match_chromium() {
    let out = convert();
    let dir = std::env::temp_dir().join(format!("h2q-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        let path = dir.join(&file.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("asset dir");
        std::fs::write(path, &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_image.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_image.qml"))
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
