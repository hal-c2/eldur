//! `IntersectionObserver`, `ResizeObserver` and `MutationObserver`: what each
//! delivers, in what order, and when, measured against Chromium.
//!
//! Each page in `tests/observers/` is converted and run through qmltestrunner;
//! what its script logged (lines starting `OB `) is compared with what Chromium
//! logged, recorded in `tests/observers/chromium.json` by
//! `node crates/eldur_convert/tests/observers/gen.mjs`. Every page steps through
//! frames and timers and says it has finished by making `#obdone`. A QML warning
//! or an uncaught script error fails the page.

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, convert_str};

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Observers"
    when: windowShown

    function named(item, name) {
        if (item.objectName === name)
            return true
        for (const kid of item.children) {
            if (named(kid, name))
                return true
        }
        return false
    }

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        tryVerify(() => named(page, "obdone"), 30000, "the page's steps ran")
    }
}
}
"##;

/// Runs one page; `Ok` has its log, `Err` what went wrong.
fn run(name: &str, html: &str, want: &[&str]) -> Result<String, String> {
    run_patched(name, html, want, &[])
}

/// `run` with each `(find, replace)` made once in the runtime the page loads.
fn run_patched(
    name: &str,
    html: &str,
    want: &[&str],
    patch: &[(&str, &str)],
) -> Result<String, String> {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(html, &options);
    let dir = std::env::temp_dir().join(format!("eldur-observers-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        let mut contents = file.contents.clone();
        if !patch.is_empty() && file.path.ends_with("eldur_runtime.js") {
            let mut text = String::from_utf8(contents).expect("the runtime is UTF-8");
            for (find, replace) in patch {
                assert_eq!(
                    text.matches(find).count(),
                    1,
                    "{name}: once in the runtime: {find}"
                );
                text = text.replacen(find, replace, 1);
            }
            contents = text.into_bytes();
        }
        std::fs::write(dir.join(&file.path), &contents).expect("write output");
    }
    std::fs::write(dir.join("tst_observers.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_observers.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let mut problems: Vec<String> = support::problem_lines(&text, |l| {
        !l.contains("OB ")
            && (l.contains("FAIL!")
                || l.contains("Warning:")
                || l.contains("QWARN")
                || l.contains("Error:"))
    });
    // Every line matching and then a crash is still a failed run.
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("OB ").map(|at| &l[at..]))
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
    if problems.is_empty() {
        Ok(text)
    } else {
        Err(format!("{name}:\n  {}", problems.join("\n  ")))
    }
}

fn pages() -> (Vec<std::path::PathBuf>, serde_json::Value) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/observers");
    let chromium: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("chromium.json")).expect("json"))
            .expect("chromium.json");
    let mut pages: Vec<_> = std::fs::read_dir(&dir)
        .expect("pages")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect();
    pages.sort();
    assert!(!pages.is_empty());
    (pages, chromium)
}

