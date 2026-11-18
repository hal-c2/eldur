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
<style>#mark.on { color: rgb(220, 0, 0) } #gen.on::before { content: "GEN " } #pre::before { content: "PRE " } #tbl.on col { background: rgb(0, 0, 220) }</style>
</head><body>
<p id="out">ready, <em id="mark">marked</em> in its line</p>
<p id="gen">tail</p>
<p id="pre">first</p>
<table id="tbl"><colgroup><col></colgroup><tr><td>cell</td></tr></table>
<script>
// A script that reaches the table makes it live: the runtime lays it out.
document.querySelectorAll("table").forEach(function(t) { t.dataset.seen = "1"; });
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
    L("state", JSON.stringify(history.state));
    history.back();
    history.forward();
    L("forward to", location.href.slice(location.href.lastIndexOf("/") + 1));
  }
  // A column's background is painted on its cells by the page's tables.
  if (e.key === "b") {
    document.getElementById("tbl").classList.add("on");
  }
  if (e.key === "i") { L("title now", JSON.stringify(document.title)); }
  // The timeline moves only while the runtime ticks for an animation; a
  // finished one's `currentTime` reads it. Nothing of this page is moving, so
  // two reads apart must agree: an animation left from a gone page would keep
  // the ticker, and the time, going.
  if (e.key === "c") {
    var probe = document.getElementById("gen").animate([{ opacity: 1 }, { opacity: 1 }], { duration: 1 });
    setTimeout(function() {
      var then = probe.currentTime;
      setTimeout(function() { L("timeline still", probe.currentTime === then); }, 300);
    }, 200);
  }
});
// A frame asked for from the timer: HTML orders a 0 ms timer and the next
// frame by nothing, so the page's own frame is checked after its timer ran.
setTimeout(function() {
  L("timer");
  requestAnimationFrame(function() { L("frame"); });
}, 0);
// Due after the first page is gone: it must not run in the next one.
setTimeout(function() { L("late timer"); }, 400);
var ticks = 0;
var tick = setInterval(function() { if (++ticks === 2) { L("interval"); } }, 30);
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

// Another page, made beside the first: generated content of its own, none of
// the first's rules, an address and a history entry of its own, and an
// animation still running when it goes. It uses only what the first
// page's runtime has, which both share.
const OTHER: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Other</title>
<style>#own::before { content: "OWN " }</style>
</head><body>
<p id="own">other</p>
<script>
console.log("LC other at " + location.href.slice(location.href.lastIndexOf("/") + 1));
// The title is the page's own from its `<title>`, and what its script sets
// is the page's too: the earlier page back as the document has its own.
console.log("LC other title " + JSON.stringify(document.title));
document.title = "other set";
document.addEventListener("keydown", function(e) {
  if (e.key === "t") { console.log("LC other key"); }
});
// Still running when the page goes, and an entry of its own on top of the
// shared history.
document.getElementById("own").animate([{ opacity: 0 }, { opacity: 1 }], { duration: 100000 });
history.pushState({ other: 1 }, '', '#other');
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
        // What the first page showed before the other came is still there,
        // before anything of its own redraws it.
        console.log("LC kept generated " + shows(first, "PRE first"))
        first.forceActiveFocus()
        keyClick(Qt.Key_G)
        wait(100)
        console.log("LC generated drawn " + shows(first, "GEN tail"))
        keyClick(Qt.Key_B)
        wait(100)
        console.log("LC column drawn " + blue(first))
        keyClick(Qt.Key_C)
        wait(700)
    }

    // There is one document, as in a browser: the page that is it hears the
    // document's and the window's keys, a page under it hears none until it is
    // the document again, and a page gone hears none at all. The title is
    // each page's own, where the host reads it too.
    function test_6_covered_and_gone_pages_keys_and_title() {
        const first = makePage("page 9")
        const component = Qt.createComponent("Other.qml")
        verify(component.status === Component.Ready, component.errorString())
        const other = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(other)
        console.log("LC host reads " + JSON.stringify(other.title))
        other.forceActiveFocus()
        keyClick(Qt.Key_T)
        wait(50)
        console.log("LC covered page heard nothing")
        other.destroy()
        wait(300)
        first.forceActiveFocus()
        keyClick(Qt.Key_T)
        wait(50)
        keyClick(Qt.Key_I)
        wait(50)
        console.log("LC host reads " + JSON.stringify(first.title))
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
    function blue(page) {
        const image = grabImage(page)
        for (let y = 0; y < 400; ++y) {
            for (let x = 0; x < 600; ++x) {
                const p = image.pixel(x, y)
                if (p.b > 0.7 && p.r < 0.3 && p.g < 0.3) {
                    return true
                }
            }
        }
        return false
    }
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
    // Up beside page 3, page 4 shares its custom elements and its history, but
    // is the document now: its own title, and a key or a click is heard by its
    // listeners only.
    "LC page 4",
    "LC defined before true",
    "LC title \"\" history 2",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
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
    "LC title \"\" history 2",
    "LC back to Main.qml true",
    "LC loaded",
    "LC timer",
    "LC frame",
    "LC interval",
    "LC document key 1",
    "LC window key 1",
    "LC document click",
    // Page 5 is the document again, and hears its own key and click again,
    // its second: it heard none under page 6. Page 6's listeners went with it.
    "LC page 6 gone",
    "LC document key 2",
    "LC window key 2",
    "LC document click",
    // And restyles from its own rules, in its own line's model.
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
    "LC other title \"Other\"",
    "LC kept generated true",
    // Page 7 is the document again: at its own address, with its own rules.
    "LC generated at Main.qml#seen",
    "LC state null",
    "LC forward to Main.qml#seen",
    "LC generated drawn true",
    "LC column drawn true",
    "LC timeline still true",
    "LC page 9",
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
    "LC other title \"Other\"",
    "LC host reads \"other set\"",
    // Page 9, under the other page, hears nothing of its key.
    "LC other key",
    "LC covered page heard nothing",
    // The other page is gone: none of its listeners hear, page 9 hears its
    // own again, and its title is the one its script set.
    "LC document key 2",
    "LC window key 2",
    "LC title now \"set\"",
    "LC host reads \"set\"",
];

