//! A press's defaults run only while its page is the document.
//!
//! A click runs its listeners, then what the press queued for after them: a
//! label's click on its control, a link's follow, and on a second press the
//! `dblclick`. Any of these may make another page the document; what is left
//! of the press is none of that page's, nor the one that went. Chromium's
//! record of the same presses is in `tests/click_defaults/chromium.json`, by
//! `node crates/eldur_convert/tests/click_defaults/gen.mjs`, its host swapping
//! the pages inside the press as the Qt host here does.
//!
//! An event that comes once another page is the document is that page's, as
//! each event in a browser goes to the document it lands on. Qt has already
//! given it to the old page's items: a second press that comes before the
//! first click ran, or a press that a move of Ljós's own catches the pointer
//! up to. Chromium has no such move, so its record of that press moves first.

mod support;

use eldur_convert::{Input, NoResources, Options, RootKind};

const PAGE: &str = include_str!("click_defaults/page.html");
const CHROMIUM: &str = include_str!("click_defaults/chromium.json");

const TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "ClickDefaults"
    when: windowShown

    // Page A, and Next made over it inside the listener that sets A's title.
    function pages() {
        const component = Qt.createComponent("A.qml")
        verify(component.status === Component.Ready, component.errorString())
        const a = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(a)
        a.forceActiveFocus()
        const at = { a: a, next: null, layers: [Eldur._in] }
        a.titleChanged.connect(function() {
            if (a.title === "Next" && !at.next) {
                at.next = createTemporaryObject(Qt.createComponent("Next.qml"), host, { width: 800, height: 600 })
                at.layers.push(Eldur._in)
            }
        })
        return at
    }

    // The item a page made for the element with this id.
    function element(item, id) {
        if (item.objectName === id) {
            return item
        }
        for (let i = 0; i < item.children.length; ++i) {
            const found = element(item.children[i], id)
            if (found) {
                return found
            }
        }
        return null
    }

    // The middle of A's element, in the host: a second press there is on
    // whichever page is on top.
    function middle(page, id) {
        const e = element(page, id)
        return e.mapToItem(host, e.width / 2, e.height / 2)
    }

    // Whether every click a release queued on either page has run: its
    // defaults and dblclick run inside it, so the press is done.
    function settled(at) {
        return at.layers.every(function(layer) { return layer.later.length === 0 })
    }

    // A double click as a person makes one: what the first press's release
    // queued has run when the second comes, as it has in a browser.
    function twice(at, p) {
        mouseClick(host, p.x, p.y)
        tryVerify(function() { return settled(at) }, 1000, "the first press's click never ran")
        mouseClick(host, p.x, p.y)
    }

    function finish(at) {
        tryVerify(function() { return at.next !== null && settled(at) }, 1000, "A was never swapped out")
    }

    function test_1_label_in_a_link() {
        const at = pages()
        console.log("LC -- label in a link")
        const p = middle(at.a, "lab")
        mouseClick(host, p.x, p.y)
        finish(at)
    }

    function test_2_double_click_whose_second_press_swaps() {
        const at = pages()
        console.log("LC -- double click whose second press swaps")
        const p = middle(at.a, "lab2")
        twice(at, p)
        finish(at)
    }

    function test_3_double_click_whose_first_press_swaps() {
        const at = pages()
        console.log("LC -- double click whose first press swaps")
        const p = middle(at.a, "lab3")
        twice(at, p)
        finish(at)
    }

    function test_4_double_click_whose_second_press_swaps_from_a_promise() {
        const at = pages()
        console.log("LC -- double click whose second press swaps from a promise")
        const p = middle(at.a, "lab4")
        twice(at, p)
        finish(at)
    }

    // Where no element is, so a page made under the pointer hears no move.
    function rest() {
        mouseMove(host, 790, 590)
    }

    // Both presses before the event loop turns: the first press's click runs
    // when the second comes, and makes Next under it.
    function test_5_double_click_whose_second_press_comes_before_the_first_click_ran() {
        const at = pages()
        console.log("LC -- double click whose second press comes before the first click ran")
        const p = middle(at.a, "lab5")
        mouseDoubleClickSequence(host, p.x, p.y)
        finish(at)
    }

    // No move first: the one Ljós catches the pointer up with makes Next.
    function test_6_press_whose_move_swaps() {
        rest()
        const at = pages()
        console.log("LC -- press whose move swaps")
        const p = middle(at.a, "mv")
        mouseClick(host, p.x, p.y)
        finish(at)
    }

    function test_7_move_whose_pointerover_swaps() {
        rest()
        const at = pages()
        console.log("LC -- move whose pointerover swaps")
        const p = middle(at.a, "ov")
        mouseMove(host, p.x, p.y)
        tryVerify(function() { return at.next !== null }, 1000, "A was never swapped out")
        mouseMove(host, p.x + 5, p.y)
        mouseMove(host, 400, p.y)
        finish(at)
    }
}
}
"##;

#[test]
fn a_press_does_nothing_more_once_its_page_is_gone() {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let dir = std::env::temp_dir().join(format!("eldur-click-defaults-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (at, name) in ["A", "Next"].into_iter().enumerate() {
        let out = eldur_convert::convert(
            &Input {
                html: PAGE.replace("NAME", name),
                url: format!("{}.html", name.to_lowercase()),
            },
            &options,
            &mut NoResources,
        );
        let script = format!("{}_page.js", name.to_lowercase());
        for file in &out.files {
            let path = file.path.to_string_lossy();
            let contents = String::from_utf8_lossy(&file.contents);
            match path.as_ref() {
                "Main.qml" => std::fs::write(
                    dir.join(format!("{name}.qml")),
                    contents
                        .replace("\"page.js\"", &format!("\"{script}\""))
                        .replace("Main.qml", &format!("{name}.qml")),
                ),
                "page.js" => std::fs::write(dir.join(&script), &file.contents),
                // The pages share the first one's runtime file, and so its state.
                _ if at == 0 => std::fs::write(dir.join(&file.path), &file.contents),
                _ => Ok(()),
            }
            .expect("write page");
        }
    }
    std::fs::write(dir.join("tst_click_defaults.qml"), TEST).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_click_defaults.qml"))
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
        l.contains("FAIL!") || l.contains("Warning:") || l.contains("QWARN") || l.contains("Error:")
    });
    if !output.status.success() {
        problems.push(format!("qmltestrunner exited with {}", output.status));
    }
    let logged: Vec<_> = text
        .lines()
        .filter_map(|l| l.find("LC ").map(|at| l[at..].to_owned()))
        .collect();
    let steps: serde_json::Value = serde_json::from_str(CHROMIUM).expect("chromium.json");
    let mut wanted = Vec::new();
    for step in steps.as_array().expect("steps") {
        wanted.push(format!("LC -- {}", step["name"].as_str().expect("name")));
        for line in step["log"].as_array().expect("log") {
            wanted.push(line.as_str().expect("line").to_owned());
        }
    }
    if logged != wanted {
        problems.push(format!(
            "logged\n    {}\n  wanted\n    {}",
            logged.join("\n    "),
            wanted.join("\n    ")
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n  "));
}
