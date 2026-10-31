//! Elements the page scripts create take the page's styles
//! (`docs/design/dynamic-styles.md` §6, §13 item 4): the converter emits a
//! template for each predicted shape and the runtime materialises created
//! elements from them, so added, toggled and deleted items look as they do
//! in a browser.
//!
//! The pages are `tests/templates/page.html` and corpus 16-script-todo,
//! driven with real input through qmltestrunner. The expected values are
//! Chromium's after the same steps, recorded in `tests/templates/chromium.json`
//! by `node crates/h2q_convert/tests/templates/gen.mjs`.

use std::process::Command;

use h2q_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("templates/chromium.json");

/// Converts `html` and runs `tests` (TestCase functions) on it in qml6, with
/// the helpers in [`TEST`]. Any QML or runtime warning fails the test.
fn check(case: &str, html: &str, tests: &str) {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = h2q_convert::convert(
        &Input {
            html: html.to_owned(),
            url: format!("{case}/index.html"),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("h2q-templates-{case}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let test = TEST
        .replace("@CHROMIUM@", CHROMIUM)
        .replace("@TESTS@", tests);
    std::fs::write(dir.join("tst_templates.qml"), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_templates.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        // `localStorage` is a LocalStorage database under the data
        // directory: each run starts without saved items.
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

const TEST: &str = r##"
import QtQuick
import QtQuick.LocalStorage
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Templates"
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
    function labels() { return rows().map(r => within(r, "span")) }
    function texts() { return labels().map(l => l.text) }

    function test_created_items_take_the_page_styles() {
        compare(texts(), order("T initial order"))
        sameText(labels()[0], "T initial label1", "label 1")
        sameText(labels()[1], "T initial label2", "label 2")
        sameColor(rows()[0].color, want("T initial item1", "background-color"), "item 1")
        sameColor(rows()[1].color, want("T initial item2", "background-color"), "item 2")
        const g = "T initial geometry"
        sameBox(list(), boxes(g, "#list")[0], "list")
        sameBoxes(rows(), g, "#list li", "item")
        sameBoxes(labels(), g, "#list .label", "label")
        sameBoxes(rows().map(r => within(r, "input")), g, "#list input", "checkbox")
        sameBoxes(rows().map(r => within(r, "button")), g, "#list .remove", "remove")
    }

    function test_adding_an_item() {
        click(named("add"))
        compare(texts(), order("T add order"))
        sameText(labels()[2], "T add label3", "label 3")
        sameBox(list(), boxes("T add geometry", "#list")[0], "list")
        sameBoxes(rows(), "T add geometry", "#list li", "item")
    }

    function test_checking_an_item_rebuilds_it_done() {
        click(within(rows()[0], "input"))
        sameText(labels()[0], "T check label1", "label 1")
        sameColor(rows()[0].color, want("T check item1", "background-color"), "item 1")
    }

    // `li.classList.toggle('done')` on a connected item: its own binding and
    // its child's, through `h2qUp1`.
    function test_toggling_a_class_on_a_created_item() {
        click(labels()[0])
        sameText(labels()[0], "T label label1", "hovered label 1")
        sameColor(rows()[0].color, want("T label item1", "background-color"), "item 1")
        hover(named("compact"))
        sameText(labels()[0], "T label away label1", "label 1")
    }

    // `.item:hover .label`: the item exports its hover to the label's template.
    function test_hovering_a_created_item() {
        hover(rows()[0])
        sameText(labels()[0], "T hover label1", "hovered label 1")
        sameText(labels()[1], "T hover label2", "label 2")
    }

    function test_deleting_an_item() {
        click(within(rows()[1], "button"))
        compare(texts(), order("T remove order"))
        sameBox(list(), boxes("T remove geometry", "#list")[0], "list")
        sameBoxes(rows(), "T remove geometry", "#list li", "item")
    }

    // `.list.compact .item`: a static ancestor's class reaches the templates.
    function test_a_static_ancestor_class_restyles_created_items() {
        click(named("compact"))
        sameBox(list(), boxes("T compact geometry", "#list")[0], "list")
        sameBoxes(rows(), "T compact geometry", "#list li", "item")
    }
"##;

#[test]
fn created_items_match_chromium() {
    check("templates", include_str!("templates/page.html"), FOCUSED);
}

const TODO: &str = r##"
    function list() { return named("todo-list") }
    function rows() { return tagged(list(), "li") }
    function labels() { return rows().map(r => within(r, "span")) }
    function texts() { return labels().map(l => l.text) }

    function test_initial_todos_take_the_page_styles() {
        compare(texts(), order("16 initial order"))
        sameText(labels()[0], "16 initial text1", "todo 1")
        sameText(labels()[2], "16 initial text3", "todo 3")
        const g = "16 initial geometry"
        sameBox(list(), boxes(g, "#todo-list")[0], "list")
        sameBoxes(rows(), g, "#todo-list li", "todo")
        sameBoxes(labels(), g, "#todo-list .todo-text", "text")
        sameBoxes(rows().map(r => within(r, "input")), g, "#todo-list input", "checkbox")
        sameBoxes(rows().map(r => within(r, "button")), g, "#todo-list .delete", "delete")
    }

    function test_adding_a_todo() {
        const input = named("todo-input")
        click(input)
        for (const c of "Buy milk")
            keyClick(c)
        click(named("add-btn"))
        compare(texts(), order("16 add order"))
        sameText(labels()[3], "16 add text4", "todo 4")
        sameBox(list(), boxes("16 add geometry", "#todo-list")[0], "list")
        sameBoxes(rows(), "16 add geometry", "#todo-list li", "todo")
    }

    function test_checking_a_todo() {
        click(within(rows()[0], "input"))
        sameText(labels()[0], "16 check text1", "todo 1")
        sameText(labels()[2], "16 check text3", "todo 3")
    }

    function test_deleting_a_todo() {
        click(within(rows()[1], "button"))
        compare(texts(), order("16 remove order"))
        sameBox(list(), boxes("16 remove geometry", "#todo-list")[0], "list")
        sameBoxes(rows(), "16 remove geometry", "#todo-list li", "todo")
    }
"##;

#[test]
fn todo_items_match_chromium() {
    check(
        "16-script-todo",
        include_str!("../../../tests/corpus/16-script-todo/index.html"),
        TODO,
    );
}
