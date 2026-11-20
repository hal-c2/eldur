//! A window that goes away again before it heard it was back.
//!
//! The runtime tells the window it has the focus again from a `callLater`
//! after Qt's report. Under Qt offscreen a real deactivation always comes
//! after that `callLater`, so the test drives the input layer's focus probe
//! itself: away, back and away again in one turn. What had the focus hears
//! its blur once, and the window hears no focus while it is not active. Back
//! for good, both hear they have the focus again.

mod support;

use eldur_convert::{Input, NoResources, Options, RootKind};

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<button id="b">b</button>
<script>
function L(){console.log("LC "+[].slice.call(arguments).join(" "))}
var b = document.getElementById("b");
["focus", "focusin", "blur", "focusout"].forEach(function(type) {
  b.addEventListener(type, function() { L("b", type, "has", document.hasFocus()); });
});
window.addEventListener("blur", function() { L("window blur"); });
window.addEventListener("focus", function() { L("window focus"); });
</script>
</body></html>"#;

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
