//! A page that picks its colour scheme with an attribute on `<html>` (a script
//! setting `data-theme` before the body paints, or a toggle later) repaints all
//! of its text, not only the boxes whose own rules name a custom property.
//!
//! Text that only inherits `color` from `body`, a box's inner text, a block
//! made of one inline element, a run's link colour and a border all follow the
//! scheme. A block of one element also takes the font its block now has, as Qt
//! holds it (a whole pixel size, never below 1), and is not rewritten while
//! it stays. Blocks in a state (two hovered at once, one pressed) keep that
//! state's values in the new scheme, and the rest take the new scheme. The
//! expected colours are what Chromium computes for the same page after the
//! same steps.

use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!DOCTYPE html>
<html><head><style>
:root { --fg: #c1c3c8; --bg: #130f0a; --link: #ff8800; --hover: #44ffaa; --wrap: #997700; --act: #3366ff; --size: 14px; --weight: 400; --frac: 16px; --tiny: 2px; }
:root[data-theme='light'] { --fg: #23262f; --bg: #ffffff; --link: #0044aa; --hover: #aa0044; --wrap: #007799; --act: #cc3300; --size: 22px; --weight: 700; --frac: 13.6px; --tiny: 0.4px; }
body { margin: 0; background: var(--bg); color: var(--fg); font: 16px sans-serif; }
a { color: var(--link); }
a:hover { color: var(--hover); }
.hov:hover { color: var(--hover); }
.wrap:hover { color: var(--wrap); }
.act:active { color: var(--act); }
.frac { font-size: var(--frac); }
.tiny { font-size: var(--tiny); }
:root[data-theme='light'] .shy { display: none; }
.sized { font-size: var(--size); }
.weighted { font-weight: var(--weight); }
.box { border: 1px solid var(--fg); padding: 4px; }
.hl { color: var(--link); }
</style>
<script>document.documentElement.dataset.theme = 'light';</script>
</head><body>
<h1>Title</h1>
<p id="p">Body <b>bold</b> and <a href="#x">a link</a>.</p>
<p><b>Only bold</b></p>
<div>Text <i>only italic</i></div>
<div class="wrap">Outer <p class="hov"><b>Only hovered</b></p></div>
<p class="act"><b>Only pressed</b></p>
<p class="frac"><em>Only fractional</em></p>
<p class="tiny"><em>Only tiny</em></p>
<p class="frac shy"><em>Shy fractional</em></p>
<p class="tiny shy"><em>Shy tiny</em></p>
<p class="sized"><em id="sized">Only sized</em></p>
<p class="weighted"><span id="weighted">Only weighted</span></p>
<p id="out">-</p>
<div class="box" id="box">Boxed</div>
<p><span>Run</span> <em>with</em> <code>parts</code> and <span class="hl">a mark</span></p>
<div>Line text <button>Inline</button> after</div>
<button id="t">Toggle</button>
<button id="poke">Poke</button>
<script>
// A key, as the pointer is busy pressing. On the body: the runtime's
// `document` is every page's in one engine, and keeps an earlier page's (#30).
document.body.addEventListener('keydown', e => {
  if (e.key === 't') {
    const r = document.documentElement;
    r.dataset.theme = r.dataset.theme === 'light' ? 'dark' : 'light';
  }
});
document.getElementById('poke').addEventListener('click', () => {
  document.documentElement.dataset.poked = 'yes';
});
document.getElementById('t').addEventListener('click', () => {
  const r = document.documentElement;
  r.dataset.theme = r.dataset.theme === 'light' ? 'dark' : 'light';
  document.getElementById('out').textContent = getComputedStyle(document.getElementById('sized')).fontSize
    + ' ' + getComputedStyle(document.getElementById('weighted')).fontWeight;
});
</script>
</body></html>
"##;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "RootScheme"
    when: windowShown

    // Every piece of page text the converter made, skipping controls, whose
    // text the style draws.
    // The `.shy` blocks are only there for their sizes: a scheme that hides
    // them leaves them showing in the old colours (#29).
    function texts(item, out) {
        if (item.eldurTag === "button")
            return out
        if (item.text !== undefined && item.color !== undefined && item.font !== undefined
                && String(item.text).trim() !== "" && String(item.text).indexOf("Shy") < 0)
            out.push(item)
        for (const child of item.children)
            texts(child, out)
        return out
    }

    function find(item, test) {
        if (test(item))
            return item
        for (const child of item.children) {
            const got = find(child, test)
            if (got)
                return got
        }
        return null
    }

    function textOf(page, words) {
        return find(page, i => i.text !== undefined && i.font !== undefined && String(i.text).indexOf(words) >= 0)
    }

    // What a block of one element inherits from the block: on its `Text`, and
    // as a script reads it.
    function fonts(page, size, weight, step) {
        const sized = textOf(page, "Only sized")
        verify(sized !== null, "no #sized")
        compare(sized.font.pixelSize, size, step + ": #sized is " + sized.font.pixelSize + " px")
        // The element's own style is on the `Text` or in its markup.
        verify(sized.font.italic || /<i>|font-style: ?italic/.test(sized.text),
               step + ": #sized is not italic: " + sized.text)
        const weighted = textOf(page, "Only weighted")
        verify(weighted !== null, "no #weighted")
        compare(weighted.font.weight, weight, step + ": #weighted weighs " + weighted.font.weight)
    }

    // A colour on the `Text`, or written into its markup.
    function painted(t, c) {
        return String(t.text).toLowerCase().indexOf(c) >= 0 || Qt.colorEqual(t.color, c)
    }

    function scheme(page, fg, bg, link, step) {
        if (page.color !== undefined)
            verify(Qt.colorEqual(page.color, bg), step + ": canvas " + page.color + " is not " + bg)
        const found = texts(page, [])
        verify(found.length >= 4, step + ": only " + found.length + " texts")
        for (const t of found) {
            const what = step + ": " + (t.eldurTag || "text") + " \"" + String(t.text).slice(0, 40) + "\""
            verify(Qt.colorEqual(t.color, fg), what + " is painted " + t.color + ", not " + fg)
            // A run's own colours are written into its markup.
            const baked = String(t.text).match(/#[0-9a-f]{6}/gi) || []
            for (const c of baked)
                verify(Qt.colorEqual(c, fg) || Qt.colorEqual(c, link), what + " keeps " + c)
            if (t.linkColor !== undefined && String(t.text).indexOf("<a ") >= 0)
                verify(Qt.colorEqual(t.linkColor, link), what + " links in " + t.linkColor + ", not " + link)
        }
        const box = find(page, i => i.eldurTag === "div" && i.border !== undefined)
        verify(box !== null, "no #box")
        verify(Qt.colorEqual(box.border.color, fg), step + ": #box border " + box.border.color + " is not " + fg)
    }

    function test_scheme() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(50)
        scheme(page, "#23262f", "#ffffff", "#0044aa", "load")
        fonts(page, 22, 700, "load")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Toggle")
        verify(button !== null, "no #t")
        mouseClick(button)
        wait(50)
        scheme(page, "#c1c3c8", "#130f0a", "#ff8800", "dark")
        fonts(page, 14, 400, "dark")
        compare(textOf(page, "px").text, "14px 400", "dark: what the script reads")
        mouseClick(button)
        wait(50)
        scheme(page, "#23262f", "#ffffff", "#0044aa", "light")
        fonts(page, 22, 700, "light")
        compare(textOf(page, "px").text, "22px 700", "light: what the script reads")
    }

    // A block under the pointer stays hovered while the scheme moves under it,
    // and its one element takes the new scheme's hover colour. The block is
    // above the text the scheme resizes, which would move it from under the
    // pointer.
    // The pointer over `.hov` is over `.wrap` too: both blocks are hovered.
    function test_hover_survives() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(50)
        const link = textOf(page, "Only hovered")
        verify(link !== null, "no .hov")
        const outer = textOf(page, "Outer")
        verify(outer !== null, "no .wrap")
        const bold = textOf(page, "Only bold")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Toggle")
        mouseMove(link, 10, link.height / 2)
        wait(50)
        verify(painted(link, "#aa0044"), "light: hovered block is " + link.color + " " + link.text)
        verify(painted(outer, "#007799"), "light: hovered outer block is " + outer.color + " " + outer.text)
        // Clicked without the pointer leaving the blocks.
        button.clicked()
        wait(50)
        verify(painted(link, "#44ffaa"), "dark: hovered block is " + link.color + " " + link.text)
        verify(painted(outer, "#997700"), "dark: hovered outer block is " + outer.color + " " + outer.text)
        verify(painted(bold, "#c1c3c8"), "dark: a block nobody hovers is " + bold.color + " " + bold.text)
        mouseMove(page, 790, 590)
        wait(50)
        verify(!painted(link, "#44ffaa") && painted(link, "#c1c3c8"),
               "dark: left block is still hovered: " + link.color + " " + link.text)
        verify(!painted(outer, "#997700") && painted(outer, "#c1c3c8"),
               "dark: left outer block is still hovered: " + outer.color + " " + outer.text)
    }

    // A block pressed while the scheme moves (from a key: the pointer is
    // busy) stays pressed in the new one's colour; let go, it is the new
    // scheme's.
    function test_pressed_survives() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(50)
        const act = textOf(page, "Only pressed")
        verify(act !== null, "no .act")
        const bold = textOf(page, "Only bold")
        // A window gives the page's keys focus as it shows; the test's host
        // does not.
        find(page, i => typeof i.keyStep === "function").forceActiveFocus()
        mousePress(act, 10, act.height / 2)
        wait(50)
        verify(painted(act, "#cc3300"), "light: pressed block is " + act.color + " " + act.text)
        keyClick(Qt.Key_T)
        wait(50)
        verify(painted(act, "#3366ff"), "dark: pressed block is " + act.color + " " + act.text)
        verify(painted(bold, "#c1c3c8"), "dark: a block nobody presses is " + bold.color + " " + bold.text)
        mouseRelease(act, 10, act.height / 2)
        wait(50)
        verify(!painted(act, "#3366ff") && painted(act, "#c1c3c8"),
               "dark: released block is " + act.color + " " + act.text)
    }

    // The converter gives `.frac` and `.tiny` the scheme without the
    // attribute (16 and 2 px); the head script's scheme gives them 13.6 and
    // 0.4 px. Qt holds a whole pixel size of at least 1: 14 and 1. The `.shy`
    // ones are hidden in that scheme, so only their first render sizes them.
    // A repaint that changes none of them does not write them again.
    function test_sizes_as_qt_holds_them() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(50)
        const frac = textOf(page, "Only fractional")
        verify(frac !== null, "no .frac")
        const tiny = textOf(page, "Only tiny")
        verify(tiny !== null, "no .tiny")
        const shyFrac = textOf(page, "Shy fractional")
        verify(shyFrac !== null, "no .frac.shy")
        const shyTiny = textOf(page, "Shy tiny")
        verify(shyTiny !== null, "no .tiny.shy")
        const sized = [frac, tiny, shyFrac, shyTiny]
        compare(frac.font.pixelSize, 14, "light: .frac")
        compare(tiny.font.pixelSize, 1, "light: .tiny")
        compare(shyFrac.font.pixelSize, 14, "light: .frac.shy")
        compare(shyTiny.font.pixelSize, 1, "light: .tiny.shy")
        let writes = 0
        const count = () => ++writes
        sized.forEach(t => t.fontChanged.connect(count))
        find(page, i => i.eldurTag === "button" && i.text === "Poke").clicked()
        wait(50)
        sized.forEach(t => t.fontChanged.disconnect(count))
        compare(writes, 0, "a repaint wrote the same sizes again")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Toggle")
        button.clicked()
        wait(50)
        compare(frac.font.pixelSize, 16, "dark: .frac")
        compare(tiny.font.pixelSize, 2, "dark: .tiny")
        compare(shyFrac.font.pixelSize, 16, "dark: .frac.shy")
        compare(shyTiny.font.pixelSize, 2, "dark: .tiny.shy")
        button.clicked()
        wait(50)
        compare(frac.font.pixelSize, 14, "light again: .frac")
        compare(tiny.font.pixelSize, 1, "light again: .tiny")
    }
}
}
"##;

#[test]
fn a_root_attribute_repaints_inherited_text() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-root-scheme-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_scheme.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_scheme.qml"))
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
    assert!(!text.contains("Warning:"), "{text}");
    // Qt's own warnings, such as a font's pixel size below 1.
    assert!(!text.contains("QWARN"), "{text}");
    assert!(!text.contains("Pixel size <= 0"), "{text}");
}