#[test]
fn observers_match_chromium() {
    if support::no_qt() {
        return;
    }
    let (pages, chromium) = pages();
    let mut failures = Vec::new();
    for page in pages {
        let name = page
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .into_owned();
        let html = std::fs::read_to_string(&page).expect("page");
        let want: Vec<&str> = chromium[name.as_str()]
            .as_array()
            .expect("a log for every page")
            .iter()
            .map(|l| l.as_str().expect("a line"))
            .collect();
        if let Err(e) = run(&name, &html, &want) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A mutation callback whose every run makes a new record never ends in
/// Chromium, so this page has no recording: Eldur stops the loop after a
/// thousand notifications in a row, says so, and the page goes on, with its
/// observer still hearing later changes. A callback that awaits before it
/// writes is the same loop and stops the same way, even while another
/// writer's records share its notifications.
#[test]
fn a_runaway_mutation_loop_stops() {
    if support::no_qt() {
        return;
    }
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/observers/runaway/runaway.html");
    let html = std::fs::read_to_string(page).expect("page");
    let log = run(
        "runaway",
        &html,
        &[
            "OB calls 1000 1000",
            "OB later true",
            "OB async 1000 1000",
            "OB both true 3000",
            "OB done",
        ],
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        log.contains("MutationObserver callbacks made new records for 1000 notifications in a row"),
        "the stop is said:\n{log}"
    );
}

/// 600 appends to body, and to a flex column, walk at most 10 items each on
/// average, by the runtime's own count, so a loaded machine cannot pass a
/// quadratic append or fail a linear one. The average holds the item table's
/// sweeps, each of which walks every record once the records double. The
/// runtime counts the sweeps' walks on their own: without them no append walks
/// more than 10 items, and the last hundred walk no more than the first
/// hundred, so a cost that grows as log n fails too. Chromium has no such
/// count, so this page has no recording.
#[test]
fn appends_walk_a_few_items_each_on_average() {
    if support::no_qt() {
        return;
    }
    let page =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/observers/scaling/append_scaling.html");
    let html = std::fs::read_to_string(page).expect("page");
    if let Err(e) = run("append_scaling", &html, APPEND_SCALING_WANT) {
        panic!("{e}");
    }
}

const APPEND_SCALING_WANT: &[&str] = &[
    "OB body: 600 appends walk at most 10 items each on average",
    "OB body: no append walks more than 10 items besides sweeps",
    "OB body: the last 100 appends walk no more than the first 100",
    "OB flex: 600 appends walk at most 10 items each on average",
    "OB flex: no append walks more than 10 items besides sweeps",
    "OB flex: the last 100 appends walk no more than the first 100",
    "OB the appends swept the item table",
    "OB rows 600 600",
];

/// The runtime with one per-append cost put back, each of which the scaling
/// page must fail. They take minutes (the Map one most), so they run on ask:
/// `ELDUR_REQUIRE_QT=1 cargo test -p eldur_convert --test observers_e2e --
/// --ignored --nocapture`, which prints what each one logged.
#[test]
#[ignore = "minutes long; run with --ignored to see each mutant fail the scaling page"]
fn every_append_cost_mutant_fails_the_scaling_page() {
    if support::no_qt() {
        return;
    }
    const LOG4: &str =
        "    for (var lg = container.children.length; lg > 1; lg >>= 2) { ++_itemsWalked; }\n";
    let wsleave = "    }\n    rawNodes.forEach(_wsLeave);\n    var places = [];";
    let log2 = "    }\n    for (var lg = container.children.length; lg > 1; lg >>= 1) { ++_itemsWalked; }\n    rawNodes.forEach(_wsLeave);\n    var places = [];".to_owned();
    let log4 = format!("    }}\n{LOG4}    rawNodes.forEach(_wsLeave);\n    var places = [];");
    let log4_body = format!(
        "    }}\n{}    rawNodes.forEach(_wsLeave);\n    var places = [];",
        LOG4.replace(
            "container.children.length;",
            "container.eldurPack === undefined ? container.children.length : 0;"
        )
    );
    let mutants: Vec<(&str, Vec<(&str, &str)>)> = vec![
        // Whitespace bookkeeping on every insert, not only where there is some.
        (
            "ws",
            vec![(
                "    if ((ref ? _wsBefore(ref) : _wsEnd(owner)).length === 0) {\n        return;\n    }\n",
                "",
            )],
        ),
        // Every container renumbered, numbered children or not.
        (
            "renumber",
            vec![(
                "if (!_numberedItems || !container ||",
                "if (false && !container ||",
            )],
        ),
        // Every record in the item table's Map, which QV4 walks key by key.
        (
            "map",
            vec![(
                "function _itemAddress(key) {\n",
                "function _itemAddress(key) {\n    return \"\";\n",
            )],
        ),
        // An extra lookup that steps through log n of the children.
        ("log2n", vec![(wsleave, log2.as_str())]),
        ("log4n", vec![(wsleave, log4.as_str())]),
        // Insert-before copies every child to find its place.
        (
            "copy",
            vec![(
                "        var kids = container.children;\n        var idx = -1;\n        for (var i = kids.length - 1; i >= 0; --i) {\n            ++_itemsWalked;\n            if (kids[i] === rawRef) {\n                idx = i;\n                break;\n            }\n        }\n",
                "        var kids = [];\n        _itemsWalked += container.children.length;\n        for (var c = 0; c < container.children.length; ++c) {\n            kids.push(container.children[c]);\n        }\n        var idx = kids.indexOf(rawRef);\n",
            )],
        ),
        // A packed flex layout repacked whole on each append.
        (
            "repack",
            vec![(
                "function _packAdded(layout, m, added) {\n",
                "function _packAdded(layout, m, added) {\n    return false;\n",
            )],
        ),
        // A settle that walks the children of an unchanged pack is not on
        // this page's append path: qml6_packed_flex_inserts_walk_what_follows
        // pins its count (pack-settle) in eldur_js's qml_runtime tests.
        // More than five sweeps in the body's first hundred appends, and a
        // log n walk on body only: a trimmed first hundred passes this.
        (
            "early-sweeps",
            vec![
                (
                    "var _itemMetaSweepAt = 256;",
                    "var _itemMetaSweepAt = 0;\nvar _earlySweeps = 0;",
                ),
                (
                    "    _itemMetaSweepAt = Math.max(256, 2 * _itemMeta.size);",
                    "    _itemMetaSweepAt = ++_earlySweeps < 25 ? _itemMeta.size + 4 : Math.max(256, 2 * _itemMeta.size);",
                ),
                (wsleave, log4_body.as_str()),
            ],
        ),
    ];
    let page =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/observers/scaling/append_scaling.html");
    let html = std::fs::read_to_string(page).expect("page");
    let mut passed = Vec::new();
    for (name, patch) in &mutants {
        match run_patched(
            &format!("append_mutant_{name}"),
            &html,
            APPEND_SCALING_WANT,
            patch,
        ) {
            Ok(_) => passed.push(*name),
            Err(e) => println!("{name} fails the page, as it should:\n{e}\n"),
        }
    }
    assert!(
        passed.is_empty(),
        "the scaling page passes these mutants: {passed:?}"
    );
}

/// The converter no longer calls the three unsupported: they run.
#[test]
fn observers_convert_without_warnings() {
    let (pages, _) = pages();
    for page in pages {
        let html = std::fs::read_to_string(&page).expect("page");
        let out = convert_str(&html, &Options::default());
        let said: Vec<String> = out
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .filter(|m| m.contains("Observer"))
            .collect();
        assert!(said.is_empty(), "{}: {said:?}", page.display());
    }
}
