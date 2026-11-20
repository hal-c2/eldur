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
        "c1", "o1", "c2", "o2", "c3", "o3", "hv", "o4", "fc", "o5", "c6", "o6", "c7", "o7", "e8", "o8", "c9", "o9", "c10", "d10", "o10", "n11",
        "k1", "v1", "k2", "v2", "k3", "v3", "k4", "v4", "k5", "m5", "v5",
        "c1", "v1", "c2", "v2", "c3", "v3", "c4", "e4", "v4", "e5", "v5", "a6", "a7", "h8", "a8", "h9", "h10", "c11", "v11", "c12", "v12", "c13", "v13", "c14", "v14", "p15", "v15",
        "at", "rt", "nt",
        "p1", "p2", "r2", "a2",
        "b3", "b4", "da", "db", "k6", "n6", "v6",
        "g1", "g2", "mq", "oq", "pq", "ax", "ox", "bx", "rx", "qx", "gx", "ix", "lx", "fa", "ff", "fc", "fv"]

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
        compare(front(page, 100, 630), "u", "#w set to auto beside #t")
        compare(front(page, 100, 650), "t", "#t over #u by its own level")
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
.hide #c9 { display: none }
.hi #d10 { z-index: 3 }
.one { z-index: 1 }
.two { z-index: 2 }
#n11 { margin-top: -20px; background: blue }
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
<div class=gap></div>
<div id=a8><div id=e8 class=in></div></div><div id=o8 class=over></div>
<div class=gap></div>
<div id=a9><div id=c9 class="in up"></div></div><div id=o9 class=over></div>
<div class=gap></div>
<div id=a10><div id=c10 class=in></div><div id=d10 class=in></div></div><div id=o10 class=over></div>
<div class=gap></div>
<div id=a11><div id=c11 class="in one"></div></div><div id=n11></div>
<script>
var named = ['a1','c1','o1','a2','c2','o2','a3','c3','o3','a4','hv','o4','a5','fc','o5','a6','c6','o6','a7','o7','a8','e8','o8','a9','c9','o9','a10','c10','d10','o10','a11','c11','n11'].map(function (id) { return document.getElementById(id); });
// A class added, set and taken away; a raised box a class on its block
// hides; a class that moves a raised box from one level over 0 to
// another, over the flow of the block after its own; focus; an attribute; a box made with a level of its own; and a
// fixed one with a level, painted over the window, which lifts nothing it
// is in; and a block a script's level lifted, whose other kid a class then
// raises. The hover is the test's. A static page keeps what is before the
// attribute.
document.getElementById('c1').classList.add('up');
document.getElementById('c2').className = 'in up';
document.getElementById('c3').classList.remove('up');
document.getElementById('a9').classList.add('hide');
document.getElementById('c11').className = 'in two';
document.getElementById('fc').focus();
document.getElementById('c6').setAttribute('data-up', '');
var made = document.createElement('div');
made.id = 'c7';
made.className = 'in up';
document.getElementById('a7').appendChild(made);
var fixed = document.createElement('div');
fixed.id = 'f8';
fixed.style.cssText = 'position: fixed; top: 1150px; left: 600px; width: 50px; z-index: 10; background: green';
document.getElementById('a8').appendChild(fixed);
document.getElementById('c10').style.zIndex = '1';
document.getElementById('a10').classList.add('hi');
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
        compare(front(page, 100, 870), "o8", "a fixed box put in lifts nothing")
        compare(front(page, 100, 990), "o9", "a raised box a class hides")
        compare(front(page, 100, 1100), "o10")
        compare(front(page, 100, 1130), "d10", "a class raises a block's kid after a script did")
        compare(front(page, 100, 1230), "c11", "a class moves a raised box to another level over 0")
        compare(front(page, 100, 1250), "n11")
    }
}
}
"##;

/// With no script and no pointer, Chromium's fronts are `o1 o2 c3 o4 o5 o6 o7 o8 c9 o10 c11`.
const PATHS_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "o1")
        compare(front(page, 100, 150), "o2")
        compare(front(page, 100, 270), "c3")
        compare(front(page, 100, 390), "o4")
        compare(front(page, 100, 510), "o5")
        compare(front(page, 100, 630), "o6")
        compare(front(page, 100, 750), "o7")
        compare(front(page, 100, 870), "o8")
        compare(front(page, 100, 990), "c9")
        compare(front(page, 100, 1100), "o10")
        compare(front(page, 100, 1130), "o10")
        compare(front(page, 100, 1230), "c11")
        compare(front(page, 100, 1250), "n11")
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
#h1, #h2, #h6 { position: relative }
#n6 { top: 0; z-index: -1; background: green }
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
<div class=gap></div>
<div id=h6><div id=k6 class=in></div><div id=n6 class=ab></div></div><div id=v6 class=over></div>
<script>
var named = ['h1','b1','k1','v1','h2','b2','k2','v2','h3','b3','k3','v3','h4','b4','k4','v4','x5','k5','m5','v5','h6','k6','n6','v6'].map(function (id) { return document.getElementById(id); });
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
        compare(front(page, 100, 690), "k6", "a box under its block stays under it")
        compare(front(page, 100, 710), "v6", "and under what follows")
    }
}
}
"##;

