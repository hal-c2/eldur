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
    readonly property var painted: ["b", "c", "d", "f", "i", "j", "l", "m", "o", "p", "t", "u", "w", "vd", "src", "mv", "vs", "rm", "gone", "vr",
        "c1", "o1", "c2", "o2", "c3", "o3", "hv", "o4", "fc", "o5", "c6", "o6", "c7", "o7",
        "k1", "v1", "k2", "v2", "k3", "v3", "k4", "v4", "k5", "m5", "v5"]

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

/// The other ways a nested box's level changes: through the rules a class,
/// a pseudo-class or an attribute brings, and with a box put in.
const PATHS: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 40px }
.over { position: relative; transform: translate(20px, -20px); z-index: 2; background: blue }
.in { position: relative; background: red; outline: none }
.up { z-index: 3 }
#hv:hover { z-index: 3 }
#fc:focus { z-index: 3 }
[data-up] { z-index: 3 }
</style></head><body>
<div id=a1><div id=c1 class=in></div></div><div id=o1 class=over></div>
<div class=gap></div>
<div id=a2><div id=c2 class=in></div></div><div id=o2 class=over></div>
<div class=gap></div>
<div id=a3><div id=c3 class="in up"></div></div><div id=o3 class=over></div>
<div class=gap></div>
<div id=a4><div id=hv class=in></div></div><div id=o4 class=over></div>
<div class=gap></div>
<div id=a5><div id=fc class=in tabindex=0></div></div><div id=o5 class=over></div>
<div class=gap></div>
<div id=a6><div id=c6 class=in></div></div><div id=o6 class=over></div>
<div class=gap></div>
<div id=a7></div><div id=o7 class=over></div>
<script>
var named = ['a1','c1','o1','a2','c2','o2','a3','c3','o3','a4','hv','o4','a5','fc','o5','a6','c6','o6','a7','o7'].map(function (id) { return document.getElementById(id); });
// A class added, set and taken away; focus; an attribute; and a box made
// with a level of its own. The hover is the test's.
document.getElementById('c1').classList.add('up');
document.getElementById('c2').className = 'in up';
document.getElementById('c3').classList.remove('up');
document.getElementById('fc').focus();
document.getElementById('c6').setAttribute('data-up', '');
var made = document.createElement('div');
made.id = 'c7';
made.className = 'in up';
document.getElementById('a7').appendChild(made);
</script></body></html>"#;

/// Chromium's fronts, with the pointer over `#hv`.
const PATHS_TEST: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        mouseMove(page, 100, 365)
        wait(50)
        compare(front(page, 100, 30), "c1", "class added")
        compare(front(page, 100, 150), "c2", "className set")
        compare(front(page, 100, 270), "o3", "class taken away")
        compare(front(page, 100, 390), "hv", ":hover")
        compare(front(page, 100, 510), "fc", ":focus")
        compare(front(page, 100, 630), "c6", "attribute set")
        compare(front(page, 100, 750), "c7", "a box put in with a level")
    }
}
}
"##;

/// With no script and no pointer, Chromium's fronts are `o1 o2 c3 o4 o5 o6 o7`.
const PATHS_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "o1")
        compare(front(page, 100, 150), "o2")
        compare(front(page, 100, 270), "c3")
        compare(front(page, 100, 390), "o4")
        compare(front(page, 100, 510), "o5")
        compare(front(page, 100, 630), "o6")
        compare(front(page, 100, 750), "o7")
    }
}
}
"##;

