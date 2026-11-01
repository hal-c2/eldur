//! Pages an ordinary web developer would write, driven with real input and
//! compared with Chromium after every step.
//!
//! Each `tests/apps/<page>.html` has a `<page>.json`: `steps` (a click on a
//! selector's centre or at a point, hover, typed text, a key) and what to `watch`.
//! After load and each step, `tests/apps/snapshot.js` reads the same things from
//! either DOM. `tests/apps/chromium.json` is recorded by
//! `node crates/eldur_convert/tests/apps/gen.mjs`, never edited by hand.
//!
//! Compared: console lines, how many elements match, geometry within
//! [`GEOMETRY_TOLERANCE`], whether shown, text, value, checked, computed
//! properties, attributes and the focused element. A QML warning or an uncaught
//! error fails the page.
//!
//! [`KNOWN`] lists what does not match yet, with why. A difference not on it fails
//! the page, and so does an entry that no longer differs, so the list stays true.

use std::path::Path;
use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};
use serde_json::Value;

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

/// Pixels a box may be off: glyph widths and line metrics differ a little
/// between the two text engines.
const GEOMETRY_TOLERANCE: f64 = 2.0;

const COMPACT: &str = "a right-aligned row: its buttons are whole pixels wider (see BUTTONWIDTH), which would start this one 2px left; not confirmed";
const BUTTONWIDTH: &str = "a button's width is whole pixels (`Math.ceil` in emit/controls.rs): \
                           five on a line add up to 2.2px against Chromium's fractions";
const SPANSCAN: &str = "a wrapped span's box comes from scanning its Text in half-font steps, so its height lands on a step: 56 for Chromium's 53";
const CONTROLROW: &str = "with the form between them hidden, the heading's bottom margin of 21.44 looks lost, leaving only #done's 16; not confirmed";
const FONT: &str = "Helvetica: Qt's fontconfig match has line height 23 where Chromium's has 18";
const FONTFOLLOW: &str = "follows from the Helvetica line height and width difference";
const HANGING: &str = "a span wrapped across lines measures the space hanging at its first line's end; \
                       the scan over the Text's links cannot tell that space from the word before it";
const NEVER_SCRIPT: &str =
    "a script in the body makes no item, so its source is not in the body's textContent";

/// `(page, what differs, why)`. `what` starts a difference's name, as printed by a
/// failing test, `<step> <selector>[<row>] <field>:`; `*` stands for any text.
const KNOWN: &[(&str, &str, &str)] = &[
    ("theme", "* #compact[*] x:", COMPACT),
    ("cards", "* #cart[*] height:", FONT),
    ("cards", "* .buy[*] y:", FONTFOLLOW),
    ("cards", "* .card[*] y:", FONTFOLLOW),
    ("cards", "* .card[*] height:", FONTFOLLOW),
    ("cards", "* h3[*] y:", FONTFOLLOW),
    ("cards", "* h3[*] height:", FONT),
    ("cards", "* .desc[*] y:", FONTFOLLOW),
    ("cards", "* .desc[*] height:", FONTFOLLOW),
    ("cards", "* .price[*] y:", FONTFOLLOW),
    ("cards", "* .price[*] height:", FONT),
    ("cards", "* #cart[*] x:", FONT),
    ("cards", "* #cart[*] width:", FONT),
    ("theme", "* body[*] text:", NEVER_SCRIPT),
    ("form", "* #done[*] y:", CONTROLROW),
    ("hidden_lines", "* #s4[*] width:", HANGING),
    ("lines_closed", "* #sp[*] height:", SPANSCAN),
    ("lines_open", "* #sp[*] height:", SPANSCAN),
    ("lines_created", "* #ws[*] width:", SPANSCAN),
    ("lines_created", "* #ws[*] height:", SPANSCAN),
    ("lines_created", "* #ms[*] width:", SPANSCAN),
    ("lines_created", "* #ms[*] height:", SPANSCAN),
    ("created_lines", "* #reset[*] x:", BUTTONWIDTH),
];

const TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "App"
    when: windowShown

    property var page

    SNAPSHOT

    // A test log line has a length limit.
    function report(name) {
        const image = spec().watch.some(w => w.paint) ? grabImage(page) : null
        const pixel = (x, y) => {
            const c = image.pixel(x, y)
            return "rgb(" + Math.round(c.r * 255) + ", " + Math.round(c.g * 255) + ", " + Math.round(c.b * 255) + ")"
        }
        const text = JSON.stringify({ name: name, state: eldurSnapshot(Eldur.document, Eldur.getComputedStyle, spec().watch, pixel) })
        for (let at = 0; at < text.length; at += 700)
            console.log("APPCHUNK " + text.slice(at, at + 700))
        console.log("APPSTATE")
    }
    function spec() { return SPEC }
    function center(selector) {
        const e = Eldur.document.querySelector(selector)
        verify(e !== null, "nothing matches " + selector)
        const r = e.getBoundingClientRect()
        return { x: r.left + r.width / 2, y: r.top + r.height / 2 }
    }
    // What is held, for the events the test sends meanwhile: a test's mouse has
    // no state of its own.
    property int mods: 0
    property int held: 0
    property point at: Qt.point(1200, 790)
    readonly property var buttonOf: ({ left: Qt.LeftButton, right: Qt.RightButton, middle: Qt.MiddleButton })
    readonly property var modOf: ({ Shift: Qt.ShiftModifier, Control: Qt.ControlModifier, Alt: Qt.AltModifier })
    readonly property var keyOf: ({ Enter: Qt.Key_Return, Escape: Qt.Key_Escape, Tab: Qt.Key_Tab, End: Qt.Key_End,
        ArrowDown: Qt.Key_Down, ArrowUp: Qt.Key_Up, ArrowLeft: Qt.Key_Left, ArrowRight: Qt.Key_Right,
        Backspace: Qt.Key_Backspace, Delete: Qt.Key_Delete, Home: Qt.Key_Home, PageDown: Qt.Key_PageDown,
        PageUp: Qt.Key_PageUp, Space: Qt.Key_Space, Shift: Qt.Key_Shift, Control: Qt.Key_Control, Alt: Qt.Key_Alt,
        KeyA: Qt.Key_A, KeyB: Qt.Key_B, KeyS: Qt.Key_S, Digit1: Qt.Key_1 })
    function press(name) {
        const parts = name.split("+")
        let mod = mods
        for (const m of parts.slice(0, -1)) mod |= modOf[m]
        const last = parts[parts.length - 1]
        // Shift+Tab is what Qt calls Backtab.
        keyClick(last === "Tab" && (mod & Qt.ShiftModifier) ? Qt.Key_Backtab : keyOf[last], mod)
    }
    function moveTo(x, y) {
        at = Qt.point(x, y)
        mouseMove(page, x, y, -1, held, mods)
    }
    function act(s) {
        if (s.click) { const p = center(s.click); moveTo(p.x, p.y); mouseClick(page, p.x, p.y, Qt.LeftButton, mods) }
        if (s.clickAt) {
            moveTo(s.clickAt[0], s.clickAt[1])
            mouseClick(page, s.clickAt[0], s.clickAt[1], buttonOf[s.button || "left"], mods)
        }
        if (s.dblAt) {
            moveTo(s.dblAt[0], s.dblAt[1])
            mouseDoubleClickSequence(page, s.dblAt[0], s.dblAt[1], Qt.LeftButton, mods)
        }
        if (s.move) moveTo(s.move[0], s.move[1])
        if (s.down) { held |= buttonOf[s.down]; mousePress(page, at.x, at.y, buttonOf[s.down], mods) }
        if (s.up) { held &= ~buttonOf[s.up]; mouseRelease(page, at.x, at.y, buttonOf[s.up], mods) }
        if (s.hold) { mods |= modOf[s.hold]; keyPress(keyOf[s.hold], mods) }
        if (s.unhold) { keyRelease(keyOf[s.unhold], mods); mods &= ~modOf[s.unhold] }
        if (s.hover) { const p = center(s.hover); mouseMove(page, p.x, p.y) }
        if (s.type) {
            const p = center(s.type[0])
            mouseMove(page, p.x, p.y)
            mouseClick(page, p.x, p.y)
            keyClick(Qt.Key_End)
            for (const c of s.type[1]) keyClick(c)
        }
        if (s.key) press(s.key)
        if (s.keys) { for (const c of s.keys) keyClick(c) }
        if (s.wheel) {
            const p = s.wheel[0] === "page" ? { x: 1200, y: 400 } : center(s.wheel[0])
            mouseMove(page, p.x, p.y)
            // A notch is 120 and moves a scroller 60px.
            mouseWheel(page, p.x, p.y, -2 * s.wheel[1], -2 * s.wheel[2])
            wait(400)
        }
        if (s.drag) {
            const p = center(s.drag[0])
            mouseDrag(page, p.x, p.y, s.drag[1], s.drag[2])
            wait(400)
        }
        if (s.settle) wait(Math.max(100, s.settle))
        if (s.viewport) { page.width = s.viewport[0]; page.height = s.viewport[1] }
    }

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        mouseMove(page, 1200, 790)
        waitForRendering(page)
        wait(50)
        report("load")
        for (const s of spec().steps) {
            act(s)
            wait(100)
            report(s.name)
        }
        SHOT
    }
}
}
"##;