/// With no script, Chromium's fronts are `v1 k2 v3 k4 k5 v5 k6 v6`.
const OUT_OF_FLOW_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "v1")
        compare(front(page, 100, 150), "k2")
        compare(front(page, 100, 270), "v3")
        compare(front(page, 100, 390), "k4")
        compare(front(page, 100, 510), "k5")
        compare(front(page, 100, 550), "v5")
        compare(front(page, 100, 690), "k6")
        compare(front(page, 100, 710), "v6")
    }
}
}
"##;

/// Out-of-flow boxes, and boxes in them, moved and removed: what leaves an
/// absolute box at `auto` drops it, and an absolute box with a level that
/// leaves a block that makes no stacking context takes its level with it.
const OUT_OF_FLOW_MOVES: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 40px }
.over { position: relative; transform: translate(20px, -20px); z-index: 2; background: blue }
.ab { position: absolute }
.in { position: relative; background: red }
.up { z-index: 3 }
#h1, #h2 { position: relative }
#b3, #b4 { background: green }
</style></head><body>
<div id=h1><div id=b1 class=ab><div id=k1 class="in up"></div></div></div><div id=v1 class=over></div>
<div class=gap></div>
<div id=h2><div id=b2 class=ab><div id=k2 class="in up"></div></div></div><div id=v2 class=over></div>
<div class=gap></div>
<div id=h3><div id=b3 class="ab up"></div></div><div id=v3 class=over></div>
<div class=gap></div>
<div id=h4><div id=b4 class="ab up"></div></div><div id=v4 class=over></div>
<div class=gap></div>
<div id=da></div>
<div class=gap></div>
<div id=db></div>
<script>
var named = ['h1','b1','k1','v1','h2','b2','k2','v2','h3','b3','v3','h4','b4','v4','da','db'].map(function (id) { return document.getElementById(id); });
// A raised box taken out of an absolute box at `auto`, and one moved out of
// another; an absolute box with a level taken out, and one moved.
var k1 = document.getElementById('k1');
k1.parentNode.removeChild(k1);
document.getElementById('da').appendChild(document.getElementById('k2'));
var b3 = document.getElementById('b3');
b3.parentNode.removeChild(b3);
document.getElementById('db').appendChild(document.getElementById('b4'));
</script></body></html>"#;

/// Chromium's fronts.
const OUT_OF_FLOW_MOVES_TEST: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "v1", "a raised box taken out of an absolute box")
        compare(front(page, 100, 150), "v2", "one moved out of another")
        compare(front(page, 100, 270), "v3", "an absolute box with a level taken out")
        compare(front(page, 100, 390), "v4", "one moved")
        compare(front(page, 100, 500), "k2", "the box moved, where it went")
        compare(front(page, 100, 580), "b4", "the absolute box moved, where it went")
    }
}
}
"##;

/// With no script, Chromium's fronts are `k1 k2 b3 b4 da db`.
const OUT_OF_FLOW_MOVES_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "k1")
        compare(front(page, 100, 150), "k2")
        compare(front(page, 100, 270), "b3")
        compare(front(page, 100, 390), "b4")
        compare(front(page, 100, 500), "da")
        compare(front(page, 100, 580), "db")
    }
}
}
"##;

