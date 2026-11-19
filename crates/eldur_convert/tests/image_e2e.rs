//! `<img>` becomes an `Image` of a copy of the file under `assets/`, sized
//! as Chromium sizes it: intrinsic size, one given dimension and the
//! aspect ratio, percentage widths, and padding and borders around the
//! image. Sizes are run in qmltestrunner against Chromium's.

mod support;

use std::path::Path;

use eldur_convert::{Input, Options, Resource, Resources, RootKind};

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
    if support::no_qt() {
        return;
    }
    let out = convert();
    let dir = std::env::temp_dir().join(format!("eldur-image-{}", std::process::id()));
    run(&dir, &out, "tst_image.qml", TEST);
}

const WIDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200"><rect width="400" height="200" fill="#c60"/></svg>"##;

/// `width`/`height` attributes are presentational hints (HTML §15.4.3):
/// any author rule beats them, `height: auto` and a layered rule included,
/// and they parse as dimension values (`80px`, ` 30.5`, `50%`).
const HINTS_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
img { display: block }
@layer base { .auto { height: auto } }
</style></head><body>
<div style="width:300px"><img id=h src="wide.svg" width="400" height="400" style="width:100%;height:auto"></div>
<div id=after style="height:10px"></div>
<img id=i src="wide.svg" width="100" height="100">
<img id=j src="wide.svg" height="100">
<img id=k class=auto src="wide.svg" width="120" height="400">
<div style="width:300px"><img id=l src="wide.svg" width="50%"></div>
<img id=m src="wide.svg" width="80px" height=" 30.5">
<script>
// Named so that they get an objectName the test can find.
var named = [document.getElementById('h'), document.getElementById('after'), document.getElementById('i'), document.getElementById('j'), document.getElementById('k'), document.getElementById('l'), document.getElementById('m')];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800.
const HINTS_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "ImageHints"
    when: windowShown

    property var page
    function box(name) {
        const item = findChild(page, name)
        verify(item, name)
        const p = item.mapToItem(page, 0, 0)
        return [p.x, p.y, item.width, item.height]
    }
    // Qt Layouts round each cell up to whole pixels: y within a pixel.
    function compareBox(name, expected) {
        const b = box(name)
        compare([b[0], b[2], b[3]], [expected[0], expected[2], expected[3]], name)
        fuzzyCompare(b[1], expected[1], 1.5, name + " y")
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_author_rules_beat_the_attributes() {
        compareBox("h", [0, 0, 300, 150])
        compareBox("after", [0, 150, 1280, 10])
        compareBox("i", [0, 160, 100, 100])
        compareBox("j", [0, 260, 200, 100])
        compareBox("k", [0, 360, 120, 60])
        compareBox("l", [0, 420, 150, 75])
        compareBox("m", [0, 495, 80, 30.5])
    }
}
}
"##;

struct Wide;

impl Resources for Wide {
    fn read(&mut self, url: &str, _base: &str) -> Option<Resource> {
        (url == "wide.svg").then(|| Resource {
            resolved: "/page/wide.svg".to_owned(),
            bytes: WIDE.as_bytes().to_vec(),
        })
    }
}

#[test]
fn image_attributes_are_hints_as_in_chromium() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: HINTS_PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut Wide,
    );
    let dir = std::env::temp_dir().join(format!("eldur-image-hints-{}", std::process::id()));
    run(&dir, &out, "tst_image_hints.qml", HINTS_TEST);
}

/// An SVG with no `width`, `height` or `viewBox`: no natural size and no
/// natural ratio, so only the `img` attributes can give it a ratio.
const BARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg"><rect width="1000" height="1000" fill="#c60"/></svg>"##;

