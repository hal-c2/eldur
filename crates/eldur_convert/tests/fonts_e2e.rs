//! Fonts as the running page has them, against Chromium (layout.md §1.3): the
//! face a family stack reaches, the height of a `line-height: normal` line, and
//! the row of the baseline, across families, sizes and weights.
//!
//! The expected values are in `tests/fonts/chromium.json`, recorded by
//! `node crates/eldur_convert/tests/fonts/gen.mjs`. They hold for the fonts of
//! the machine they were recorded on.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = include_str!("fonts/page.html");
const CHROMIUM: &str = include_str!("fonts/chromium.json");
/// What `tools/visual/paint/gen.mjs` measured for the fonts in `SHIFT_FONTS`.
const SHIFTS: &str = include_str!("../../../tools/visual/paint/shifts.txt");

const TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_fonts.js" as EldurFonts

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "Fonts"
    when: windowShown

    readonly property var chromium: (@CHROMIUM@)
    property var page

    function initTestCase() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = component.createObject(host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function texts(item, out, wanted = "Hx") {
        if (item.text === wanted)
            out.push(item)
        for (let i = 0; i < item.children.length; i++)
            texts(item.children[i], out, wanted)
        return out
    }

    // Line height, baseline row and where the line is on the page. Chromium's
    // boxes are in 1/64 px units, ours are the CSS values; a `Text` of fractional
    // height has the whole height Qt rounds it up to, and is carried
    // (layout.md §1.1).
    function test_lines() {
        const all = texts(page, [])
        compare(all.length, chromium.lines.length, "rows")
        const wrong = []
        chromium.lines.forEach((c, i) => {
            const text = all[i]
            const got = text.height
            const base = text.baselineOffset + (text.transform.length > 0 ? text.transform[0].y : 0)
            const y = text.parent.mapToItem(page, text.x, text.y).y
            if (got !== Math.ceil(c.height - 1 / 32) || Math.round(base) !== c.baseline || Math.abs(y - c.top) > 0.5) {
                wrong.push(c.id + " " + c.family + " " + c.size + "px " + c.weight + (c.italic ? " italic" : "")
                    + (c.line !== null ? " /" + c.line : "")
                    + ": height " + got + " want " + c.height + ", baseline " + base + " want " + c.baseline
                    + ", y " + y + " want " + c.top)
            }
        })
        compare(wrong.join("\n"), "")
    }

    // Text a flex container places sits on Chromium's row, line and all.
    function test_placed() {
        const wrong = []
        for (const c of chromium.placed) {
            const text = texts(page, [], c.text)[0]
            const shift = text.transform.length > 0 ? text.transform[0].y : 0
            // Qt paints the item on a whole row and the glyphs a whole ascent below it.
            const top = text.parent.mapToItem(page, text.x, text.y).y + shift
            const base = Math.round(top) + Math.round(text.baselineOffset)
            if (base !== Math.round(c.baseline))
                wrong.push(c.text + ": baseline " + base + " want " + c.baseline)
        }
        compare(wrong.join("\n"), "")
    }

    // The face Qt loaded for each stack is the one Chromium used.
    function test_stacks() {
        const wrong = []
        for (const c of chromium.stacks) {
            const name = EldurFonts.family(c.stack)
            // Chromium reports the font's full name; Qt its family.
            const want = c.faces[0] === "JetBrainsMono NF" ? "JetBrainsMono Nerd Font" : c.faces[0]
            if (name !== want)
                wrong.push("`" + c.stack + "`: " + name + " want " + want)
        }
        compare(wrong.join("\n"), "")
    }

    // `.a` .. `.f` of tools/visual/paint: Chromium's baseline below the line's
    // top less the row Qt paints a fixed line's on.
    function test_measured_shifts() {
        const fonts = {
            a: ["sans-serif", 15, 400, 22.5], b: ["sans-serif", 17, 400, 25.5],
            c: ["sans-serif", 28, 700, 42], d: ["serif", 16, 400, 24],
            e: ["sans-serif", 16, 400, 17.6], f: ["sans-serif", 13, 400, 13],
        }
        const measured = @SHIFTS@.split("\n").filter(l => l !== "" && !l.startsWith("#"))
        compare(measured.length, 6)
        for (const line of measured) {
            const fields = line.split(" ")
            const f = fonts[fields[0]]
            compare(EldurFonts.shift(f[0], f[1], f[2], false, f[3]), Number(fields[3]), line)
        }
    }

    // The table is what Chromium measures.
    function test_table() {
        const wrong = []
        for (const f of chromium.faces) {
            const e = EldurFonts.extents({ family: f.family, pixelSize: 1000, weight: f.weight, italic: f.italic })
            if (e.ascent !== f.ascent || e.descent !== f.descent || e.gap !== f.gap) {
                wrong.push(f.family + " " + f.weight + (f.italic ? " italic" : "") + ": "
                    + [e.ascent, e.descent, e.gap] + " want " + [f.ascent, f.descent, f.gap])
            }
        }
        compare(wrong.join("\n"), "")
    }
}
}
"##;

#[test]
fn fonts_match_chromium_at_run_time() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "fonts/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-fonts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_fonts.qml"),
        TEST.replace("@CHROMIUM@", CHROMIUM)
            .replace("@SHIFTS@", &format!("{SHIFTS:?}")),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_fonts.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("Warning:"), "{text}");
}