/// What stops a lift and what gives none: a transformed or sticky box at
/// `auto` is a stacking context, which keeps what it holds in, so it stays
/// at its own level; a box with no box (`display: none`) lifts nothing; a
/// fixed box is painted over the window, not in its block. And an overlay
/// taken out of its block keeps its place at its block, and at the
/// positioned box its block is in, and at a block its `position: relative`
/// offset moves (`#h8`, painted 30px under and 20px right of where the
/// flow has it). In a
/// right-to-left block, `right` wins over `left` (`#h9`, 60px to the left).
/// An animated `left` wins over `right` and moves the box from there
/// (`#h10`, halfway from 0 to 40px).
const CONTEXTS: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 40px }
.over { position: relative; transform: translate(20px, -20px); z-index: 2; background: blue }
.aside { margin-left: 400px }
.in { position: relative; background: red }
.up { z-index: 10 }
.tf { transform: translateX(0px) }
.st { position: sticky }
.rel { position: relative }
.ab { position: absolute; left: 0; top: 0; width: 50px; background: green }
#f5 { position: fixed; top: 1150px; left: 600px; z-index: 10; width: 50px; background: green }
#h6 { position: relative; margin-left: 40px }
#o7 { position: relative; margin-left: 30px }
#h8 { position: relative; top: 30px; left: 20px; margin-left: 20px }
#h9 { position: relative; left: 20px; right: 60px }
@keyframes nudge { from { left: 0 } to { left: 40px } }
#h10 { position: relative; right: 20px; animation: nudge 10s linear -5s paused }
.lv0 { z-index: 0 }
.hide { display: none }
#p15 { height: 80px; overflow: hidden }
#p15::after { content: ''; display: block; position: relative; z-index: 10; height: 20px; background: red }
#v15 { height: 20px; margin-bottom: 20px; transform: translate(20px, -40px) }
</style></head><body>
<div id=t1 class=tf><div id=c1 class=in></div></div><div id=v1 class=over></div>
<div class=gap></div>
<div id=t2 class=tf><div id=c2 class="in up"></div></div><div id=v2 class=over></div>
<div class=gap></div>
<div id=t3 class=st><div id=c3 class="in up"></div></div><div id=v3 class=over></div>
<div class=gap></div>
<div id=p4 class=rel><div id=c4 class="in up"></div><div id=e4 class=in></div></div><div id=v4 class=over></div>
<div class=gap></div>
<div id=p5 class=rel><div id=e5 class=in></div><div id=f5></div></div><div id=v5 class=over></div>
<div class=gap></div>
<div id=h6><div id=a6 class=ab></div></div><div id=v6 class="over aside"></div>
<div class=gap></div>
<div id=o7><div id=h7 class=rel><div id=a7 class=ab></div></div></div><div id=v7 class="over aside"></div>
<div class=gap></div>
<div id=h8><div id=a8 class=ab></div></div>
<div class=gap></div>
<div dir=rtl><div id=h9></div></div>
<div class=gap></div>
<div id=h10></div>
<div class=gap></div>
<div id=p11 class=rel><div id=c11 class=in></div></div><div id=v11 class=over></div>
<div class=gap></div>
<div id=p12 class="rel lv0"><div id=c12 class="in up"></div></div><div id=v12 class=over></div>
<div class=gap></div>
<div id=a13><div id=c13 class="in up"></div></div><div id=v13 class=over></div>
<div class=gap></div>
<div id=p14 class=rel><div id=c14 class=in></div></div><div id=v14 class=over></div>
<div class=gap></div>
<div id=p15><div id=c15 class="in up"></div></div><div id=v15 class=over></div>
<script>
var named = ['t1','c1','v1','t2','c2','v2','t3','c3','v3','p4','c4','e4','v4','p5','e5','f5','v5','h6','a6','v6','o7','h7','a7','v7','h8','a8','h9','h10','p11','c11','v11','p12','c12','v12','a13','c13','v13','p14','c14','v14','p15','c15','v15'].map(function (id) { return document.getElementById(id); });
// A box raised inside a transformed box at `auto`, which keeps it in; a
// box that lifted its block hidden.
document.getElementById('c1').style.zIndex = '10';
document.getElementById('c4').style.display = 'none';
// A level set under a hidden block, which then shows; a block's level
// taken away, which leaves it at `auto`; and a raised block hidden and
// shown again.
document.getElementById('p11').style.display = 'none';
document.getElementById('c11').style.zIndex = '10';
document.getElementById('p11').style.display = '';
document.getElementById('p12').classList.remove('lv0');
document.getElementById('c13').style.display = 'none';
document.getElementById('c13').style.display = '';
// Row 11 again, hidden and shown by a class: a binding, not a style.
document.getElementById('p14').classList.add('hide');
document.getElementById('c14').style.zIndex = '10';
document.getElementById('p14').classList.remove('hide');
// A kid that lifted its block lowered: the block's `::after` still lifts
// it. `#v15` meets only the `::after`: a block that clips holds what it
// lifts, and one item has one level, so its own background goes up too.
document.getElementById('c15').style.zIndex = '-1';
</script></body></html>"#;

/// Chromium's fronts.
const CONTEXTS_TEST: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "v1", "raised in a transformed box")
        compare(front(page, 100, 150), "v2", "a transformed box keeps its kid's level in")
        compare(front(page, 100, 270), "v3", "a sticky box keeps its kid's level in")
        compare(front(page, 100, 390), "v4", "the box that lifted its block hidden")
        compare(front(page, 100, 510), "v5", "a fixed box lifts nothing")
        compare(front(page, 70, 630), "a6", "at its block")
        compare(front(page, 20, 610), "", "not at its stop")
        compare(front(page, 70, 730), "a7", "at its block in a positioned box")
        compare(front(page, 10, 730), "", "not at that box's stop")
        compare(front(page, 70, 890), "a8", "at its block, moved by its offset")
        compare(front(page, 150, 890), "h8", "the block, moved")
        compare(front(page, 30, 890), "", "by its left offset too")
        compare(front(page, 70, 850), "", "not where the flow has the block")
        compare(front(page, 10, 940), "h9", "moved by right in a right-to-left block")
        compare(front(page, 210, 940), "", "not by left")
        compare(front(page, 150, 940), "")
        compare(front(page, 10, 1020), "", "an animated left wins over right")
        compare(front(page, 30, 1020), "h10")
        compare(front(page, 210, 1020), "h10")
        compare(front(page, 100, 1110), "c11", "a level set under a hidden block")
        compare(front(page, 100, 1230), "c12", "its block's level taken away")
        compare(front(page, 100, 1350), "c13", "a raised block hidden and shown")
        compare(front(page, 100, 1470), "c14", "a level set under a block a class hides")
        compare(front(page, 100, 1610), "p15", "its ::after lifts the block over the box after it")
        compare(front(page, 210, 1610), "v15")
        compare(front(page, 100, 1630), "p15")
    }
}
}
"##;

