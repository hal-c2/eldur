//! What a script creates paints, sits and reads as in Chromium, on pages that
//! name its classes literally (closed) and on pages that compute them (open).
//!
//! Each page in `tests/created_paint/` is converted and run through
//! qmltestrunner on the software renderer. Its `CS ` log lines are compared with
//! what Chromium logged, and real pixels with a Chromium screenshot's, at load
//! and after each hover, press or click; `tests/created_paint/chromium.json` is
//! recorded by `node crates/eldur_convert/tests/created_paint/gen.mjs`. Logged
//! values alone once hid a box painted black. A QML warning or an uncaught
//! script error fails the page.

use std::path::Path;
use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "CreatedPaint"
    when: windowShown

    // Antialiasing and blur curves differ a little between the two painters.
    readonly property int tolerance: 16

    function compare_(image, points, when) {
        for (const p of points) {
            const got = image.pixel(p.x, p.y)
            const near = Math.abs(got.r * 255 - p.rgb[0]) <= tolerance
                && Math.abs(got.g * 255 - p.rgb[1]) <= tolerance
                && Math.abs(got.b * 255 - p.rgb[2]) <= tolerance
            if (!near) {
                console.log("MISMATCH " + when + " " + p.id + "@" + p.at + ": got "
                    + Math.round(got.r * 255) + "," + Math.round(got.g * 255) + ","
                    + Math.round(got.b * 255) + " Chromium " + p.rgb)
            }
        }
    }

    function test_page() {
        const data = DATA
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        mouseMove(page, 1200, 780)
        waitForRendering(page)
        wait(50)
        compare_(grabImage(page), data.pixels, "load")
        for (const s of data.steps) {
            if (s.verb === "hover" || s.verb === "away") mouseMove(page, s.x, s.y)
            if (s.verb === "press") { mouseMove(page, s.x, s.y); mousePress(page, s.x, s.y) }
            if (s.verb === "release") mouseRelease(page, s.x, s.y)
            if (s.verb === "click") { mouseMove(page, s.x, s.y); mouseClick(page, s.x, s.y) }
            wait(250)
            compare_(grabImage(page), s.pixels, s.name)
        }
    }
}
}
"##;

/// What the page printed, for a run that holds no surprises.
fn run(name: &str, html: &str, data: &serde_json::Value) -> String {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-cpaint-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_created.qml"),
        TEST.replace("DATA", &data.to_string()),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_created.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("QT_QUICK_BACKEND", "software")
        .output()
        .expect("run qmltestrunner");
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

fn page(name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/created_paint");
    let chromium: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("chromium.json")).expect("json"))
            .expect("chromium.json");
    let html = std::fs::read_to_string(dir.join(format!("{name}.html"))).expect("page");
    let want: Vec<&str> = chromium[name]["log"]
        .as_array()
        .expect("a log for every page")
        .iter()
        .map(|l| l.as_str().expect("a line"))
        .collect();
    let text = run(name, &html, &chromium[name]);
    let mut problems: Vec<String> = text
        .lines()
        .filter(|l| {
            l.contains("FAIL!")
                || l.contains("MISMATCH")
                || l.contains("Warning:")
                || l.contains("QWARN")
                || l.contains("Error:")
        })
        .map(str::to_owned)
        .collect();
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("CS ").map(|at| &l[at..]))
        .collect();
    for i in 0..logged.len().max(want.len()) {
        let (got, chromium) = (logged.get(i), want.get(i));
        if got != chromium {
            problems.push(format!(
                "line {i}\n    got      {}\n    Chromium {}",
                got.unwrap_or(&"(nothing)"),
                chromium.unwrap_or(&"(nothing)"),
            ));
        }
    }
    assert!(problems.is_empty(), "{name}:\n  {}", problems.join("\n  "));
}

#[test]
fn margins_lit() {
    page("margins_lit");
}

#[test]
fn margins_collapse() {
    page("margins_collapse");
}

#[test]
fn margins_closed() {
    page("margins_closed");
}

#[test]
fn paint_open() {
    page("paint_open");
}

#[test]
fn states_open() {
    page("states_open");
}

#[test]
fn computed_closed() {
    page("computed_closed");
}

#[test]
fn computed_open() {
    page("computed_open");
}

#[test]
fn computed_static() {
    page("computed_static");
}

/// What cannot be painted is said, naming the element and the property.
#[test]
fn unpaintable_warns() {
    let html = r#"<!doctype html><html><head><style>
.f{height:20px;filter:url(#none)}
.a{height:20px;animation:spin 1s}
@keyframes spin{to{opacity:0}}
.t{height:20px;transition:width .1s;width:10px}
</style></head><body><div id="l"></div><script>
var n=['f','a','t'];n.forEach(function(c){var e=document.createElement('div');e.className=c;e.id='w'+c;document.getElementById('l').appendChild(e)});
document.getElementById('wf').style.transform='translateX(1px) perspective(9px)';
</script></body></html>"#;
    let empty = serde_json::json!({"pixels": [], "steps": []});
    let text = run("warns", html, &empty);
    // These run now, on a created box as on any other.
    for (element, property) in [("#wa", "animation"), ("#wt", "transition")] {
        assert!(
            !text
                .lines()
                .any(|l| l.contains(element) && l.contains(property) && l.contains("eldur: ")),
            "{element} still warns about {property}:\n{text}"
        );
    }
    for (element, property) in [("#wf", "filter"), ("#wf", "transform")] {
        assert!(
            text.lines()
                .any(|l| l.contains(element) && l.contains(property) && l.contains("eldur: ")),
            "nothing says {element} loses {property}:\n{}",
            text.lines()
                .filter(|l| l.contains("eldur: ") || l.contains("arning"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}

/// Margins under two pixels, fractional ones and a fractional height in a row.
#[test]
fn margins_hair() {
    page("margins_hair");
}

/// A box margins collapse through, inside a column and at its end.
#[test]
fn margins_empty() {
    page("margins_empty");
}

/// Text a stylesheet colours, in markup a script builds: logged values alone
/// said white and red over text painted black.
#[test]
fn paint_text() {
    page("paint_text");
}

/// A created control's corners and border, read from the pixels.
#[test]
fn paint_control() {
    page("paint_control");
}
