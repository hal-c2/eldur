//! An inline element's `opacity` fades the text and the boxes it holds, and
//! goes on doing so: where a class or a state gives it or takes it away,
//! under an opacity a script writes, on an item that replaces a box, and in a
//! paragraph the runtime draws again. The values are CSS's: an element paints
//! at its own opacity times those of the elements around it.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 300

TestCase {
    name: "InlineFade"
    when: windowShown

    function named(item, name) {
        if (item.objectName === name && item.visible) {
            return item
        }
        for (let i = item.children.length - 1; i >= 0; --i) {
            const hit = named(item.children[i], name)
            if (hit) {
                return hit
            }
        }
        return null
    }

    function drawn(item) {
        let out = item.visible && typeof item.text === "string" && item.lineCount !== undefined ? item.text : ""
        for (let i = 0; i < item.children.length; ++i) {
            out += " " + drawn(item.children[i])
        }
        return out.toLowerCase()
    }

    function painted(page) {
        const box = named(page, "ix")
        verify(box !== null, "#ix is gone")
        let all = 1
        for (let at = box; at && at !== host; at = at.parent) {
            all *= at.opacity
        }
        return all
    }

    function test_page() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 300 })
        waitForRendering(page)
        // Past the page's last timer.
        wait(350)
        CHECKS
    }
}
}
"##;