/// With no script, Chromium's fronts are the same but `c4`, `v11`, `v12`
/// and `v14` on rows four, eleven, twelve and fourteen.
const CONTEXTS_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 100, 30), "v1")
        compare(front(page, 100, 150), "v2")
        compare(front(page, 100, 270), "v3")
        compare(front(page, 100, 390), "c4")
        compare(front(page, 100, 510), "v5")
        compare(front(page, 70, 630), "a6")
        compare(front(page, 20, 610), "")
        compare(front(page, 70, 730), "a7")
        compare(front(page, 10, 730), "")
        compare(front(page, 70, 890), "a8")
        compare(front(page, 150, 890), "h8")
        compare(front(page, 30, 890), "")
        compare(front(page, 70, 850), "")
        compare(front(page, 10, 940), "h9")
        compare(front(page, 210, 940), "")
        compare(front(page, 150, 940), "")
        compare(front(page, 10, 1020), "", "an animated left wins over right")
        compare(front(page, 30, 1020), "h10")
        compare(front(page, 210, 1020), "h10")
        compare(front(page, 100, 1110), "v11")
        compare(front(page, 100, 1230), "v12")
        compare(front(page, 100, 1350), "c13")
        compare(front(page, 100, 1470), "v14")
        compare(front(page, 100, 1610), "p15")
        compare(front(page, 210, 1610), "v15")
        compare(front(page, 100, 1630), "p15")
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

/// With no box made and no attribute set the page stays static: no rules go
/// to the runtime, and the converter's own bindings give each level. (An
/// attribute a script sets under a rule with a level opens the page, as
/// `rules_a_script_brings_restack_the_boxes_around_them` has it.)
#[test]
fn rules_a_script_brings_restack_them_on_a_static_page() {
    let start = PATHS
        .find("document.getElementById('c6')")
        .expect("the attribute set");
    let end = PATHS.find("</script>").expect("the script's end");
    let page = format!("{}{}", &PATHS[..start], &PATHS[end..]);
    let test = PATHS_TEST
        .replace(
            r#"compare(front(page, 100, 630), "c6", "attribute set")"#,
            r#"compare(front(page, 100, 630), "o6", "no attribute set")"#,
        )
        .replace(
            r#"compare(front(page, 100, 750), "c7", "a box put in with a level")"#,
            r#"compare(front(page, 100, 750), "o7", "no box put in")"#,
        )
        .replace(
            r#"compare(front(page, 100, 1130), "d10", "a class raises a block's kid after a script did")"#,
            r#"compare(front(page, 100, 1130), "o10", "no level set, no class added")"#,
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

#[test]
fn out_of_flow_boxes_moved_and_removed_restack_what_they_leave() {
    run(
        OUT_OF_FLOW_MOVES,
        &with_test(OUT_OF_FLOW_MOVES_TEST),
        "out-of-flow-moves",
        false,
    );
}

#[test]
fn the_moved_out_of_flow_rules_levels_match_chromium() {
    run(
        &unscripted(OUT_OF_FLOW_MOVES, "// A raised box"),
        &with_test(OUT_OF_FLOW_MOVES_UNSCRIPTED),
        "out-of-flow-moves-unscripted",
        false,
    );
}

#[test]
fn contexts_keep_what_they_hold_in() {
    run(CONTEXTS, &with_test(CONTEXTS_TEST), "contexts", false);
}

#[test]
fn contexts_keep_what_they_hold_in_without_the_rules() {
    run(
        CONTEXTS,
        &with_test(CONTEXTS_TEST),
        "contexts-no-rules",
        true,
    );
}

#[test]
fn the_contexts_rules_levels_match_chromium() {
    run(
        &unscripted(CONTEXTS, "// A box raised"),
        &with_test(CONTEXTS_UNSCRIPTED),
        "contexts-unscripted",
        false,
    );
}

/// A raised block a script makes taller (`#rt`), risen to its stop's column
/// after the hoisted `#at`: what follows it in its block moves down, and it
/// stays over that. A `height` write opens the page, so the runtime lays the
/// block out; on its own page, so CONTEXTS stays the converter's.
const TALLER: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 40px }
.ab { position: absolute; left: 0; top: 0; width: 50px; background: green }
#ht { position: relative; margin-left: 40px }
#bt { height: auto }
#rt { position: relative; z-index: 1; background: red }
</style></head><body>
<div id=ht><div id=at class=ab></div></div>
<div class=gap></div>
<div id=bt><div id=rt></div><div id=nt></div></div>
<script>
var named = ['ht','at','bt','rt','nt'].map(function (id) { return document.getElementById(id); });
// A raised block made taller moves what follows it.
document.getElementById('rt').style.height = '80px';
</script></body></html>"#;

/// Chromium's fronts.
const TALLER_TEST: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 60, 20), "at")
        compare(front(page, 100, 100), "rt")
        compare(front(page, 100, 140), "rt", "a raised block made taller")
        compare(front(page, 100, 180), "nt", "moves what follows it")
        compare(front(page, 100, 220), "")
    }
}
}
"##;

