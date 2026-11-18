//! Converted pages driven with real input events through qmltestrunner:
//! the tests go from HTML to a signal firing in Qt, with nothing in between
//! mocked.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, convert_str};

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
    if support::no_qt() {
        return;
    }
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
    let output = support::qmltestrunner()
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

/// Issue #6: a flex `<button>` lays its children out in its own box. The
/// numbers are Chromium's (1280×800, its default fonts, the UA's 13.33px
/// button font): button 105.92×31,
/// "Go" at x 16, the square at (41.78, 9.5), "bold" at x 61.78.
const PILL: &str = r#"<!DOCTYPE html><html><body style="margin:0;font:16px sans-serif">
<button id="pill" style="display:flex;gap:8px;align-items:center;padding:8px 16px;background:#7a4a00;color:#fff;border:0;border-radius:999px"><span>Go</span><span style="display:inline-block;width:12px;height:12px;background:#ffd400"></span><b>bold</b></button>
<button id="tall" style="display:flex;padding:8px;border:0"><span style="width:48px;height:48px;background:#ff0000"></span></button>
<button id="swap" style="display:flex;gap:4px"><span>Go</span><b>bold</b><span aria-hidden="true">G</span></button>
<button id="wide" style="display:inline-flex;width:200px;padding:0;border:0;justify-content:space-between"><span style="width:20px;height:20px;background:#00ff00"></span><span style="width:20px;height:20px;background:#0000ff"></span></button>
<button id="column" style="display:inline-flex;flex-direction:column;justify-content:space-between;height:200px;width:60px;padding:0;border:0"><span style="width:20px;height:20px;background:#00ffff"></span><span style="width:20px;height:20px;background:#ff00ff"></span></button>
<button id="mark">Go</button>
<button id="count" style="display:flex;gap:4px"><span id="n">3</span><span>left</span><span aria-hidden="true">⌘D</span></button>
<input type="submit" id="sub">
<button id="restyle">Old<span aria-hidden="true">X</span></button>
<p id="log">-</p>
<script>
document.getElementById('swap').addEventListener('click', function () {
  this.textContent = 'Done';
});
document.getElementById('mark').addEventListener('click', function () {
  this.innerHTML = '<b>Save</b>';
  this.append('<i> now</i>');
  document.getElementById('sub').append(' now');
});
document.getElementById('restyle').addEventListener('click', function () {
  this.innerHTML = '<b>New</b> <img src="x.png" alt="file"><span aria-hidden="true">⌘N</span>';
});
document.getElementById('count').addEventListener('click', function () {
  document.getElementById('n').textContent = '4';
});
document.getElementById('pill').addEventListener('click', function () {
  var log = document.getElementById('log');
  log.textContent = log.textContent + 'click;';
});
</script></body></html>"#;

