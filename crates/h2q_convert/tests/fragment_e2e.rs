//! A fragment link scrolls the page as Chromium does: positions are run
//! in qmltestrunner against Chromium's `scrollY` after clicking each link
//! (`tools/visual/geometry.mjs --click`).

use std::path::Path;
use std::process::Command;

use h2q_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!doctype html><html><body style="margin:0">
<p style="margin:0;height:20px"><a href="#b">b</a> <a href="#last">last</a> <a href="#end">end</a> <a href="#nope">nope</a> <a href="#top">top</a></p>
<div style="height:600px"></div><h2 id=b style="margin:30px 0 0;border-top:5px solid red;height:40px">B</h2><div style="height:600px"></div><p id=last style="margin:0;height:20px">last</p><div style="height:1200px"></div><div id=end style="height:10px"></div>
</body></html>"##;

/// Chromium at 1280×800: `scrollY` after following each link.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Fragments"
    when: windowShown

    property var page
    property var flick

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        for (const child of page.children)
            if (child.contentY !== undefined)
                flick = child
        verify(flick)
    }

    function test_a_target_goes_to_the_top() {
        page.scrollToFragment("#b")
        compare(flick.contentY, 650)
        page.scrollToFragment("#last")
        compare(flick.contentY, 1295)
    }

    function test_the_end_of_the_page_clamps() {
        page.scrollToFragment("#end")
        compare(flick.contentY, 1725)
    }

    function test_unknown_and_top() {
        page.scrollToFragment("#b")
        page.scrollToFragment("#nope")
        compare(flick.contentY, 650)
        page.scrollToFragment("#top")
        compare(flick.contentY, 0)
        page.scrollToFragment("#b")
        page.scrollToFragment("#")
        compare(flick.contentY, 0)
    }
}
}
"##;

#[test]
fn fragment_links_scroll_like_chromium() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("h2q-fragment-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_fragment.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_fragment.qml"))
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

fn write(dir: &Path, out: &h2q_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