// Each test, by the line it starts with, and how many late timers run in it:
// page 1's must not run once page 2 is made, and two pages up at once run both.
const LATE: &[(&str, usize)] = &[
    ("LC page 1", 0),
    ("LC page 2", 1),
    ("LC page 3", 2),
    ("LC page 5", 2),
    ("LC page 7", 1),
    ("LC page 9", 1),
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
    // A runner that crashed after its last line logged all it had to.
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
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

// Pages with no timer, frame or animation: nothing ticks between one going
// and the next being made, so `init` is the first to see it gone. A covered
// page's root resized or scrolled is no resize or scroll of the document.
fn static_page(name: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>{name}</title></head><body>
<p>{name}</p>
<div style="height: 3000px"></div>
<script>
// The test makes the page a title names, inside the key's dispatch.
document.addEventListener("keydown", function(e) {{ if (e.key === "s") document.title = "Next"; }});
window.addEventListener("keydown", function(e) {{ if (e.key === "w") document.title = "Last"; }}, true);
window.addEventListener("resize", function() {{ if (document.title === "Last") document.title = "Fin"; }});
document.addEventListener("keydown", function(e) {{ if (e.key === "e") document.dispatchEvent(new Event("go")); }});
window.addEventListener("go", function() {{ document.title = "End"; }}, true);
document.addEventListener("go", function() {{ console.log("LC {name} go"); }});
// The key a keydown listener made another page with goes no further.
document.addEventListener("keypress", function(e) {{ if (e.key === "s" || e.key === "c") console.log("LC {name} press " + e.key); }});
document.addEventListener("keydown", function(e) {{ console.log("LC {name} key " + e.key); }});
window.addEventListener("resize", function() {{ console.log("LC {name} resize"); }});
window.addEventListener("scroll", function() {{ console.log("LC {name} scroll"); }});
// A signal's abort takes only the registration it came with.
var kept = function(e) {{ console.log("LC {name} kept key " + e.key); }};
var c = new AbortController();
document.addEventListener("keydown", kept, {{ signal: c.signal }});
document.removeEventListener("keydown", kept);
document.addEventListener("keydown", kept);
c.abort();
// And a once listener that ran is gone with its signal's record.
var again = function(e) {{ console.log("LC {name} again key " + e.key); }};
var d = new AbortController();
document.addEventListener("keydown", again, {{ once: true, signal: d.signal }});
document.addEventListener("keydown", function() {{
  document.addEventListener("keydown", again);
  d.abort();
}}, {{ once: true }});
</script>
</body></html>"#
    )
}

