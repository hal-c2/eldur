//! A `<details>` takes part in block flow as Chromium lays it out. While it is
//! closed its folded blocks bring no margins: the box after it sits right under
//! the summary (#70), on a page with no script, and their margins come back
//! when it opens. A rule that hides the last block of an open one does the same.
//! A `<details open>` a script makes, by `innerHTML` or by `createElement` and
//! `setAttribute`, is an open disclosure like one the page wrote: its summary
//! shows the open marker, its content keeps its margins through the end of the
//! `<details>` and its host, and its summary closes and opens it (#63). What a
//! script moves into, out of or within a `<details>` shows as its own `hidden`
//! and the fold it is in say, on pages the runtime lays out too.
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
<div id="toggles">toggles 0</div>
<button id="make">Make</button>
<button id="ask">Ask</button>
<button id="unshut">Unshut</button>
<button id="shut">Shut</button>
<button id="lead">Lead</button>
<script>
let built = null;
let toggles = 0;
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
  d.addEventListener('toggle', () => {
    toggles += 1;
    document.getElementById('toggles').textContent = 'toggles ' + toggles;
  });
  document.getElementById('built-host').append(d);
  built = d;
});
// On a <details> in the document, the attribute is its state.
document.getElementById('unshut').addEventListener('click', () => built.setAttribute('open', ''));
document.getElementById('shut').addEventListener('click', () => built.removeAttribute('open'));
// A summary put first takes over; the one before it is folded content.
document.getElementById('lead').addEventListener('click', () => {
  const l = document.createElement('summary');
  l.textContent = 'Lead';
  built.prepend(l);
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

        // Chromium: written and removed, the attribute opens and closes it,
        // each a toggle (one as it was made open, one for the click).
        compare(textOf(page, "toggles 2").text, "toggles 2")
        press(page, "Unshut")
        verify(builtInside.visible, "Built inside is hidden once the attribute is written")
        verify(disclosure(builtSummary).open, "Built summary keeps the closed marker")
        press(page, "Ask")
        compare(textOf(page, "true/true true/true").text, "true/true true/true")
        compare(textOf(page, "toggles 3").text, "toggles 3")
        press(page, "Shut")
        verify(!builtInside.visible, "Built inside shows once the attribute is removed")
        verify(!disclosure(builtSummary).open, "Built summary keeps the open marker")
        press(page, "Ask")
        compare(textOf(page, "true/true false/false").text, "true/true false/false")
        compare(textOf(page, "toggles 4").text, "toggles 4")

        // Chromium folds the old summary once another is first, and the new
        // one toggles it.
        press(page, "Lead")
        const lead = textOf(page, "Lead")
        verify(lead && lead.visible, "Lead is not shown")
        verify(!builtSummary.visible, "the old summary shows in a closed details")
        mouseClick(lead, 20, lead.height / 2)
        wait(100)
        verify(builtSummary.visible && builtInside.visible, "Lead does not open it")
        compare(textOf(page, "toggles 5").text, "toggles 5")
        mouseClick(builtSummary, 60, builtSummary.height / 2)
        wait(100)
        verify(builtInside.visible, "the old summary still toggles it")
    }
}
}
"##;

