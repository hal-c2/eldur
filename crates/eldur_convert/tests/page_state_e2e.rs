//! A page's state ends with the page.
//!
//! The runtime is a `.pragma library`, one per QML engine, so every page
//! made in that engine shares its module state. In a browser each page has a
//! `document` and a `window` of its own: what a page listens to, the timers and
//! frames it asked for, go with it. Here a `TestCase` makes the same page twice
//! in one engine, one after the other, as a host app does when it loads a page
//! again; each must hear only its own listeners, and get its own timers.

mod support;

use eldur_convert::{Input, NoResources, Options, RootKind};

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8">
<style>#mark.on { color: rgb(220, 0, 0) } #gen.on::before { content: "GEN " }</style>
</head><body>
<p id="out">ready, <em id="mark">marked</em> in its line</p>
<p id="gen">tail</p>
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
// Restyles the page from its own rules: the earlier page, the document again,
// must still have them, and its items.
document.addEventListener("keydown", function(e) {
  if (e.key === "r") {
    document.getElementById("mark").classList.add("on");
    L("restyled", getComputedStyle(document.getElementById("mark")).color);
  }
  if (e.key === "g") {
    document.getElementById("gen").classList.add("on");
    L("generated at", location.href.slice(location.href.lastIndexOf("/") + 1));
  }
});
setTimeout(function() { L("timer"); }, 0);
// Due after the first page is gone: it must not run in the next one.
setTimeout(function() { L("late timer"); }, 400);
var ticks = 0;
var tick = setInterval(function() { if (++ticks === 2) { L("interval"); } }, 30);
requestAnimationFrame(function() { L("frame"); });
// A custom element is the document's: defining it again is the new page's first.
// A page up beside another shares its document, where a second `define` throws.
L("defined before", customElements.get("x-note") !== undefined);
if (!customElements.get("x-note")) {
  customElements.define("x-note", class extends HTMLElement {});
}
// What a page did to its title and its address is the page's.
L("title", JSON.stringify(document.title), "history", history.length);
document.title = "set";
var opened = location.href;
history.pushState(null, '', '#seen');
history.back();
L("back to", opened.slice(opened.lastIndexOf("/") + 1), location.href === opened);
history.forward();
L("loaded");
</script>
</body></html>"#;

// Another page, made beside the first: no generated content, an address of
// its own. It uses only what the first page's runtime has, which both share.
const OTHER: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body>
<p>other</p>
<script>
console.log("LC other at " + location.href.slice(location.href.lastIndexOf("/") + 1));
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
        return page
    }

    function test_1_first() {
        makePage("page 1")
    }

    function test_2_again() {
        makePage("page 2")
        // Longer than the first page's late timer had left.
        wait(500)
    }

    // A page made while another is still up stops nothing of the first: its
    // late timer still runs, and so does the second's.
    function test_3_side_by_side() {
        const first = makePage("page 3")
        makePage("page 4")
        wait(500)
        console.log("LC first still up " + (first.width === 800))
    }

    // And the later one going first leaves the earlier one as it was.
    function test_4_newer_goes_first() {
        const first = makePage("page 5")
        const second = makePage("page 6")
        second.destroy()
        wait(200)
        console.log("LC page 6 gone")
        first.forceActiveFocus()
        keyClick(Qt.Key_T)
        mouseClick(first, 400, 300)
        wait(600)
        keyClick(Qt.Key_R)
        wait(100)
        console.log("LC drawn red " + red(first))
    }

    // A page different from the earlier one going leaves the earlier one with
    // its own generated-content rules and its own address.
    function test_5_other_page_goes_first() {
        const first = makePage("page 7")
        const component = Qt.createComponent("Other.qml")
        verify(component.status === Component.Ready, component.errorString())
        const other = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(other)
        other.destroy()
        wait(300)
        first.forceActiveFocus()
        keyClick(Qt.Key_G)
        wait(100)
        console.log("LC generated drawn " + shows(first, "GEN tail"))
    }

    // Whether some `Text` under `item` shows `text`.
    function shows(item, text) {
        if (typeof item.text === "string" && item.text.indexOf(text) >= 0) {
            return true
        }
        for (let i = 0; i < item.children.length; ++i) {
            if (shows(item.children[i], text)) {
                return true
            }
        }
        return false
    }

    // Whether any of the page's pixels are drawn in `#mark.on`'s colour.
    function red(page) {
        const image = grabImage(page)
        for (let y = 0; y < 60; ++y) {
            for (let x = 0; x < 300; ++x) {
                const p = image.pixel(x, y)
                if (p.r > 0.7 && p.g < 0.3 && p.b < 0.3) {
                    return true
                }
            }
        }
        return false
    }
}
}
"##;

