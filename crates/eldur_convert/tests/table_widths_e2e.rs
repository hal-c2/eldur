//! Auto table layout (CSS 2.1 §17.5.2.2, css-tables-3 §3.9; layout.md §8):
//! column widths as Chromium gives them, for tables of one to five columns
//! with and without widths in px and %, spans, spacing, padding, and
//! collapsed borders, whose outer halves are the table's and not its
//! columns'.
//!
//! The page is `tests/table_widths/page.html`; the expected boxes are
//! Chromium's, recorded in `tests/table_widths/chromium.json` by
//! `node crates/eldur_convert/tests/table_widths/gen.mjs`.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = include_str!("table_widths/page.html");
const CHROMIUM: &str = include_str!("table_widths/chromium.json");

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "TableWidths"
    when: windowShown

    readonly property var chromium: (@CHROMIUM@)
    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function descendants(item, out) {
        out.push(item)
        for (let i = 0; i < item.children.length; i++)
            descendants(item.children[i], out)
        return out
    }
    // A static table is a grid of cells with slots; a live one, which a
    // script can reach, holds rows that hold its cells, and has an object
    // name.
    function measuredCells(t, found) {
        if (!@LIVE@) {
            const grid = found[t]
            const last = Math.max.apply(null, Array.from(grid.children)
                .filter(c => c.tableSlot !== undefined).map(c => c.tableSlot[0]))
            return Array.from(grid.children)
                .filter(c => c.tableSlot !== undefined && c.tableSlot[0] === last)
                .sort((a, b) => a.tableSlot[1] - b.tableSlot[1])
        }
        const table = findChild(page, chromium[t].id)
        verify(table !== null, "no table " + chromium[t].id)
        const row = descendants(table, []).find(i => i.eldurTag === "tr"
            && (i.eldurClasses || "").split(" ").indexOf("m") >= 0)
        return Array.from(row.children).filter(c => c.eldurTag === "td")
            .sort((a, b) => a.x - b.x)
    }
    function grids(item, out) {
        if (item.columnSpecs !== undefined && item.columnWidths !== undefined)
            out.push(item)
        for (let i = 0; i < item.children.length; i++)
            grids(item.children[i], out)
        return out
    }
    function pageX(item) { return item.parent.mapToItem(page, item.x, 0).x }

    // Every column edge within a pixel of Chromium's: its columns are in
    // 1/64 px, ours whole pixels, and Qt's text is a little narrower or
    // wider. A collapsed table's outer border halves are its own, not its
    // cells': ours draw them, so their outer sides are the table's.
    function test_columns() {
        const found = grids(page, [])
        compare(found.length, chromium.length, "tables")
        const wrong = []
        for (let t = 0; t < chromium.length; t++) {
            const want = chromium[t]
            const cells = measuredCells(t, found)
            if (cells.length !== want.cells.length) {
                wrong.push(want.id + ": " + cells.length + " cells, not " + want.cells.length)
                continue
            }
            const n = cells.length
            const odd = want.collapse ? (want.inner / 2 - Math.floor(want.inner / 2)) : 0
            for (let c = 0; c < n; c++) {
                const w = want.cells[c]
                const left = want.collapse && c === 0 ? want.table.x : w.x - (c > 0 ? odd : 0)
                const right = want.collapse && c === n - 1
                    ? want.table.x + want.table.width : w.x + w.width - (c < n - 1 ? odd : 0)
                // A column a fraction narrower than its text would wrap it.
                // Collapsed tables here have one row, whose cells draw the
                // outer borders whole: the table's height.
                const height = want.collapse ? want.table.height : w.height
                if (Math.abs(cells[c].height - height) > 1)
                    wrong.push(want.id + " cell " + c + " height: " + cells[c].height + " is not " + height)
                const x = pageX(cells[c])
                const got = [x, x + cells[c].width]
                for (const [side, g, e, before] of [["left", got[0], left, c], ["right", got[1], right, c + 1]]) {
                    // At max-content each column is rounded up, so its text
                    // does not wrap: an edge is up to a pixel later for each
                    // column before it.
                    const late = want.auto ? before : 0
                    if (g - e > 1 + late || e - g > 1)
                        wrong.push(want.id + " cell " + c + " " + side + ": " + g + " is not " + e)
                }
            }
        }
        verify(wrong.length === 0, "\n" + wrong.join("\n"))
    }
}
}
"##;

/// The tables are live, as on a page whose script looks for tables: the
/// runtime places their cells with the same solver.
const SCRIPT: &str = r#"<script>
document.querySelectorAll("table").forEach(function (t) { t.dataset.seen = "1"; });
</script>
</body>"#;

#[test]
fn auto_table_columns_have_chromium_widths() {
    columns_match(PAGE, false, "static");
}

#[test]
fn live_table_columns_have_chromium_widths() {
    columns_match(&PAGE.replace("</body>", SCRIPT), true, "live");
}

fn columns_match(page: &str, live: bool, name: &str) {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: page.to_owned(),
            url: "table_widths/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir =
        std::env::temp_dir().join(format!("eldur-table-widths-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(
        dir.join("tst_table_widths.qml"),
        TEST.replace("@CHROMIUM@", CHROMIUM)
            .replace("@LIVE@", if live { "true" } else { "false" }),
    )
    .expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_table_widths.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
    assert!(!text.contains("eldur:"), "{text}");
    assert!(!text.contains("Warning:"), "{text}");
}