/// With no script, Chromium's fronts are the block at its own height.
const TALLER_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 60, 20), "at")
        compare(front(page, 100, 100), "rt")
        compare(front(page, 100, 140), "nt")
        compare(front(page, 100, 180), "")
    }
}
}
"##;

#[test]
fn a_raised_block_a_script_makes_taller_moves_what_follows_it() {
    let out = convert_str(TALLER, &Options::default());
    let qml = out.main_qml().expect("Main.qml");
    assert!(qml.contains("eldurRisen: true"), "#rt does not rise\n{qml}");
    run(TALLER, &with_test(TALLER_TEST), "taller", false);
}

#[test]
fn a_raised_blocks_rules_height_matches_chromium() {
    run(
        &unscripted(TALLER, "// A raised block made"),
        &with_test(TALLER_UNSCRIPTED),
        "taller-unscripted",
        false,
    );
}

/// Overlays of the initial block at one level with the boxes of the
/// page: `#p1` at 10 over `#p2` at 0 after it, though `#p2` rises; and
/// `#r2` raised to 5 over `#a2` at 1 after it. No script.
const INITIAL_BLOCK: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 80px }
.ab { position: absolute; width: 50px; background: green }
#p1 { z-index: 10; left: 0; top: 0 }
#p2 { z-index: 0; left: 20px; top: 0; background: red }
#r2 { position: relative; z-index: 5; background: blue }
#a2 { z-index: 1; left: 0; top: 80px }
</style></head><body>
<div id=p1 class=ab></div><div id=p2 class=ab></div>
<div class=gap></div>
<div id=r2></div><div id=a2 class=ab></div>
<script>
// Named so that they get an objectName the test can find: by literal
// lookups, which leave the page static, with no DOM at run time.
var p1 = document.getElementById('p1');
var p2 = document.getElementById('p2');
var r2 = document.getElementById('r2');
var a2 = document.getElementById('a2');
</script></body></html>"#;

/// Chromium's fronts.
const INITIAL_BLOCK_TEST: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 10, 20), "p1")
        compare(front(page, 40, 20), "p1", "over a later overlay at a lower level")
        compare(front(page, 60, 20), "p2")
        compare(front(page, 100, 20), "")
        compare(front(page, 10, 100), "r2", "a raised block over a later overlay")
        compare(front(page, 100, 100), "r2")
    }
}
}
"##;

#[test]
fn the_initial_blocks_overlays_keep_one_layer_with_the_page() {
    let out = convert_str(INITIAL_BLOCK, &Options::default());
    let qml = out.main_qml().expect("Main.qml");
    assert!(
        qml.contains(r#"objectName: "p1""#),
        "p1 is not named\n{qml}"
    );
    // A page with a DOM at run time gives each overlay a slot to move.
    assert!(
        !qml.contains("eldurSlot: p1Slot"),
        "the page is not static\n{qml}"
    );
    run(
        INITIAL_BLOCK,
        &with_test(INITIAL_BLOCK_TEST),
        "initial-block",
        false,
    );
}

/// A static block's overlay and a later static block's relative box:
/// Chromium paints both in the page's positioned layer, in tree order, so
/// `#ox` is over `#bx`'s flow and `#rx` is over `#ox`. Neither block
/// rises, so a lift for `#ax` alone would put `#ox` over `#rx` too.
const HELD: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
#ax { background: yellow }
#ox { position: absolute; margin-top: 20px; width: 100px; background: green }
#bx { background: blue }
#rx { position: relative; width: 50px; background: red }
</style></head><body>
<div id=ax><div id=ox></div></div><div id=bx><div id=rx></div></div>
<script>
var ax = document.getElementById('ax');
var ox = document.getElementById('ox');
var bx = document.getElementById('bx');
var rx = document.getElementById('rx');
</script></body></html>"#;

/// Chromium's fronts (`elementFromPoint`).
const HELD_TEST: &str = r##"
    function test_a_later_blocks_relative_box_is_over_an_earlier_overlay() {
        compare(front(page, 25, 50), "rx", "over the earlier block's overlay")
        compare(front(page, 75, 50), "ox", "over the later block's flow")
        compare(front(page, 150, 50), "bx")
        compare(front(page, 75, 30), "ox")
        compare(front(page, 150, 30), "ax")
        compare(front(page, 25, 70), "rx")
    }
}
}
"##;

#[test]
fn a_later_blocks_relative_box_paints_over_an_earlier_blocks_overlay() {
    run(HELD, &with_test(HELD_TEST), "held", false);
}

