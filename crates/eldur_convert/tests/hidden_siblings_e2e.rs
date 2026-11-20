//! A sibling hidden until `:hover` shows it is not there while hidden
//! (hal-c2/eldur#9, found while batching state worlds): an `ol` numbers its
//! items without it (counting down too, when `reversed`) and renumbers them
//! when it shows, a `counter()` counts it only while it shows, and the
//! margins either side of it collapse with each other, also when it has no
//! `id`. Two such siblings shown by different states number the items after
//! both by how many show at once.
//!
//! The pages are in `tests/hidden_siblings/`, driven with real hover through
//! qmltestrunner, in the steps `tests/hidden_siblings/steps.json` lists. The
//! expected values are Chromium's after the same steps, recorded in
//! `tests/hidden_siblings/chromium.json` by
//! `node crates/eldur_convert/tests/hidden_siblings/gen.mjs`.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const CHROMIUM: &str = include_str!("hidden_siblings/chromium.json");
const STEPS: &str = include_str!("hidden_siblings/steps.json");

fn check(name: &str, html: &str) {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!(
        "eldur-hidden-siblings-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let test = TEST
        .replace("@CHROMIUM@", CHROMIUM)
        .replace("@STEPS@", STEPS)
        .replace("@PAGE@", name);
    std::fs::write(dir.join("tst_hidden_siblings.qml"), test).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_hidden_siblings.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
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
    name: "HiddenSiblings_@PAGE@"
    when: windowShown

    readonly property var chromium: (@CHROMIUM@)
    // [step, text of the item the pointer goes to, or null for outside].
    readonly property var steps: (@STEPS@)["@PAGE@"]
    property var page

    function items(item, out) {
        out.push(item)
        for (let i = 0; i < item.children.length; i++)
            items(item.children[i], out)
        return out
    }
    function topOf(item) { return item.mapToItem(page, 0, 0).y }
    // An item's own text, without a `counter()` `::before` ("2. b" is b).
    function own(item) {
        const m = typeof item.text === "string" && /^(?:\d+\. *)?([a-z0-9])$/.exec(item.text)
        return m ? m[1] : undefined
    }
    function withText(text) {
        const found = items(page, []).find(i => own(i) === text)
        verify(found !== undefined, "no item with text " + text)
        return found
    }
    // The numbers shown, list markers and `counter()` text, top to bottom.
    function markers() {
        return items(page, [])
            .filter(i => typeof i.text === "string" && /^\d+\./.test(i.text) && i.visible)
            .sort((a, b) => topOf(a) - topOf(b))
            .map(i => /^\d+\./.exec(i.text)[0])
    }
    // Where each shown item is, from the first one's top: the glyphs' own
    // offset in their box cancels out.
    function tops() {
        const first = withText("a")
        return items(page, [])
            .filter(i => own(i) !== undefined && i.visible)
            .map(i => ({ text: own(i), top: Math.round(topOf(i) - topOf(first)) }))
            .sort((a, b) => a.top - b.top)
    }
    function expected(step) {
        const c = chromium["@PAGE@ " + step]
        verify(c !== undefined, "no Chromium values for " + step)
        const a = c.tops.find(t => t.text === "a").top
        return {
            markers: c.markers.map(m => m.trim()),
            tops: c.tops.filter(t => t.shown)
                .map(t => ({ text: t.text, top: t.top - a }))
                .sort((x, z) => x.top - z.top),
        }
    }
    function expect(step) {
        const want = JSON.stringify(expected(step))
        const got = () => JSON.stringify({ markers: markers(), tops: tops() })
        // A layout is polished on the next frame: wait a bounded time.
        for (let waited = 0; waited < 40 && got() !== want; ++waited)
            wait(25)
        compare(got(), want, step)
    }

    function test_hover_shows_the_sibling() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        // Start with the mouse outside every element, as Playwright does.
        mouseMove(page, 1279, 799)
        for (const [step, over] of steps) {
            if (over === null) {
                mouseMove(page, 1279, 799)
            } else {
                const item = withText(over)
                const p = item.mapToItem(page, item.width / 2, item.height / 2)
                mouseMove(page, p.x, p.y)
            }
            expect(step)
        }
    }
}
}
"##;

#[test]
fn a_hidden_list_item_is_not_numbered_until_it_shows() {
    check("numbered", include_str!("hidden_siblings/numbered.html"));
}

#[test]
fn a_hidden_block_without_an_id_lets_the_margins_either_side_collapse() {
    check("margin", include_str!("hidden_siblings/margin.html"));
}

#[test]
fn a_reversed_list_counts_down_from_the_items_shown() {
    check("reversed", include_str!("hidden_siblings/reversed.html"));
}

#[test]
fn a_hidden_sibling_s_counter_changes_count_only_while_it_shows() {
    check("counted", include_str!("hidden_siblings/counted.html"));
}

#[test]
fn items_after_two_hidden_siblings_count_both_when_both_show() {
    check("joint", include_str!("hidden_siblings/joint.html"));
}
