//! A page that picks its colour scheme with an attribute on `<html>` (a script
//! setting `data-theme` before the body paints, or a toggle later) repaints all
//! of its text, not only the boxes whose own rules name a custom property.
//!
//! Text that only inherits `color` from `body`, a box's inner text, a block
//! made of one inline element, a run's link colour and a border all follow the
//! scheme. A block of one element also takes the font its block now has, and a
//! hovered run stays hovered. The expected colours are
//! what Chromium computes for the same page after the same steps.

use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!DOCTYPE html>
<html><head><style>
:root { --fg: #c1c3c8; --bg: #130f0a; --link: #ff8800; --hover: #44ffaa; --size: 14px; --weight: 400; }
:root[data-theme='light'] { --fg: #23262f; --bg: #ffffff; --link: #0044aa; --hover: #aa0044; --size: 22px; --weight: 700; }
body { margin: 0; background: var(--bg); color: var(--fg); font: 16px sans-serif; }
a { color: var(--link); }
a:hover { color: var(--hover); }
.hov:hover { color: var(--hover); }
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
<p class="hov"><b>Only hovered</b></p>
<p class="sized"><em id="sized">Only sized</em></p>
<p class="weighted"><span id="weighted">Only weighted</span></p>
<p id="out">-</p>
<div class="box" id="box">Boxed</div>
<p><span>Run</span> <em>with</em> <code>parts</code> and <span class="hl">a mark</span></p>
<div>Line text <button>Inline</button> after</div>
<button id="t">Toggle</button>
<script>
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
    function texts(item, out) {
        if (item.eldurTag === "button")
            return out
        if (item.text !== undefined && item.color !== undefined && item.font !== undefined
                && String(item.text).trim() !== "")
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
    function test_hover_survives() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(50)
        const link = textOf(page, "Only hovered")
        verify(link !== null, "no .hov")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Toggle")
        mouseMove(link, 10, link.height / 2)
        wait(50)
        verify(String(link.text).indexOf("#aa0044") >= 0 || Qt.colorEqual(link.color, "#aa0044"),
               "light: hovered block is " + link.color + " " + link.text)
        // Clicked without the pointer leaving the block.
        button.clicked()
        wait(50)
        verify(String(link.text).indexOf("#44ffaa") >= 0 || Qt.colorEqual(link.color, "#44ffaa"),
               "dark: hovered block is " + link.color + " " + link.text)
        mouseMove(page, 790, 590)
        wait(50)
        verify(String(link.text).indexOf("#44ffaa") < 0 && !Qt.colorEqual(link.color, "#44ffaa"),
               "dark: left block is still hovered: " + link.color + " " + link.text)
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
}