const STATIC_TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "StaticPages"
    when: windowShown

    function make(file) {
        const component = Qt.createComponent(file)
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        page.forceActiveFocus()
        return page
    }

    // The page `file` made when `page`'s script sets its title to `file`.
    function opened(page, file) {
        const at = { made: null }
        page.titleChanged.connect(function() {
            if (page.title === file && !at.made) {
                const component = Qt.createComponent(file + ".qml")
                at.made = component.createObject(host, { width: 800, height: 600 })
            }
        })
        return at
    }

    // The page's root scroller: what moves when its document scrolls.
    function scroller(item) {
        for (let i = 0; i < item.children.length; ++i) {
            const c = item.children[i]
            if (c.contentHeight !== undefined && c.contentHeight > c.height) {
                return c
            }
            const deeper = scroller(c)
            if (deeper) {
                return deeper
            }
        }
        return null
    }

    function test_static_pages() {
        const home = make("Home.qml")
        const about = make("About.qml")
        // Home is covered: its window hears no resize or scroll of its root.
        home.width = 700
        scroller(home).contentY = 100
        wait(100)
        // Nor one dispatched at its root by name.
        Eldur.dispatchEvent(home, { type: "resize" })
        console.log("LC covered home resized and scrolled")
        // A key typed in the covered page is no key of the document's.
        home.forceActiveFocus()
        keyClick(Qt.Key_X)
        wait(50)
        console.log("LC key in covered home heard nothing")
        about.forceActiveFocus()
        keyClick(Qt.Key_A)
        wait(50)
        // The host has the focus when About goes, so not even a focus change
        // runs between About going and Contact being made.
        host.forceActiveFocus()
        about.destroy()
        wait(200)
        const contact = make("Contact.qml")
        console.log("LC host reads " + JSON.stringify(contact.title))
        keyClick(Qt.Key_C)
        wait(50)
        keyClick(Qt.Key_D)
        wait(50)
        // A listener that makes another page ends the event's document and
        // window steps, the rest of its own list too: none is the new page's.
        const next = opened(contact, "Next")
        keyClick(Qt.Key_S)
        wait(50)
        console.log("LC key s made " + next.made.title)
        const last = opened(next.made, "Last")
        next.made.forceActiveFocus()
        keyClick(Qt.Key_W)
        wait(50)
        console.log("LC key w made " + last.made.title)
        // The same for a root's resize, and for an event dispatched at the
        // document itself.
        const fin = opened(last.made, "Fin")
        last.made.width = 700
        wait(50)
        console.log("LC resize made " + fin.made.title)
        const end = opened(fin.made, "End")
        fin.made.forceActiveFocus()
        keyClick(Qt.Key_E)
        wait(50)
        console.log("LC key e made " + end.made.title)
        end.made.destroy()
        fin.made.destroy()
        last.made.destroy()
        next.made.destroy()
        wait(200)
        // A scroll queued by a page that goes before the frame is not heard.
        scroller(contact).contentY = 100
        contact.destroy()
        wait(200)
        home.forceActiveFocus()
        keyClick(Qt.Key_H)
        wait(50)
        // Home is the document again, and hears its own resize and scroll again.
        home.width = 650
        wait(50)
        scroller(home).contentY = 200
        wait(100)
        console.log("LC host reads " + JSON.stringify(home.title))
    }
}
}
"##;

const STATIC_WANT: &[&str] = &[
    "LC covered home resized and scrolled",
    "LC key in covered home heard nothing",
    "LC About key a",
    "LC About kept key a",
    "LC About again key a",
    "LC host reads \"Contact\"",
    "LC Contact key c",
    "LC Contact kept key c",
    "LC Contact again key c",
    "LC Contact press c",
    "LC Contact key d",
    "LC Contact kept key d",
    "LC Contact again key d",
    "LC key s made Next",
    "LC key w made Last",
    "LC resize made Fin",
    "LC key e made End",
    "LC Home key h",
    "LC Home kept key h",
    "LC Home again key h",
    "LC Home resize",
    "LC Home scroll",
    "LC host reads \"Home\"",
];

#[test]
fn a_page_made_after_one_went_with_nothing_ticking_hears_only_its_own_listeners() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let dir = std::env::temp_dir().join(format!("eldur-static-pages-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (at, name) in ["Home", "About", "Contact", "Next", "Last", "Fin", "End"]
        .into_iter()
        .enumerate()
    {
        let out = eldur_convert::convert(
            &Input {
                html: static_page(name),
                url: format!("{}/index.html", name.to_lowercase()),
            },
            &options,
            &mut NoResources,
        );
        let script = format!("{}_page.js", name.to_lowercase());
        for file in &out.files {
            let path = file.path.to_string_lossy();
            let contents = String::from_utf8_lossy(&file.contents);
            match path.as_ref() {
                "Main.qml" => std::fs::write(
                    dir.join(format!("{name}.qml")),
                    contents
                        .replace("\"page.js\"", &format!("\"{script}\""))
                        .replace("Main.qml", &format!("{name}.qml")),
                ),
                "page.js" => std::fs::write(dir.join(&script), &file.contents),
                // The pages share the first one's runtime file, and so its state.
                _ if at == 0 => std::fs::write(dir.join(&file.path), &file.contents),
                _ => Ok(()),
            }
            .expect("write page");
        }
    }
    std::fs::write(dir.join("tst_static_pages.qml"), STATIC_TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_static_pages.qml"))
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
    // A runner that crashed after its last line logged all it had to.
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("LC ").map(|at| &l[at..]))
        .collect();
    if logged != STATIC_WANT {
        problems.push(format!(
            "logged\n    {}\n  wanted\n    {}",
            logged.join("\n    "),
            STATIC_WANT.join("\n    ")
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n  "));
}
