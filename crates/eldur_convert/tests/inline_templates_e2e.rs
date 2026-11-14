//! Inline elements a script creates in a block (`docs/design/dynamic-styles.md`
//! §6.8): `<span>`, `<strong>` and `<a>` appended to a block `<li>`, or
//! written into it as markup, are nodes on the line's `EldurLines` model,
//! drawn in its runs with the page's colours, weights, decorations and
//! backgrounds, a class toggle on them and `:hover` on their block. What is
//! drawn is read from the runs' markup and the boxes behind them; where an
//! element is, from its rect as a script reads it.
//!
//! The page is `tests/inline_templates/page.html`, driven with real input
//! through qmltestrunner. The expected values are Chromium's after the same
//! steps, recorded in `tests/inline_templates/chromium.json` by
//! `node crates/eldur_convert/tests/inline_templates/gen.mjs`.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("inline_templates/chromium.json");
const PAGE: &str = include_str!("inline_templates/page.html");

/// Converts the page and runs `tests` (TestCase functions) on it in qml6,
/// with the helpers in [`TEST`]. Any QML or runtime warning fails the test.
fn check(case: &str, tests: &str) {
    let options = Options {
        root: RootKind::Item,
        // What the test asks the runtime itself.
        runtime_uses: ["document"].map(String::from).to_vec(),
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: format!("{case}/index.html"),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-inline-{case}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let test = TEST
        .replace("@CHROMIUM@", CHROMIUM)
        .replace("@TESTS@", tests);
    std::fs::write(dir.join("tst_inline.qml"), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_inline.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("XDG_DATA_HOME", dir.join("data"))
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
    assert!(!text.contains("Warning:"), "{text}");
}

#[test]
fn created_inline_elements_in_a_block() {
    check("inline", FOCUSED);
}

const TEST: &str = r##"
import QtQuick
import QtQuick.LocalStorage
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineTemplates"
    when: windowShown

    readonly property int settled: 100
    readonly property var chromium: (@CHROMIUM@)
    property var page

    function init() {
        // Each Chromium case starts from an empty `localStorage`.
        LocalStorage.openDatabaseSync("eldur_local_storage", "1.0", "Eldur localStorage", 1048576)
            .transaction(tx => tx.executeSql("DROP TABLE IF EXISTS eldur_kv"))
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        mouseMove(page, 1279, 799)
    }

    function want(name, prop) {
        const c = chromium[name]
        verify(c !== undefined, "no Chromium values for " + name)
        verify(c[prop] !== undefined, "no " + prop + " in " + name)
        return c[prop]
    }
    function boxes(name, select) {
        const c = chromium[name]
        verify(c !== undefined && c[select] !== undefined, "no boxes for " + select + " in " + name)
        return c[select]
    }
    // `--eldur-order`: the texts, in document order, as one CSS string.
    function order(name) {
        return JSON.parse(want(name, "--eldur-order")).split("|")
    }

    function items(item, out) {
        out.push(item)
        for (let i = 0; i < item.children.length; i++)
            items(item.children[i], out)
        return out
    }
    function named(name) {
        const found = findChild(page, name)
        verify(found !== null, "no item named " + name)
        return found
    }
    function pageY(item) { return item.mapToItem(page, 0, 0).y }
    // The shown objects of the elements with `tag` under `item`, top to bottom.
    function tagged(item, tag) {
        return items(item, []).filter(i => i.eldurTag === tag && i.visible && i.width > 0)
            .sort((a, b) => pageY(a) - pageY(b))
    }

    function position(item) {
        return item.mapToItem(page, item.width / 2, item.height / 2)
    }
    function hover(item) {
        const p = position(item)
        mouseMove(page, p.x, p.y)
        wait(settled)
    }
    function click(item) {
        const p = position(item)
        mouseMove(page, p.x, p.y)
        mouseClick(page, p.x, p.y)
        wait(settled)
    }

    function css(text) {
        const m = text.match(/rgba?\(([^)]*)\)/)
        verify(m !== null, "not a colour: " + text)
        const v = m[1].split(",").map(Number)
        return { r: v[0], g: v[1], b: v[2], a: v.length > 3 ? v[3] : 1 }
    }
    function sameColor(got, cssText, what) {
        const w = css(cssText)
        const ok = Math.abs(got.r * 255 - w.r) <= 1 && Math.abs(got.g * 255 - w.g) <= 1
            && Math.abs(got.b * 255 - w.b) <= 1 && Math.abs(got.a - w.a) <= 0.01
        verify(ok, what + ": " + got + " is not " + cssText)
    }
    function near(got, expected, tolerance, what) {
        verify(Math.abs(got - expected) <= tolerance, what + ": " + got + " is not " + expected)
    }
    // `color` and `text-decoration-line` of a text element.
    function sameText(text, name, what) {
        sameColor(text.color, want(name, "color"), what + " colour")
        compare(text.font.strikeout, want(name, "text-decoration-line") === "line-through",
            what + " line-through")
    }
    // `origin` is the page, or the list for the items in it: the static
    // page above the list is not what these tests are about.
    function sameBox(item, box, what, origin) {
        const p = item.mapToItem(origin ? origin.item : page, 0, 0)
        near(p.x, box.x - (origin ? origin.box.x : 0), 1, what + " x")
        near(p.y, box.y - (origin ? origin.box.y : 0), 1, what + " y")
        near(item.width, box.width, 1, what + " width")
        near(item.height, box.height, 1, what + " height")
    }
    function sameBoxes(list, name, select, what) {
        const want = boxes(name, select)
        const container = select.split(" ")[0]
        const origin = { item: named(container.slice(1)),
                         box: boxes(name, container)[0] }
        compare(list.length, want.length, what + " count")
        for (let i = 0; i < want.length; i++)
            sameBox(list[i], want[i], what + " " + (i + 1), origin)
    }

    // The runtime instantiates each template with no parent, no ancestors
    // and every class off, to read its properties: that must be clean.
    function test_templates_instantiate_without_a_parent() {
        const keys = Object.keys(page.eldurTemplates)
        verify(keys.length > 0, "no templates")
        for (const key of keys) {
            const made = page.eldurTemplates[key].createObject(null)
            verify(made !== null, key)
            compare(typeof made.eldurTemplate, "string", key)
            made.destroy()
        }
    }

@TESTS@
}
}
"##;

