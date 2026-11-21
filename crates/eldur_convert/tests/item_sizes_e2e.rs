//! Sizes that flex and grid items take from their container, against
//! Chromium's: a row's stretched items are definite once the row has a
//! height of its own; a grid item fills a pinned track with its margins, a
//! fixed auto row holds its used height, and a percentage-wide item keeps
//! its frozen `fr` column's minimum.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const FLEX_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.r { display: flex; width: 400px; background: #eee; margin-bottom: 10px }
.c { display: flex; flex-direction: column; width: 400px; background: #ddd; margin-bottom: 10px }
.r > div { width: 100px }
.k { height: 50%; background: #00c; width: 40px }
.s { height: 30px; background: #0c0 }
</style></head><body>
<div class=r id=a style="min-height:100px"><div id=a1><div class=k id=a1k></div></div><div class=s id=a2></div></div>
<div class=r id=b style="height:100px"><div id=b1><div class=k id=b1k></div></div><div class=s id=b2></div></div>
<div class=r id=f style="height:50%"><div id=f1><div class=k id=f1k></div></div><div class=s id=f2></div></div>
<div class=c id=c style="min-height:200px"><div id=c1 style="flex-grow:1"><div class=k id=c1k></div><div id=c1m style="height:10px;width:20px"></div></div><div class=s id=c2></div></div>
<div class=c id=d style="height:200px"><div id=d1 style="flex-grow:1"><div class=k id=d1k></div></div><div class=s id=d2></div></div>
<div class=c id=e style="height:50%"><div id=e1 style="flex-grow:1"><div class=k id=e1k></div></div><div class=s id=e2></div></div>
<script>var named = Array.from(document.querySelectorAll('[id]'));</script>
</body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800: `[x, y, width, height]`.
const FLEX_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "FlexDefinite"
    when: windowShown

    property var page
    // Within half a pixel: Qt rounds layout margins to whole pixels.
    function check(name, x, y, width, height) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0)
        const got = [at.x, at.y, item.width, item.height]
        const want = [x, y, width, height]
        for (let i = 0; i < 4; ++i)
            verify(Math.abs(got[i] - want[i]) <= 0.5, name + " " + got + ", Chromium " + want)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    // A row with a min-height, or a percentage height left auto, gives its
    // stretched items a definite height for their children's percentages.
    function test_row_items_are_definite() {
        check("a1k", 0, 0, 40, 50)
        check("b1k", 0, 110, 40, 50)
        check("f1k", 0, 220, 40, 15)
    }
    // A column's min-height alone does not: the child stays 0 high, and
    // the children stay at the top of the item the column grew.
    function test_column_min_height_is_not_definite() {
        check("c1k", 0, 260, 40, 0)
        check("c1m", 0, 260, 20, 10)
        check("d1k", 0, 470, 40, 85)
    }

}
}
"##;

const AUTO_ROW_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.r { display: flex; width: 400px; background: #eee; margin-bottom: 10px }
.r > div { width: 100px }
.k { height: 50%; background: #00c; width: 40px }
.s { height: 30px; background: #0c0 }
</style></head><body>
<div class=r id=g><div id=g1><div class=k id=g1k></div></div><div class=s id=g2></div></div>
<div class=r id=h style="height:50%;min-height:40px"><div id=h1><div class=k id=h1k></div></div><div class=s id=h2></div></div>
<script>var named = Array.from(document.querySelectorAll('[id]'));</script>
</body></html>"#;

const AUTO_ROW_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "FlexMinRow"
    when: windowShown

    property var page
    // Within half a pixel: Qt rounds layout margins to whole pixels.
    function check(name, x, y, width, height) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0)
        const got = [at.x, at.y, item.width, item.height]
        const want = [x, y, width, height]
        for (let i = 0; i < 4; ++i)
            verify(Math.abs(got[i] - want[i]) <= 0.5, name + " " + got + ", Chromium " + want)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_min_height_row_is_definite() {
        check("h1k", 0, 40, 40, 20)
    }
    // An auto-high row's stretched item is definite once its line is sized
    // (css-flexbox §9.4 step 11).
    function test_auto_row_items_are_definite_once_the_line_is_sized() {
        check("g1k", 0, 0, 40, 15)
    }

}
}
"##;

