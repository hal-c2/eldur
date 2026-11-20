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

    // `b` is `gap` px below the bottom of `a`'s line. Layout runs after the
    // event that changed it, later on a loaded machine: it gets two seconds.
    function below(page, a, b, gap, step) {
        const gapNow = () => pageY(page, b) - pageY(page, a) - a.height
        for (let t = 0; t < 40 && Math.round(gapNow()) !== gap; ++t)
            wait(50)
        const got = gapNow()
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
        tryVerify(() => ["Made summary", "Made inside", "Built summary", "Built inside"]
                  .every(w => textOf(page, w) !== null), 2000, "made lines")
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
        tryVerify(() => textOf(page, "Made summary") !== null && textOf(page, "Made inside") !== null,
                  2000, "made lines")
        waitForRendering(page)
        const summary = textOf(page, "Made summary")
        const inside = textOf(page, "Made inside")
        let line = summary
        let marker = null
        while (line && line !== page && marker === null) {
            marker = disclosure(line)
            line = line.parent
        }
        verify(marker !== null, "Made summary has no marker")
        verify(marker.open, "Made summary shows the closed marker")
        const edgeNow = () => marker.mapToItem(page, marker.width, 0).x - inside.mapToItem(page, 0, 0).x
        for (let t = 0; t < 40 && Math.abs(edgeNow() - 16.94) > 1; ++t)
            wait(50)
        const edge = edgeNow()
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

// What a script makes in a `<details>` on a page that styles made elements
// folds as the rest does: a text run after the summary folds, and a summary
// whose text is a heading's shows its marker.
const FOLDS_PAGE: &str = r##"<!doctype html>
<style>
:root { --fg: #c1c3c8; }
:root[data-theme='light'] { --fg: #23262f; }
body { color: var(--fg); font: 16px/18px sans-serif; }
h3 { margin: 0; font: 16px sans-serif; }
</style>
<script>document.documentElement.dataset.theme = 'light';</script>
<div id="host"></div>
<button id="make">Make</button>
<script>
document.getElementById('make').addEventListener('click', () => {
  document.getElementById('host').innerHTML =
    '<details id="t"><summary>Text sum</summary></details>'
    + '<details><summary><h3>Head sum</h3></summary><p>Head inside</p></details>';
  document.getElementById('t').appendChild(document.createTextNode('Body run'));
});
</script>
"##;

const FOLDS_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsFolds"
    when: windowShown
SUPPORT

    function shows(page, item) {
        for (let i = item; i && i !== page; i = i.parent) {
            if (!i.visible || i.opacity === 0)
                return false
        }
        return true
    }

    function click(page, label) {
        const button = find(page, i => i.eldurTag === "button" && i.text === label)
        verify(button !== null, "no " + label)
        mouseClick(button)
    }

    function test_made_folds() {
        const page = load(Qt.createComponent("Main.qml"))
        verify(page.eldurOpenElements === true, "the page does not style made elements")
        click(page, "Make")
        tryVerify(() => textOf(page, "Text sum") !== null, 2000, "made lines")
        waitForRendering(page)
        // A made element's form is made again once its rules apply: each press
        // finds the summary as it is then.
        const press = () => {
            const sum = textOf(page, "Text sum")
            mouseClick(sum, sum.width / 2, sum.height / 2)
        }
        tryVerify(() => { const b = textOf(page, "Body run"); return b === null || !shows(page, b) },
                  2000, "Body run shows while its details is closed")
        press()
        tryVerify(() => { const b = textOf(page, "Body run"); return b !== null && shows(page, b) },
                  2000, "Body run stays hidden once its details opens")
        press()
        tryVerify(() => { const b = textOf(page, "Body run"); return b === null || !shows(page, b) },
                  2000, "Body run shows once its details closes again")

        let line = textOf(page, "Head sum")
        let marker = null
        while (line && line !== page && marker === null) {
            marker = disclosure(line)
            line = line.parent
        }
        verify(marker !== null, "Head sum has no marker")
        verify(shows(page, marker), "Head sum's marker is hidden")
    }
}
}
"##;

// A made summary is activated as Chromium does it: a button inside it takes
// the click itself, a summary a rule makes a block has no marker, and the
// value a script gives `open` stays.
const ACTS_PAGE: &str = r##"<!doctype html>
<style>
:root { --fg: #c1c3c8; }
:root[data-theme='light'] { --fg: #23262f; }
body { color: var(--fg); font: 16px/18px sans-serif; }
#b > summary { display: block; }
</style>
<script>document.documentElement.dataset.theme = 'light';</script>
<div id="host"></div>
<p id="out">Out none</p>
<button id="make">Make</button>
<button id="attr">Attr</button>
<script>
const out = document.getElementById('out');
document.getElementById('make').addEventListener('click', () => {
  document.getElementById('host').innerHTML =
    '<details id="a"><summary>Ctl sum <button id="inner">Inner</button></summary><p>Ctl body</p></details>'
    + '<details id="b"><summary>Block sum</summary><p>Block body</p></details>'
    + '<details id="c"><summary>Attr sum</summary><p>Attr body</p></details>';
  document.getElementById('inner').addEventListener('click', () => { out.textContent = 'Out inner'; });
});
document.getElementById('attr').addEventListener('click', () => {
  const c = document.getElementById('c');
  c.setAttribute('open', 'open');
  out.textContent = 'Out ' + c.getAttribute('open') + ' ' + c.open;
});
</script>
"##;

const ACTS_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsActs"
    when: windowShown
SUPPORT

    function shows(page, item) {
        for (let i = item; i && i !== page; i = i.parent) {
            if (!i.visible || i.opacity === 0)
                return false
        }
        return true
    }

    function button(page, label) {
        return find(page, i => i.eldurTag === "button" && i.text === label)
    }

    function hidden(page, words) {
        const t = textOf(page, words)
        return t === null || !shows(page, t)
    }

    // The marker in the summary the words are in: up to its details, which
    // holds the body too, and no further.
    function markerOf(page, words) {
        let marker = null
        for (let i = textOf(page, words); i && i !== page && marker === null; i = i.parent) {
            if (textOf(i, " body") !== null)
                break
            marker = disclosure(i)
        }
        return marker
    }

    function test_made_activation() {
        const page = load(Qt.createComponent("Main.qml"))
        verify(page.eldurOpenElements === true, "the page does not style made elements")
        mouseClick(button(page, "Make"))
        tryVerify(() => textOf(page, "Block sum") !== null && button(page, "Inner") !== null,
                  2000, "made lines")
        waitForRendering(page)

        const inner = button(page, "Inner")
        mouseClick(inner)
        tryVerify(() => textOf(page, "Out inner") !== null, 2000, "the inner button did not hear its click")
        wait(100)
        verify(hidden(page, "Ctl body"), "a click on a button in the summary opened its details")

        verify(markerOf(page, "Attr sum") !== null, "a list-item summary has no marker")
        verify(markerOf(page, "Block sum") === null, "a summary made a block has a marker")

        mouseClick(button(page, "Attr"))
        tryVerify(() => textOf(page, "Out open true") !== null, 2000,
                  "the open attribute lost its value: " + (textOf(page, "Out ") || {}).text)
        tryVerify(() => !hidden(page, "Attr body"), 2000, "setting open did not open it")
    }
}
}
"##;

// A `<details>` a script closes while an ancestor hides it stays closed once
// the ancestor shows again, and opens as before.
const UNDER_PAGE: &str = r##"<!doctype html>
<style>
body { font: 16px/18px sans-serif; }
:root[data-theme='dim'] .fold { display: none; }
</style>
<div id="wrap"><details id="w" open><summary>Wrap sum</summary><p>Wrap inside</p><p class="fold">Wrap dim</p></details></div>
<button id="dim">Dim</button>
<button id="hide">Hide</button>
<button id="close">Close</button>
<button id="show">Show</button>
<button id="open">Open</button>
<script>
const wrap = document.getElementById('wrap');
const w = document.getElementById('w');
document.getElementById('dim').addEventListener('click', () => {
  const r = document.documentElement;
  r.dataset.theme = r.dataset.theme === 'dim' ? '' : 'dim';
});
document.getElementById('hide').addEventListener('click', () => { wrap.style.display = 'none'; });
document.getElementById('close').addEventListener('click', () => { w.open = false; });
document.getElementById('show').addEventListener('click', () => { wrap.style.display = ''; });
document.getElementById('open').addEventListener('click', () => { w.open = true; });
</script>
"##;

const UNDER_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsUnder"
    when: windowShown
SUPPORT

    function shows(page, item) {
        for (let i = item; i && i !== page; i = i.parent) {
            if (!i.visible)
                return false
        }
        return true
    }

    function click(page, label) {
        const button = find(page, i => i.eldurTag === "button" && i.text === label)
        verify(button !== null, "no " + label)
        mouseClick(button)
    }

    function test_closed_under_a_hidden_ancestor() {
        const page = load(Qt.createComponent("Main.qml"))
        const sum = textOf(page, "Wrap sum")
        const inside = textOf(page, "Wrap inside")
        verify(sum !== null && inside !== null, "the details' lines")
        verify(shows(page, inside), "Wrap inside is hidden while open")
        click(page, "Hide")
        tryVerify(() => !shows(page, sum), 2000, "Wrap sum shows once its ancestor is hidden")
        click(page, "Close")
        click(page, "Show")
        tryVerify(() => shows(page, sum), 2000, "Wrap sum stays hidden once its ancestor shows again")
        verify(!shows(page, inside), "Wrap inside shows though its details was closed while hidden")
        click(page, "Open")
        tryVerify(() => shows(page, inside), 2000, "Wrap inside stays hidden once its details opens")
    }

    // The fold writes only what it hides: a block its own rule hides keeps
    // the rule's binding, and shows again once the rule lets it go.
    function test_a_block_its_rule_hides_keeps_the_rule_under_a_hidden_ancestor() {
        const page = load(Qt.createComponent("Main.qml"))
        const sum = textOf(page, "Wrap sum")
        const dim = textOf(page, "Wrap dim")
        verify(sum !== null && dim !== null, "the details' lines")
        verify(shows(page, dim), "Wrap dim is hidden while open")
        click(page, "Dim")
        tryVerify(() => !shows(page, dim), 2000, "Wrap dim shows under dim")
        click(page, "Hide")
        tryVerify(() => !shows(page, sum), 2000, "Wrap sum shows once its ancestor is hidden")
        click(page, "Close")
        click(page, "Dim")
        click(page, "Show")
        tryVerify(() => shows(page, sum), 2000, "Wrap sum stays hidden once its ancestor shows again")
        verify(!shows(page, dim), "Wrap dim shows though its details is closed")
        click(page, "Open")
        tryVerify(() => shows(page, dim), 2000, "Wrap dim stays hidden once its details opens and no rule hides it")
    }
}
}
"##;