const FOCUSED: &str = r##"
    function list() { return named("list") }
    function rows() { return tagged(list(), "li") }
    function element(select, n) {
        const found = Eldur.document.querySelectorAll(select)[n || 0]
        verify(found !== undefined && found !== null, "no " + select)
        return found
    }
    function label(n) { return element("#list .label", n - 1) }
    function strong(n) { return element("#list strong", n - 1) }
    function tag() { return element("#list .tag") }
    function link() { return element("#list a") }

    // The runs a row's line draws: the `Text`s on its `EldurLines`.
    function runs(row) {
        return items(row, []).filter(i => typeof i.text === "string" && i.parent
            && i.parent.eldurTextRun !== undefined && i.visible)
    }
    function decode(s) {
        return s.replace(/&nbsp;/g, " ").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
            .replace(/&quot;/g, "\"").replace(/&amp;/g, "&")
    }
    // What a run draws, piece by piece: its text read through the StyledText
    // (or RichText) markup the runtime wrote, each piece with the colour,
    // weight, italic and decorations Qt draws it with.
    function pieces(run) {
        const out = []
        const stack = [{ color: run.color, weight: run.font.weight, italic: run.font.italic,
                         underline: run.font.underline, strike: run.font.strikeout, tag: "" }]
        const re = /<(\/?)([a-z]+)([^>]*)>|([^<]+)/g
        let m
        while ((m = re.exec(run.text)) !== null) {
            const top = stack[stack.length - 1]
            if (m[4] !== undefined) {
                out.push(Object.assign({}, top, { text: decode(m[4]) }))
                continue
            }
            const name = m[2]
            if (m[1] === "/") {
                for (let i = stack.length - 1; i > 0; i--) {
                    if (stack[i].tag === name) { stack.length = i; break }
                }
                continue
            }
            if (name === "br")
                continue
            const s = Object.assign({}, top, { tag: name })
            const attrs = m[3]
            const color = attrs.match(/color="([^"]*)"/)
            if (name === "font" && color)
                s.color = Qt.color(color[1])
            if (name === "b")
                s.weight = 700
            if (name === "i")
                s.italic = true
            if (name === "u")
                s.underline = true
            if (name === "s")
                s.strike = true
            if (name === "a") {
                // Qt draws a StyledText link in `linkColor`, underlined.
                s.color = run.linkColor
                s.underline = true
            }
            const style = attrs.match(/style="([^"]*)"/)
            if (style) {
                const css = style[1]
                const c = css.match(/(?:^|;)color:([^;]*)/)
                if (c)
                    s.color = Qt.color(c[1])
                if (/text-decoration:none/.test(css))
                    s.underline = false
            }
            stack.push(s)
        }
        return out
    }
    // The style `words` are drawn with in `row`, which must be one style.
    function drawn(row, words) {
        const all = runs(row).map(pieces).reduce((a, b) => a.concat(b), [])
        const text = all.map(p => p.text).join("")
        const at = text.indexOf(words)
        verify(at >= 0, JSON.stringify(words) + " is not drawn in " + JSON.stringify(text))
        const covering = []
        let pos = 0
        for (const p of all) {
            if (pos < at + words.length && pos + p.text.length > at)
                covering.push(p)
            pos += p.text.length
        }
        for (const p of covering) {
            verify(String(p.color) === String(covering[0].color) && p.weight === covering[0].weight
                && p.italic === covering[0].italic && p.underline === covering[0].underline
                && p.strike === covering[0].strike, JSON.stringify(words) + " is drawn in more than one style")
        }
        return covering[0]
    }

    // `color`, `font-weight`, `font-style` and `text-decoration-line` of
    // `words` as drawn, the last also drawn for a decoration a block
    // propagates (`decorated`).
    function sameStyle(row, words, name, what, decorated) {
        const p = drawn(row, words)
        sameColor(p.color, want(name, "color"), what + " colour")
        compare(p.weight, Number(want(name, "font-weight")), what + " weight")
        compare(p.italic, want(name, "font-style") === "italic", what + " italic")
        const line = want(name, "text-decoration-line")
        compare(p.underline, line === "underline", what + " underline")
        compare(p.strike, line === "line-through" || decorated === "line-through",
            what + " line-through")
    }
    // An element's box as a script reads it (the runtime's rects of the runs
    // on its line), or an item's: Qt's line is 18-19px where Chromium's
    // inline box is its 17px content area, and each glyph run is measured by
    // its own font engine, so positions and widths along a line may drift by
    // a few pixels; the rows themselves must match to 1px.
    function sameTextBox(got, box, what) {
        const listBox = boxes("initial geometry", "#list")[0]
        const at = element("#list").getBoundingClientRect()
        near(got.x - at.x, box.x - listBox.x, 3, what + " x")
        near(got.y - at.y, box.y - listBox.y, 2, what + " y")
        near(got.width, box.width, 3, what + " width")
        near(got.height, box.height, 2, what + " height")
    }
    function sameElementBox(e, box, what) {
        sameTextBox(e.getBoundingClientRect(), box, what)
    }
    // Where an item is, in the coordinates a script's rects are in.
    function itemBox(item) {
        const p = item.mapToItem(list(), 0, 0)
        const at = element("#list").getBoundingClientRect()
        return { x: at.x + p.x, y: at.y + p.y, width: item.width, height: item.height }
    }

    function test_created_inline_elements_take_the_page_styles() {
        compare(rows().length, 3, "rows")
        sameStyle(rows()[0], "First", "initial label1", "label 1")
        sameStyle(rows()[0], "Ada", "initial strong1", "strong 1")
        sameStyle(rows()[2], "tagged", "initial tag", "tag")
        sameStyle(rows()[2], "a link", "initial link", "link")
        compare(label(1).textContent, "First")
        compare(strong(2).textContent, "Grace")
        compare(tag().textContent, "tagged")
        compare(link().textContent, "a link")
        // The tag's background is its box's, painted behind its text.
        const bg = css(want("initial tag", "background-color"))
        const paint = items(rows()[2], []).filter(i => i.color !== undefined && typeof i.text !== "string"
            && i.visible && i.width > 0 && Math.abs(i.color.r * 255 - bg.r) <= 1
            && Math.abs(i.color.g * 255 - bg.g) <= 1 && Math.abs(i.color.b * 255 - bg.b) <= 1)
        compare(paint.length, 1, "tag backgrounds")
        sameColor(paint[0].color, want("initial tag", "background-color"), "tag background")
        sameTextBox(itemBox(paint[0]), boxes("initial geometry", "#list .tag")[0], "tag background")
    }

    function test_text_nodes_take_the_line_styles() {
        const kids = Array.from(element("#list li", 0).childNodes)
        const between = kids.filter(n => n.nodeType === 3)
        compare(between.length, 1, "text nodes in row 1")
        compare(between[0].data, " by ")
        sameStyle(rows()[0], " by ", "initial item1", "text node")
        compare(Array.from(element("#list li", 2).childNodes).filter(n => n.nodeType === 3).length, 1,
            "text nodes in row 3")
        sameStyle(rows()[2], " and ", "initial item1", "text node in row 3")
    }

    function test_created_inline_geometry() {
        const g = "initial geometry"
        sameBox(list(), boxes(g, "#list")[0], "list")
        sameBoxes(rows(), g, "#list li", "item")
        sameElementBox(label(1), boxes(g, "#list .label")[0], "label 1")
        sameElementBox(label(2), boxes(g, "#list .label")[1], "label 2")
        sameElementBox(strong(1), boxes(g, "#list strong")[0], "strong 1")
        sameElementBox(strong(2), boxes(g, "#list strong")[1], "strong 2")
        sameElementBox(tag(), boxes(g, "#list .tag")[0], "tag")
        sameElementBox(link(), boxes(g, "#list a")[0], "link")
    }

    // `classList.toggle('warn')` on a created span: its own style, and the
    // line moves the rest over.
    function test_toggling_a_class_on_a_created_span() {
        click(named("warn"))
        sameStyle(rows()[0], "First", "warn label1", "label 1")
        sameStyle(rows()[1], "Second", "warn label2", "label 2")
        const g = "warn geometry"
        sameBoxes(rows(), g, "#list li", "item")
        sameElementBox(label(1), boxes(g, "#list .label")[0], "label 1")
        sameElementBox(strong(1), boxes(g, "#list strong")[0], "strong 1")
    }

    function test_hovering_the_item_recolours_its_label() {
        hover(rows()[0])
        sameStyle(rows()[0], "First", "hover label1", "label 1")
        sameStyle(rows()[1], "Second", "hover label2", "label 2")
    }

    function test_hover_and_class_together() {
        click(named("warn"))
        hover(rows()[0])
        sameStyle(rows()[0], "First", "warn hover label1", "label 1")
    }

    // `.item.done` on a created block: its colour and its line-through,
    // which CSS draws through every inline descendant.
    function test_a_class_on_the_block_reaches_its_inline_children() {
        click(named("done"))
        const line = want("done item2", "text-decoration-line")
        sameStyle(rows()[1], " by ", "done item2", "text node")
        sameStyle(rows()[1], "Second", "done label2", "label 2", line)
        sameStyle(rows()[1], "Grace", "done strong2", "strong 2", line)
        sameStyle(rows()[0], "First", "initial label1", "label 1")
    }
"##;
