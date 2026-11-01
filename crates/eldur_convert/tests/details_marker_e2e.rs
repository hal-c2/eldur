//! A `<details>` draws Chromium's disclosure triangle: right when closed, down
//! when open, in the summary's colour, taking the room it takes there, on a page
//! with no script, a closed page and an open one. `details_marker/gen.mjs` records
//! Chromium's ink for the same page and the same presses on each marker.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = include_str!("details_marker/page.html");
const CHROMIUM: &str = include_str!("details_marker/chromium.json");

const TEST: &str = r###"
import QtQuick
import QtTest

Item {
    id: host
    width: 800
    height: 600

TestCase {
    name: "DetailsMarker"
    when: windowShown

    readonly property var chromium: CHROMIUM_JSON

    function ink(image, x, y) {
        const c = image.pixel(x, y)
        return Math.min(c.r, c.g, c.b) < 0.5
    }

    // Same rows inked, each from and to the same column give or take the one
    // edge pixel that one rasteriser has just under the threshold and the other
    // just over; the same goes for where text starts.
    function sameRow(got, want) {
        const a = got.indexOf("#")
        const b = want.indexOf("#")
        return (a < 0) === (b < 0) && Math.abs(a - b) <= 1
            && Math.abs(got.lastIndexOf("#") - want.lastIndexOf("#")) <= 1
    }

    function sameState(state, image) {
        let wrong = []
        for (const s of chromium.summaries) {
            const want = chromium.states[state][s.id]
            const x = Math.round(s.x)
            const y = Math.round(s.y)
            const rows = []
            for (let r = 0; r < s.height; ++r) {
                let row = ""
                for (let col = 0; col < s.side; ++col)
                    row += ink(image, x + col, y + r) ? "#" : "."
                rows.push(row)
            }
            if (!rows.every((row, r) => sameRow(row, want.rows[r])))
                wrong.push(state + " " + s.id + "\n got  " + rows.join(" ") + "\n want " + want.rows.join(" "))
            const starts = []
            for (let r = 0; r < s.rows; ++r) {
                let first = -1
                for (let col = r < s.height ? s.side : 0; col < 120; ++col) {
                    if (ink(image, x + col, y + r)) {
                        first = col
                        break
                    }
                }
                starts.push(first)
            }
            const near = starts.every((v, i) => (v < 0) === (want.starts[i] < 0) && Math.abs(v - want.starts[i]) <= 1)
            if (!near)
                wrong.push(state + " " + s.id + " text\n got  " + starts.join(",") + "\n want " + want.starts.join(","))
            if (want.paint) {
                const c = image.pixel(x + want.paint[0], y + want.paint[1])
                const same = Math.abs(c.r * 255 - want.paint[2]) < 30 && Math.abs(c.g * 255 - want.paint[3]) < 30
                    && Math.abs(c.b * 255 - want.paint[4]) < 30
                if (!same)
                    wrong.push(state + " " + s.id + " paint " + c + " want " + want.paint)
            }
        }
        return wrong
    }

    function press(page) {
        for (const s of chromium.summaries) {
            mouseClick(page, s.click[0], s.click[1])
            wait(0)
        }
        mouseMove(page, 790, 590)
        wait(100)
    }

    function test_markers() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 800, height: 600 })
        waitForRendering(page)
        wait(100)
        let wrong = sameState("initial", grabImage(page))
        press(page)
        wrong = wrong.concat(sameState("pressed", grabImage(page)))
        press(page)
        wrong = wrong.concat(sameState("again", grabImage(page)))
        // One long message would be cut.
        for (const line of wrong)
            console.warn(line)
        compare(wrong.length, 0, "what differs is above")
    }
}
}
"###;

fn run(name: &str, script: &str) {
    let html = PAGE.replace("</body>", &format!("{script}</body>"));
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html,
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-marker-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_marker.qml"),
        TEST.replace("CHROMIUM_JSON", CHROMIUM),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_marker.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{name}: {text}");
}

#[test]
fn markers_on_a_page_with_no_script() {
    run("none", "");
}

#[test]
fn markers_on_a_closed_page() {
    run(
        "closed",
        "<script>var one=document.getElementById('s16');</script>",
    );
}

#[test]
fn markers_on_an_open_page() {
    run(
        "open",
        "<script>document.querySelectorAll('details').forEach(function(d){d.addEventListener('toggle',function(){});});</script>",
    );
}

/// Measured: only a `<details>`' first summary is a list item.
#[test]
fn only_the_first_summary_of_a_details_has_a_marker() {
    let out = eldur_convert::convert(
        &Input {
            html: "<!DOCTYPE html><summary>lone</summary>\
                   <details open><summary>one</summary><summary>two</summary></details>"
                .to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &Options::default(),
        &mut NoResources,
    );
    let main = out.main_qml().unwrap_or_default();
    assert_eq!(main.matches("EldurDisclosure {").count(), 1, "{main}");
}
