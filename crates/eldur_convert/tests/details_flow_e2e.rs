//! A `<details>` takes part in block flow as Chromium lays it out. While it is
//! closed its folded blocks bring no margins: the box after it sits right under
//! the summary (#70), on a page with no script, and their margins come back
//! when it opens. A rule that hides the last block of an open one does the same.
//! A `<details open>` a script makes, by `innerHTML` or by `createElement` and
//! `setAttribute`, is an open disclosure like one the page wrote: its summary
//! shows the open marker, its content keeps its margins through the end of the
//! `<details>` and its host, and its summary closes and opens it (#63).
//!
//! Chromium's places, at 18 px lines and 16 px paragraph margins: a block after
//! a closed `<details>` is one line below the summary; open, the paragraph is a
//! line plus 16 below the summary and the next block a line plus 16 below that.

mod support;

use eldur_convert::{Options, RootKind, convert_str};

const SUPPORT: &str = r##"
    function find(item, test) {
        if (test(item))
            return item
        for (const child of item.children) {
            const got = find(child, test)
            if (got)
                return got
        }
        return null
    }

    function textOf(page, words) {
        return find(page, i => i.text !== undefined && i.font !== undefined
                    && i.eldurTag !== "button" && String(i.text).indexOf(words) >= 0)
    }

    function pageY(page, item) {
        return item.mapToItem(page, 0, 0).y
    }

    // `b` is `gap` px below the bottom of `a`'s line.
    function below(page, a, b, gap, step) {
        const got = pageY(page, b) - pageY(page, a) - a.height
        compare(Math.round(got), gap, step + ": " + b.text + " is " + got + " px below "
                + a.text + ", Chromium " + gap)
    }

    function load(component) {
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(50)
        return page
    }

    function disclosure(item) {
        return find(item, i => i.open !== undefined && i.em !== undefined)
    }
"##;

// No script at all: the runtime opens and closes it when the summary is pressed.
const CLOSED_PAGE: &str = r##"<!doctype html>
<style>body { font: 16px/18px sans-serif; }</style>
<details><summary>Summary</summary><p>Folded</p></details>
<div id="next">Next</div>
"##;

const CLOSED_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsClosed"
    when: windowShown
SUPPORT

    function test_folded_margins() {
        const page = load(Qt.createComponent("Main.qml"))
        const summary = textOf(page, "Summary")
        const folded = textOf(page, "Folded")
        const next = textOf(page, "Next")
        verify(summary && folded && next, "lines")
        verify(!folded.visible, "Folded shows while closed")
        below(page, summary, next, 0, "closed")
        mouseClick(summary, 40, summary.height / 2)
        wait(100)
        verify(folded.visible, "Folded is hidden once open")
        below(page, summary, folded, 16, "open")
        below(page, folded, next, 16, "open")
        mouseClick(summary, 40, summary.height / 2)
        wait(100)
        verify(!folded.visible, "Folded shows once closed again")
        below(page, summary, next, 0, "closed again")
    }
}
}
"##;

// An open `<details>` whose last block a scheme rule hides.
const DIM_PAGE: &str = r##"<!doctype html>
<style>
body { font: 16px/18px sans-serif; }
:root[data-theme='dim'] .fold { display: none; }
</style>
<details open><summary>Summary</summary><p class="fold">Folded</p></details>
<div id="next">Next</div>
<button id="t">Dim</button>
<script>
document.getElementById('t').addEventListener('click', () => {
  const r = document.documentElement;
  r.dataset.theme = r.dataset.theme === 'dim' ? '' : 'dim';
});
</script>
"##;

const DIM_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsDim"
    when: windowShown
SUPPORT

    function test_hidden_last_block() {
        const page = load(Qt.createComponent("Main.qml"))
        const summary = textOf(page, "Summary")
        const folded = textOf(page, "Folded")
        const next = textOf(page, "Next")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Dim")
        verify(summary && folded && next && button, "lines")
        below(page, summary, folded, 16, "load")
        below(page, folded, next, 16, "load")
        mouseClick(button)
        wait(100)
        verify(!folded.visible, "Folded shows under dim")
        below(page, summary, next, 0, "dim")
        mouseClick(button)
        wait(100)
        verify(folded.visible, "Folded is hidden once not dim")
        below(page, folded, next, 16, "not dim")
    }
}
}
"##;