/// What a page is to show once its script has run.
enum Want {
    /// `#ix` paints at this opacity.
    Paints(f64),
    /// It paints at these at rest, with the pointer over it, and after.
    Hovered([f64; 3]),
    /// Some `Text` holds this markup.
    Draws(&'static str),
}

impl Want {
    fn checks(&self) -> String {
        let paints = |v: &f64| format!("fuzzyCompare(painted(page), {v}, 0.005)");
        match self {
            Want::Paints(v) => paints(v),
            Want::Hovered([rest, over, left]) => [
                paints(rest),
                "mouseMove(page, 40, 20); wait(80)".to_owned(),
                paints(over),
                "mouseMove(page, 600, 250); wait(80)".to_owned(),
                paints(left),
            ]
            .join("\n"),
            Want::Draws(markup) => {
                format!("verify(drawn(page).indexOf('{markup}') >= 0, drawn(page))")
            }
        }
    }
}

/// Runs `cases`, failing with every one that differs.
fn check(cases: &[(&str, String, Want)]) {
    if support::no_qt() {
        return;
    }
    let mut failures = Vec::new();
    for (name, html, want) in cases {
        let options = Options {
            root: RootKind::Item,
            ..Options::default()
        };
        let out = convert_str(html, &options);
        let dir = std::env::temp_dir().join(format!("eldur-fade-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        for file in &out.files {
            std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
        }
        let test = dir.join("tst_fade.qml");
        std::fs::write(&test, TEST.replace("CHECKS", &want.checks())).expect("write test");
        let run = support::qmltestrunner()
            .arg("-input")
            .arg(&test)
            .current_dir(&dir)
            .env("QT_QPA_PLATFORM", "offscreen")
            .output()
            .expect("run qmltestrunner");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        if run.status.success() {
            let _ = std::fs::remove_dir_all(&dir);
        } else {
            let lines = support::problem_lines(&text, |l| {
                l.starts_with("FAIL!") || l.contains("Actual") || l.contains("Expected")
            });
            failures.push(format!("{name}:\n{}", lines.join("\n")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// `steps`, each in a timer of its own after the last one's.
fn later(steps: &[&str]) -> String {
    let timed = steps.iter().enumerate();
    timed
        .map(|(i, step)| format!("setTimeout(function(){{{step}}},{});", 20 + 30 * i))
        .collect()
}

/// `#ix`, an inline-block, inside the inline element `#sp`.
fn boxed(css: &str, span: &str, script: &str) -> String {
    format!(
        "<!doctype html><html><head><style>body{{margin:0}}\
         #ix{{display:inline-block;width:80px;height:40px;background:red}}{css}</style></head>\
         <body><div><a id=sp {span}><i id=ix></i></a></div><script>{script}</script></body></html>"
    )
}

/// [`boxed`] with a runtime DOM, which a script that reaches an inline
/// element gives a page.
fn live(css: &str, span: &str, steps: &[&str]) -> String {
    let script = format!("var sp=document.getElementById('sp');{}", later(steps));
    boxed(css, span, &script)
}

fn paragraph(css: &str, body: &str, steps: &[&str]) -> String {
    format!(
        "<!doctype html><html><head><style>{css}</style></head>\
         <body>{body}<script>{}</script></body></html>",
        later(steps)
    )
}

const WRITE_1: &str = "document.getElementById('ix').style.opacity='1'";
const WRITE_6: &str = "document.getElementById('ix').style.opacity='0.6'";
const CLEAR: &str = "document.getElementById('ix').style.opacity=''";
const SHOW: &str = "document.getElementById('ix').style.visibility='visible'";
const ANIMATE: &str =
    "document.getElementById('ix').animate([{opacity:.8},{opacity:.8}],{duration:4000})";
const ANIMATE_BRIEFLY: &str =
    "document.getElementById('ix').animate([{opacity:.8},{opacity:.8}],{duration:20})";
const ADD_DIM: &str = "document.getElementById('sp').classList.add('dim')";
const DROP_DIM: &str = "document.getElementById('sp').classList.remove('dim')";
const ADD_GONE: &str = "document.getElementById('sp').classList.add('gone')";
const DROP_GONE: &str = "document.getElementById('sp').classList.remove('gone')";
const ADD_OTHER: &str = "document.getElementById('sp').classList.add('other')";
/// A class is one the page can have only where a script may add it.
const MAY_DIM: &str =
    "document.body.onclick=function(){document.getElementById('sp').classList.add('dim')}";
const MAY_GO: &str =
    "document.body.onclick=function(){document.getElementById('sp').classList.add('gone')}";
const SET_C: &str = "document.getElementById('c').textContent='z'";

const DIM: &str = ".dim{opacity:.5}";
const GONE: &str = ".gone{opacity:0}";
const HALF: &str = "#sp{opacity:.5}";
const HOVER: &str = "a:hover{opacity:.5}";

#[test]
fn a_fade_a_class_or_state_gives_follows_it_under_a_scripts_write() {
    use Want::{Hovered, Paints};
    let own = |css: &str| format!("{css}#ix{{opacity:.8}}");
    check(&[
        (
            "hover",
            live(HOVER, "href='#'", &[WRITE_1]),
            Hovered([1.0, 0.5, 1.0]),
        ),
        (
            "dim-absent",
            live(DIM, "", &[MAY_DIM, WRITE_6]),
            Paints(0.6),
        ),
        (
            "gone-absent",
            live(GONE, "", &[MAY_GO, WRITE_1]),
            Paints(1.0),
        ),
        (
            "dim-removed",
            live(DIM, "class=dim", &[DROP_DIM, WRITE_6]),
            Paints(0.6),
        ),
        (
            "write-then-dim",
            live(DIM, "", &[WRITE_1, ADD_DIM]),
            Paints(0.5),
        ),
        ("dim-added", live(DIM, "", &[ADD_DIM, WRITE_6]), Paints(0.3)),
        (
            "write-then-dim-removed",
            live(DIM, "class=dim", &[WRITE_6, DROP_DIM]),
            Paints(0.6),
        ),
        (
            "write-then-gone",
            live(GONE, "", &[WRITE_6, ADD_GONE]),
            Paints(0.0),
        ),
        (
            "gone-and-back",
            live(GONE, "", &[WRITE_6, ADD_GONE, DROP_GONE]),
            Paints(0.6),
        ),
        (
            "gone-removed",
            live(GONE, "class=gone", &[DROP_GONE, WRITE_6]),
            Paints(0.6),
        ),
        (
            "gone-write-removed",
            live(GONE, "class=gone", &[WRITE_6, DROP_GONE]),
            Paints(0.6),
        ),
        (
            "cleared",
            live(&own(DIM), "", &[WRITE_6, ADD_DIM, CLEAR]),
            Paints(0.4),
        ),
        (
            "visibility-only",
            live(&own(DIM), "", &[SHOW, ADD_DIM]),
            Paints(0.4),
        ),
    ]);
}

#[test]
fn a_scripts_write_keeps_the_fade_without_a_runtime_dom() {
    use Want::{Hovered, Paints};
    let hidden = "#sp{opacity:.5}#ix{visibility:hidden}";
    check(&[
        ("plain", boxed(HALF, "", &later(&[WRITE_6])), Paints(0.3)),
        (
            "plain-hidden",
            boxed("#sp{opacity:0}", "", &later(&[WRITE_6])),
            Paints(0.0),
        ),
        (
            "plain-visibility",
            boxed(hidden, "", &later(&[SHOW])),
            Paints(0.5),
        ),
        (
            "plain-hover",
            boxed(HOVER, "href='#'", &later(&[WRITE_1])),
            Hovered([1.0, 0.5, 1.0]),
        ),
    ]);
}

#[test]
fn the_fade_survives_an_item_that_replaces_the_box() {
    use Want::Paints;
    check(&[
        ("animated", live(HALF, "", &[ANIMATE]), Paints(0.4)),
        (
            "animated-hidden",
            live("#sp{opacity:0}", "", &[ANIMATE]),
            Paints(0.0),
        ),
        (
            "animated-written",
            live(HALF, "", &[ANIMATE_BRIEFLY, "0", "0", WRITE_6]),
            Paints(0.3),
        ),
        (
            "dimmed-animated",
            live(DIM, "", &[ADD_DIM, ANIMATE]),
            Paints(0.4),
        ),
        (
            "animated-dimmed",
            live(DIM, "", &[ANIMATE, ADD_DIM]),
            Paints(0.4),
        ),
        (
            "animated-undimmed",
            live(DIM, "class=dim", &[ANIMATE, DROP_DIM]),
            Paints(0.8),
        ),
        // The replacement is the element's box still, whatever lays the line out.
        (
            "animated-classed",
            live(DIM, "", &[MAY_DIM, ANIMATE, ADD_OTHER]),
            Paints(0.8),
        ),
    ]);
}

#[test]
fn a_redrawn_paragraph_keeps_its_texts_fade() {
    use Want::Draws;
    let half = ".r{color:red;opacity:.5}";
    check(&[
        (
            "redraw-hidden",
            paragraph(
                "",
                "<p>a <span style='opacity:0'>secret</span> <span id=c>c</span></p>",
                &[SET_C],
            ),
            Draws("<font color=\"#00000000\">secret"),
        ),
        (
            "redraw-half",
            paragraph(
                half,
                "<p>a <span class=r>red</span> <span id=c>c</span></p>",
                &[SET_C],
            ),
            Draws("<font color=\"#80ff0000\">red"),
        ),
        (
            "redraw-nested",
            paragraph(
                half,
                "<p>a <span class=r>x<b style='opacity:.5'>deep</b></span> <span id=c>c</span></p>",
                &[SET_C],
            ),
            Draws("#40ff0000"),
        ),
        (
            "redraw-class",
            paragraph(
                ".r{color:red}.dim{opacity:.5}",
                "<p>a <span id=sp class=r>red</span> <span id=c>c</span></p>",
                &[ADD_DIM, SET_C],
            ),
            Draws("<font color=\"#80ff0000\">red"),
        ),
    ]);
}
