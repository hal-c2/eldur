//! A level a script gives a nested box re-levels the boxes around it, as
//! Chromium re-stacks (CSS 2 Appendix E): an ancestor that is no stacking
//! context of its own was raised for what it holds, so it rises or falls
//! with what it holds, and one that is a stacking context keeps its own
//! level. Run in qmltestrunner against Chromium's `elementFromPoint` at
//! 900×1200.

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
#s { position: relative }
#t { position: absolute; top: 40px; left: 0; z-index: 5; background: red }
#w { position: relative; z-index: 7; background: red }
#u { z-index: 2; background: blue }
#mv, #gone { position: relative; z-index: 3; background: red }
#src, #rm { background: green }
#vd, #vs, #vr { z-index: 2; background: blue }
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
<div class=gap></div>
<div id=r><div id=s><div id=w></div><div id=t></div></div></div><div id=u class=over></div>
<div class=gap></div>
<div id=dst></div><div id=vd class=over></div>
<div class=gap></div>
<div id=src><div id=mv></div></div><div id=vs class=over></div>
<div class=gap></div>
<div id=rm><div id=gone></div></div><div id=vr class=over></div>
<script>
// Named so that they get an objectName the test can find.
var named = ['a','b','c','d','e','f','g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','w','dst','vd','src','mv','vs','rm','gone','vr'].map(function (id) { return document.getElementById(id); });
// Lowered under its ancestor's sibling; raised over it; raised through two
// ancestors; raised inside an ancestor with its own level, which keeps it;
// an ancestor's own level taken away; and one taken from a box beside an
// absolutely positioned one, which still lifts their block by its own level;
// a raised box moved from one box into another, which each re-level; and
// one taken out.
document.getElementById('c').style.zIndex = 'auto';
document.getElementById('d').style.zIndex = '1';
document.getElementById('i').style.zIndex = '3';
document.getElementById('l').style.zIndex = '5';
document.getElementById('n').style.zIndex = 'auto';
document.getElementById('w').style.zIndex = 'auto';
document.getElementById('dst').appendChild(document.getElementById('mv'));
document.getElementById('gone').remove();
</script></body></html>"#;

/// Chromium's `elementFromPoint(100, y)` for each row.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 900
    height: 1200

TestCase {
    name: "Stacking"
    when: windowShown

    property var page
    readonly property var painted: ["b", "c", "d", "f", "i", "j", "l", "m", "o", "p", "t", "u", "w", "vd", "src", "mv", "vs", "rm", "gone", "vr"]

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
        page = createTemporaryObject(component, host, { width: 900, height: 1200 })
        waitForRendering(page)
    }

    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "b", "#c set to auto")
        compare(front(page, 100, 150), "d", "#d set to 1")
        compare(front(page, 100, 270), "i", "#i set to 3")
        compare(front(page, 100, 390), "m", "#l set to 5 inside #k")
        compare(front(page, 100, 510), "p", "#n set to auto")
        // Chromium has `#u` over `#w` at 630; the converter lifts `#t`'s
        // block, `#w` with it, by `#t`'s level, so only `#t` is checked.
        compare(front(page, 100, 650), "t", "#t still lifts its block")
        compare(front(page, 100, 750), "mv", "#mv moved into #dst")
        compare(front(page, 100, 870), "vs", "#mv moved out of #src")
        compare(front(page, 100, 990), "vr", "#gone taken out of #rm")
    }
}
}
"##;

/// With no script, Chromium's fronts are `c f j m o w t vd mv gone`.
const UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "c")
        compare(front(page, 100, 150), "f")
        compare(front(page, 100, 270), "j")
        compare(front(page, 100, 390), "m")
        compare(front(page, 100, 510), "o")
        compare(front(page, 100, 630), "w")
        compare(front(page, 100, 650), "t")
        compare(front(page, 100, 750), "vd")
        compare(front(page, 100, 870), "mv")
        compare(front(page, 100, 990), "gone")
    }
}
}
"##;

#[test]
fn a_scripted_level_restacks_the_boxes_around_it() {
    run(PAGE, TEST, "plain", false);
}

/// The page as one with no rules at run time has it: the levels come from
/// what the converter writes on each box (`eldurRuleZ`, `eldurHoldZ`).
#[test]
fn a_scripted_level_restacks_them_without_the_rules() {
    run(PAGE, TEST, "no-rules", true);
}

#[test]
fn the_rules_levels_match_chromium() {
    let start = PAGE.find("// Lowered").expect("the script's writes");
    let end = PAGE.find("</script>").expect("the script's end");
    let page = format!("{}{}", &PAGE[..start], &PAGE[end..]);
    let cut = TEST.find("    function test_levels").expect("the test");
    run(
        &page,
        &format!("{}{UNSCRIPTED}", &TEST[..cut]),
        "unscripted",
        false,
    );
}

fn run(page: &str, test: &str, name: &str, no_rules: bool) {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        viewport: Viewport {
            width: 900.0,
            height: 1200.0,
            ..Viewport::default()
        },
        ..Options::default()
    };
    let out = convert_str(page, &options);
    let dir = std::env::temp_dir().join(format!("eldur-stacking-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    if no_rules {
        let main = dir.join("Main.qml");
        let qml = std::fs::read_to_string(&main).expect("read Main.qml");
        assert!(qml.contains("eldurRuleZ"), "no level on any box\n{qml}");
        let kept: Vec<&str> = qml
            .lines()
            .filter(|l| {
                !l.trim_start()
                    .starts_with("property string eldurCascadeRules:")
            })
            .collect();
        assert!(
            kept.len() < qml.lines().count(),
            "no rules to take out\n{qml}"
        );
        std::fs::write(&main, kept.join("\n")).expect("write Main.qml");
    }
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