fn run(name: &str, html: &str, spec: &str, snapshot: &str) -> String {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    // The test reads computed styles from outside the script analysis looks at:
    // this says so to the converter, which otherwise keeps no cascade to read.
    let html = html.replace(
        "</body>",
        "<script>getComputedStyle(document.body);</script></body>",
    );
    let out = convert_str(&html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-app-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    // `ELDUR_APPS_SHOTS=DIR` keeps each page's last state, to score it.
    let shot = match std::env::var("ELDUR_APPS_SHOTS") {
        Ok(to) => {
            format!("const image = grabImage(page); image.save(\"{to}/{name}.png\"); wait(10)")
        }
        Err(_) => String::new(),
    };
    let test = TEST
        .replace("SNAPSHOT", snapshot)
        .replace("SPEC", spec)
        .replace("SHOT", &shot);
    std::fs::write(dir.join("tst_app.qml"), test).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-maxwarnings", "0", "-input"])
        .arg(dir.join("tst_app.qml"))
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

/// The steps as `{name, log, state}`, from what the test printed, and every line
/// that says something went wrong.
fn parse(text: &str) -> (Vec<Value>, Vec<String>) {
    let mut steps = Vec::new();
    let mut log: Vec<Value> = Vec::new();
    let mut chunk = String::new();
    let mut trouble = Vec::new();
    for line in text.lines() {
        if line.contains("FAIL!")
            || line.contains("Warning:")
            || line.contains("QWARN")
            || line.contains("Error:")
            || line.contains("eldur: ")
        {
            trouble.push(line.trim().to_owned());
        }
        let Some(at) = line.find("qml: ") else {
            continue;
        };
        let said = &line[at + 5..];
        if let Some(part) = said.strip_prefix("APPCHUNK ") {
            chunk.push_str(part);
        } else if said == "APPSTATE" {
            let mut step: Value = serde_json::from_str(&chunk).expect("a state");
            step["log"] = Value::Array(std::mem::take(&mut log));
            steps.push(step);
            chunk.clear();
        } else if !said.starts_with("eldur: ") {
            log.push(Value::String(said.to_owned()));
        }
    }
    (steps, trouble)
}

fn differences(steps: &[Value], want: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    if steps.len() != want.len() {
        out.push(format!("steps: got {} of {}", steps.len(), want.len()));
    }
    for (got, want) in steps.iter().zip(want) {
        let step = want["name"].as_str().expect("a name");
        if got["log"] != want["log"] {
            out.push(format!(
                "{step} log: got {} Chromium {}",
                got["log"], want["log"]
            ));
        }
        if got["state"]["(focus)"] != want["state"]["(focus)"] {
            out.push(format!(
                "{step} (focus): got {} Chromium {}",
                got["state"]["(focus)"], want["state"]["(focus)"]
            ));
        }
        for (selector, rows) in want["state"].as_object().expect("a state") {
            let Some(rows) = rows.as_array() else {
                continue;
            };
            let empty = Vec::new();
            let have = got["state"][selector].as_array().unwrap_or(&empty);
            if have.len() != rows.len() {
                out.push(format!(
                    "{step} {selector} count: got {} Chromium {}",
                    have.len(),
                    rows.len()
                ));
                continue;
            }
            for (i, (g, w)) in have.iter().zip(rows).enumerate() {
                let at = format!("{step} {selector}[{i}]");
                for (field, axis) in ["x", "y", "width", "height"].iter().enumerate() {
                    let (a, b) = (g["box"][field].as_f64(), w["box"][field].as_f64());
                    let (a, b) = (a.unwrap_or(f64::NAN), b.unwrap_or(f64::NAN));
                    // A box nobody sees has no place to compare.
                    let hidden = w["shown"] == false && g["shown"] == false;
                    // A value only one side has is NaN, and is never near.
                    let near = (a - b).abs() <= GEOMETRY_TOLERANCE;
                    if !hidden && !near {
                        out.push(format!("{at} {axis}: got {a} Chromium {b}"));
                    }
                }
                for field in ["shown", "text", "value", "checked", "paint"] {
                    if g[field] != w[field] {
                        out.push(format!(
                            "{at} {field}: got {} Chromium {}",
                            g[field], w[field]
                        ));
                    }
                }
                for group in ["props", "attrs"] {
                    if let Some(all) = w[group].as_object() {
                        for (key, value) in all {
                            if &g[group][key] != value {
                                out.push(format!(
                                    "{at} {key}: got {} Chromium {value}",
                                    g[group][key]
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// A known difference's pattern: `*` stands for any text, the rest in order, and
/// the difference starts with it.
fn matches_known(difference: &str, pattern: &str) -> bool {
    let mut rest = difference;
    for (i, part) in pattern.split('*').enumerate() {
        match rest.find(part) {
            Some(at) if i > 0 || at == 0 => rest = &rest[at + part.len()..],
            _ => return false,
        }
    }
    true
}

fn page(name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/apps");
    let read = |file: String| std::fs::read_to_string(dir.join(file)).expect("a test file");
    let chromium: Value = serde_json::from_str(&read("chromium.json".to_owned())).expect("json");
    let text = run(
        name,
        &read(format!("{name}.html")),
        &read(format!("{name}.json")),
        &read("snapshot.js".to_owned()),
    );
    let (steps, trouble) = parse(&text);
    let mut all = differences(&steps, chromium[name].as_array().expect("recorded"));
    all.extend(trouble);
    // Chromium's made-up move at load and the test's first real one are not a
    // user's: the test starts from what each thinks is under the mouse.
    let spec: Value = serde_json::from_str(&read(format!("{name}.json"))).expect("json");
    if spec["quietLoad"] == true {
        all.retain(|d| !d.starts_with("load log"));
    }
    // Read mid-flight on two real clocks, its load state has nothing to agree on.
    if name == "motion_running" {
        all.retain(|d| !d.starts_with("load "));
    }
    // `ELDUR_APPS_ALL=1` prints every difference, the known ones too.
    if std::env::var_os("ELDUR_APPS_ALL").is_some() {
        eprintln!("{name}:\n  {}", all.join("\n  "));
    }
    let known: Vec<_> = KNOWN.iter().filter(|k| k.0 == name).collect();
    let new: Vec<_> = all
        .iter()
        .filter(|d| !known.iter().any(|k| matches_known(d, k.1)))
        .collect();
    let gone: Vec<_> = known
        .iter()
        .filter(|k| !all.iter().any(|d| matches_known(d, k.1)))
        .collect();
    assert!(
        new.is_empty() && gone.is_empty(),
        "{name}: {} new differences:\n  {}\nknown differences that no longer differ:\n  {}",
        new.len(),
        new.iter()
            .map(|d| d.as_str())
            .collect::<Vec<_>>()
            .join("\n  "),
        gone.iter().map(|k| k.1).collect::<Vec<_>>().join("\n  "),
    );
}

#[test]
fn controls() {
    page("controls");
}

#[test]
fn created_tables() {
    page("created_tables");
}

#[test]
fn created_controls() {
    page("created_controls");
}

#[test]
fn tabs() {
    page("tabs");
}

#[test]
fn accordion() {
    page("accordion");
}

#[test]
fn roster() {
    page("roster");
}

#[test]
fn table_dom() {
    page("table_dom");
}

#[test]
fn focus_hide() {
    page("focus_hide");
}

#[test]
fn hidden_focus() {
    page("hidden_focus");
}

#[test]
fn text_nodes() {
    page("text_nodes");
}

#[test]
fn modal() {
    page("modal");
}

#[test]
fn todo() {
    page("todo");
}

#[test]
fn form() {
    page("form");
}

#[test]
fn table() {
    page("table");
}

#[test]
fn dropdown() {
    page("dropdown");
}

#[test]
fn cards() {
    page("cards");
}

#[test]
fn theme() {
    page("theme");
}

#[test]
fn generated() {
    page("generated");
}

#[test]
fn created_lines() {
    page("created_lines");
}

#[test]
fn lines_justify() {
    page("lines_justify");
}

#[test]
fn lines_wrap() {
    page("lines_wrap");
}

#[test]
fn inline_lines() {
    page("inline_lines");
}

#[test]
fn hidden_lookup() {
    page("hidden_lookup");
}

#[test]
fn hidden_open() {
    page("hidden_open");
}

#[test]
fn hidden_closed() {
    page("hidden_closed");
}

#[test]
fn hidden_created() {
    page("hidden_created");
}

#[test]
fn hidden_lines() {
    page("hidden_lines");
}

#[test]
fn lines_closed() {
    page("lines_closed");
}

#[test]
fn lines_open() {
    page("lines_open");
}

#[test]
fn lines_created() {
    page("lines_created");
}

#[test]
fn scroll_box() {
    page("scroll_box");
}

#[test]
fn scroll_page() {
    page("scroll_page");
}

#[test]
fn motion_keyframes() {
    page("motion_keyframes");
}

#[test]
fn motion_transitions() {
    page("motion_transitions");
}

#[test]
fn scroll_created() {
    page("scroll_created");
}

#[test]
fn timers_order() {
    page("timers_order");
}

#[test]
fn scroll_keys() {
    page("scroll_keys");
}

#[test]
fn created_empty_body() {
    page("created_empty_body");
}

#[test]
fn input_pointer() {
    page("input_pointer");
}

#[test]
fn motion_closed() {
    page("motion_closed");
}

#[test]
fn scroll_smooth() {
    page("scroll_smooth");
}

#[test]
fn motion_static() {
    page("motion_static");
}

#[test]
fn motion_running() {
    page("motion_running");
}

#[test]
fn scroll_more() {
    page("scroll_more");
}

/// A mouse drag scrolls here, as a touch does, and in Chromium it selects text; so
/// there is no recording to compare with, only that it moved and said so.
#[test]
fn drags_move_scrollers() {
    let html = r#"<!DOCTYPE html><html><head><style>
.port { height: 200px; width: 400px; overflow: auto; }
.tall { height: 900px; background: #cde; }
</style></head><body>
<div class="port" id="still"><div class="tall"></div></div>
<script>
const port = document.createElement("div");
port.className = "port";
port.id = "made";
const tall = document.createElement("div");
tall.className = "tall";
port.appendChild(tall);
document.body.appendChild(port);
const still = document.getElementById("still");
for (const e of [still, port])
  e.addEventListener("scroll", () => console.log("scrolled", e.id, e.scrollTop));
</script></body></html>"#;
    let spec = r##"{"steps":[{"name":"still","drag":["#still",0,-150]},{"name":"made","drag":["#made",0,-150]}],"watch":[]}"##;
    let snapshot = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/apps/snapshot.js"),
    )
    .expect("snapshot.js");
    let (steps, trouble) = parse(&run("drag", html, spec, &snapshot));
    assert!(trouble.is_empty(), "{trouble:?}");
    for (step, id) in steps[1..].iter().zip(["still", "made"]) {
        let last = step["log"]
            .as_array()
            .and_then(|l| l.last())
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("no scroll event from {id}: {step}"));
        let (who, top) = last["scrolled ".len()..]
            .split_once(' ')
            .expect("two words");
        let top: f64 = top.parse().expect("a number");
        assert_eq!(who, id);
        // An instant drag lets go at speed, and flicks on towards the end, at 700.
        assert!((100.0..=700.0).contains(&top), "{id} at {top}");
    }
}