// #63's page, with a log of what the made disclosures say.
const MADE_PAGE: &str = r##"<!doctype html>
<style>body { font: 16px/18px sans-serif; }</style>
<div id="made-host"></div>
<div id="built-host"></div>
<div id="log">-</div>
<button id="make">Make</button>
<button id="ask">Ask</button>
<script>
document.getElementById('make').addEventListener('click', () => {
  document.getElementById('made-host').innerHTML =
    '<details open><summary>Made summary</summary><p><em>Made inside</em></p></details>';
  const d = document.createElement('details');
  d.setAttribute('open', '');
  const s = document.createElement('summary');
  s.textContent = 'Built summary';
  const p = document.createElement('p');
  p.textContent = 'Built inside';
  d.append(s, p);
  document.getElementById('built-host').append(d);
});
document.getElementById('ask').addEventListener('click', () => {
  document.getElementById('log').textContent = [...document.querySelectorAll('details')]
    .map(d => String(d.open) + '/' + d.hasAttribute('open')).join(' ');
});
</script>
"##;

const MADE_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsMade"
    when: windowShown
SUPPORT

    function press(page, label) {
        const button = find(page, i => i.eldurTag === "button" && i.text === label)
        verify(button !== null, "no " + label)
        mouseClick(button)
        wait(100)
    }

    function test_made_disclosures() {
        const page = load(Qt.createComponent("Main.qml"))
        press(page, "Make")
        const madeSummary = textOf(page, "Made summary")
        const madeInside = textOf(page, "Made inside")
        const builtSummary = textOf(page, "Built summary")
        const builtInside = textOf(page, "Built inside")
        verify(madeSummary && madeInside && builtSummary && builtInside, "made lines")
        for (const s of [madeSummary, builtSummary]) {
            const marker = disclosure(s)
            verify(marker !== null, s.text + " has no marker")
            verify(marker.open, s.text + " shows the closed marker")
        }
        below(page, madeSummary, madeInside, 16, "made")
        below(page, madeInside, builtSummary, 16, "made")
        below(page, builtSummary, builtInside, 16, "made")
        press(page, "Ask")
        compare(textOf(page, "true").text, "true/true true/true")

        // Its summary closes it, as any disclosure's does.
        mouseClick(madeSummary, 60, madeSummary.height / 2)
        wait(100)
        verify(!madeInside.visible, "Made inside shows once closed")
        verify(!disclosure(madeSummary).open, "Made summary keeps the open marker")
        below(page, madeSummary, builtSummary, 0, "made closed")
        mouseClick(builtSummary, 60, builtSummary.height / 2)
        wait(100)
        verify(!builtInside.visible, "Built inside shows once closed")
        press(page, "Ask")
        compare(textOf(page, "false").text, "false/false false/false")
        mouseClick(madeSummary, 60, madeSummary.height / 2)
        wait(100)
        verify(madeInside.visible, "Made inside is hidden once open again")
        below(page, madeInside, builtSummary, 16, "made open again")
    }
}
}
"##;

#[test]
fn a_closed_details_brings_no_margin_of_what_it_folds() {
    run(CLOSED_PAGE, CLOSED_TEST, "eldur-details-closed");
}

#[test]
fn an_open_details_whose_last_block_a_rule_hides_brings_no_margin_of_it() {
    run(DIM_PAGE, DIM_TEST, "eldur-details-dim");
}

#[test]
fn a_details_a_script_makes_open_is_an_open_disclosure() {
    run(MADE_PAGE, MADE_TEST, "eldur-details-made");
}

fn run(page: &str, test: &str, name: &str) {
    if support::no_qt() {
        return;
    }
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = convert_str(page, &options);
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    let test = test.replace("SUPPORT", SUPPORT);
    std::fs::write(dir.join("tst_details.qml"), test).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_details.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("Warning:"), "{text}");
    assert!(!text.contains("QWARN"), "{text}");
}
