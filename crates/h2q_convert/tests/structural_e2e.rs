//! Structural pseudo-classes (`:first-child`, `:last-child`,
//! `:nth-child()`) on the children of a list scripts add to, remove from
//! and re-render (`docs/design/dynamic-styles.md` §5.5, §8.1, §9.2): the
//! converter binds the styles to each item's place (`h2qIndex`,
//! `h2qCount`), static items and templates alike, and the runtime keeps the
//! places up to date, so the first, last and even items look as they do in
//! a browser after every change.
//!
//! The page is `tests/structural/page.html`, driven with real input
//! through qmltestrunner. The expected values are Chromium's after the same
//! steps, recorded in `tests/structural/chromium.json` by
//! `node crates/h2q_convert/tests/structural/gen.mjs`.

use std::process::Command;

use h2q_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("structural/chromium.json");

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
    let dir = std::env::temp_dir().join(format!("h2q-structural-{case}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let test = TEST
        .replace("@CHROMIUM@", CHROMIUM)
        .replace("@TESTS@", tests);
    std::fs::write(dir.join("tst_structural.qml"), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_structural.qml"))
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
    name: "Structural"
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

const STRUCTURAL: &str = r##"
    function list() { return named("list") }
    function rows() { return tagged(list(), "li") }
    function labels() { return rows().map(r => within(r, "span")) }
    function texts() { return labels().map(l => l.text) }
    // The border strips of a row: full width, at its top or bottom edge.
    function strip(row, bottom) {
        const found = []
        for (let i = 0; i < row.children.length; i++) {
            const c = row.children[i]
            if (c.color === undefined || Math.abs(c.width - row.width) > 0.5 || c.height > 8)
                continue
            if (bottom ? Math.abs(c.y + c.height - row.height) < 0.5 && c.y > 0 : c.y === 0)
                found.push(c)
        }
        compare(found.length, 1, (bottom ? "bottom" : "top") + " strip of " + row)
        return found[0]
    }

    // Each item's background, borders and label colour, and the boxes,
    // after `clicks`.
    function sameAfter(step, clicks) {
        for (const id of clicks)
            click(named(id))
        compare(texts(), order(step + " order"), step + " order")
        const list_ = rows()
        for (let n = 1; n <= list_.length; n++) {
            const row = list_[n - 1]
            const item = step + " item" + n
            const what = step + " item " + n
            sameColor(row.color, want(item, "background-color"), what + " background")
            near(strip(row, false).height, parseFloat(want(item, "border-top-width")), 0.5,
                what + " border-top-width")
            sameColor(strip(row, true).color, want(item, "border-bottom-color"),
                what + " border-bottom-color")
            sameColor(labels()[n - 1].color, want(step + " label" + n, "color"),
                what + " label colour")
        }
        const g = step + " geometry"
        sameBox(list(), boxes(g, "#list")[0], step + " list")
        sameBoxes(rows(), g, "#list li", step + " item")
        sameBoxes(labels(), g, "#list .label", step + " label")
    }

    function test_static_items_take_their_places() { sameAfter("initial", []) }
    // `appendChild`: the old last item is no longer last.
    function test_adding_an_item() { sameAfter("add", ["add"]) }
    function test_adding_two_items() { sameAfter("addTwice", ["add", "add"]) }
    // `insertBefore(…, firstElementChild)`: every place moves by one.
    function test_prepending_an_item() { sameAfter("prepend", ["prepend"]) }
    // `removeChild(firstElementChild)`.
    function test_removing_the_first_item() { sameAfter("shift", ["shift"]) }
    function test_removing_two_items() { sameAfter("shiftTwice", ["shift", "shift"]) }
    function test_adding_then_removing() { sameAfter("addShift", ["add", "shift"]) }
    // `textContent = ''` and the items made again from the templates.
    function test_rendering_again() { sameAfter("reverse", ["reverse"]) }
    function test_adding_then_rendering_again() { sameAfter("addReverse", ["add", "reverse"]) }
    function test_rendering_again_then_adding() { sameAfter("reverseAdd", ["reverse", "add"]) }
"##;

#[test]
fn positions_match_chromium_as_scripts_change_the_list() {
    check(
        "structural",
        include_str!("structural/page.html"),
        STRUCTURAL,
    );
}