// In the order each page logs them; the late timers, due while the next page
// is being made, are counted per test in `LATE` instead.
const WANT: &[&str] = &[
    "LC page 1",
    "LC defined before false",
    "LC title \"\" history 1",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    "LC page 2",
    "LC defined before false",
    "LC title \"\" history 1",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    "LC page 3",
    "LC defined before false",
    "LC title \"\" history 1",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    // Up beside page 3, page 4 shares its `document`: its custom elements, and
    // a key or a click there is heard by both pages' listeners.
    "LC page 4",
    "LC defined before true",
    "LC title \"set\" history 2",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 2",
    "LC document key 1",
    "LC window key 2",
    "LC window key 1",
    "LC document click",
    "LC first still up true",
    "LC page 5",
    "LC defined before false",
    "LC title \"\" history 1",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    "LC page 6",
    "LC defined before true",
    "LC title \"set\" history 2",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 2",
    "LC document key 1",
    "LC window key 2",
    "LC window key 1",
    "LC document click",
    // Page 5 is the document again, and still hears its key and its click.
    // What page 6 added to the shared document stays with it.
    "LC page 6 gone",
    "LC document key 3",
    "LC document key 2",
    "LC window key 3",
    "LC window key 2",
    "LC document click",
    // And restyles from its own rules, in its own line's model. Page 6's
    // listener is still on the shared document, so it logs too.
    "LC restyled rgb(220, 0, 0)",
    "LC restyled rgb(220, 0, 0)",
    "LC drawn red true",
    "LC page 7",
    "LC defined before false",
    "LC title \"\" history 1",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    "LC other at Other.qml",
    // Page 7 is the document again: at its own address, with its own rules.
    "LC generated at Main.qml#seen",
    "LC generated drawn true",
];

// Each test, by the line it starts with, and how many late timers run in it:
// page 1's must not run once page 2 is made, and two pages up at once run both.
const LATE: &[(&str, usize)] = &[
    ("LC page 1", 0),
    ("LC page 2", 1),
    ("LC page 3", 2),
    ("LC page 5", 2),
    ("LC page 7", 1),
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
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "state/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-page-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    // The other page shares the first one's runtime file, and so its state.
    let other = eldur_convert::convert(
        &Input {
            html: OTHER.to_owned(),
            url: "other/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    for file in &other.files {
        let name = file.path.to_string_lossy();
        let contents = String::from_utf8_lossy(&file.contents);
        match name.as_ref() {
            "Main.qml" => std::fs::write(
                dir.join("Other.qml"),
                contents
                    .replace("\"page.js\"", "\"other_page.js\"")
                    .replace("Main.qml", "Other.qml"),
            ),
            "page.js" => std::fs::write(dir.join("other_page.js"), &file.contents),
            _ => Ok(()),
        }
        .expect("write other page");
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
    let ordered: Vec<_> = logged
        .iter()
        .copied()
        .filter(|l| *l != "LC late timer")
        .collect();
    if ordered != WANT {
        problems.push(format!(
            "logged\n    {}\n  wanted\n    {}",
            ordered.join("\n    "),
            WANT.join("\n    ")
        ));
    }
    let mut late: Vec<(&str, usize)> = Vec::new();
    for line in &logged {
        if let Some(&(start, _)) = LATE.iter().find(|(start, _)| start == line) {
            late.push((start, 0));
        } else if *line == "LC late timer"
            && let Some(last) = late.last_mut()
        {
            last.1 += 1;
        }
    }
    if late != LATE {
        problems.push(format!("late timers per test: {late:?}, wanted {LATE:?}"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n  "));
}