/// The same items with a background, padding and a border: the percentage is
/// of the height inside them.
const DECORATED_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.r { display: flex; width: 400px; margin-bottom: 10px }
.r > div { width: 100px; background: #c00; padding: 5px 0; border-top: 2px solid #000 }
.k { height: 50%; background: #00c; width: 40px }
.s { height: 30px }
</style></head><body>
<div class=r id=a style="min-height:100px"><div id=a1><div class=k id=a1k></div></div><div class=s id=a2></div></div>
<div class=r id=b style="height:100px"><div id=b1><div class=k id=b1k></div></div></div>
<div style="height:300px"><div class=r id=e style="height:50%"><div id=e1><div class=k id=e1k></div></div></div></div>
<div class=r id=g><div id=g1><div class=k id=g1k></div></div><div class=s id=g2></div></div>
<div class=r id=h style="min-height:100px"><div id=h1><div id=h1g style="display:flex;flex-direction:column;width:40px;background:#fc0"><div style="height:20px"></div></div></div><div class=s id=h2></div></div>
<div class=r id=m><div id=m1><div class=k id=m1k style="height:100%;margin-top:10px"></div></div><div class=s id=m2></div></div>
<div class=r id=n><div id=n1><div class=k id=n1k style="min-height:5px"></div></div><div class=s id=n2></div></div>
<div class=r id=p><div id=p1><div class=k id=p1k style="height:100%;min-height:5px"></div></div><div class=s id=p2></div></div>
<div class=r id=q><div id=q1><input class=k id=q1k style="display:block;padding:0;border:0"><div class=k id=q1d></div></div><div class=s id=q2></div></div>
<script>var named = Array.from(document.querySelectorAll('[id]'));</script>
</body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800: `[x, y, width, height]`.
const DECORATED_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "FlexDecorated"
    when: windowShown

    property var page
    // Within half a pixel: Qt rounds layout margins to whole pixels.
    function check(name, x, y, width, height) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0)
        const got = [at.x, at.y, item.width, item.height]
        const want = [x, y, width, height]
        for (let i = 0; i < 4; ++i)
            verify(Math.abs(got[i] - want[i]) <= 0.5, name + " " + got + ", Chromium " + want)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function test_decorated_items_resolve_inside_their_insets() {
        check("a1k", 0, 7, 40, 44)
        check("b1k", 0, 117, 40, 44)
        check("e1k", 0, 227, 40, 69)
        check("g1k", 0, 527, 40, 15)
    }
    // Without a percentage child, the content keeps its own height.
    function test_decorated_items_keep_their_content_height() {
        check("h1g", 0, 579, 40, 20)
    }
    // A percentage child with a margin would outgrow the line it resolves
    // against, so it stays `auto` (Chromium gives m1k 30 high) and the row
    // settles.
    function test_a_child_with_a_margin_leaves_the_line_alone() {
        check("m1", 0, 682, 100, 42)
        check("m2", 100, 682, 100, 42)
    }
    // A min-height is a floor the line settles at: the child still resolves.
    function test_a_child_with_a_min_height_still_resolves() {
        check("n1k", 0, 741, 40, 15)
        check("p1k", 0, 793, 40, 30)
    }
    // A form control with a percentage height resolves against the line like
    // an empty box, so its percentage sibling still does.
    function test_a_form_control_leaves_its_sibling_resolving() {
        check("q1k", 0, 845, 40, 15)
        check("q1d", 0, 860, 40, 15)
    }

}
}
"##;