/// The attributes map to `aspect-ratio` through the same rule they are
/// hints with (HTML §15.4.3, §2.3.4.4): `80junk` is 80, `60e1` is 60, `45.`
/// is 45, while `+80`, a percentage or a zero gives no ratio. A natural
/// width or height the file lacks is the default object size's, 300×150
/// (CSS 2.2 §10.3.2, §10.6.2); a zero one is kept.
const BARE_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
img { display: block }
</style></head><body>
<img id=n src="bare.svg" width="80junk" height="40" style="height:auto">
<img id=o src="bare.svg" width="80" height="40.5px" style="width:auto">
<img id=p src="bare.svg" width=" 120" height="60e1" style="height:auto">
<img id=q src="bare.svg" width="90" height="45." style="height:auto">
<div style="width:300px"><img id=r src="bare.svg" width="50%" height="40" style="height:auto"></div>
<img id=s src="bare.svg" width="80" height="0" style="height:auto">
<img id=t src="bare.svg" width="+80" height="40" style="height:auto">
<img id=za src="z80x0.svg">
<img id=zb src="z0x80.svg">
<img id=zc src="w80.svg">
<img id=zd src="h60.svg">
<img id=ze src="pct.svg">
<img id=zf src="z0vb.svg">
<img id=zg src="z80x0.svg" width="100">
<img id=zh src="w80.svg" width="100" height="50" style="height:auto">
<img id=zi src="z80x0.svg" width="100" height="50" style="height:auto">
<script>
// Named so that they get an objectName the test can find.
var named = "n o p q r s t za zb zc zd ze zf zg zh zi".split(" ").map(function (id) { return document.getElementById(id) });
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800.
const BARE_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "ImageRatioAttributes"
    when: windowShown

    property var page
    function box(name) {
        const item = findChild(page, name)
        verify(item, name)
        const p = item.mapToItem(page, 0, 0)
        return [p.x, p.y, item.width, item.height]
    }
    // Qt Layouts round each cell up to whole pixels: y within a pixel.
    function compareBox(name, expected) {
        const b = box(name)
        compare([b[0], b[2], b[3]], [expected[0], expected[2], expected[3]], name)
        fuzzyCompare(b[1], expected[1], 1.5, name + " y")
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_attributes_give_the_ratio_as_dimension_values() {
        compareBox("n", [0, 0, 80, 40])
        compareBox("o", [0, 40, 80, 40.5])
        compareBox("p", [0, 80.5, 120, 60])
        compareBox("q", [0, 140.5, 90, 45])
    }

    function test_no_ratio_takes_the_default_size() {
        compareBox("r", [0, 185.5, 150, 150])
        compareBox("s", [0, 335.5, 80, 150])
        compareBox("t", [0, 485.5, 300, 150])
    }

    function test_zero_and_single_natural_dimensions() {
        compareBox("za", [0, 635.5, 80, 0])
        compareBox("zb", [0, 635.5, 0, 80])
        compareBox("zc", [0, 715.5, 80, 150])
        compareBox("zd", [0, 865.5, 300, 60])
        compareBox("ze", [0, 925.5, 300, 40])
        compareBox("zf", [0, 965.5, 80, 40])
        compareBox("zg", [0, 1005.5, 100, 0])
        compareBox("zh", [0, 1005.5, 100, 50])
        compareBox("zi", [0, 1055.5, 100, 50])
    }
}
}
"##;

/// SVG files with a zero or a single natural dimension (SVG 2 §8.2).
const SVG_FILES: [(&str, &str); 6] = [
    ("z80x0.svg", r#"width="80" height="0""#),
    ("z0x80.svg", r#"width="0" height="80""#),
    ("w80.svg", r#"width="80""#),
    ("h60.svg", r#"height="60""#),
    ("pct.svg", r#"width="50%" height="40""#),
    ("z0vb.svg", r#"width="80" height="0" viewBox="0 0 20 10""#),
];

struct Bare;

impl Resources for Bare {
    fn read(&mut self, url: &str, _base: &str) -> Option<Resource> {
        let bytes = if url == "bare.svg" {
            BARE.to_owned()
        } else {
            let (_, attrs) = SVG_FILES.iter().find(|(name, _)| *name == url)?;
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" {attrs}><rect width="1000" height="1000" fill="#c60"/></svg>"##
            )
        };
        Some(Resource {
            resolved: format!("/page/{url}"),
            bytes: bytes.into_bytes(),
        })
    }
}

fn convert_bare() -> eldur_convert::Output {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    eldur_convert::convert(
        &Input {
            html: BARE_PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut Bare,
    )
}

/// The emitted `implicitWidth`/`implicitHeight` of `<img id=…>`.
fn implicit_size(qml: &str, id: &str) -> (String, String) {
    let start = qml
        .find(&format!("// <img id=\"{id}\">"))
        .unwrap_or_else(|| panic!("no img {id} in\n{qml}"));
    let block = &qml[start..];
    let value = |name: &str| {
        let at = block.find(&format!("{name}: ")).expect(name) + name.len() + 2;
        block[at..].lines().next().unwrap_or_default().to_owned()
    };
    (value("implicitWidth"), value("implicitHeight"))
}

#[test]
fn ratio_attributes_are_dimension_values() {
    let out = convert_bare();
    let qml = out.main_qml().expect("Main.qml");
    let size = |w: &str, h: &str| (w.to_owned(), h.to_owned());
    assert_eq!(implicit_size(qml, "n"), size("80", "40"), "80junk is 80");
    assert_eq!(implicit_size(qml, "p"), size("120", "60"), "60e1 is 60");
    assert_eq!(implicit_size(qml, "q"), size("90", "45"), "45. is 45");
    assert_eq!(
        implicit_size(qml, "t"),
        size("300", "150"),
        "+80 is no width"
    );
    assert_eq!(
        implicit_size(qml, "za"),
        size("80", "0"),
        "a zero height is kept"
    );
    assert_eq!(implicit_size(qml, "zc"), size("80", "150"), "only a width");
}

#[test]
fn image_ratio_attributes_match_chromium() {
    if support::no_qt() {
        return;
    }
    let out = convert_bare();
    let dir = std::env::temp_dir().join(format!("eldur-image-bare-{}", std::process::id()));
    run(&dir, &out, "tst_image_bare.qml", BARE_TEST);
}

/// Writes `out` and `test` to `dir` and runs the test in qmltestrunner.
fn run(dir: &Path, out: &eldur_convert::Output, name: &str, test: &str) {
    std::fs::create_dir_all(dir).expect("temp dir");
    for file in &out.files {
        let path = dir.join(&file.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("asset dir");
        std::fs::write(path, &file.contents).expect("write output");
    }
    std::fs::write(dir.join(name), test).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join(name))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
}
