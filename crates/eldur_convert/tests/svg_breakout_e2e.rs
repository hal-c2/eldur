//! An HTML block in SVG markup a script sets with `innerHTML` leaves the SVG
//! as Chromium's parser takes it out: the `<p>` closes the `<g>` and `<svg>`
//! open around it and lays out after the SVG, in the element the markup went
//! into, and what follows the `<p>` goes on there too.
//!
//! Chromium's places, at 18 px lines and 16 px paragraph margins: the
//! paragraph's line at 16, the `<div>` after it at 50 and the page's next
//! block at 68. The SVG is 0 px tall: an `<svg>` a script makes is not drawn
//! by the runtime yet, and has no size of its own, so its height is kept out
//! of what this measures.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const PAGE: &str = r##"<!doctype html>
<style>body { margin: 0; font: 16px/18px sans-serif; } svg { display: block; }</style>
<div id="host"></div>
<div id="next">Next</div>
<script>
document.getElementById('host').innerHTML =
  '<svg width="40" height="0"><rect width="40" height="20" fill="#c00"></rect><g><p>Para</p></g></svg><div>Tail</div>';
</script>
"##;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "SvgBreakout"
    when: windowShown

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
        return find(page, i => i.text !== undefined && i.font !== undefined
                    && String(i.text).indexOf(words) >= 0)
    }

    // Layout runs after the script that changed it, later on a loaded
    // machine: it gets two seconds.
    function at(page, words, y) {
        let item = null
        const now = () => {
            item = textOf(page, words)
            return item && item.visible ? Math.round(item.mapToItem(page, 0, 0).y) : -1
        }
        for (let t = 0; t < 40 && now() !== y; ++t)
            wait(50)
        verify(item, words + " has a line")
        verify(item.visible, words + " is hidden")
        compare(now(), y, words + " is at " + now() + ", Chromium " + y)
    }

    function test_paragraph_leaves_the_svg() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        at(page, "Para", 16)
        at(page, "Tail", 50)
        at(page, "Next", 68)
    }
}
}
"##;

#[test]
fn a_paragraph_in_svg_markup_a_script_sets_lays_out_after_the_svg() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(PAGE, &options);
    let dir = std::env::temp_dir().join(format!("eldur-svg-breakout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_breakout.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_breakout.qml"))
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
    // The SVG elements a script makes have no template yet (see above); any
    // other warning is one.
    let warnings = text.lines().filter(|line| {
        line.contains("QWARN")
            && !["svg@", "rect@", "g@"]
                .iter()
                .any(|tag| line.contains(&format!("no style template for {tag}")))
    });
    assert_eq!(warnings.count(), 0, "{text}");
}
