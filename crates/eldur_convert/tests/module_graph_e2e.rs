//! A page's module script and the modules it reaches, as a bundler splits a
//! site: an entry module with static imports of shared chunks, and chunks
//! loaded lazily with `import()`. The pages under `tests/module_graph/` are
//! served from that directory, whose root is `/`, as `--site-root` makes it;
//! `tests/module_graph/chromium.json` is what Chromium did with them, recorded
//! by `node crates/eldur_convert/tests/module_graph/gen.mjs`.

mod support;

use std::path::{Component, Path, PathBuf};

use eldur_convert::{Input, Options, Resource, Resources, RootKind, Severity};
use serde_json::Value;

const CHROMIUM: &str = include_str!("module_graph/chromium.json");

/// The files of `tests/module_graph`, found as a web server rooted there finds
/// them: `/x` from the root, anything else from the referencing file's
/// directory. A resolved name is the URL path, `/order/a.js`.
struct Site {
    root: PathBuf,
    read: Vec<String>,
}

impl Site {
    fn new() -> Self {
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/module_graph"),
            read: Vec::new(),
        }
    }
}

impl Resources for Site {
    fn read(&mut self, url: &str, base: &str) -> Option<Resource> {
        let joined = if url.starts_with('/') {
            PathBuf::from(url)
        } else {
            Path::new(base).parent()?.join(url)
        };
        let mut parts: Vec<String> = Vec::new();
        for part in joined.components() {
            match part {
                Component::Normal(p) => parts.push(p.to_string_lossy().into_owned()),
                Component::ParentDir => {
                    parts.pop();
                }
                _ => {}
            }
        }
        let resolved = format!("/{}", parts.join("/"));
        self.read.push(resolved.clone());
        let bytes = std::fs::read(self.root.join(parts.join("/"))).ok()?;
        Some(Resource { resolved, bytes })
    }
}

fn convert(page: &str, site: &mut Site) -> eldur_convert::Output {
    let url = format!("/{page}/index.html");
    let html = std::fs::read_to_string(site.root.join(&url[1..])).expect("page");
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    eldur_convert::convert(&Input { html, url }, &options, site)
}

fn chromium(page: &str) -> Value {
    let all: Value = serde_json::from_str(CHROMIUM).expect("chromium.json");
    all[page].clone()
}

fn warnings(out: &eldur_convert::Output) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.severity >= Severity::Warning)
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn modules_a_module_script_reaches_are_read_beside_it_and_inlined() {
    let mut site = Site::new();
    let out = convert("repro", &mut site);
    for file in [
        "/repro/js/main.js",
        "/repro/js/helper.js",
        "/repro/js/chunk.js",
    ] {
        assert!(
            site.read.iter().any(|r| r == file),
            "{file}: {:?}",
            site.read
        );
    }
    let warnings = warnings(&out);
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("module") || w.contains("import")),
        "{warnings:#?}"
    );
}

/// What cannot be loaded still says so: a bare specifier (no import map is
/// read), a file that is not there. A module only ever named by a computed
/// `import()` is not read at all.
#[test]
fn a_module_that_cannot_be_loaded_is_reported() {
    let mut site = Site::new();
    let out = convert("order", &mut site);
    assert!(
        !site.read.iter().any(|r| r.contains("bare-package")),
        "{:?}",
        site.read
    );
    let warnings = warnings(&out);
    let about_imports: Vec<&String> = warnings.iter().filter(|w| w.contains("import")).collect();
    assert_eq!(
        about_imports,
        vec![
            "dynamic import('./missing.js') cannot load a module in QML: the promise rejects as for a missing module",
            "dynamic import('bare-package') cannot load a module in QML: the promise rejects as for a missing module",
        ],
        "{warnings:#?}"
    );
}

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "ModuleGraph"
    when: windowShown

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(500)
        const go = findChild(page, "go")
        if (go) {
            mouseClick(go)
            wait(300)
        }
        for (const id of READ) {
            console.log("TEXT " + id + "=" + findChild(page, id).text)
        }
    }
}
}
"##;

/// What a page logged (the `LC ` lines), the text of the elements it read, and
/// the uncaught errors it reported, each as `Name: message`.
type Seen = (Vec<String>, Vec<(String, String)>, Vec<String>);