/// The same blocks, with a class that makes the later one a stacking
/// context (`opacity`): Chromium paints it, and `#rx` in it, as one layer
/// after `#ox`, so its flow is over `#ox` too. Without the class `#rx` rises
/// out of `#bx`, and with it `#rx` stays in it: the earlier block sends
/// `#ox` to the stop in both, so they keep one shape and the class applies.
const DIMMED_TEST: &str = r##"
    function test_a_later_block_a_class_makes_a_context_is_over_an_earlier_overlay() {
        compare(front(page, 25, 50), "rx", "what it holds is over the earlier block's overlay")
        compare(front(page, 75, 50), "bx", "and its flow is too")
        compare(front(page, 150, 50), "bx")
        compare(front(page, 75, 30), "ox")
        compare(front(page, 150, 30), "ax")
        compare(front(page, 25, 70), "rx")
        compare(front(page, 75, 70), "bx")
    }
}
}
"##;

#[test]
fn a_class_that_makes_a_later_block_a_context_keeps_the_earlier_blocks_shape() {
    let page = HELD
        .replace(
            "#bx { background: blue }",
            "#bx { background: blue }\n.dim { opacity: 0.5 }",
        )
        .replace(
            "var rx = document.getElementById('rx');",
            "var rx = document.getElementById('rx');\nbx.classList.add('dim');",
        );
    let options = Options {
        root: RootKind::Item,
        viewport: Viewport {
            width: 900.0,
            height: 1200.0,
            ..Viewport::default()
        },
        ..Options::default()
    };
    let out = convert_str(&page, &options);
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.to_string().contains("dynamic-form")),
        "{:?}",
        out.diagnostics
    );
    run(&page, &with_test(DIMMED_TEST), "dimmed", false);
}

/// The same blocks, with a context (`#qx`, `opacity`) in the later block's
/// relative box whose kid (`#gx`) a class gives a level. CSS keeps that
/// level inside `#qx`, so `#rx` still paints over `#ox` in tree order, as
/// it does without the class.
const CONTAINED_TEST: &str = r##"
    function test_a_level_a_class_gives_inside_a_context_leaves_the_box_over_an_earlier_overlay() {
        compare(front(page, 5, 50), "gx", "the kid in the context is over the earlier overlay")
        compare(front(page, 15, 50), "qx", "and the context is")
        compare(front(page, 40, 50), "rx", "and the box that holds it is")
        compare(front(page, 75, 50), "ox")
        compare(front(page, 150, 50), "bx")
        compare(front(page, 75, 30), "ox")
        compare(front(page, 150, 30), "ax")
        compare(front(page, 5, 70), "gx")
        compare(front(page, 40, 70), "rx")
    }
}
}
"##;

#[test]
fn a_level_a_class_gives_inside_a_context_does_not_keep_its_box_from_rising() {
    let page = HELD
        .replace(
            "#rx { position: relative; width: 50px; background: red }",
            "#rx { position: relative; width: 50px; background: red }\n\
             #qx { opacity: .9; width: 20px; background: orange }\n\
             #gx { position: relative; width: 10px; background: purple }\n\
             .up { z-index: 3 }",
        )
        .replace(
            "<div id=rx></div>",
            "<div id=rx><div id=qx><div id=gx></div></div></div>",
        )
        .replace(
            "var rx = document.getElementById('rx');",
            "var rx = document.getElementById('rx');\n\
             var qx = document.getElementById('qx');\n\
             document.getElementById('gx').classList.add('up');",
        );
    assert_ne!(page, HELD);
    run(&page, &with_test(CONTAINED_TEST), "contained", false);
}

/// The same blocks, with an inline-block (`#ix`) in the later block's
/// relative box that a class gives a level, and a box at 1 (`#lx`) over the
/// lower half of that block. Chromium paints `#ix` over `#lx` and `#rx`
/// over `#ox` in tree order; without the class `#lx` is over `#ix` too.
/// To lift `#ix` the converter raises its ancestors, as it does for a level
/// `#ix` has from the start or for a block in its place ("Overlays of
/// different ancestors", docs/spec/layout.md), so `#bx` and the rest of
/// `#rx` come over `#ox` (75,50) and `#lx` (30,70; 75,70), where Chromium
/// paints them under; those points are not checked.
const INLINE_LEVEL_TEST: &str = r##"
    function test_a_level_a_class_gives_an_inline_box_lifts_it_over_a_later_box() {
        compare(front(page, 5, 70), "ix", "the inline box is over the later box")
        compare(front(page, 5, 50), "ix")
        compare(front(page, 30, 50), "rx", "and over the earlier block's overlay")
        compare(front(page, 150, 50), "bx")
        compare(front(page, 75, 30), "ox")
        compare(front(page, 150, 30), "ax")
        compare(front(page, 150, 90), "lx")
    }
}
}
"##;

#[test]
fn a_level_a_class_gives_an_inline_box_lifts_it_over_a_later_box() {
    let page = inline_level_page(INLINE_BLOCK, "<span id=ix></span>", RELATIVE, LATER_AT_1);
    run(&page, &with_test(INLINE_LEVEL_TEST), "inline-level", false);
}

