//! An SVG whose root has an attribute Chromium ignores as invalid (a negative
//! `width` or `height`, a `viewBox` with a negative side or not four numbers)
//! paints in Qt as it does in Chromium. Qt's SVG reader refuses the whole
//! file over such an attribute, so the box was laid out right and left empty.
//!
//! `tests/svg_invalid_root/page.html` is converted and run through
//! qmltestrunner on the software renderer, and the real pixels are compared
//! with a Chromium screenshot's at each `data-pixel` point;
//! `tests/svg_invalid_root/chromium.json` is recorded by
//! `node crates/eldur_convert/tests/svg_invalid_root/gen.mjs`. The `<img>`
//! files get their size from CSS; the inline `jf` takes its height from its
//! own `height` beside a negative `width`, as Chromium does.
//!
//! `bg` is a `background-image`, which a ShapePath fills from the file. The
//! software renderer these tests run on fills a ShapePath from no file at all,
//! a PNG included, so `bg` is checked by its file decoding, not by its pixels.
//! Its pixels were compared on the GPU renderer by hand.

mod support;

use std::path::{Path, PathBuf};

use eldur_convert::{Input, Options, Resource, Resources, RootKind};

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 860

TestCase {
    name: "SvgInvalidRoot"
    when: windowShown

    // The two painters smooth an edge differently; the points are inside.
    readonly property int tolerance: 16

    // Cases whose pixels the software renderer cannot show, by the file they
    // draw from.
    readonly property var decodes: ({ bg: "assets/vb-negw.svg" })

    function test_decodes() {
        for (const id in decodes) {
            const image = createTemporaryQmlObject(
                'import QtQuick; Image { source: "' + Qt.resolvedUrl(decodes[id]) + '" }', host)
            tryVerify(() => image.status !== Image.Loading)
            verify(image.status === Image.Ready && image.sourceSize.width > 0,
                id + ": " + decodes[id] + " does not decode")
        }
    }

    function test_paint() {
        const data = DATA
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 860 })
        waitForRendering(page)
        wait(50)
        const image = grabImage(page)
        for (const id in data) {
            if (id in decodes)
                continue
            for (const p of data[id].pixels) {
                const got = image.pixel(p.x, p.y)
                const near = Math.abs(got.r * 255 - p.rgb[0]) <= tolerance
                    && Math.abs(got.g * 255 - p.rgb[1]) <= tolerance
                    && Math.abs(got.b * 255 - p.rgb[2]) <= tolerance
                if (!near) {
                    console.log("MISMATCH " + id + "@" + p.at + ": got "
                        + Math.round(got.r * 255) + "," + Math.round(got.g * 255) + ","
                        + Math.round(got.b * 255) + " Chromium " + p.rgb)
                }
            }
        }
    }
}
}
"##;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/svg_invalid_root")
}

/// The page's files, read from its directory.
struct Files;

impl Resources for Files {
    fn read(&mut self, url: &str, _base: &str) -> Option<Resource> {
        let bytes = std::fs::read(dir().join(url)).ok()?;
        Some(Resource {
            resolved: format!("/page/{url}"),
            bytes,
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
            html: std::fs::read_to_string(dir().join("page.html")).expect("page"),
            url: "/page/page.html".to_owned(),
        },
        &options,
        &mut Files,
    )
}

#[test]
fn invalid_svg_roots_paint_as_in_chromium() {
    if support::no_qt() {
        return;
    }
    let chromium = std::fs::read_to_string(dir().join("chromium.json")).expect("chromium.json");
    let out = convert();
    let run = std::env::temp_dir().join(format!("eldur-svg-root-{}", std::process::id()));
    for file in &out.files {
        let path = run.join(&file.path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
        std::fs::write(path, &file.contents).expect("write output");
    }
    std::fs::write(
        run.join("tst_paint.qml"),
        TEST.replace("DATA", chromium.trim()),
    )
    .expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(run.join("tst_paint.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("QT_QUICK_BACKEND", "software")
        .output()
        .expect("run qmltestrunner");
    std::fs::remove_dir_all(&run).expect("remove temp dir");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let problems = support::problem_lines(&text, |l| {
        l.contains("FAIL!")
            || l.contains("MISMATCH")
            || l.contains("Error decoding")
            || l.contains("QWARN")
            || l.contains("Error:")
    });
    assert!(
        problems.is_empty() && output.status.success(),
        "{}\n{text}",
        problems.join("\n")
    );
}