/// Runs a converted page in qmltestrunner and gives what it showed, or the
/// problems it had. An uncaught error is not a problem here: it is compared
/// with Chromium's.
fn run(page: &str, read: &[&str]) -> Seen {
    let mut site = Site::new();
    let out = convert(page, &mut site);
    let dir = std::env::temp_dir().join(format!(
        "eldur-modules-{}-{}",
        page.replace('/', "-"),
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let ids: Vec<String> = read.iter().map(|id| format!("{id:?}")).collect();
    let test = TEST.replace("READ", &format!("[{}]", ids.join(", ")));
    std::fs::write(dir.join("tst_modules.qml"), test).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_modules.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let page_js = out
        .files
        .iter()
        .find(|f| f.path == Path::new("page.js"))
        .map(|f| String::from_utf8_lossy(&f.contents).into_owned())
        .unwrap_or_default();
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    let uncaught = |l: &str| {
        l.find("Uncaught ")
            .map(|at| l[at + 9..].trim_end().to_owned())
    };
    let mut problems = support::problem_lines(&text, |l| {
        uncaught(l).is_none()
            && (l.contains("FAIL!")
                || l.contains("Warning:")
                || l.contains("QWARN")
                || l.contains("Error:"))
    });
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
    assert!(
        problems.is_empty(),
        "{}\npage.js:\n{page_js}",
        problems.join("\n  ")
    );
    let logged = text
        .lines()
        .filter_map(|l| l.find("LC ").map(|at| l[at + 3..].to_owned()))
        .collect();
    let texts = text
        .lines()
        .filter_map(|l| {
            let rest = &l[l.find("TEXT ")? + 5..];
            let (id, value) = rest.split_once('=')?;
            Some((id.to_owned(), value.to_owned()))
        })
        .collect();
    let errors = text.lines().filter_map(uncaught).collect();
    (logged, texts, errors)
}

fn wanted(page: &str, read: &[&str]) -> Seen {
    let recorded = chromium(page);
    let errors = recorded["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|e| e.as_str().expect("error").to_owned())
        .collect();
    let lines = recorded["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|l| l.as_str().expect("line").to_owned())
        .collect();
    let texts = read
        .iter()
        .map(|id| {
            let text = recorded["text"][*id].as_str().expect("text");
            ((*id).to_owned(), text.to_owned())
        })
        .collect();
    (lines, texts, errors)
}

/// The issue's page (hal-c2/eldur#22): `n=2` from the static import, `chunk`
/// from the lazy one.
#[test]
fn a_static_import_and_an_import_call_load_as_in_chromium() {
    if support::no_qt() {
        return;
    }
    let read = ["a", "b"];
    assert_eq!(run("repro", &read), wanted("repro", &read));
}

/// Chromium's evaluation order: dependencies first, a module shared by two
/// importers once, a cycle's far side before its near side, the entry last. A
/// lazily imported module runs after the code that imported it and its
/// microtasks, once, with its own lazy dependency before it; a module that
/// throws rejects every import of it with that error; a missing file and a
/// bare specifier reject with a TypeError; a module nothing imports never
/// runs; an inline handler's `import()` resolves against the page.
#[test]
fn modules_run_in_chromiums_order_and_import_calls_settle_as_there() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("order", &read), wanted("order", &read));
}

/// Root-relative specifiers in an inline module, resolved under the site root.
#[test]
fn root_relative_specifiers_resolve_under_the_site_root() {
    if support::no_qt() {
        return;
    }
    let read = ["a", "b"];
    assert_eq!(run("rooted", &read), wanted("rooted", &read));
}

/// Lazily imported modules among themselves: a cycle entered from either side
/// runs its far side first; a module that throws fails its whole cycle; a
/// module whose own import is missing rejects before any of it runs; a class
/// exists once its module has run; a listener for `load` or
/// `DOMContentLoaded` added after the page loaded never hears either; a
/// namespace import reads the module's live bindings; and a `let`, `const`,
/// `class` or default export read across a cycle before its module has run
/// throws a `ReferenceError`, while a write to a `const` or an import throws
/// a `TypeError`.
#[test]
fn lazily_imported_modules_link_and_fail_as_in_chromium() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("lazy", &read), wanted("lazy", &read));
}

/// A page in a subdirectory: its inline module's `../` import is found above
/// it, and an `import()` of a file that is not beside it rejects even though a
/// file of that name is loaded from another directory.
#[test]
fn a_nested_pages_imports_resolve_from_its_own_directory() {
    if support::no_qt() {
        return;
    }
    let read = ["a", "b"];
    assert_eq!(run("nested/page", &read), wanted("nested", &read));
}

/// Module scripts at load, one of which throws: the scripts after it still
/// run, a script importing it does not, each of the two reports the error, and
/// every `import()` of either rejects with it, one made before the module ran
/// too, once every script has run.
#[test]
fn a_module_that_throws_at_load_fails_only_its_importers() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("eager", &read), wanted("eager", &read));
}

/// A lazily imported module's element constant is looked up when the module
/// runs: read across a cycle before then it throws a `ReferenceError`, and
/// what the page changed in the element before is there.
#[test]
fn a_lazy_modules_element_lookup_runs_with_the_module() {
    if support::no_qt() {
        return;
    }
    let read = ["out", "label"];
    assert_eq!(run("element", &read), wanted("element", &read));
}

/// A module that throws fails every module of its cycle: one of them that ran
/// to its end still rejects its `import()` and reports at its own script with
/// that error, a module importing it does not run, and a `DOMContentLoaded`
/// listener the finished one added is heard.
#[test]
fn a_throwing_cycle_fails_every_module_in_it() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("cycle", &read), wanted("cycle", &read));
}

/// A module whose import threw stops before the imports after it: one of those
/// runs at its own script, another at its `import()`, and of the listeners the
/// throwing module would add only the one before the throw is heard.
#[test]
fn a_module_skipped_by_a_failed_importer_runs_later() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("skip", &read), wanted("skip", &read));
}

