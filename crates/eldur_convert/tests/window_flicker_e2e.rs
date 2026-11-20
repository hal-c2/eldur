//! A window that goes away again before it heard it was back.
//!
//! The runtime tells the window it has the focus again from a `callLater`
//! after Qt's report. Under Qt offscreen a real deactivation always comes
//! after that `callLater`, so the test drives the input layer's focus probe
//! itself: away, back and away again in one turn. What had the focus hears
//! its blur once, and the window hears no focus while it is not active. Back
//! for good, both hear they have the focus again.
//!
//! A script that moves the focus while the window is away moves the page's
//! active element at once. What it moved to hears of the focus when the
//! window is back, and what had it hears no second blur. A field it moved
//! from hears its `change` then, and a link in a line moves as any element.

mod support;

use eldur_convert::{Input, NoResources, Options, RootKind};

const PAGE: &str = r##"<!doctype html><html><head><meta charset="utf-8"></head><body>
<button id="b">b</button>
<button id="c">c</button>
<input id="t">
<p>a line with <a href="#l" id="l">a link</a> in it</p>
<script>
function L(){console.log("LC "+[].slice.call(arguments).join(" "))}
["b", "c", "t", "l"].forEach(function(id) {
  var el = document.getElementById(id);
  ["focus", "focusin", "blur", "focusout"].forEach(function(type) {
    el.addEventListener(type, function() { L(id, type, "has", document.hasFocus()); });
  });
});
document.getElementById("t").addEventListener("change", function() { L("t change has", document.hasFocus()); });
window.addEventListener("blur", function() { L("window blur"); });
window.addEventListener("focus", function() { L("window focus"); });
</script>
</body></html>"##;

const TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "WindowFlicker"
    when: windowShown

    function element(item, id) {
        if (item.objectName === id) {
            return item
        }
        for (let i = 0; i < item.children.length; ++i) {
            const found = element(item.children[i], id)
            if (found) {
                return found
            }
        }
        return null
    }

    function test_away_back_and_away_in_one_turn() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        mouseClick(element(page, "b"))
        wait(50)
        const layer = Eldur._in
        verify(layer && layer.focusProbe, "no input layer")
        const real = layer.focusProbe
        const held = real.held
        verify(held, "nothing holds the focus")
        // What the probe would read of Qt, set by the test.
        const win = { active: true }
        const probe = { held: held, active: true, window: win }
        layer.focusProbe = probe
        function report(active) {
            probe.held = active ? held : null
            win.active = active
            probe.active = active
            Eldur._inFocusChanged()
        }
        console.log("LC away")
        report(false)
        console.log("LC back")
        report(true)
        console.log("LC away again")
        report(false)
        wait(100)
        console.log("LC back for good")
        report(true)
        wait(100)
        layer.focusProbe = real
        console.log("LC done")
    }

    function test_field_left_by_a_script_while_away() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        mouseClick(element(page, "t"))
        wait(50)
        keyClick(Qt.Key_X)
        wait(50)
        const layer = Eldur._in
        verify(layer && layer.focusProbe, "no input layer")
        const real = layer.focusProbe
        verify(real.held, "nothing holds the focus")
        const c = element(page, "c")
        const win = { active: true }
        const probe = { held: real.held, active: true, window: win }
        layer.focusProbe = probe
        function report(active, held) {
            probe.held = held
            win.active = active
            probe.active = active
            Eldur._inFocusChanged()
        }
        console.log("LC away")
        report(false, null)
        console.log("LC script focuses c")
        Eldur.document.getElementById("c").focus()
        console.log("LC active " + Eldur.document.activeElement.id)
        wait(50)
        console.log("LC Qt reports c while away")
        report(false, c)
        console.log("LC back")
        report(true, c)
        wait(100)
        console.log("LC active " + Eldur.document.activeElement.id)
        layer.focusProbe = real
        console.log("LC done")
    }
    function test_focus_moved_by_a_script_while_away() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        mouseClick(element(page, "b"))
        wait(50)
        const layer = Eldur._in
        verify(layer && layer.focusProbe, "no input layer")
        const real = layer.focusProbe
        verify(real.held, "nothing holds the focus")
        const c = element(page, "c")
        const win = { active: true }
        const probe = { held: real.held, active: true, window: win }
        layer.focusProbe = probe
        function report(active, held) {
            probe.held = held
            win.active = active
            probe.active = active
            Eldur._inFocusChanged()
        }
        console.log("LC away")
        report(false, null)
        console.log("LC script focuses c")
        Eldur.document.getElementById("c").focus()
        console.log("LC active " + Eldur.document.activeElement.id)
        wait(50)
        console.log("LC Qt reports c while away")
        report(false, c)
        console.log("LC back")
        report(true, c)
        wait(100)
        console.log("LC active " + Eldur.document.activeElement.id)
        layer.focusProbe = real
        console.log("LC done")
    }
    function test_link_in_a_line_focused_by_a_script_while_away() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        mouseClick(element(page, "b"))
        wait(50)
        const layer = Eldur._in
        verify(layer && layer.focusProbe, "no input layer")
        const real = layer.focusProbe
        verify(real.held, "nothing holds the focus")
        const c = Eldur._inInlineHolder(Eldur._unwrap(Eldur.document.getElementById("l")))
        verify(c, "no holder for the link")
        const win = { active: true }
        const probe = { held: real.held, active: true, window: win }
        layer.focusProbe = probe
        function report(active, held) {
            probe.held = held
            win.active = active
            probe.active = active
            Eldur._inFocusChanged()
        }
        console.log("LC away")
        report(false, null)
        console.log("LC script focuses l")
        Eldur.document.getElementById("l").focus()
        console.log("LC active " + Eldur.document.activeElement.id)
        wait(50)
        console.log("LC Qt reports l while away")
        report(false, c)
        console.log("LC back")
        report(true, c)
        wait(100)
        console.log("LC active " + Eldur.document.activeElement.id)
        layer.focusProbe = real
        console.log("LC done")
    }
}
}
"##;

