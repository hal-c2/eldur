//! A paragraph whose inline runs differ in font, line height or alignment, on the
//! running page against Chromium (layout.md §1.3): where each paragraph is and how
//! tall on a page with no script, and the boxes `getBoundingClientRect` and
//! `getClientRects` give on one with a script, before and after it changes a run.
//!
//! The expected values are in `tests/inline_runs/chromium.json`, recorded by
//! `node crates/eldur_convert/tests/inline_runs/gen.mjs`. They hold for the fonts
//! of the machine they were recorded on.

use std::collections::BTreeMap;
use std::process::Command;

use eldur_convert::{Options, RootKind, convert_str};
use serde_json::Value;

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const CHROMIUM: &str = include_str!("inline_runs/chromium.json");
const CLOSED: &str = include_str!("inline_runs/closed.html");
const OPEN: &str = include_str!("inline_runs/open.html");
const CHANGES: &str = include_str!("inline_runs/changes.html");

/// Heights and rows are Chromium's to its 1/64 px, less what adding them up in
/// floating point leaves.
const EXACT: f64 = 0.02;
/// Qt draws text at whole-pixel sizes and hints its advances: across a line that
/// many glyphs away from the page's edge, an inline box's left and width drift by
/// this much (layout.md §2).
const ACROSS: f64 = 4.0;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineRuns"
    when: windowShown

    function holder(item, count) {
        const shown = []
        for (let i = 0; i < item.children.length; i++) {
            const kid = item.children[i]
            if (kid.visible && kid.height > 0)
                shown.push(kid)
        }
        if (shown.length === count)
            return shown
        for (let i = 0; i < item.children.length; i++) {
            const found = holder(item.children[i], count)
            if (found)
                return found
        }
        return null
    }

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        wait(300)
        if (@COUNT@ > 0) {
            for (const block of holder(page, @COUNT@)) {
                const at = block.parent.mapToItem(page, block.x, block.y)
                console.log("LC block " + at.y + " " + block.height)
            }
        }
    }
}
}
"##;

/// The `LC ` lines of a page, and what is wrong in the log.
fn run(name: &str, html: &str, count: usize) -> (Vec<String>, Vec<String>) {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-runs-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_runs.qml"),
        TEST.replace("@COUNT@", &count.to_string()),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_runs.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let problems = text
        .lines()
        .filter(|l| {
            l.contains("FAIL!")
                || l.contains("Warning:")
                || l.contains("QWARN")
                || l.contains("Error:")
        })
        .map(str::to_owned)
        .collect();
    let logged = text
        .lines()
        .filter_map(|l| l.find("LC ").map(|at| l[at + 3..].to_owned()))
        .collect();
    (logged, problems)
}

fn rect_problem(got: &Value, want: &Value) -> Option<String> {
    let number = |v: &Value, i: usize| v[i].as_f64().unwrap_or(f64::NAN);
    let off = |i: usize, by: f64| (number(got, i) - number(want, i)).abs() > by;
    (off(1, EXACT) || off(3, EXACT) || off(0, ACROSS) || off(2, ACROSS))
        .then(|| format!("{got} want {want}"))
}

/// What differs between two `<head> <id> {"b":…,"l":…}` lines, if anything.
fn line_problem(got: &str, want: &str) -> Option<String> {
    let split = |l: &str| {
        let at = l.find('{')?;
        Some((
            l[..at].to_owned(),
            serde_json::from_str::<Value>(&l[at..]).ok()?,
        ))
    };
    let (Some((got_head, got_json)), Some((want_head, want_json))) = (split(got), split(want))
    else {
        return (got != want).then(|| format!("{got} want {want}"));
    };
    if got_head != want_head {
        return Some(format!("{got} want {want}"));
    }
    let lines = |v: &Value| v["l"].as_array().cloned().unwrap_or_default();
    let (got_lines, want_lines) = (lines(&got_json), lines(&want_json));
    let mut problems = Vec::new();
    if let Some(p) = rect_problem(&got_json["b"], &want_json["b"]) {
        problems.push(format!("b {p}"));
    }
    if got_lines.len() == want_lines.len() {
        for (i, (g, w)) in got_lines.iter().zip(&want_lines).enumerate() {
            if let Some(p) = rect_problem(g, w) {
                problems.push(format!("l{i} {p}"));
            }
        }
    } else {
        problems.push(format!(
            "{} rects want {}",
            got_lines.len(),
            want_lines.len()
        ));
    }
    (!problems.is_empty()).then(|| format!("{want_head}{}", problems.join("; ")))
}

