//! Conditional styles on converted pages (`docs/design/dynamic-styles.md`
//! §4, §13 item 4): a class the page script toggles, `:hover`, `:focus` and
//! `:disabled` change the converted items as they change the page in a
//! browser, and transitions end where Chromium's do.
//!
//! The pages are corpus cases, driven with real input through
//! qmltestrunner. The expected values are Chromium's after the same steps,
//! recorded in `tests/dynamic/chromium.json` by
//! `node crates/h2q_convert/tests/dynamic/gen.mjs`.

use std::process::Command;

use h2q_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("dynamic/chromium.json");

/// Converts the corpus page `case` and runs `tests` (TestCase functions) on
/// it in qml6, with the helpers in [`TEST`].
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
    let dir = std::env::temp_dir().join(format!("h2q-dynamic-{case}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let test = TEST
        .replace("@CHROMIUM@", CHROMIUM)
        .replace("@TESTS@", tests);
    std::fs::write(dir.join("tst_dynamic.qml"), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_dynamic.qml"))
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
    assert!(!text.contains("H2Q:"), "{text}");
    assert!(!text.contains("Warning:"), "{text}");
}

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Dynamic"
    when: windowShown

    // Past every transition on the pages (0.2s), as in gen.mjs.
    readonly property int settled: 400
    readonly property var chromium: (@CHROMIUM@)
    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        // Start with the mouse outside every element, as Playwright does.
        mouseMove(page, 1279, 799)
    }

    function want(name, prop) {
        const c = chromium[name]
        verify(c !== undefined, "no Chromium values for " + name)
        verify(c[prop] !== undefined, "no " + prop + " in " + name)
        return c[prop]
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
    function withText(text) {
        const found = items(page, []).find(i => i.text === text)
        verify(found !== undefined, "no item with text " + text)
        return found
    }
    // The `Text` of a box, whose colour is the CSS `color`.
    function textIn(item) {
        const found = items(item, []).find(i => typeof i.text === "string" && i.font !== undefined)
        verify(found !== undefined, "no text in " + item)
        return found
    }
    function up(item, pred, what) {
        for (let i = item; i; i = i.parent)
            if (pred(i))
                return i
        fail("no " + what + " around " + item)
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

    // CSS `rgb(…)`/`rgba(…)` as 0–255 channels and an alpha.
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

@TESTS@
}
}
"##;

const CLASS_TOGGLES: &str = r##"
    // The DOMContentLoaded handler marks item1 done and selects tab1; the
    // opacity is there at once, not animated from 1.
    function test_initial_classes() {
        compare(named("item1").opacity, Number(want("29 initial item1", "opacity")))
        wait(settled)
        compare(named("item1").opacity, Number(want("29 initial item1", "opacity")))
        sameColor(textIn(named("tab1")).color, want("29 initial tab1", "color"), "tab1 text")
        sameColor(named("tab1").color, want("29 initial tab1", "background-color"), "tab1")
        // `.tab.active + .tab`
        sameColor(textIn(named("tab2")).color, want("29 initial tab2", "color"), "tab2 text")
        sameColor(named("tab2").color, want("29 initial tab2", "background-color"), "tab2")
        sameColor(textIn(named("tab3")).color, want("29 initial tab3", "color"), "tab3 text")
        sameColor(textIn(named("status")).color, want("29 initial status", "color"), "status")
        compare(named("panel").visible, want("29 initial panel", "display") !== "none")
    }

    function test_selecting_a_tab_moves_the_sibling_rule() {
        click(named("tab2"))
        sameColor(textIn(named("tab1")).color, want("29 tab2 tab1", "color"), "tab1 text")
        sameColor(named("tab1").color, want("29 tab2 tab1", "background-color"), "tab1")
        sameColor(textIn(named("tab2")).color, want("29 tab2 tab2", "color"), "tab2 text")
        sameColor(named("tab2").color, want("29 tab2 tab2", "background-color"), "tab2")
        sameColor(textIn(named("tab3")).color, want("29 tab2 tab3", "color"), "tab3 text")
    }

    function test_conflicting_classes_follow_the_cascade() {
        click(named("ok"))
        sameColor(textIn(named("status")).color, want("29 ok", "color"), "ok")
        click(named("warn"))
        sameColor(textIn(named("status")).color, want("29 ok warn", "color"), "ok warn")
        click(named("ok"))
        sameColor(textIn(named("status")).color, want("29 warn", "color"), "warn")
        click(named("warn"))
        sameColor(textIn(named("status")).color, want("29 ok twice", "color"), "neither")
    }

    function test_display_none_toggles_visible() {
        const panel = named("panel")
        compare(panel.visible, false)
        click(named("more"))
        compare(panel.visible, want("29 more", "display") !== "none")
        near(panel.height, chromium["29 more geometry"]["#panel"].height, 1, "panel height")
    }

    function test_an_ancestor_class_changes_the_items() {
        const list = named("list")
        const item = named("item2")
        near(list.height, chromium["29 initial geometry"]["#list"].height, 1, "list height")
        near(item.height, chromium["29 initial geometry"]["#item2"].height, 1, "item height")
        click(named("compact"))
        near(list.height, chromium["29 compact geometry"]["#list"].height, 1, "compact list height")
        near(item.height, chromium["29 compact geometry"]["#item2"].height, 1, "compact item height")
    }

    function test_hover_and_done_combine() {
        hover(named("item2"))
        compare(named("item2").opacity, Number(want("29 hover item2", "opacity")))
        sameColor(named("item2").color, want("29 hover item2", "background-color"), "hovered item2")
        hover(named("item1"))
        near(named("item1").opacity, Number(want("29 hover item1", "opacity")), 0.001, "done and hovered")
    }

    function test_a_toggled_class_transitions_to_its_end_value() {
        click(named("item2"))
        near(named("item2").opacity, Number(want("29 click item2", "opacity")), 0.001, "done, hovered")
        hover(withText("Class toggles"))
        near(named("item2").opacity, Number(want("29 click item2 leave", "opacity")), 0.001, "done")
    }
"##;

#[test]
fn class_toggles_match_chromium() {
    check(
        "29-class-toggles",
        include_str!("../../../tests/corpus/29-class-toggles/index.html"),
        CLASS_TOGGLES,
    );
}

const HOVER_STATES: &str = r##"
    function button(text) { return withText(text) }
    function card() { return up(withText("Analytics"), i => i.radius === 12, "card") }
    function menuItem() { return up(withText("Dashboard"), i => i.radius === 8, "menu item") }
    function icon(item) {
        const found = items(item, []).find(i => i.radius === 4 && i.width === 16)
        verify(found !== undefined, "no icon")
        return found
    }

    function test_hovered_buttons_change_background() {
        hover(button("Primary"))
        sameColor(button("Primary").background.color, want("12 hover primary", "background-color"), "primary")
        hover(button("Secondary"))
        sameColor(button("Secondary").background.color, want("12 hover secondary", "background-color"), "secondary")
    }

    function test_a_disabled_button_keeps_its_style_when_hovered() {
        hover(button("Disabled"))
        sameColor(button("Disabled").background.color, want("12 hover disabled", "background-color"), "disabled")
    }

    function test_focus_adds_the_ring() {
        const b = button("Primary")
        b.forceActiveFocus()
        wait(settled)
        const shadow = items(b.background, []).find(i => i.spread !== undefined && i.color.a > 0)
        verify(shadow !== undefined, "no focus ring")
        // rgba(59, 111, 224, 0.35) 0px 0px 0px 3px
        const ring = want("12 focus primary", "box-shadow")
        sameColor(shadow.color, ring, "ring colour")
        compare(shadow.spread, Number(ring.match(/(\d+)px$/)[1]))
    }

    function test_a_hovered_card_lifts() {
        const before = card().mapToItem(page, 0, 0).y
        hover(card())
        const after = card().mapToItem(page, 0, 0).y
        const lift = chromium["12 hover card geometry"][".lift-card"].y
            - chromium["12 card geometry"][".lift-card"].y
        near(after - before, lift, 0.5, "lift")
    }

    function test_hovering_an_ancestor_restyles_its_descendant() {
        const item = menuItem()
        sameColor(icon(item).color, want("12 initial item icon", "background-color"), "icon")
        hover(item)
        sameColor(item.color, want("12 hover item", "background-color"), "item")
        sameColor(icon(item).color, want("12 hover item icon", "background-color"), "hovered icon")
    }
"##;

#[test]
fn hover_states_match_chromium() {
    check(
        "12-hover-states",
        include_str!("../../../tests/corpus/12-hover-states/index.html"),
        HOVER_STATES,
    );
}

const SCRIPT_COUNTER: &str = r##"
    function test_a_script_class_recolours_the_count() {
        click(named("dec"))
        sameColor(textIn(named("count")).color, want("15 dec", "color"), "negative")
        click(named("inc"))
        sameColor(textIn(named("count")).color, want("15 dec inc", "color"), "zero")
    }

    function test_hovered_buttons_change_background() {
        hover(named("inc"))
        sameColor(named("inc").background.color, want("15 hover inc", "background-color"), "inc")
        hover(named("dec"))
        sameColor(named("dec").background.color, want("15 hover dec", "background-color"), "dec")
    }

    function test_a_focused_input_hides_its_border() {
        const input = named("mirror-input")
        input.forceActiveFocus()
        wait(settled)
        sameColor(input.background.border.color, want("15 focus input", "border-top-color"), "border")
    }
"##;

#[test]
fn script_counter_matches_chromium() {
    check(
        "15-script-counter",
        include_str!("../../../tests/corpus/15-script-counter/index.html"),
        SCRIPT_COUNTER,
    );
}