/// The same, with `#ix` in a span that makes no stacking context: the
/// level lifts through it as through a block at `auto`.
#[test]
fn a_level_a_class_gives_an_inline_box_in_a_plain_span_lifts_it() {
    let page = inline_level_page(
        INLINE_BLOCK,
        "<span id=wx><span id=ix></span></span>",
        RELATIVE,
        LATER_AT_1,
    );
    run(
        &page,
        &with_test(INLINE_LEVEL_TEST),
        "inline-level-span",
        false,
    );
}

/// The same, with `#ix` in a span that makes a stacking context. The
/// context keeps the level in: Chromium paints `#lx` over `#ix` (5,70),
/// whether the span is a box (`inline-block`) or not.
const CONTEXT_SPAN_TEST: &str = r##"
    function test_a_span_that_makes_a_context_keeps_the_level_in() {
        compare(front(page, 5, 70), "lx", "the later box is over the inline box")
        compare(front(page, 5, 50), "ix")
        compare(front(page, 30, 50), "rx")
        compare(front(page, 150, 50), "bx")
        compare(front(page, 75, 30), "ox")
        compare(front(page, 150, 30), "ax")
        compare(front(page, 30, 70), "lx")
        compare(front(page, 150, 90), "lx")
    }
}
}
"##;

#[test]
fn a_span_that_makes_a_context_keeps_the_level_a_class_gives_in() {
    for (name, wrapper) in [
        ("context-span", "opacity: .9"),
        (
            "context-inline-block",
            "display: inline-block; vertical-align: top; opacity: .9",
        ),
    ] {
        let page = inline_level_page(
            INLINE_BLOCK,
            &format!("<span id=wx style=\"{wrapper}\"><span id=ix></span></span>"),
            RELATIVE,
            LATER_AT_1,
        );
        run(&page, &with_test(CONTEXT_SPAN_TEST), name, false);
    }
}

/// The same, with `#ix` in a span that makes a stacking context with a
/// level of its own (5): over `#lx` (1), the context lifts `#ix` with it.
#[test]
fn a_span_with_a_level_of_its_own_lifts_the_level_it_keeps_in() {
    let page = inline_level_page(
        INLINE_BLOCK,
        "<span id=wx style=\"position: relative; z-index: 5\"><span id=ix></span></span>",
        RELATIVE,
        LATER_AT_1,
    );
    run(
        &page,
        &with_test(INLINE_LEVEL_TEST),
        "context-span-level",
        false,
    );
}

/// The inline-block in a static box (`#rx`) and a static box after (`#lx`):
/// nothing after `#bx` paints later, so only the level a class gives `#ix`
/// sends `#ox` to the stop, where Chromium has it, under `#ix` (5,50).
/// `#bx` lifts with that level ("Overlays of different ancestors"), so it
/// and `#rx` come over `#ox` (30,50; 75,50), where Chromium paints them
/// under; those points are not checked.
const SENT_TEST: &str = r##"
    function test_a_level_a_class_gives_an_inline_box_sends_an_earlier_overlay_under_it() {
        compare(front(page, 5, 50), "ix", "the inline box is over the earlier block's overlay")
        compare(front(page, 5, 70), "ix")
        compare(front(page, 5, 30), "ox")
        compare(front(page, 30, 70), "rx")
        compare(front(page, 75, 70), "bx")
        compare(front(page, 150, 30), "ax")
        compare(front(page, 150, 90), "lx")
    }
}
}
"##;

#[test]
fn a_level_a_class_gives_an_inline_box_sends_an_earlier_overlay_under_it() {
    let page = inline_level_page(
        INLINE_BLOCK,
        "<span id=ix></span>",
        "#rx { width: 50px; background: red }",
        "#lx { background: gray }",
    );
    run(&page, &with_test(SENT_TEST), "inline-level-sent", false);
}

const INLINE_BLOCK: &str = "#ix { display: inline-block; position: relative; width: 10px; height: 40px; vertical-align: top; background: purple }";
const RELATIVE: &str = "#rx { position: relative; width: 50px; background: red }";
const LATER_AT_1: &str = "#lx { position: relative; z-index: 1; top: -20px; background: gray }";

/// `HELD` with `kid` in `#rx`, a box after `#bx` (`#lx`), and a class that
/// gives `#ix` a level.
fn inline_level_page(ix: &str, kid: &str, rx: &str, lx: &str) -> String {
    let page = HELD
        .replace(RELATIVE, &format!("{rx}\n{ix}\n{lx}\n.up {{ z-index: 3 }}"))
        .replace(
            "<div id=rx></div></div>",
            &format!("<div id=rx>{kid}</div></div><div id=lx></div>"),
        )
        .replace(
            "var rx = document.getElementById('rx');",
            "var rx = document.getElementById('rx');\n\
             var lx = document.getElementById('lx');\n\
             document.getElementById('ix').classList.add('up');",
        );
    assert_eq!(page.matches("id=ix").count(), 1);
    assert_eq!(page.matches("id=lx").count(), 1);
    assert_eq!(page.matches(".up").count(), 1);
    page
}

