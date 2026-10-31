//! Inline elements a script creates in a block (`docs/design/dynamic-styles.md`
//! §6.8): `<span>`, `<strong>` and `<a>` appended to a block `<li>`, or
//! written into it as markup, are items of their own in the line's `Flow`,
//! with the page's colours, weights, decorations and backgrounds, a class
//! toggle on them and `:hover` on their item.
//!
//! The page is `tests/inline_templates/page.html`, driven with real input
//! through qmltestrunner. The expected values are Chromium's after the same
//! steps, recorded in `tests/inline_templates/chromium.json` by
//! `node crates/h2q_convert/tests/inline_templates/gen.mjs`.

use std::process::Command;

use h2q_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("inline_templates/chromium.json");
const PAGE: &str = include_str!("inline_templates/page.html");

/// Converts the page and runs `tests` (TestCase functions) on it in qml6,
/// with the helpers in [`TEST`]. Any QML or runtime warning fails the test.
fn check(case: &str, tests: &str) {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = h2q_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: format!("{case}/index.html"),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("h2q-inline-{case}-{}", std::process::id()));
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
    assert!(!text.contains("H2Q:"), "{text}");
    assert!(!text.contains("h2q:"), "{text}");
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
        LocalStorage.openDatabaseSync("h2q_local_storage", "1.0", "html-2-qml localStorage", 1048576)
            .transaction(tx => tx.executeSql("DROP TABLE IF EXISTS h2q_kv"))
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
    // `--h2q-order`: the texts, in document order, as one CSS string.
    function order(name) {
        return JSON.parse(want(name, "--h2q-order")).split("|")
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
        return items(item, []).filter(i => i.h2qTag === tag && i.visible && i.width > 0)
            .sort((a, b) => pageY(a) - pageY(b))
    }
    function within(item, tag) {
        const found = items(item, []).find(i => i.h2qTag === tag)
        verify(found !== undefined, "no " + tag + " in " + item)
        return found
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
        const keys = Object.keys(page.h2qTemplates)
        verify(keys.length > 0, "no templates")
        for (const key of keys) {
            const made = page.h2qTemplates[key].createObject(null)
            verify(made !== null, key)
            compare(typeof made.h2qTemplate, "string", key)
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
    function label(n) { return within(rows()[n - 1], "span") }
    function strong(n) { return within(rows()[n - 1], "strong") }
    function tag() { return within(rows()[2], "span") }
    function link() { return within(rows()[2], "a") }
    // The Text that draws an element: itself, or the one in its box.
    function textOf(item) {
        return typeof item.text === "string" ? item : items(item, []).find(i => typeof i.text === "string" && i !== item)
    }
    // The text nodes between the elements: the Texts in a row's line that
    // are no element.
    function runs(row) {
        return items(row, []).filter(i => typeof i.text === "string" && i.h2qTag === undefined
            && i.parent && i.parent.h2qTextRun !== undefined)
    }

    // `color`, `font-weight`, `font-style` and `text-decoration-line`, the
    // last also drawn for a decoration a block propagates (`decorated`).
    function sameStyle(item, name, what, decorated) {
        const text = textOf(item)
        sameColor(text.color, want(name, "color"), what + " colour")
        compare(text.font.weight, Number(want(name, "font-weight")), what + " weight")
        compare(text.font.italic, want(name, "font-style") === "italic", what + " italic")
        const line = want(name, "text-decoration-line")
        compare(text.font.underline, line === "underline", what + " underline")
        compare(text.font.strikeout, line === "line-through" || decorated === "line-through",
            what + " line-through")
    }
    // Text boxes: Qt's line is 18-19px where Chromium's inline box is its
    // 17px content area, and each glyph run is measured by its own font
    // engine, so positions and widths along a line may drift by a few
    // pixels; the rows themselves must match to 1px.
    function sameTextBox(item, box, what) {
        const origin = { item: list(), box: boxes("initial geometry", "#list")[0] }
        const p = item.mapToItem(origin.item, 0, 0)
        near(p.x, box.x - origin.box.x, 3, what + " x")
        near(p.y, box.y - origin.box.y, 2, what + " y")
        near(item.width, box.width, 3, what + " width")
        near(item.height, box.height, 2, what + " height")
    }

    function test_created_inline_elements_take_the_page_styles() {
        compare(rows().length, 3, "rows")
        sameStyle(label(1), "initial label1", "label 1")
        sameStyle(strong(1), "initial strong1", "strong 1")
        sameStyle(tag(), "initial tag", "tag")
        sameColor(tag().color, want("initial tag", "background-color"), "tag background")
        sameStyle(link(), "initial link", "link")
        compare(label(1).text, "First")
        compare(strong(2).text, "Grace")
        compare(textOf(tag()).text, "tagged")
        compare(link().text, "a link")
    }

    function test_text_nodes_take_the_line_styles() {
        const between = runs(rows()[0])
        compare(between.length, 1, "text nodes in row 1")
        compare(between[0].text, " by ")
        sameStyle(between[0], "initial item1", "text node")
        compare(runs(rows()[2]).length, 1, "text nodes in row 3")
    }

    function test_created_inline_geometry() {
        const g = "initial geometry"
        sameBox(list(), boxes(g, "#list")[0], "list")
        sameBoxes(rows(), g, "#list li", "item")
        sameTextBox(label(1), boxes(g, "#list .label")[0], "label 1")
        sameTextBox(label(2), boxes(g, "#list .label")[1], "label 2")
        sameTextBox(strong(1), boxes(g, "#list strong")[0], "strong 1")
        sameTextBox(strong(2), boxes(g, "#list strong")[1], "strong 2")
        sameTextBox(tag(), boxes(g, "#list .tag")[0], "tag")
        sameTextBox(link(), boxes(g, "#list a")[0], "link")
    }

    // `classList.toggle('warn')` on a created span: its own binding, and the
    // line moves the rest over.
    function test_toggling_a_class_on_a_created_span() {
        click(named("warn"))
        sameStyle(label(1), "warn label1", "label 1")
        sameStyle(label(2), "warn label2", "label 2")
        const g = "warn geometry"
        sameBoxes(rows(), g, "#list li", "item")
        sameTextBox(label(1), boxes(g, "#list .label")[0], "label 1")
        sameTextBox(strong(1), boxes(g, "#list strong")[0], "strong 1")
    }

    function test_hovering_the_item_recolours_its_label() {
        hover(rows()[0])
        sameStyle(label(1), "hover label1", "label 1")
        sameStyle(label(2), "hover label2", "label 2")
    }

    function test_hover_and_class_together() {
        click(named("warn"))
        hover(rows()[0])
        sameStyle(label(1), "warn hover label1", "label 1")
    }

    // `.item.done` on a created block: its colour and its line-through,
    // which CSS draws through every inline descendant.
    function test_a_class_on_the_block_reaches_its_inline_children() {
        click(named("done"))
        const line = want("done item2", "text-decoration-line")
        sameStyle(runs(rows()[1])[0], "done item2", "text node")
        sameStyle(label(2), "done label2", "label 2", line)
        sameStyle(strong(2), "done strong2", "strong 2", line)
        sameStyle(label(1), "initial label1", "label 1")
    }
"##;