// Two made `<div>`s share a template: one gets text, one holds a block.
// The text one keeps its lines (#63's shared-key rule must not pick a probe
// that has none).
const SHARED_PAGE: &str = r##"<!doctype html>
<style>body { font: 16px/18px sans-serif; } p { margin: 0; }
#list > div > p { background-color: #f00; }</style>
<div id="list"></div>
<button id="make">Make</button>
<script>
document.getElementById('make').addEventListener('click', () => {
  const a = document.createElement('div');
  a.textContent = 'Alpha text';
  const b = document.createElement('div');
  const p = document.createElement('p');
  p.textContent = 'Beta inside';
  b.append(p);
  document.getElementById('list').append(a, b);
});
</script>
"##;

const SHARED_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsShared"
    when: windowShown
SUPPORT

    function test_text_and_block_share_a_template() {
        const page = load(Qt.createComponent("Main.qml"))
        const button = find(page, i => i.eldurTag === "button" && i.text === "Make")
        verify(button !== null, "no Make")
        mouseClick(button)
        wait(100)
        const alpha = textOf(page, "Alpha text")
        const beta = textOf(page, "Beta inside")
        verify(alpha !== null, "the text-only div lost its text")
        verify(beta !== null, "the block inside the other div is missing")
        verify(alpha.visible && beta.visible, "made lines are hidden")
        below(page, alpha, beta, 0, "shared")
        // The paragraph's rule names its parent: it is found under the
        // template the parent's key went to.
        let box = beta
        while (box && box !== page && !(box.eldurTag === "p" && box.color !== undefined))
            box = box.parent
        verify(box && box.eldurTag === "p", "Beta inside has no paragraph box")
        verify(Qt.colorEqual(box.color, "#ff0000"), "Beta's paragraph is " + box.color + ", Chromium red")
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

// Margins an open `<details>` passes on as it opens and closes, where they
// meet a row of floats, a bordered box's bottom past a block a rule hides,
// and the end of a `<details>` around it.
const PASSED_PAGE: &str = r##"<!doctype html>
<style>
body { font: 16px/18px sans-serif; }
:root[data-theme='dim'] .fold { display: none; }
.f { float: left; }
.box { border-bottom: 1px solid black; }
</style>
<details><summary>Float summary</summary><p>Float para</p></details>
<div class="f">Float one</div><div class="f">Float two</div>
<div style="clear: both"></div>
<div class="box"><details open><summary>Box summary</summary><p>Box para</p></details><p class="fold">Box tail</p></div>
<div>Box next</div>
<details open><summary>Outer summary</summary><details open><summary>Nested summary</summary><p>Nested para</p></details></details>
<div>Nested next</div>
<details><summary>Wrap summary</summary><p>Wrap para</p></details>
<div class="f">Wrap float</div><p>Wrap text</p>
<div style="clear: both"></div>
<div class="f">Tail float</div><details open><summary>Tail summary</summary><p>Tail para</p></details>
<div style="clear: both">Tail after</div>
<button id="t">Dim</button>
<script>
document.getElementById('t').addEventListener('click', () => {
  const r = document.documentElement;
  r.dataset.theme = r.dataset.theme === 'dim' ? '' : 'dim';
});
</script>
"##;

const PASSED_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsPassed"
    when: windowShown
SUPPORT

    function click(page, words) {
        const line = textOf(page, words)
        mouseClick(line, 60, line.height / 2)
        wait(100)
    }

    function test_passed_margins() {
        const page = load(Qt.createComponent("Main.qml"))
        const floatSummary = textOf(page, "Float summary")
        const floatPara = textOf(page, "Float para")
        const floatOne = textOf(page, "Float one")
        verify(floatSummary && floatPara && floatOne, "float lines")
        // Chromium: closed, the floats are right under the summary; open,
        // past the paragraph's 16 px.
        below(page, floatSummary, floatOne, 0, "floats, closed")
        click(page, "Float summary")
        below(page, floatPara, floatOne, 16, "floats, open")
        click(page, "Float summary")
        below(page, floatSummary, floatOne, 0, "floats, closed again")

        // Chromium: with the tail hidden and the details closed, the border
        // (1 px) is all between the summary and the next block.
        const boxSummary = textOf(page, "Box summary")
        const boxNext = textOf(page, "Box next")
        const button = find(page, i => i.eldurTag === "button" && i.text === "Dim")
        mouseClick(button)
        wait(100)
        verify(!textOf(page, "Box tail").visible, "Box tail shows under dim")
        below(page, textOf(page, "Box para"), boxNext, 17, "box, open, dim")
        click(page, "Box summary")
        below(page, boxSummary, boxNext, 1, "box, closed, dim")

        // Chromium: the nested one closed, the next block is right under
        // its summary; open, past the paragraph's 16 px.
        const nestedSummary = textOf(page, "Nested summary")
        const nestedPara = textOf(page, "Nested para")
        const nestedNext = textOf(page, "Nested next")
        below(page, nestedPara, nestedNext, 16, "nested, open")
        click(page, "Nested summary")
        below(page, nestedSummary, nestedNext, 0, "nested, closed")

        // Chromium: closed, the float is right under the summary and the
        // text beside it 16 px lower; open, both are past the paragraph's
        // 16 px, the float's margin before it as the text's.
        const wrapSummary = textOf(page, "Wrap summary")
        const wrapFloat = textOf(page, "Wrap float")
        const wrapText = textOf(page, "Wrap text")
        verify(wrapSummary && wrapFloat && wrapText, "wrap lines")
        below(page, wrapSummary, wrapFloat, 0, "wrapped float, closed")
        below(page, wrapSummary, wrapText, 16, "wrapped float, closed")
        click(page, "Wrap summary")
        below(page, textOf(page, "Wrap para"), wrapFloat, 16, "wrapped float, open")
        below(page, textOf(page, "Wrap para"), wrapText, 16, "wrapped float, open")

        // Chromium: a block clearing a float the details was wrapped around
        // is past the paragraph's 16 px while it is open, and right under
        // the summary once it closes.
        const tailSummary = textOf(page, "Tail summary")
        const tailAfter = textOf(page, "Tail after")
        verify(tailSummary && tailAfter, "tail lines")
        below(page, textOf(page, "Tail para"), tailAfter, 16, "cleared float, open")
        click(page, "Tail summary")
        below(page, tailSummary, tailAfter, 0, "cleared float, closed")
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
fn what_a_script_makes_in_a_details_folds_as_the_rest() {
    run(FOLDS_PAGE, FOLDS_TEST, "eldur-details-folds");
}

#[test]
fn a_details_closed_under_a_hidden_ancestor_stays_closed_when_it_shows() {
    run(UNDER_PAGE, UNDER_TEST, "eldur-details-under");
}

#[test]
fn a_made_summary_is_activated_and_marked_as_chromium_does_it() {
    run(ACTS_PAGE, ACTS_TEST, "eldur-details-acts");
}

#[test]
fn made_elements_sharing_a_template_keep_their_text_and_blocks() {
    run(SHARED_PAGE, SHARED_TEST, "eldur-details-shared");
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
fn margins_a_details_passes_on_meet_floats_borders_and_an_outer_details() {
    run(PASSED_PAGE, PASSED_TEST, "eldur-details-passed");
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
