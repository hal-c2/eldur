//! `text-decoration` style and colour from a rule a script's element matches.
//! The computed values are Chromium's for this page.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!DOCTYPE html><html><head><style>
body{margin:20px;font:20px/30px sans-serif}
.w{text-decoration:underline wavy red}
.p{color:#080;text-decoration:underline dotted}
.s{text-decoration:underline}
.o{text-decoration:overline dashed green}
</style></head><body><p>Static</p><script>
var made = {};
["w", "p", "s", "o"].forEach(function(c) {
  var d = document.createElement("div");
  d.className = c;
  d.textContent = "Created " + c;
  document.body.appendChild(d);
  made[c] = d;
});
</script></body></html>"##;

const TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "RuntimeTextLines"
    when: windowShown

    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        wait(50)
    }

    function read(c, prop) {
        return Eldur.getComputedStyle(Eldur.document.querySelector("." + c)).getPropertyValue(prop)
    }

    function drawn(item, out) {
        if (item.lineStyle !== undefined)
            out.push(item.lineStyle + " " + item.lineColor + " " + item.underline)
        for (let i = 0; i < item.children.length; i++)
            drawn(item.children[i], out)
        return out
    }

    function test_style_and_colour_are_drawn() {
        const lines = drawn(page, []).sort()
        compare(lines, ["dashed #008000 false", "dotted #008800 true", "wavy #ff0000 true"])
    }

    function test_they_read_back_as_chromium_says() {
        compare(read("w", "text-decoration-style"), "wavy")
        compare(read("w", "text-decoration-color"), "rgb(255, 0, 0)")
        compare(read("p", "text-decoration-color"), "rgb(0, 136, 0)")
        compare(read("s", "text-decoration-line"), "underline")
    }
}
}
"##;

#[test]
fn a_run_time_rules_decoration_keeps_its_style_and_colour() {
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &Options {
            root: RootKind::Item,
            // What the test asks the runtime itself.
            runtime_uses: ["document", "getComputedStyle"].map(String::from).to_vec(),
            ..Options::default()
        },
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-text-lines-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_lines.qml"), TEST).expect("write test");
    let run = Command::new(RUNNER)
        .args(["-input", "tst_lines.qml"])
        .current_dir(&dir)
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_FONT_DPI", "96")
        .output()
        .expect("qmltestrunner");
    let log = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{log}");
}
