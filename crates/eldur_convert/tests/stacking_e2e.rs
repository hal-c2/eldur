//! A level a script gives a nested box re-levels the boxes around it, as
//! Chromium re-stacks (CSS 2 Appendix E): an ancestor that is no stacking
//! context of its own was raised for what it holds, so it rises or falls
//! with what it holds, and one that is a stacking context keeps its own
//! level. Run in qmltestrunner against Chromium's `elementFromPoint` at
//! 900×800, on a page with no runtime rules and on one with them.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, Viewport, convert_str};

const PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 40px }
.over { position: relative; transform: translate(20px, -20px) }
#c { position: relative; z-index: 1; background: red }
#b { background: blue }
#d { position: relative; background: red }
#f { background: blue }
#i { position: relative; background: red }
#j { z-index: 2; background: blue }
#l { position: relative; background: red }
#k { position: relative; z-index: 1 }
#m { z-index: 2; background: blue }
#o { position: relative; background: red }
#n { position: relative; z-index: 3 }
#p { z-index: 2; background: blue }
</style></head><body>
<div id=a><div id=c></div></div><div id=b class=over></div>
<div class=gap></div>
<div id=e><div id=d></div></div><div id=f class=over></div>
<div class=gap></div>
<div id=g><div id=h><div id=i></div></div></div><div id=j class=over></div>
<div class=gap></div>
<div id=k><div id=l></div></div><div id=m class=over></div>
<div class=gap></div>
<div id=q><div id=n><div id=o></div></div></div><div id=p class=over></div>
<script>
// Named so that they get an objectName the test can find.
var named = ['a','b','c','d','e','f','g','h','i','j','k','l','m','n','o','p','q'].map(function (id) { return document.getElementById(id); });
/*READ*/
// Lowered under its ancestor's sibling; raised over it; raised through two
// ancestors; raised inside an ancestor with its own level, which keeps it;
// and an ancestor's own level taken away.
document.getElementById('c').style.zIndex = 'auto';
document.getElementById('d').style.zIndex = '1';
document.getElementById('i').style.zIndex = '3';
document.getElementById('l').style.zIndex = '5';
document.getElementById('n').style.zIndex = 'auto';
</script></body></html>"#;

/// Chromium's `elementFromPoint(100, y)` for each row.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 800

TestCase {
    name: "Stacking"
    when: windowShown

    property var page
    readonly property var painted: ["b", "c", "d", "f", "i", "j", "l", "m", "o", "p"]

    // The painted box nearest the front at a point, in Qt Quick's paint
    // order: the children over an item from the last level down, then the
    // item, then the children under it.
    function front(item, x, y) {
        const kids = Array.from(item.children).map((kid, i) => ({ kid: kid, i: i }))
            .filter(k => k.kid.visible)
            .sort((p, q) => (q.kid.z - p.kid.z) || (q.i - p.i))
        for (const k of kids) {
            if (k.kid.z < 0) {
                break
            }
            const hit = front(k.kid, x, y)
            if (hit) {
                return hit
            }
        }
        const at = item.mapFromItem(page, x, y)
        if (painted.indexOf(item.objectName) >= 0 && at.x >= 0 && at.y >= 0
                && at.x < item.width && at.y < item.height) {
            return item.objectName
        }
        for (const k of kids) {
            if (k.kid.z < 0) {
                const hit = front(k.kid, x, y)
                if (hit) {
                    return hit
                }
            }
        }
        return ""
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 900, height: 800 })
        waitForRendering(page)
    }

    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "b", "#c set to auto")
        compare(front(page, 100, 150), "d", "#d set to 1")
        compare(front(page, 100, 270), "i", "#i set to 3")
        compare(front(page, 100, 390), "m", "#l set to 5 inside #k")
        compare(front(page, 100, 510), "p", "#n set to auto")
    }
}
}
"##;

/// With no script, Chromium's fronts are `c f j m o`.
const UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "c")
        compare(front(page, 100, 150), "f")
        compare(front(page, 100, 270), "j")
        compare(front(page, 100, 390), "m")
        compare(front(page, 100, 510), "o")
    }
}
}
"##;

#[test]
fn a_scripted_level_restacks_the_boxes_around_it() {
    run(PAGE, TEST, "plain");
}

#[test]
fn a_scripted_level_restacks_them_with_runtime_rules() {
    // A script that reads the cascade makes the page keep its rules.
    let page = PAGE.replace("/*READ*/", "getComputedStyle(named[0]).zIndex;");
    run(&page, TEST, "rules");
}

#[test]
fn the_rules_levels_match_chromium() {
    let start = PAGE.find("// Lowered").expect("the script's writes");
    let end = PAGE.find("</script>").expect("the script's end");
    let page = format!("{}{}", &PAGE[..start], &PAGE[end..]);
    let cut = TEST.find("    function test_levels").expect("the test");
    run(&page, &format!("{}{UNSCRIPTED}", &TEST[..cut]), "unscripted");
}

fn run(page: &str, test: &str, name: &str) {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        viewport: Viewport {
            width: 900.0,
            height: 800.0,
            ..Viewport::default()
        },
        ..Options::default()
    };
    let out = convert_str(page, &options);
    let dir = std::env::temp_dir().join(format!("eldur-stacking-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    std::fs::write(dir.join("tst_stacking.qml"), test).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_stacking.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = format!(
        "{}\n{stderr}\n{}",
        String::from_utf8_lossy(&output.stdout),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    assert!(!stderr.contains("Binding loop"), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
}