const WANT: &[&str] = &[
    "LC b focus has true",
    "LC b focusin has true",
    "LC away",
    "LC b blur has false",
    "LC b focusout has false",
    "LC window blur",
    "LC back",
    "LC away again",
    "LC back for good",
    "LC window focus",
    "LC b focus has true",
    "LC b focusin has true",
    "LC done",
    "LC t focus has true",
    "LC t focusin has true",
    "LC away",
    "LC t blur has false",
    "LC t focusout has false",
    "LC window blur",
    "LC script focuses c",
    "LC t change has false",
    "LC active c",
    "LC Qt reports c while away",
    "LC back",
    "LC window focus",
    "LC c focus has true",
    "LC c focusin has true",
    "LC active c",
    "LC done",
    "LC b focus has true",
    "LC b focusin has true",
    "LC away",
    "LC b blur has false",
    "LC b focusout has false",
    "LC window blur",
    "LC script focuses c",
    "LC active c",
    "LC Qt reports c while away",
    "LC back",
    "LC window focus",
    "LC c focus has true",
    "LC c focusin has true",
    "LC active c",
    "LC done",
    "LC b focus has true",
    "LC b focusin has true",
    "LC away",
    "LC b blur has false",
    "LC b focusout has false",
    "LC window blur",
    "LC script focuses l",
    "LC active l",
    "LC Qt reports l while away",
    "LC back",
    "LC window focus",
    "LC l focus has true",
    "LC l focusin has true",
    "LC active l",
    "LC done",
];

#[test]
fn a_window_away_again_before_it_heard_it_was_back_hears_one_blur_and_no_focus() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let dir = std::env::temp_dir().join(format!("eldur-window-flicker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    for file in &out.files {
        let path = dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("page dir");
        }
        std::fs::write(path, &file.contents).expect("write page");
    }
    std::fs::write(dir.join("tst_window_flicker.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_window_flicker.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let mut problems: Vec<String> = support::problem_lines(&text, |l| {
        l.contains("FAIL!") || l.contains("Warning:") || l.contains("QWARN") || l.contains("Error:")
    });
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("LC ").map(|at| &l[at..]))
        .collect();
    if logged != WANT {
        problems.push(format!(
            "logged\n    {}\n  wanted\n    {}",
            logged.join("\n    "),
            WANT.join("\n    ")
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n  "));
}
