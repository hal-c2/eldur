//! A span with a `white-space` of its own inside a text-only button leaves the
//! button its Text node, and the button draws and reads its spaces as each says.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const PAGE: &str = r##"<!doctype html><style>
body{margin:0;font:16px/20px sans-serif}
.p{white-space:pre}.n{white-space:normal}.nw{white-space:nowrap}.f{font-size:20px}
</style>
<p><button id="s"><span class="nw">Save</span></button></p>
<p><button id="m" class="p">A   B<span class="n">x   y</span></button></p>
<p><button id="l" class="p">A   B<span class="f">C</span></button></p>
<p><button id="w" class="p">x</button></p>
<p><button id="i" class="p">x</button></p>
<script>
document.addEventListener("DOMContentLoaded", function () {
    function say(name, value) { console.log("BW " + name + " " + JSON.stringify(value)); }
    var s = document.getElementById("s"), m = document.getElementById("m");
    say("save", [s.childNodes.length, s.textContent, s.getBoundingClientRect().width >= 40]);
    say("mixed", [m.innerText, m.textContent, m.childNodes.length, m.firstChild.nodeValue]);
    say("line", document.getElementById("l").innerText);
    var w = document.getElementById("w"), i = document.getElementById("i");
    w.innerHTML = '<span style="white-space: normal">A  B</span>';
    i.innerHTML = "<span>A  B</span>";
    say("written", [w.innerText, i.innerText]);
    s.firstChild.nodeValue = "Saved";
    say("set", [s.textContent, s.innerText]);
});
</script>
"##;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "ButtonWhiteSpace"
    when: windowShown

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
    }
}
}
"##;

#[test]
fn a_span_with_its_own_white_space_leaves_a_button_its_text() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-button-space-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_space.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_space.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    for want in [
        r#"BW save [1,"Save",true]"#,
        r#"BW mixed ["A   Bx y","A   Bx   y",1,"A   Bx   y"]"#,
        r#"BW line "A   BC""#,
        r#"BW written ["A B","A  B"]"#,
        r#"BW set ["Saved","Saved"]"#,
    ] {
        assert!(text.contains(want), "{want}\n{text}");
    }
    let problems = support::problem_lines(&text, |l| {
        l.contains("FAIL!") || l.contains("QWARN") || l.contains("Error:")
    });
    assert!(problems.is_empty(), "{problems:?}\n{text}");
}
