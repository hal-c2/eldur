//! A page's state ends with the page.
//!
//! The runtime is a `.pragma library`, one per QML engine, so every page
//! made in that engine shares its module state. In a browser each page has a
//! `document` and a `window` of its own: what a page listens to, the timers and
//! frames it asked for, go with it. Here a `TestCase` makes the same page twice
//! in one engine, one after the other, as a host app does when it loads a page
//! again; each must hear only its own listeners, and get its own timers.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<p id="out">ready</p>
<script>
var keys = 0, resized = 0;
function L(){console.log("LC "+[].slice.call(arguments).join(" "))}
document.addEventListener("keydown", function(e) {
  if (e.key === "t") { keys++; L("document key", keys); }
});
window.addEventListener("keydown", function(e) {
  if (e.key === "t") { L("window key", keys); }
});
document.onclick = function() { L("document click"); };
setTimeout(function() { L("timer"); }, 0);
// Due after the first page is gone: it must not run in the next one.
setTimeout(function() { L("late timer"); }, 400);
var ticks = 0;
var tick = setInterval(function() { if (++ticks === 2) { L("interval"); } }, 30);
requestAnimationFrame(function() { L("frame"); });
// A custom element is the document's: defining it again is the new page's first.
L("defined before", customElements.get("x-note") !== undefined);
customElements.define("x-note", class extends HTMLElement {});
L("loaded");
</script>
</body></html>"#;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "PageState"
    when: windowShown

    function makePage(label) {
        console.log("LC " + label)
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(150)
        page.forceActiveFocus()
        keyClick(Qt.Key_T)
        mouseClick(page, 400, 300)
        wait(50)
    }

    function test_1_first() {
        makePage("page 1")
    }

    function test_2_again() {
        makePage("page 2")
        // Longer than the first page's late timer had left.
        wait(500)
    }
}
}
"##;

const WANT: &[&str] = &[
    "LC page 1",
    "LC defined before false",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    "LC page 2",
    "LC defined before false",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    "LC late timer",
];

#[test]
fn a_page_made_again_in_one_engine_hears_only_its_own_listeners() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-page-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_page_state.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_page_state.qml"))
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