const GRID_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.g { display: grid; margin-bottom: 10px; background: #eee }
.g div { height: 20px; background: #c00 }
</style></head><body>
<div class=g id=m style="grid-template-columns:300px 100px"><div id=m1 style="margin:0 10px 0 5px"></div><div id=m2></div></div>
<div class=g id=n style="width:300px;grid-template-columns:minmax(200px,1fr) 1fr"><div id=n1 style="margin:0 10px 0 5px"></div><div id=n2></div></div>
<div class=g id=o style="width:400px;grid-template-columns:repeat(auto-fit,minmax(100px,1fr));grid-auto-rows:50px"><div id=o1 style="height:100px;max-height:50px"></div><div id=o2 style="height:20px;min-height:40px"></div><div id=o3 style="height:100px"></div><div id=o4></div><div id=o5></div></div>
<div class=g id=p style="width:300px;grid-template-columns:minmax(200px,1fr) 1fr"><div id=p1 style="width:50%"></div><div id=p2></div></div>
<div class=g id=q style="width:300px;grid-template-columns:minmax(200px,1fr) 1fr"><div id=q1 style="width:50%;justify-self:center"></div><div id=q2></div></div>
<div class=g id=r style="width:300px;grid-template-columns:minmax(200px,1fr) 1fr"><div id=r1 style="width:50%;justify-self:end"></div><div id=r2></div></div>
<div class=g id=s style="width:600px;grid-template-columns:minmax(200px,1fr) 1fr"><div id=s1 style="width:50%"></div><div id=s2></div></div>
<div class=g id=t style="width:400px;grid-template-columns:repeat(auto-fit,minmax(100px,1fr));grid-auto-rows:50px"><div id=t1 style="height:100px;max-height:40px"><div id=t2 style="height:50%"></div></div><div id=t3 style="box-sizing:border-box;height:100px;max-height:10px;padding:15px 0"></div></div>
<script>var named = ['m','m1','m2','n','n1','n2','o','o1','o2','o3','o4','o5','p','p1','p2','q','q1','q2','r','r1','r2','s','s1','s2','t','t1','t2','t3'].map(function (i) { return document.getElementById(i); });</script>
</body></html>"#;

const GRID_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "GridPins"
    when: windowShown

    property var page
    // Within half a pixel: Qt rounds layout margins to whole pixels.
    function check(name, x, y, width, height) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0)
        const got = [at.x, at.y, item.width, item.height]
        const want = [x, y, width, height]
        for (let i = 0; i < 4; ++i)
            verify(Math.abs(got[i] - want[i]) <= 0.5, name + " " + got + ", Chromium " + want)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    // An auto-width item in a fixed or frozen track fills it with its margins.
    function test_margins_are_in_the_track() {
        check("m1", 5, 0, 285, 20)
        check("m2", 300, 0, 100, 20)
        check("n1", 5, 30, 185, 20)
        check("n2", 200, 30, 100, 20)
    }
    // A fixed auto row holds the used height, after min-height and max-height.
    function test_auto_row_heights_are_clamped() {
        const o1 = findChild(page, "o1"), o2 = findChild(page, "o2"), o3 = findChild(page, "o3")
        verify(o1 && o2 && o3)
        compare(o1.height, 50)
        compare(o2.height, 40)
        compare(o3.height, 100)
    }
    // A percentage height is of the used height, and a border box keeps its
    // padding however small its max-height.
    function test_used_heights_reach_children_and_keep_padding() {
        const t1 = findChild(page, "t1"), t2 = findChild(page, "t2"), t3 = findChild(page, "t3")
        verify(t1 && t2 && t3)
        compare(t1.height, 40)
        compare(t2.height, 20)
        compare(t3.height, 30)
    }
    // A percentage width in a frozen fr column keeps the column's minimum,
    // aligned or not.
    function test_percentage_items_keep_the_column() {
        const y = [findChild(page, "p").mapToItem(page, 0, 0).y,
                   findChild(page, "q").mapToItem(page, 0, 0).y,
                   findChild(page, "r").mapToItem(page, 0, 0).y,
                   findChild(page, "s").mapToItem(page, 0, 0).y]
        check("p1", 0, y[0], 100, 20)
        check("p2", 200, y[0], 100, 20)
        check("q1", 50, y[1], 100, 20)
        check("q2", 200, y[1], 100, 20)
        check("r1", 100, y[2], 100, 20)
        check("r2", 200, y[2], 100, 20)
        check("s1", 0, y[3], 150, 20)
        check("s2", 300, y[3], 300, 20)
    }

}
}
"##;

const INLINE_PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0; width: 300px; font: 16px/20px sans-serif }
.f { display: inline-flex; flex-wrap: wrap; gap: 5px 0 }
.g { display: inline-flex; flex-wrap: wrap; row-gap: 5px; column-gap: 10px }
.f div, .g div { width: 100px; height: 20px; background: #c00 }
.g div { width: 60px }
.t { margin-bottom: 10px }
</style></head><body>
<div class=t id=p1>Text <span class=f id=f><div id=f1></div><div id=f2></div><div id=f3></div><div id=f4></div></span></div>
<div class=t id=p2>Hi <span class=g id=g><div id=g1></div><div id=g2></div></span> after</div>
<div class=t id=p3>Some words <span class=g id=h><div id=h1></div><div id=h2></div><div id=h3></div><div id=h4></div></span> end</div>
<script>var named = ['p1','f','f1','f2','f3','f4','p2','g','g1','g2','p3','h','h1','h2','h3','h4'].map(function (i) { return document.getElementById(i); });</script>
</body></html>"#;

const INLINE_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineFlexWrap"
    when: windowShown

    property var page
    // Within half a pixel: Qt rounds layout margins to whole pixels.
    function check(name, x, y, width, height) {
        const item = findChild(page, name)
        verify(item, name)
        const at = item.mapToItem(page, 0, 0)
        const got = [at.x, at.y, item.width, item.height]
        const want = [x, y, width, height]
        for (let i = 0; i < 4; ++i)
            verify(Math.abs(got[i] - want[i]) <= 0.5, name + " " + got + ", Chromium " + want)
    }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    // A wrapping inline-flex that does not fit after its line's text takes a
    // line of its own, at its full width.
    function test_inline_flex_wraps_below_the_text() {
        check("p1", 0, 0, 300, 65)
        check("f", 0, 20, 300, 45)
        check("f1", 0, 20, 100, 20)
        check("f2", 100, 20, 100, 20)
        check("f3", 200, 20, 100, 20)
        check("f4", 0, 45, 100, 20)
        const h = findChild(page, "h")
        verify(h, "h")
        compare(h.width, 270)
        const top = h.mapToItem(page, 0, 0).y
        check("h1", 0, top, 60, 20)
        check("h2", 70, top, 60, 20)
        check("h3", 140, top, 60, 20)
        check("h4", 210, top, 60, 20)
    }

}
}
"##;