fn known(list: &[(&str, &str)]) -> BTreeMap<String, String> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// Fails on a difference not in `known`, and on an entry of `known` that passes.
fn report(found: &BTreeMap<String, String>, known: &BTreeMap<String, String>) {
    let mut wrong: Vec<String> = found
        .iter()
        .filter(|(id, _)| !known.contains_key(*id))
        .map(|(id, p)| format!("{id}: {p}"))
        .collect();
    wrong.extend(
        known
            .keys()
            .filter(|id| !found.contains_key(*id))
            .map(|id| format!("{id}: is in KNOWN and passes")),
    );
    if let Ok(path) = std::env::var("ELDUR_RUNS_DUMP") {
        std::fs::write(path, wrong.join("\n")).expect("dump");
    }
    assert!(
        wrong.is_empty(),
        "{} differ from Chromium:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

fn chromium() -> Value {
    serde_json::from_str(CHROMIUM).expect("chromium.json")
}

/// The log lines of `key`, as strings.
fn lines<'a>(chromium: &'a Value, key: &str) -> Vec<&'a str> {
    chromium[key]
        .as_array()
        .expect("recorded")
        .iter()
        .map(|l| l.as_str().expect("a line"))
        .collect()
}

/// Cases Chromium and Eldur still disagree on, by paragraph; each is in the
/// report and in layout.md §2.
const KNOWN_CLOSED: &[(&str, &str)] = &[];
const KNOWN_OPEN: &[(&str, &str)] = &[
    (
        "open p99r",
        "a culled <q> answers with one rect where Chromium has one for each quote and the text; its bounding box agrees",
    ),
    (
        "open p166r",
        "a long 13.33px run is drawn at 13px or 14px by Qt, which has no fractional size: 26 px across 875",
    ),
    (
        "open p167r",
        "a long 13.33px run is drawn at 13px or 14px by Qt, which has no fractional size: 26 px across 875",
    ),
    (
        "open p176r",
        "the zero-width rect Chromium gives a culled span for a <br> inside it is not given; bounding box and lines agree",
    ),
];
const KNOWN_CHANGES: &[(&str, &str)] = &[];

#[test]
fn closed_paragraphs_are_where_chromium_has_them() {
    let chromium = chromium();
    let cases = chromium["cases"].as_array().expect("cases");
    let want: BTreeMap<&str, Value> = lines(&chromium, "open")
        .iter()
        .filter_map(|l| {
            let rest = l.strip_prefix("LC open ")?;
            let (id, json) = rest.split_once(' ')?;
            Some((id, serde_json::from_str(json).ok()?))
        })
        .collect();
    let (logged, problems) = run("closed", CLOSED, cases.len());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert_eq!(logged.len(), cases.len(), "one block for each paragraph");
    let mut found = BTreeMap::new();
    for (case, line) in cases.iter().zip(&logged) {
        let id = case["id"].as_str().expect("an id");
        let mut fields = line
            .split(' ')
            .skip(1)
            .map(|n| n.parse::<f64>().expect("a number"));
        let (y, height) = (
            fields.next().expect("y and height"),
            fields.next().expect("y and height"),
        );
        let b = &want[id]["b"];
        let (wy, wh) = (
            b[1].as_f64().expect("a number"),
            b[3].as_f64().expect("a number"),
        );
        // A fixed line of a fraction is in a whole-height `Text` (layout.md §1.1),
        // and the difference is carried, not added up.
        if (y - wy).abs() > 0.5 || (height - wh).abs() > 1.0 - 1.0 / 64.0 {
            found.insert(
                id.to_owned(),
                format!("y {y} h {height} want y {wy} h {wh}: {}", case["inner"]),
            );
        }
    }
    report(&found, &known(KNOWN_CLOSED));
}

fn compare_log(name: &str, html: &str, key: &str, known_cases: &[(&str, &str)]) {
    let chromium = chromium();
    let want = lines(&chromium, key);
    let (logged, problems) = run(name, html, 0);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let mut found = BTreeMap::new();
    for i in 0..logged.len().max(want.len()) {
        let got = logged.get(i).map_or("(nothing)", String::as_str);
        let wanted = want
            .get(i)
            .map_or("(nothing)", |l| l.strip_prefix("LC ").unwrap_or(l));
        if let Some(p) = line_problem(got, wanted) {
            let id = wanted.split(' ').take(2).collect::<Vec<_>>().join(" ");
            found.insert(id, p);
        }
    }
    report(&found, &known(known_cases));
}

#[test]
fn open_page_gives_chromium_boxes_for_inline_runs() {
    compare_log("open", OPEN, "open", KNOWN_OPEN);
}

#[test]
fn a_script_changing_a_run_gives_chromium_boxes() {
    compare_log("changes", CHANGES, "changes", KNOWN_CHANGES);
}
