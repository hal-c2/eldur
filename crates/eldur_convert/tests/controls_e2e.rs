//! Converted pages driven with real input events through qmltestrunner:
//! the tests go from HTML to a signal firing in Qt, with nothing in between
//! mocked.

use std::path::Path;
use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r#"<!doctype html><html><body>
<form id="f">
  <input id="t" value="abc">
  <input id="t2">
  <input id="n" type="number" value="4">
  <input id="c" type="checkbox" value="yes">
  <label id="cl" for="c" style="display:block;width:100px;height:30px">Toggle</label>
  <input id="r1" type="radio" name="g" value="one" checked>
  <input id="r2" type="radio" name="g" value="two">
  <select id="s"><option value="a">Alpha</option><option value="b">Beta</option></select>
  <button id="go">Go</button>
  <button id="clear" type="reset">Clear</button>
</form>
<div id="hidden" hidden style="width:200px;height:50px;background:#102030">
  <span id="inner" style="color:#112233">Revealed</span>
</div>
<p id="after">after</p>
<p id="log">-</p>
<script>
var log = document.getElementById('log');
// Named so that they get an objectName the test can find.
var named = [document.getElementById('cl'), document.getElementById('r1'), document.getElementById('r2'),
  document.getElementById('clear'), document.getElementById('after')];
function say(s) { log.textContent = log.textContent + s + ';'; }
var t = document.getElementById('t');
var c = document.getElementById('c');
var n = document.getElementById('n');
var s = document.getElementById('s');
t.addEventListener('input', function (e) { say('input:' + e.target.value); });
t.addEventListener('change', function () { say('change'); });
t.addEventListener('keydown', function (e) {
  say('down:' + e.key + ':' + e.code);
  if (e.key === 'x') { e.preventDefault(); }
});
t.addEventListener('keyup', function (e) { say('up:' + e.key); });
t.addEventListener('focus', function () { say('focus'); });
t.addEventListener('blur', function () { say('blur'); });
c.addEventListener('change', function () { say('check:' + c.checked + ':' + c.value); });
s.addEventListener('change', function () { say('select:' + s.value); });
document.getElementById('f').addEventListener('submit', function (e) {
  e.preventDefault();
  say('submit:' + e.defaultPrevented);
});
var shared = function () { say('click'); };
var go = document.getElementById('go');
go.addEventListener('click', shared);
function rmw() {
  t.value = t.value + '!';
  c.checked = !c.checked;
  n.value = '7';
  s.value = 'b';
  say(t.value + ',' + c.checked + ',' + n.value + ',' + s.value);
  document.getElementById('hidden').hidden = false;
}
</script></body></html>"#;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "Controls"
    when: windowShown

    property var page
    function logged() { return find("log").text }
    function find(name) { return findChild(page, name) }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
    }

    function test_typing_fires_key_and_input() {
        const t = find("t")
        t.forceActiveFocus()
        t.cursorPosition = 3
        keyClick(Qt.Key_D)
        compare(t.text, "abcd")
        verify(logged().indexOf("down:d:KeyD;") !== -1, logged())
        verify(logged().indexOf("input:abcd;") !== -1, logged())
        verify(logged().indexOf("up:d;") !== -1, logged())
    }

    function test_prevent_default_stops_the_key() {
        const t = find("t")
        t.forceActiveFocus()
        keyClick(Qt.Key_X)
        compare(t.text, "abc")
    }

    function test_focus_blur_and_change() {
        const t = find("t")
        t.forceActiveFocus()
        verify(logged().indexOf("focus;") !== -1, logged())
        keyClick(Qt.Key_A) // A browser sends `change` only for an edit.
        keyClick(Qt.Key_Tab)
        verify(logged().indexOf("blur;") !== -1, logged())
        verify(logged().indexOf("change;") !== -1, logged())
    }

    function test_enter_submits_with_prevent_default() {
        find("t").forceActiveFocus()
        keyClick(Qt.Key_Return)
        verify(logged().indexOf("submit:true;") !== -1, logged())
    }

    function test_button_click_once_and_submit() {
        mouseClick(find("go"))
        wait(0)
        compare(logged().split("click;").length - 1, 1, logged())
        verify(logged().indexOf("submit:true;") !== -1, logged())
    }

    function test_label_toggles_its_checkbox_once() {
        const c = find("c")
        mouseClick(find("cl"))
        wait(0)
        compare(c.checked, true)
        verify(logged().indexOf("check:true:yes;") !== -1, logged())
    }

    function test_radio_group_is_exclusive() {
        const r1 = find("r1"), r2 = find("r2")
        compare(r1.checked, true)
        mouseClick(r2)
        compare(r2.checked, true)
        compare(r1.checked, false)
    }

    function test_reset_restores() {
        const t = find("t"), c = find("c")
        t.text = "changed"
        c.checked = true
        mouseClick(find("clear"))
        compare(t.text, "abc")
        compare(c.checked, false)
    }

    function test_script_read_modify_write_and_reveal() {
        const hidden = find("hidden"), after = find("after")
        compare(hidden.visible, false)
        const before = after.mapToItem(page, 0, 0).y
        page.rmwForTest()
        compare(find("t").text, "abc!")
        compare(find("c").checked, true)
        compare(find("n").value, 7)
        compare(find("s").currentIndex, 1)
        verify(logged().indexOf("abc!,true,7,b;") !== -1, logged())
        compare(hidden.visible, true)
        wait(50)
        compare(after.mapToItem(page, 0, 0).y, before + 50)
        compare(hidden.color, Qt.color("#102030"))
    }
}
}
"##;

#[test]
fn converted_controls_respond_to_real_input() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let html = PAGE;
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-controls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    // A test can only reach the page's functions through the page's root.
    let main = dir.join("Main.qml");
    let qml = std::fs::read_to_string(&main).expect("Main.qml");
    let at = qml.rfind('}').expect("root object");
    let patched = format!(
        "{}    function rmwForTest() {{ Page.rmw(); }}\n{}",
        &qml[..at],
        &qml[at..]
    );
    std::fs::write(&main, patched).expect("write");
    std::fs::write(dir.join("tst_controls.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_controls.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("eldur:"), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