// #74: on a page whose made elements follow its rules (a `:root[data-theme]`
// rule a script sets), a made `<summary>` still has its disclosure marker.
// Chromium puts its text 16.94 px in from the paragraph under it.
const MARKED_PAGE: &str = r##"<!doctype html>
<style>
:root { --fg: #c1c3c8; }
:root[data-theme='light'] { --fg: #23262f; }
body { color: var(--fg); font: 16px/18px sans-serif; }
</style>
<script>document.documentElement.dataset.theme = 'light';</script>
<div id="host"></div>
<button id="make">Make</button>
<script>
document.getElementById('make').addEventListener('click', () => {
  document.getElementById('host').innerHTML =
    '<details open><summary>Made summary</summary><p>Made inside</p></details>';
});
</script>
"##;

const MARKED_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsMarked"
    when: windowShown
SUPPORT

    function firstInk(page, image, text) {
        const at = text.mapToItem(page, 0, 0)
        const paper = image.pixel(image.width - 1, Math.round(at.y))
        for (let x = 0; x < image.width; ++x) {
            for (let y = Math.round(at.y); y < Math.round(at.y + text.height); ++y) {
                const p = image.pixel(x, y)
                if (Math.abs(p.r - paper.r) + Math.abs(p.g - paper.g) + Math.abs(p.b - paper.b) > 0.6)
                    return x
            }
        }
        return -1
    }

    function test_made_summary_has_its_marker() {
        const page = load(Qt.createComponent("Main.qml"))
        verify(page.eldurOpenElements === true, "the page does not style made elements")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Make")
        verify(button !== null, "no Make")
        mouseClick(button)
        wait(100)
        const summary = textOf(page, "Made summary")
        const inside = textOf(page, "Made inside")
        verify(summary !== null && inside !== null, "made lines")
        let line = summary
        let marker = null
        while (line && line !== page && marker === null) {
            marker = disclosure(line)
            line = line.parent
        }
        verify(marker !== null, "Made summary has no marker")
        verify(marker.open, "Made summary shows the closed marker")
        const left = inside.mapToItem(page, 0, 0).x
        const edge = marker.mapToItem(page, marker.width, 0).x - left
        verify(Math.abs(edge - 16.94) <= 1, "the marker ends " + edge + " px in, Chromium's text starts at 16.94")
        // Both lines start with "M": where their first ink is, without the
        // marker, is where their text starts, give or take a pixel.
        marker.visible = false
        waitForRendering(page)
        const image = grabImage(page)
        marker.visible = true
        const start = firstInk(page, image, summary) - firstInk(page, image, inside)
        verify(Math.abs(start - 16.94) <= 1, "Made summary's text starts " + start + " px in, Chromium 16.94")
        mouseClick(summary, summary.width / 2, summary.height / 2)
        wait(100)
        verify(!inside.visible, "Made inside shows once closed")
        verify(!marker.open, "the marker stays open once closed")
    }
}
}
"##;

// A script moving and taking out a `<details>`' children: each shows as
// Chromium shows it, as its own `hidden` says and as the fold it is now in says.
const CHANGE_PAGE: &str = r##"<!doctype html>
<style>body { font: 16px/18px sans-serif; }</style>
<details id="o" open><summary>Open summary</summary><p hidden>Kept hidden</p><p>Open body</p></details>
<details id="c"><summary>First summary</summary><summary>Second summary</summary><p id="mover">Mover</p></details>
<div id="away">Away host</div>
<button id="add">Add</button>
<button id="swap">Swap</button>
<button id="drop">Drop</button>
<button id="out">Out</button>
<button id="back">Back</button>
<button id="over">Over</button>
<script>
const o = document.getElementById('o');
const c = document.getElementById('c');
const mover = document.getElementById('mover');
document.getElementById('add').addEventListener('click', () => {
  const d = document.createElement('div');
  d.textContent = 'Added';
  o.append(d);
});
// The second summary put first: it is the toggle now, and the first is folded.
document.getElementById('swap').addEventListener('click', () => c.prepend(c.querySelectorAll('summary')[1]));
// The first summary taken out: the one left is the toggle.
document.getElementById('drop').addEventListener('click', () => c.querySelector('summary').remove());
document.getElementById('out').addEventListener('click', () => document.getElementById('away').append(mover));
document.getElementById('back').addEventListener('click', () => c.append(mover));
document.getElementById('over').addEventListener('click', () => o.append(mover));
</script>
"##;

const CHANGE_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsChanged"
    when: windowShown
SUPPORT

    function press(page, label) {
        const button = find(page, i => i.eldurTag === "button" && i.text === label)
        verify(button !== null, "no " + label)
        mouseClick(button)
        wait(100)
    }

    function test_changed_children() {
        const page = load(Qt.createComponent("Main.qml"))
        const kept = textOf(page, "Kept hidden")
        const first = textOf(page, "First summary")
        const second = textOf(page, "Second summary")
        const mover = textOf(page, "Mover")
        verify(kept && first && second && mover, "lines")
        verify(!kept.visible, "Kept hidden shows on load")
        verify(first.visible && !second.visible && !mover.visible, "closed details on load")

        press(page, "Add")
        verify(textOf(page, "Added").visible, "Added is hidden")
        verify(!kept.visible, "Kept hidden shows once something is appended")

        press(page, "Swap")
        verify(second.visible, "Second summary is hidden once first")
        verify(!first.visible, "First summary shows once it is no longer first")
        verify(!mover.visible, "Mover shows in the closed details")

        press(page, "Drop")
        verify(first.visible, "First summary is hidden once the other is gone")
        mouseClick(first, 60, first.height / 2)
        wait(100)
        verify(mover.visible, "First summary does not open it once first again")
        mouseClick(first, 60, first.height / 2)
        wait(100)
        verify(!mover.visible, "First summary does not close it")

        press(page, "Out")
        verify(mover.visible, "Mover stays hidden out of the closed details")
        press(page, "Back")
        verify(!mover.visible, "Mover shows back in the closed details")
        press(page, "Over")
        verify(mover.visible, "Mover stays hidden in the open details")
    }
}
}
"##;

