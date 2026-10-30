//! A run in another font family, or with a background or horizontal
//! padding, such as inline `<code>`: the line keeps the run's own advance
//! and the paragraph keeps its line height. Widths and heights are
//! Chromium's `getBoundingClientRect()` for the same page; the code is
//! 14px so that Qt's whole-pixel font sizes are not what is measured.

use std::process::Command;

use h2q_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!DOCTYPE html><html><head><style>
body{margin:0;font-family:sans-serif;font-size:16px;line-height:1.6}
code{padding:2px 5px;border-radius:4px;background:#eff1f3;font-family:monospace;font-size:14px}
p{margin:0}
a{color:#0969da}
</style></head><body>
<p id="a">Reads <code>tidewater.toml</code> here</p>
<p id="b" style="width:200px">one two three <code>four_five_six</code> seven eight nine</p>
<p id="c">See <a href="#a">the <code>queue.url</code> key</a> now</p>
<script>
var named = [document.getElementById('a'),
  document.getElementById('b'),
  document.getElementById('c')];
</script></body></html>"##;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineCode"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_a_monospace_run_with_padding_has_chromiums_width() {
        fuzzyCompare(find("a").implicitWidth, 214.78, 1.5)
        fuzzyCompare(find("a").height, 25.59, 0.5)
    }

    // Chromium's screenshot: the first dark row of "Reads", at the top of
    // the page, is y = 6.
    function test_the_glyphs_sit_where_chromium_puts_them() {
        const image = grabImage(page)
        let first = -1
        for (let y = 0; y < 26 && first < 0; y++)
            for (let x = 0; x < 40; x++)
                if (image.pixel(x, y).r < 0.4) { first = y; break }
        fuzzyCompare(first, 6, 1)
    }

    function test_a_wrapped_paragraph_keeps_its_line_height() {
        fuzzyCompare(find("b").height, 76.78, 0.5)
    }

    function test_a_run_inside_a_link() {
        fuzzyCompare(find("c").implicitWidth, 208.36, 1.5)
    }
}
}
"##;

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
        &mut NoResources,
    )
}

#[test]
fn inline_code_keeps_its_font_background_and_padding() {
    let out = convert();
    let qml = out.main_qml().expect("Main.qml");
    assert!(qml.contains("font-family:"), "{qml}");
    assert!(qml.contains("background-color:"), "{qml}");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("inline background is not converted")
                || d.message.contains("font family `monospace`")
                || d.message.contains("inline padding")),
        "{:#?}",
        out.diagnostics
    );
}

#[test]
fn inline_code_measures_as_in_chromium() {
    let out = convert();
    let dir = std::env::temp_dir().join(format!("h2q-code-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_code.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_code.qml"))
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