/// An absolute box at `z-index: auto` makes no stacking context, so what it
/// holds stacks with the boxes beside it: its overlay is raised and lowered
/// by what it holds, in a positioned block and in the initial one. And a box
/// raised to under its box's level leaves that level to the kid above it.
const OUT_OF_FLOW: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 40px }
.over { position: relative; transform: translate(20px, -20px); z-index: 2; background: blue }
.ab { position: absolute }
.in { position: relative; background: red }
.up { z-index: 3 }
#h1, #h2 { position: relative }
#x5 { height: 80px }
#k5 { z-index: 4 }
#v5 { z-index: 3; height: 80px; transform: translate(20px, -60px) }
</style></head><body>
<div id=h1><div id=b1 class=ab><div id=k1 class=in></div></div></div><div id=v1 class=over></div>
<div class=gap></div>
<div id=h2><div id=b2 class=ab><div id=k2 class="in up"></div></div></div><div id=v2 class=over></div>
<div class=gap></div>
<div id=h3><div id=b3 class=ab><div id=k3 class=in></div></div></div><div id=v3 class=over></div>
<div class=gap></div>
<div id=h4><div id=b4 class=ab><div id=k4 class="in up"></div></div></div><div id=v4 class=over></div>
<div class=gap></div>
<div id=x5><div id=k5 class=in></div><div id=m5 class=in></div></div><div id=v5 class=over></div>
<script>
var named = ['h1','b1','k1','v1','h2','b2','k2','v2','h3','b3','k3','v3','h4','b4','k4','v4','x5','k5','m5','v5'].map(function (id) { return document.getElementById(id); });
// An absolute box at `auto` lifted and dropped by what it holds, in a
// positioned block and in the initial one; and a box raised beside one
// that stays higher, which keeps their box at that one's level.
document.getElementById('k1').style.zIndex = '3';
document.getElementById('k2').style.zIndex = 'auto';
document.getElementById('k3').style.zIndex = '3';
document.getElementById('k4').style.zIndex = 'auto';
document.getElementById('m5').style.zIndex = '2';
</script></body></html>"#;

/// Chromium's fronts.
const OUT_OF_FLOW_TEST: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "k1", "raised in a positioned block")
        compare(front(page, 100, 150), "v2", "dropped in a positioned block")
        compare(front(page, 100, 270), "k3", "raised in the initial block")
        compare(front(page, 100, 390), "v4", "dropped in the initial block")
        compare(front(page, 100, 510), "k5", "the higher kid keeps its box's level")
        compare(front(page, 100, 550), "v5", "raised under that level")
    }
}
}
"##;

/// With no script, Chromium's fronts are `v1 k2 v3 k4 k5 v5`.
const OUT_OF_FLOW_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "v1")
        compare(front(page, 100, 150), "k2")
        compare(front(page, 100, 270), "v3")
        compare(front(page, 100, 390), "k4")
        compare(front(page, 100, 510), "k5")
        compare(front(page, 100, 550), "v5")
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
    run(
        &unscripted(PAGE, "// Lowered"),
        &with_test(UNSCRIPTED),
        "unscripted",
        false,
    );
}

#[test]
fn rules_a_script_brings_restack_the_boxes_around_them() {
    run(PATHS, &with_test(PATHS_TEST), "paths", false);
}

/// With no box made the page stays static: no rules go to the runtime, and
/// the converter's own bindings give each level.
#[test]
fn rules_a_script_brings_restack_them_on_a_static_page() {
    let start = PATHS.find("var made").expect("the box made");
    let end = PATHS.find("</script>").expect("the script's end");
    let page = format!("{}{}", &PATHS[..start], &PATHS[end..]);
    let test = PATHS_TEST.replace(
        r#"compare(front(page, 100, 750), "c7", "a box put in with a level")"#,
        r#"compare(front(page, 100, 750), "o7", "no box put in")"#,
    );
    run(&page, &with_test(&test), "paths-static", false);
}

#[test]
fn the_paths_rules_levels_match_chromium() {
    run(
        &unscripted(PATHS, "// A class added"),
        &with_test(PATHS_UNSCRIPTED),
        "paths-unscripted",
        false,
    );
}

#[test]
fn what_an_absolute_box_holds_restacks_it() {
    run(
        OUT_OF_FLOW,
        &with_test(OUT_OF_FLOW_TEST),
        "out-of-flow",
        false,
    );
}

#[test]
fn what_an_absolute_box_holds_restacks_it_without_the_rules() {
    run(
        OUT_OF_FLOW,
        &with_test(OUT_OF_FLOW_TEST),
        "out-of-flow-no-rules",
        true,
    );
}

#[test]
fn the_out_of_flow_rules_levels_match_chromium() {
    run(
        &unscripted(OUT_OF_FLOW, "// An absolute"),
        &with_test(OUT_OF_FLOW_UNSCRIPTED),
        "out-of-flow-unscripted",
        false,
    );
}

/// The page with its script's writes cut, from `from` on.
fn unscripted(page: &str, from: &str) -> String {
    let start = page.find(from).expect("the script's writes");
    let end = page.find("</script>").expect("the script's end");
    format!("{}{}", &page[..start], &page[end..])
}

/// The harness of `TEST` with another test in it.
fn with_test(test: &str) -> String {
    let cut = TEST.find("    function test_levels").expect("the test");
    format!("{}{test}", &TEST[..cut])
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