// No rule the runtime reads: `hidden` lives in the item alone, and a block a
// script moves into an open `<details>` does not show its hidden sibling.
const KEPT_PAGE: &str = r##"<!doctype html>
<style>body { font: 16px/18px sans-serif; }</style>
<details id="o" open><summary>Open summary</summary><p hidden>Kept hidden</p><p>Open body</p></details>
<div id="extra">Added</div>
<button id="add">Add</button>
<script>
document.getElementById('add').addEventListener('click', () => {
  document.getElementById('o').append(document.getElementById('extra'));
});
</script>
"##;

const KEPT_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsKept"
    when: windowShown
SUPPORT

    function test_hidden_stays_hidden() {
        const page = load(Qt.createComponent("Main.qml"))
        verify(page.eldurCascadeRules === undefined, "the page has rules the runtime reads")
        const kept = textOf(page, "Kept hidden")
        const added = textOf(page, "Added")
        verify(kept && added, "lines")
        verify(!kept.visible, "Kept hidden shows on load")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Add")
        mouseClick(button)
        wait(100)
        verify(added.visible, "Added is hidden in the open details")
        verify(!kept.visible, "Kept hidden shows once something is appended")
    }
}
}
"##;

// A page whose created elements the runtime lays out (`eldurOpenElements`):
// its layout passes keep a closed `<details>`' content folded.
const LAID_PAGE: &str = r##"<!doctype html>
<style>
body { font: 16px/18px sans-serif; }
:root[data-theme='light'] .shy { display: none; }
.lifted { position: absolute; left: 0; top: 300px; }
</style>
<script>document.documentElement.dataset.theme = 'light';</script>
<p class="shy"><em>Shy</em></p>
<div id="host"></div>
<div id="after">After</div>
<button id="make">Make</button>
<button id="lift">Lift</button>
<script>
document.getElementById('make').addEventListener('click', () => {
  document.getElementById('host').innerHTML =
    '<details><summary>Shut summary</summary><p><em>Shut inside</em></p></details>';
});
// Out of flow, the summary is another item, and still the toggle.
document.getElementById('lift').addEventListener('click', () => {
  document.querySelector('summary').classList.add('lifted');
});
</script>
"##;

const LAID_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsLaidOut"
    when: windowShown
SUPPORT

    function test_closed_stays_folded() {
        const page = load(Qt.createComponent("Main.qml"))
        verify(page.eldurOpenElements === true, "the runtime does not lay this page out")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Make")
        mouseClick(button)
        wait(200)
        const summary = textOf(page, "Shut summary")
        const inside = textOf(page, "Shut inside")
        verify(summary && inside, "made lines")
        verify(summary.visible, "Shut summary is hidden")
        verify(!inside.visible, "Shut inside shows in a closed details after layout")
        mouseClick(summary, 40, summary.height / 2)
        wait(200)
        verify(inside.visible, "Shut inside is hidden once open")
        mouseClick(summary, 40, summary.height / 2)
        wait(200)
        verify(!inside.visible, "Shut inside shows once closed again")
        const lift = find(page, i => i.eldurTag === "button" && i.text === "Lift")
        mouseClick(lift)
        wait(200)
        const lifted = textOf(page, "Shut summary")
        verify(lifted && lifted.visible, "the lifted summary is hidden")
        mouseClick(lifted, 40, lifted.height / 2)
        wait(200)
        verify(textOf(page, "Shut inside").visible, "the lifted summary does not open it")
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

#[test]
fn a_made_summary_on_a_page_that_styles_made_elements_has_its_marker() {
    run(MARKED_PAGE, MARKED_TEST, "eldur-details-marked");
}

#[test]
fn a_details_children_a_script_moves_show_as_they_and_their_fold_say() {
    run(CHANGE_PAGE, CHANGE_TEST, "eldur-details-changed");
}

#[test]
fn a_hidden_child_stays_hidden_when_a_block_moves_into_its_open_details() {
    run(KEPT_PAGE, KEPT_TEST, "eldur-details-kept");
}

#[test]
fn a_closed_details_the_runtime_lays_out_keeps_its_content_folded() {
    run(LAID_PAGE, LAID_TEST, "eldur-details-laid");
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
