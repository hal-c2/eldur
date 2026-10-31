//! A block whose whole content is one inline box: the line box is the
//! strut and the inline box aligned on their baseline, and the inline
//! box's `letter-spacing` and `text-transform` apply. Positions and widths
//! are Chromium's `getBoundingClientRect()` for the same page.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r###"<!DOCTYPE html><html><head><style>
body{margin:0;font-family:serif;font-size:18px;line-height:1.6}
#head{padding:20px 0;text-align:center}
#head a{font-family:sans-serif;font-size:22px;font-weight:700;letter-spacing:0.2em;text-transform:uppercase;text-decoration:none;color:#222}
#small{font-size:30px;line-height:40px}
#small span{font-size:12px;letter-spacing:2px}
p{margin:0}
</style></head><body>
<header id="head"><a id="title" href="#">Field Notes</a></header>
<p id="after">after</p>
<div id="small"><span id="sm">tiny words</span></div>
<p id="end">end</p>
<script>var named=[document.getElementById('head'),document.getElementById('after'),document.getElementById('small'),document.getElementById('end')];</script>
</body></html>"###;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "SingleSpanLine"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    // Heights of the boxes as laid out; a ColumnLayout then rounds each
    // cell up to whole pixels, which these do not measure.
    function heightOf(name) { return find(name).implicitHeight }

    // A 22px span with `line-height: 1.6` in 18px text: a 35.2px line
    // and 40px of padding.
    function test_a_larger_span_makes_the_line_taller() {
        fuzzyCompare(heightOf("head"), 75.19, 0.5)
    }

    // A 12px span in a 40px line of 30px text: the strut's ascent and the
    // span's descent.
    function test_a_smaller_span_hangs_below_the_strut() {
        fuzzyCompare(heightOf("small"), 46, 0.5)
    }

    // Uppercase with 0.2em after every letter.
    function test_the_span_keeps_its_letter_spacing_and_case() {
        const text = find("head").children[0]
        fuzzyCompare(text.contentWidth, 193.86, 3)
        fuzzyCompare(find("small").contentWidth, 71, 3)
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
fn a_single_span_sets_the_line() {
    let out = convert();
    let dir = std::env::temp_dir().join(format!("eldur-single-span-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_span.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_span.qml"))
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