/// A float the runtime lays out (`#fm`, which a script makes) and one it
/// lays out again (`#ff`), whose raised kid (`#fc` at 10) lifts it: the kid
/// is over the box after the float (`#fv` at 2) and over the new float.
const FLOAT_LIFT: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.fl { float: left; width: 100px; background: #aaa }
#fc { position: relative; z-index: 10; background: red }
#fv { position: relative; transform: translate(20px, -20px); z-index: 2; background: blue }
</style></head><body>
<div id=fa><div id=ff class=fl><div id=fc></div></div></div><div id=fv></div>
<script>
var named = ['fa','ff','fc','fv'].map(function (id) { return document.getElementById(id); });
var made = document.createElement('div');
made.className = 'fl';
made.id = 'fm';
document.getElementById('fa').appendChild(made);
</script></body></html>"#;

/// Chromium's fronts.
const FLOAT_LIFT_TEST: &str = r##"
    function test_a_floats_raised_kid_stays_raised() {
        compare(front(page, 50, 30), "fc", "over the box after its float")
        compare(front(page, 150, 30), "fc", "over the float the script made")
        compare(front(page, 150, 50), "fv")
        compare(front(page, 210, 30), "fv")
    }
}
}
"##;

#[test]
fn a_runtime_laid_float_keeps_the_level_its_kid_lifts_it_to() {
    run(FLOAT_LIFT, &with_test(FLOAT_LIFT_TEST), "float-lift", false);
}

/// Hoisted overlays at the top of the `z-index` range keep their order: the
/// host is one over the level, which does not wrap or meet the next.
#[test]
fn the_highest_hoisted_levels_stay_apart() {
    let page = r#"<!doctype html><html><body>
<div><div id=top style="position: absolute; z-index: 2147483647"></div><div id=next style="position: absolute; z-index: 2147483646"></div></div>
<div style="position: relative"></div>
</body></html>"#;
    let out = convert_str(page, &Options::default());
    let qml = out.main_qml().expect("Main.qml");
    assert!(qml.contains("z: 2147483648"), "the highest host\n{qml}");
    assert!(qml.contains("z: 2147483647"), "the next host\n{qml}");
}

/// Boxes a script makes out of flow are painted in gates over the page,
/// which stack as the boxes do: the earlier one raised over the later. And
/// a raised block a script moves into a stacking context (`#dq`, by its
/// opacity) is held in it, under what that context is under.
const MADE_AND_MOVED: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
div { height: 40px; width: 200px }
.gap { height: 40px }
.over { position: relative; transform: translate(20px, -20px); z-index: 2; background: blue }
.in { position: relative; background: red }
.up { z-index: 3 }
.made { position: absolute; top: 0; width: 100px; background: green }
#dq { opacity: .5 }
</style></head><body>
<div id=hq></div>
<div class=gap></div>
<div id=dq></div><div id=oq class=over></div>
<div class=gap></div>
<div id=sq><div id=mq class="in up"></div></div><div id=pq class=over></div>
<script>
var named = ['hq','dq','oq','sq','mq','pq'].map(function (id) { return document.getElementById(id); });
// Two boxes made out of flow, the earlier raised over the later; and a
// raised box moved into a stacking context, which holds it.
var g1 = document.createElement('div');
g1.id = 'g1';
g1.className = 'made';
g1.style.left = '0px';
var g2 = document.createElement('div');
g2.id = 'g2';
g2.className = 'made';
g2.style.left = '50px';
document.getElementById('hq').appendChild(g1);
document.getElementById('hq').appendChild(g2);
g1.style.zIndex = '10';
document.getElementById('dq').appendChild(document.getElementById('mq'));
</script></body></html>"#;

/// Chromium's fronts.
const MADE_AND_MOVED_TEST: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 25, 20), "g1")
        compare(front(page, 75, 20), "g1", "the earlier made box, raised over the later")
        compare(front(page, 125, 20), "g2")
        compare(front(page, 100, 90), "mq", "the moved box, where it went")
        compare(front(page, 100, 110), "oq", "held in the context it went into")
        compare(front(page, 100, 230), "pq")
    }
}
}
"##;

/// With no script, Chromium's fronts are `oq mq` where the boxes paint.
const MADE_AND_MOVED_UNSCRIPTED: &str = r##"
    function test_levels_follow_what_the_boxes_hold() {
        compare(front(page, 75, 20), "")
        compare(front(page, 100, 90), "")
        compare(front(page, 100, 110), "oq")
        compare(front(page, 100, 230), "mq")
    }
}
}
"##;

#[test]
fn boxes_a_script_makes_or_moves_stack_where_they_are_painted() {
    run(
        MADE_AND_MOVED,
        &with_test(MADE_AND_MOVED_TEST),
        "made-and-moved",
        false,
    );
}

#[test]
fn the_made_and_moved_rules_levels_match_chromium() {
    run(
        &unscripted(MADE_AND_MOVED, "// Two boxes made"),
        &with_test(MADE_AND_MOVED_UNSCRIPTED),
        "made-and-moved-unscripted",
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
