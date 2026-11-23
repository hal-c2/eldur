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
    /// Some `Text` draws this, in lower case as `drawn` gives it, and none
    /// draws in a colour with no alpha.
    Shows(&'static str),
}

impl Want {
    /// The want as a check outside this test reads it.
    fn label(&self) -> String {
        match self {
            Want::Paints(v) => format!("paints {v}"),
            Want::Hovered([rest, over, left]) => format!("hovered {rest} {over} {left}"),
            Want::Draws(markup) => format!("draws {markup}"),
            Want::Shows(word) => format!("shows {word}"),
        }
    }

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
            Want::Shows(word) => format!(
                "verify(drawn(page).indexOf('{word}') >= 0, drawn(page))\n\
                 verify(!/#00[0-9a-f]{{6}}\\b/.test(drawn(page)), drawn(page))"
            ),
        }
    }
}

/// Runs `cases`, failing with every one that differs.
fn check(cases: &[(&str, String, Want)]) {
    if support::no_qt() {
        return;
    }
    // A directory to keep each page and its want in, for Chromium to load
    // the same pages and say whether it agrees with what is wanted.
    let pages = std::env::var_os("ELDUR_FADE_PAGES").map(std::path::PathBuf::from);
    let mut failures = Vec::new();
    for (name, html, want) in cases {
        if let Some(pages) = &pages {
            let at = pages.join(name);
            std::fs::create_dir_all(&at).expect("pages dir");
            std::fs::write(at.join("page.html"), html).expect("write page");
            std::fs::write(at.join("want"), want.label()).expect("write want");
        }
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

/// Moves `#ix` and lays it out again, which replaces its item.
const NUDGE: &str = "document.getElementById('ix').animate([{transform:'translateX(1px)'},\
                     {transform:'translateX(2px)'}],{duration:20,fill:'forwards'})";
/// Takes `#ix` out of the flow, which replaces its item again.
const PLACE: &str = "document.getElementById('ix').style.position='absolute'";
const MOVE_OUT: &str = "document.body.appendChild(document.getElementById('ix'))";

const DIM: &str = ".dim{opacity:.5}";
const GONE: &str = ".gone{opacity:0}";
const HALF: &str = "#sp{opacity:.5}";
/// A script that only finds `#ix`, so that it keeps its name in the QML.
const NAMED: &str = "document.getElementById('ix');";
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
        // A box with a margin is placed by a wrapper, which takes the fade:
        // the item that replaces the box must not take it again.
        (
            "margin-left",
            live(&format!("{HALF}#ix{{margin-left:6px}}"), "", &[NUDGE]),
            Paints(0.5),
        ),
        (
            "margin-top",
            live(&format!("{HALF}#ix{{margin-top:4px}}"), "", &[NUDGE]),
            Paints(0.5),
        ),
        (
            "margin",
            live(&format!("{HALF}#ix{{margin:4px}}"), "", &[NUDGE]),
            Paints(0.5),
        ),
        (
            "margin-animated",
            live(&format!("{HALF}#ix{{margin:4px}}"), "", &[ANIMATE]),
            Paints(0.4),
        ),
        // A second replacement keeps it, as does one alone.
        (
            "animated-placed",
            live(HALF, "", &[ANIMATE, PLACE]),
            Paints(0.4),
        ),
        (
            "animated-placed-hidden",
            live("#sp{opacity:0}", "", &[ANIMATE, PLACE]),
            Paints(0.0),
        ),
        ("placed", live(HALF, "", &[PLACE]), Paints(0.5)),
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

/// `#sp` starts hidden by its opacity and something then shows it, or the
/// other way round: what it holds follows it.
#[test]
fn what_shows_an_inline_element_shows_what_it_holds() {
    use Want::{Draws, Paints, Shows};
    const HIDDEN: &str = "#sp{opacity:0}";
    const FADE_IN: &str = "{opacity:0;animation:i .2s forwards}@keyframes i{to{opacity:1}}";
    let text = "<p>a <span id=sp>secret</span> b</p>";
    let sp = "var sp=document.getElementById('sp');";
    let in_text = |step: &str| paragraph(HIDDEN, text, &[&format!("{sp}{step}")]);
    check(&[
        (
            "css-animation",
            boxed(&format!("#sp{FADE_IN}"), "", NAMED),
            Paints(1.0),
        ),
        (
            "css-animation-text",
            paragraph(
                &format!(".fi{{color:red}}.fi{FADE_IN}"),
                "<h1><span class=fi>RED</span> <span class=fi>TWO</span></h1>",
                &[],
            ),
            Shows("red"),
        ),
        (
            "write",
            live(HIDDEN, "", &["sp.style.opacity='1'"]),
            Paints(1.0),
        ),
        (
            "set-property",
            live(HIDDEN, "", &["sp.style.setProperty('opacity','1')"]),
            Paints(1.0),
        ),
        (
            "css-text",
            live(HIDDEN, "", &["sp.style.cssText='opacity:1'"]),
            Paints(1.0),
        ),
        (
            "style-attribute",
            live(HIDDEN, "", &["sp.setAttribute('style','opacity:1')"]),
            Paints(1.0),
        ),
        (
            "animate",
            live(
                HIDDEN,
                "",
                &["sp.animate([{opacity:0},{opacity:1}],{duration:60,fill:'forwards'})"],
            ),
            Paints(1.0),
        ),
        ("moved-out", live(HIDDEN, "", &[MOVE_OUT]), Paints(1.0)),
        (
            "moved-out-margin",
            live(&format!("{HIDDEN}#ix{{margin-left:6px}}"), "", &[MOVE_OUT]),
            Paints(1.0),
        ),
        (
            "written-back",
            live(HALF, "", &["sp.style.opacity='1'", "sp.style.opacity=''"]),
            Paints(0.5),
        ),
        (
            "text-write",
            in_text("sp.style.opacity='1'"),
            Shows("secret"),
        ),
        (
            "text-animate",
            in_text("sp.animate([{opacity:0},{opacity:1}],{duration:60,fill:'forwards'})"),
            Shows("secret"),
        ),
        ("hide", live("", "", &["sp.style.opacity='0'"]), Paints(0.0)),
        (
            "hide-margin",
            live("#ix{margin-left:6px}", "", &["sp.style.opacity='.5'"]),
            Paints(0.5),
        ),
        (
            "hide-text",
            paragraph("", text, &[&format!("{sp}sp.style.opacity='0'")]),
            Draws("<font color=\"#00000000\">secret"),
        ),
    ]);
}

/// A CSS animation of the box's own opacity is the box's: the element around
/// it fades it still.
#[test]
fn an_animation_of_the_boxs_own_opacity_keeps_the_fade() {
    use Want::Paints;
    const STILL: &str = "#ix{animation:i 4s}@keyframes i{from{opacity:1}to{opacity:1}}";
    const FADE_IN: &str = "#ix{animation:i .2s forwards}@keyframes i{to{opacity:1}}";
    check(&[
        (
            "own",
            boxed(&format!("{HALF}{STILL}"), "", NAMED),
            Paints(0.5),
        ),
        (
            "own-margin",
            boxed(&format!("{HALF}{STILL}#ix{{margin-left:6px}}"), "", NAMED),
            Paints(0.5),
        ),
        (
            "own-hidden",
            boxed(&format!("#sp{{opacity:0}}{FADE_IN}"), "", NAMED),
            Paints(0.0),
        ),
    ]);
}
