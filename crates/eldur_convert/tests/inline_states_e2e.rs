//! `:hover` and `:active` on an inline element of a block's line (#17): the
//! `<a>` and `<em>` in `<li><a>The guide</a> — …, <em>and more</em>.</li>` are
//! markup in the line's `Text`, with no item of their own. A pointer over the
//! glyphs of one of them hovers it, and only it; holding the button down on it
//! makes it active; and the line is drawn again in the colours that gives.
//!
//! The page is `tests/inline_states/page.html`, driven with real input through
//! qmltestrunner. Where each element is comes from its rect as a script reads
//! it, and the colour from the pixels Qt draws there. The expected colours are
//! Chromium's after the same steps, recorded in `tests/inline_states/chromium.json`
//! by `node crates/eldur_convert/tests/inline_states/gen.mjs`.

mod support;

use eldur_convert::{Input, NoResources, Options, RootKind};

const CHROMIUM: &str = include_str!("inline_states/chromium.json");
const PAGE: &str = include_str!("inline_states/page.html");

#[test]
fn a_pointer_on_an_inline_element_restyles_its_run() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        // What the test asks the runtime itself.
        runtime_uses: ["document"].map(String::from).to_vec(),
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "states/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let notes: Vec<_> = out
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .filter(|d| d.contains("left out"))
        .collect();
    assert!(
        notes.is_empty(),
        "the states are drawn, not left out: {notes:#?}"
    );
    let dir = std::env::temp_dir().join(format!("eldur-inline-states-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_states.qml"),
        TEST.replace("@CHROMIUM@", CHROMIUM),
    )
    .expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_states.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let problems = support::problem_lines(&text, |l| {
        l.contains("FAIL!") || l.contains("Warning:") || l.contains("QWARN") || l.contains("Error:")
    });
    assert!(problems.is_empty(), "{}", problems.join("\n  "));
    assert!(output.status.success(), "{text}");
    // Every case ran: a page that fails to load runs none, and fails nothing.
    assert!(text.contains("Totals: 7 passed, 0 failed"), "{text}");
    // The page's script ran to its end, after its `replaceChildren` (#17's
    // first report was a crash there).
    assert!(text.contains("LC GWATCH ran"), "{text}");
}

const TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineStates"
    when: windowShown

    readonly property int settled: 100
    readonly property var chromium: (@CHROMIUM@)
    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        mouseMove(page, 1279, 799)
        wait(settled)
    }

    function want(name) {
        const c = chromium[name]
        verify(c !== undefined && c.color !== undefined, "no Chromium colour for " + name)
        const m = c.color.match(/rgba?\(([^)]*)\)/)
        const v = m[1].split(",").map(Number)
        return { r: v[0], g: v[1], b: v[2] }
    }
    function rect(select) {
        const found = Eldur.document.querySelector(select)
        verify(found !== null, "no " + select)
        const r = found.getBoundingClientRect()
        verify(r.width > 0 && r.height > 0, select + " has no box")
        return r
    }
    function middle(select) {
        const r = rect(select)
        return { x: r.left + r.width / 2, y: r.top + r.height / 2 }
    }
    // The ink of an element's glyphs: of the pixels in its box, the one
    // furthest from the white page. A stem's middle is fully covered, so it
    // is the colour the glyphs are drawn in, not a blend.
    function ink(select) {
        const r = rect(select)
        const image = grabImage(page)
        let best = null
        let far = -1
        for (let y = Math.ceil(r.top); y < Math.floor(r.top + r.height); ++y) {
            for (let x = Math.ceil(r.left); x < Math.floor(r.left + r.width); ++x) {
                const p = image.pixel(x, y)
                const d = 3 - p.r - p.g - p.b
                if (d > far) {
                    far = d
                    best = { r: Math.round(p.r * 255), g: Math.round(p.g * 255), b: Math.round(p.b * 255) }
                }
            }
        }
        verify(best !== null && far > 0.5, select + " has no ink")
        return best
    }
    function sameInk(select, name) {
        const got = ink(select)
        const w = want(name)
        const ok = Math.abs(got.r - w.r) <= 24 && Math.abs(got.g - w.g) <= 24 && Math.abs(got.b - w.b) <= 24
        verify(ok, name + ": " + select + " is drawn rgb(" + [got.r, got.g, got.b]
            + "), not rgb(" + [w.r, w.g, w.b] + ")")
    }
    function hover(select) {
        const p = middle(select)
        mouseMove(page, p.x, p.y)
        wait(settled)
    }

    function test_initial() {
        sameInk(".c a", "initial link")
        sameInk(".c em", "initial em")
    }

    function test_hover_link() {
        hover(".c a")
        sameInk(".c a", "hover link")
        sameInk(".c em", "hover link em")
    }

    function test_hover_em() {
        hover(".c em")
        sameInk(".c em", "hover em")
        sameInk(".c a", "hover em link")
    }

    function test_left_link() {
        hover(".c a")
        const out = middle("#out")
        mouseMove(page, out.x, out.y)
        wait(settled)
        sameInk(".c a", "left link")
    }

    function test_press_link() {
        const p = middle(".c a")
        mouseMove(page, p.x, p.y)
        mousePress(page, p.x, p.y)
        wait(settled)
        sameInk(".c a", "press link")
        mouseRelease(page, p.x, p.y)
        wait(settled)
        sameInk(".c a", "hover link")
    }
}
}
"##;