/// Waits for `child` until `limit`, killing it then, and returns its output
/// and whether it finished by itself. Both pipes are read while it runs: a
/// child that writes more than a pipe holds would otherwise block on the
/// write and look like a run that never ends.
fn wait_draining(
    mut child: std::process::Child,
    limit: std::time::Duration,
) -> (std::process::Output, bool) {
    use std::io::Read;
    fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = std::time::Instant::now() + limit;
    let mut finished = true;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for the child") {
            break status;
        }
        if std::time::Instant::now() > deadline {
            finished = false;
            child.kill().expect("kill the child");
            break child.wait().expect("wait for the killed child");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let output = std::process::Output {
        status,
        stdout: stdout.join().expect("stdout reader"),
        stderr: stderr.join().expect("stderr reader"),
    };
    (output, finished)
}

#[cfg(unix)]
#[test]
fn a_child_that_writes_more_than_a_pipe_holds_still_finishes() {
    let child = std::process::Command::new("sh")
        .args([
            "-c",
            "head -c 200000 /dev/zero; head -c 200000 /dev/zero >&2",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run sh");
    let (output, finished) = wait_draining(child, std::time::Duration::from_secs(10));
    assert!(finished, "the child blocked on a full pipe");
    assert!(output.status.success());
    assert_eq!(
        (output.stdout.len(), output.stderr.len()),
        (200_000, 200_000)
    );
}

#[cfg(unix)]
#[test]
fn a_grandchild_that_holds_the_pipes_open_does_not_outlast_the_limit() {
    use std::os::unix::process::CommandExt;
    let started = std::time::Instant::now();
    let child = std::process::Command::new("sh")
        .args(["-c", "sleep 120 & echo started"])
        .process_group(0)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run sh");
    let (output, _) = wait_draining(child, std::time::Duration::from_secs(5));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "waited {:?} for a grandchild's pipe",
        started.elapsed()
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "started\n");
}

fn run(name: &str, page: &str, test: &str) -> String {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(page, &options);
    let dir = std::env::temp_dir().join(format!("eldur-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let qml = dir.join(format!("tst_{name}.qml"));
    std::fs::write(&qml, test).expect("write test");
    let child = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(&qml)
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run qmltestrunner");
    // A layout that never settles keeps the runner busy forever; fail
    // instead of hanging the suite.
    let (output, finished) = wait_draining(child, std::time::Duration::from_secs(60));
    if !finished {
        let _ = std::fs::remove_dir_all(&dir);
        panic!(
            "{name}: qmltestrunner did not finish in 60 s; the layout never settles\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let text = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        out.main_qml().unwrap_or_default()
    );
    assert!(output.status.success(), "{text}");
    let problems = support::problem_lines(&text, |l| {
        l.contains("TypeError") || l.contains("of null") || l.contains("Binding loop")
    });
    assert!(problems.is_empty(), "{problems:#?}\n{text}");
    text
}

#[test]
fn stretched_row_items_are_definite_when_the_row_has_a_height() {
    if support::no_qt() {
        return;
    }
    run("flex_definite", FLEX_PAGE, FLEX_TEST);
    run("flex_min_row", AUTO_ROW_PAGE, AUTO_ROW_TEST);
    run("flex_decorated", DECORATED_PAGE, DECORATED_TEST);
}

#[test]
fn grid_items_keep_margins_used_heights_and_column_minimums() {
    if support::no_qt() {
        return;
    }
    run("grid_pins", GRID_PAGE, GRID_TEST);
}

#[test]
fn a_wrapping_inline_flex_takes_its_own_line() {
    if support::no_qt() {
        return;
    }
    run("inline_flex_wrap", INLINE_PAGE, INLINE_TEST);
}