/// A module that reaches a cycle through a module of it that already ran, not
/// back to the module still running, is part of that cycle: when the cycle
/// throws it fails too, its `import()` rejects, and a module importing it does
/// not run.
#[test]
fn a_module_reaching_a_running_cycle_fails_with_it() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("cross", &read), wanted("cross", &read));
}

/// A top-level initializer that throws while it is worked out, here a template
/// whose value's `toString` throws, fails its module only: the scripts after it
/// run and an `import()` of it rejects with that error.
#[test]
fn a_throwing_initializer_fails_its_module_only() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("hoist", &read), wanted("hoist", &read));
}

/// `wa.js` and `wb.js` import each other. `wb.js` runs to its end, then `wa.js`
/// calls `import("./wb.js")` and throws. The cycle shares one result, so that
/// `import()` and a later one reject with `wa`'s error, as Chromium's do.
#[test]
fn an_import_of_a_cycle_member_waits_for_its_cycle() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("cyclewait", &read), wanted("cyclewait", &read));
}

/// `entry.js` imports `bad.js`, which throws, before `x.js`, so the x↔y cycle
/// is first entered at `y.js`'s script, from the member the conversion did
/// not see first. `y.js` calls `import("./x.js")` after `x.js` ran and then
/// throws: that `import()` and a later one reject with `y`'s error.
#[test]
fn a_cycle_entered_at_another_member_settles_with_it() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("cycleentry", &read), wanted("cycleentry", &read));
}

/// `bad.js` throws before it adds a click listener to `go`, which is wired in
/// at conversion as the button's only one. A click is not heard, as in
/// Chromium: a listener its module never reached is never added.
#[test]
fn a_listener_after_its_modules_throw_is_never_heard() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("listenthrow", &read), wanted("listenthrow", &read));
}

/// The same with `go.onclick = …` after the throw: the click is not heard.
#[test]
fn an_onclick_after_its_modules_throw_is_never_heard() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("onclickthrow", &read), wanted("onclickthrow", &read));
}

/// A classic script sets `window.onload`; two modules throw before they set it
/// to another handler and to `null`. The classic script's handler is called at
/// load: an assignment that never ran changes nothing.
#[test]
fn a_window_onload_assignment_counts_only_if_it_ran() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("onloadthrow", &read), wanted("onloadthrow", &read));
}

/// A module sets `window.onload = null` after a classic script set a handler,
/// and a later module throws before it sets another. No handler is called at
/// load: the last assignment that ran wins.
#[test]
fn the_last_window_onload_assignment_that_ran_wins() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("onloadnull", &read), wanted("onloadnull", &read));
}

/// `handler.js` throws before it sets `window.onload`, `listen.js` adds a
/// `load` listener, and `late.js` sets `window.onload`. The handler takes its
/// place among the `load` listeners when the assignment runs, so the listener
/// is heard first.
#[test]
fn a_window_onload_takes_its_place_when_it_is_set() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("onloadplace", &read), wanted("onloadplace", &read));
}

/// A classic script sets `window.onload`, another adds a `load` listener, and a
/// module sets `window.onload = null` and then another handler. `null` takes
/// the handler out of the `load` listeners, so the new one is heard after the
/// listener.
#[test]
fn a_window_onload_set_again_after_null_is_heard_last() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("onloadreset", &read), wanted("onloadreset", &read));
}

/// A classic script and a module each add a `DOMContentLoaded` or `load`
/// listener by name, then assign another function to the name. A browser takes
/// the listener's value at the `addEventListener` call, so the first function
/// is called.
#[test]
fn a_lifecycle_listener_is_the_value_it_had_when_added() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("loadvalue", &read), wanted("loadvalue", &read));
}

/// `first.js` adds the same function, and the same `handleEvent` object, to
/// `load` twice, and the function a third and fourth time as a capture
/// listener; a module adds its arrow twice. A browser keeps one listener per
/// target, type, callback and capture, so each is called once and the capture
/// one once more. A `load` listener on `document` is never called, and the
/// `DOMContentLoaded` listeners run window capture, document capture, document,
/// then window.
#[test]
fn a_listener_added_twice_is_heard_once() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("loadtwice", &read), wanted("loadtwice", &read));
}

/// `addEventListener` reads its options (`capture`, `once`, `passive`,
/// `signal`, each once, in that order) before it looks at the callback, so an
/// options object's getters run for a `null` callback too: in a classic
/// script, an eager module and a lazy one. A `load` listener whose `signal`
/// aborted before the call is not added, and one whose `signal` aborts in a
/// `DOMContentLoaded` listener is taken out.
#[test]
fn a_listener_reads_its_options_first() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("loadoptions", &read), wanted("loadoptions", &read));
}

/// `source.js` pushes to an array before copying it, and defines a getter
/// that throws before destructuring the object it is on. Each initializer
/// sees what ran before it: the copy has two items and the destructuring
/// throws, so only `source.js` fails and its `import()` rejects.
#[test]
fn an_initializer_sees_what_ran_before_it() {
    if support::no_qt() {
        return;
    }
    let read = ["out"];
    assert_eq!(run("readlater", &read), wanted("readlater", &read));
}