const PILL_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Pill"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }
    function under(item, test) {
        if (test(item)) return item
        for (let i = 0; i < item.children.length; ++i) {
            const hit = under(item.children[i], test)
            if (hit) return hit
        }
        return null
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function near(actual, expected, slack, what) {
        verify(Math.abs(actual - expected) <= slack, what + ": " + actual + ", Chromium " + expected)
    }

    function test_children_lay_out_in_the_button() {
        const pill = find("pill")
        near(pill.width, 105.92, 3, "width")
        near(pill.height, 31, 1, "height")
        const go = under(pill, i => i.text === "Go" && i !== pill)
        const dot = under(pill, i => i.color !== undefined && Qt.colorEqual(i.color, "#ffd400"))
        const bold = under(pill, i => i.text === "bold")
        verify(go && dot && bold, "children drawn")
        near(go.mapToItem(pill, 0, 0).x, 16, 1, "Go x")
        near(dot.mapToItem(pill, 0, 0).x, 41.78, 1.5, "square x")
        near(dot.mapToItem(pill, 0, 0).y, 9.5, 1, "square y")
        compare(dot.width, 12)
        compare(dot.height, 12)
        near(bold.mapToItem(pill, 0, 0).x, 61.78, 2, "bold x")
        compare(bold.font.bold, true)
    }

    // A child taller than a line of the button's font makes the button taller.
    function test_a_tall_child_grows_the_button() {
        const tall = find("tall")
        near(tall.width, 64, 1, "width")
        near(tall.height, 64, 1, "height")
        const red = under(tall, i => i.color !== undefined && Qt.colorEqual(i.color, "#ff0000"))
        verify(red, "child drawn")
        near(red.mapToItem(tall, 0, 0).y, 8, 1, "child y")
        compare(red.height, 48)
    }

    // A script's text replaces the boxes a button draws, as in a browser.
    function test_written_text_replaces_the_buttons_children() {
        const swap = find("swap")
        const go = under(swap, i => i.text === "Go" && i !== swap)
        verify(go && go.visible, "children drawn first")
        mouseClick(swap)
        wait(0)
        const done = under(swap, i => i.text === "Done" && i !== swap)
        verify(done && done.visible, "written text drawn")
        verify(!go || !go.visible, "old children no longer drawn")
        // The converter named it from its content, less the hidden "G".
        tryCompare(swap, "eldurContentName", "Done")
        compare(swap.Accessible.name, "Done")
    }

    // A name read again after a write follows the rules it was first read
    // by: hidden content stays out, and flex items stand apart.
    function test_a_content_name_is_read_again_as_a_name() {
        const count = find("count")
        compare(count.eldurContentName, "3 left")
        mouseClick(count)
        tryCompare(count, "eldurContentName", "4 left")
    }

    // Markup written with `innerHTML` names by what it marks up (Chromium:
    // "New file").
    function test_a_content_name_is_read_from_written_markup() {
        const restyle = find("restyle")
        compare(restyle.eldurContentName, "Old")
        mouseClick(restyle)
        tryCompare(restyle, "eldurContentName", "New file")
    }

    // A set height is the column's to share (Chromium: y 0 and 180 in 200 px).
    function test_a_fixed_height_column_button_spreads_its_children() {
        const column = find("column")
        near(column.height, 200, 0.5, "height")
        const cyan = under(column, i => i.color !== undefined && Qt.colorEqual(i.color, "#00ffff"))
        const magenta = under(column, i => i.color !== undefined && Qt.colorEqual(i.color, "#ff00ff"))
        verify(cyan && magenta, "children drawn")
        near(cyan.mapToItem(column, 0, 0).y, 0, 1, "first y")
        near(magenta.mapToItem(column, 0, 0).y, 180, 1, "second y")
    }

    // Text appended after markup is text: its angle brackets are not tags.
    function test_text_appended_after_markup_stays_literal() {
        const mark = find("mark")
        mouseClick(mark)
        wait(0)
        const label = under(mark, i => typeof i.text === "string" && i.text.indexOf("now") >= 0 && i !== mark)
        verify(label, "appended text drawn")
        verify(label.text.indexOf("&lt;i&gt;") >= 0, "escaped: " + label.text)
        // An <input> is void: what a script puts in it is not drawn.
        const sub = find("sub")
        compare(sub.text, "Submit")
        const extra = under(sub, i => typeof i.text === "string" && i.text.indexOf("now") >= 0)
        verify(!extra, "nothing drawn over the value")
    }

    // A set width is the flex layout's to share: space-between puts the
    // second child at the far edge (Chromium: x 0 and 180 in 200 px).
    function test_a_fixed_width_flex_button_spreads_its_children() {
        const wide = find("wide")
        near(wide.width, 200, 0.5, "width")
        const green = under(wide, i => i.color !== undefined && Qt.colorEqual(i.color, "#00ff00"))
        const blue = under(wide, i => i.color !== undefined && Qt.colorEqual(i.color, "#0000ff"))
        verify(green && blue, "children drawn")
        near(green.mapToItem(wide, 0, 0).x, 0, 1, "first x")
        near(blue.mapToItem(wide, 0, 0).x, 180, 1, "second x")
    }

    function test_the_button_still_clicks() {
        mouseClick(find("pill"))
        wait(0)
        compare(find("log").text, "-click;")
    }
}
}
"##;

#[test]
fn a_flex_buttons_children_lay_out_inside_it() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PILL, &options);
    let dir = std::env::temp_dir().join(format!("eldur-pill-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_pill.qml"), PILL_TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_pill.qml"))
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
