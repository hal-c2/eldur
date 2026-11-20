//! An element's bindings to its parent read nothing while it has none: the
//! runtime moves an element by clearing its parent first, a created one has
//! none until it is appended, and `remove()` leaves it without one. Each
//! read of `parent.width` would throw `Cannot read property 'width' of null`.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

/// Wrapped flex items (`Math.min(150, parent.width)`) and a percentage width
/// (`parent.width * 0.5`), each created, appended and removed by the script;
/// `x4` is then moved to another box and back.
const PAGE: &str = r#"<!doctype html><html><head><style>
body { margin: 0 }
.w { display: flex; flex-wrap: wrap; gap: 10px; width: 400px }
.v { display: flex; flex-wrap: wrap; row-gap: 5px; column-gap: 20px; width: 400px }
.r { display: flex; width: 400px }
.i { width: 150px; height: 20px; background: #c00 }
.j { height: 20px; background: #0c0 }
.p { width: 50%; height: 20px; background: #00c }
</style></head><body>
<div class=w id=w><div class=i id=a></div><div class=i id=b></div></div>
<div class=v id=v><div class=i id=c></div><div class=j id=d>text</div></div>
<div class=r id=r><div class=p id=e></div></div>
<script>
var w = document.getElementById('w'), v = document.getElementById('v'), r = document.getElementById('r');
function add(box, cls, id) { var n = document.createElement('div'); n.className = cls; n.id = id; n.textContent = cls == 'j' ? 'new' : ''; box.appendChild(n); return n; }
add(w, 'i', 'x1'); add(v, 'i', 'x2'); add(v, 'j', 'x3'); var x4 = add(r, 'p', 'x4');
document.getElementById('b').remove(); document.getElementById('d').remove(); document.getElementById('e').remove();
w.appendChild(x4); r.appendChild(x4);
var named = [document.getElementById('a'), document.getElementById('c'), x4];
</script></body></html>"#;

/// Chromium, `getBoundingClientRect()` at 1280×800: `[x, y, width, height]`.
const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "NullParent"
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

    function test_the_page_lays_out_without_warnings() {
        check("a", 0, 0, 150, 20)
        check("c", 0, 20, 150, 20)
        check("x4", 0, 40, 200, 20)
    }
}
}
"##;

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
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(&qml)
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
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
fn moved_created_and_removed_elements_read_no_null_parent() {
    if support::no_qt() {
        return;
    }
    run("null_parent", PAGE, TEST);
}
