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

const WS: &str = "text: inter-element whitespace between blocks is not kept, so textContent runs them together (dom-api.md §12)";
const UA: &str = "static controls lack the UA 2px border and 13.33px Arial; adding them changes script-less pages";
const UAFOLLOW: &str = "follows from static controls being smaller than Chromium's";
const UAPAINT: &str = "computed paint of a static control: its UA background and border colours are not in the cascade";
const INLINE: &str =
    "a checkbox beside inline text is block-level: no inline replaced elements (boxes/mod.rs)";
const FONT: &str = "Helvetica: Qt's fontconfig match has line height 23 where Chromium's has 18";
const FONTFOLLOW: &str = "follows from the Helvetica line height and width difference";
const TABLE: &str = "created rows do not take part in the table's auto column sizing";
const CLASS: &str = "classList.remove on an element with no class attribute leaves no attribute, Chromium an empty one";
const HOVER: &str = "li:hover under a list that starts display:none gets no hover tracking";
const FOCUS: &str = "hiding the form leaves focus on its hidden button";

/// `(page, what differs, why)`. `what` starts a difference's name, as printed by a
/// failing test, `<step> <selector>[<row>] <field>:`; `*` stands for any text.
const KNOWN: &[(&str, &str, &str)] = &[
    ("tabs", "* .panel[*] text:", WS),
    ("theme", "* #compact[*] x:", UA),
    ("theme", "* .card[*] text:", WS),
    ("theme", "* .top[*] text:", WS),
    ("theme", "* body[*] text:", WS),
    ("cards", "* #cart[*] height:", FONT),
    ("cards", "* .buy[*] y:", FONTFOLLOW),
    ("cards", "* .card[*] y:", FONTFOLLOW),
    ("cards", "* .card[*] height:", FONTFOLLOW),
    ("cards", "* .card[*] text:", WS),
    ("cards", "* h3[*] y:", FONTFOLLOW),
    ("cards", "* h3[*] height:", FONT),
    ("cards", "* .desc[*] y:", FONTFOLLOW),
    ("cards", "* .desc[*] height:", FONTFOLLOW),
    ("cards", "* .price[*] y:", FONTFOLLOW),
    ("cards", "* .price[*] height:", FONT),
    ("cards", "* .price[*] width:", FONT),
    ("cards", "* #cart[*] x:", FONT),
    ("cards", "* #cart[*] width:", FONT),
    ("accordion", "* details[*] text:", WS),
    ("table", "* #filter[*] width:", UA),
    ("table", "* #filter[*] height:", UA),
    ("table", "* #people[*] y:", UAFOLLOW),
    ("table", "* #people[*] height:", UAFOLLOW),
    ("table", "* #people[*] text:", WS),
    ("table", "* #summary[*] y:", UAFOLLOW),
    ("table", "* td[*] y:", UAFOLLOW),
    ("table", "* td[*] width:", TABLE),
    ("table", "* td[*] x:", TABLE),
    ("table", "* tr[*] y:", UAFOLLOW),
    ("table", "* tr[*] width:", TABLE),
    ("table", "* th[*] y:", UAFOLLOW),
    ("table", "* th[*] width:", TABLE),
    ("table", "* th[*] x:", TABLE),
    ("table", "* th[*] class:", CLASS),
    ("dropdown", "* #list[*] text:", WS),
    ("dropdown", "* #m-settings[*] background-color:", HOVER),
    ("dropdown", "* #m-settings[*] color:", HOVER),
    ("modal", "* #overlay[*] text:", WS),
    ("modal", "* .dialog[*] text:", WS),
    ("form", "* #email[*] border-top-color:", UAPAINT),
    ("form", "* #go[*] y:", UAFOLLOW),
    ("form", "* #go[*] width:", UA),
    ("form", "* #go[*] height:", UA),
    ("form", "* #name[*] background-color:", UAPAINT),
    ("form", "* #name[*] border-top-color:", UAPAINT),
    ("form", "* #signup[*] height:", INLINE),
    ("form", "* #signup[*] text:", WS),
    ("form", "* #terms[*] y:", INLINE),
    ("form", "* .field[*] text:", WS),
    ("form", "* .field[*] height:", INLINE),
    ("form", "* .error[*] y:", UAFOLLOW),
    ("form", "* (focus):", FOCUS),
    ("form", "* #done[*] y:", UAFOLLOW),
    ("todo", "* .app[*] text:", WS),
    ("todo", "* li.item[*] text:", WS),
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
        const text = JSON.stringify({ name: name, state: eldurSnapshot(Eldur.document, Eldur.getComputedStyle, spec().watch) })
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
    function press(name) {
        const keys = { Enter: Qt.Key_Return, Escape: Qt.Key_Escape, Tab: Qt.Key_Tab, End: Qt.Key_End,
            ArrowDown: Qt.Key_Down, ArrowUp: Qt.Key_Up, Backspace: Qt.Key_Backspace }
        keyClick(keys[name])
    }
    function act(s) {
        if (s.click) { const p = center(s.click); mouseMove(page, p.x, p.y); mouseClick(page, p.x, p.y) }
        if (s.clickAt) { mouseMove(page, s.clickAt[0], s.clickAt[1]); mouseClick(page, s.clickAt[0], s.clickAt[1]) }
        if (s.hover) { const p = center(s.hover); mouseMove(page, p.x, p.y) }
        if (s.type) {
            const p = center(s.type[0])
            mouseMove(page, p.x, p.y)
            mouseClick(page, p.x, p.y)
            keyClick(Qt.Key_End)
            for (const c of s.type[1]) keyClick(c)
        }
        if (s.key) press(s.key)
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
                    if !hidden && !((a - b).abs() <= GEOMETRY_TOLERANCE) {
                        out.push(format!("{at} {axis}: got {a} Chromium {b}"));
                    }
                }
                for field in ["shown", "text", "value", "checked"] {
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
fn tabs() {
    page("tabs");
}

#[test]
fn accordion() {
    page("accordion");
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
