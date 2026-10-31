//! A control with a percentage width fills up to that percentage of its
//! container rather than binding a preferred width to it, so it neither
//! widens the column it sits in nor stays wider than a fixed-width column.
//! Widths are Chromium's `getBoundingClientRect()` for the same page.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!DOCTYPE html><html><head><style>
body{margin:0;font-family:sans-serif;font-size:16px}
main{display:grid;grid-template-columns:1fr 1fr;gap:48px;width:800px}
form{display:grid;grid-template-columns:1fr 120px;gap:12px;padding:20px}
label{display:block}
select,input{width:100%;padding:8px 10px;border:1px solid #c5ccd3;font:inherit}
button{grid-column:1 / 3}
#half{width:50%;box-sizing:border-box}
</style></head><body>
<main><div id="left">Gallery</div>
<form id="buy"><div><label>Colour</label><select id="colour"><option>Fjord blue</option></select></div>
<div><label>Qty</label><input id="qty" type="number" value="1"></div>
<button>Add to cart</button></form></main>
<div style="width:300px"><input id="half" type="text"></div>
<script>
var named = [document.getElementById('left'),
  document.getElementById('buy'),
  document.getElementById('colour'),
  document.getElementById('qty'),
  document.getElementById('half')];
</script></body></html>"##;

const TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "PercentWidth"
    when: windowShown

    property var page
    function find(name) { return findChild(page, name) }

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    function leftOf(name) { return find(name).mapToItem(page, 0, 0).x }

    // The controls must not feed their widths back into the grid columns.
    function test_the_grid_columns_keep_their_shares() {
        fuzzyCompare(find("left").width, 376, 1)
        fuzzyCompare(leftOf("buy"), 424, 1)
        fuzzyCompare(find("buy").width, 376, 1)
    }

    // A select is border-box in Chromium.
    function test_a_full_width_select_fills_the_flexible_column() {
        fuzzyCompare(leftOf("colour"), 444, 1)
        fuzzyCompare(find("colour").width, 204, 1)
    }

    // A content-box input is the 120px column plus its padding and border.
    function test_a_full_width_input_takes_the_fixed_column() {
        fuzzyCompare(leftOf("qty"), 660, 1)
        fuzzyCompare(find("qty").width, 142, 1)
    }

    function test_a_half_width_field_is_half_its_container() {
        fuzzyCompare(find("half").width, 150, 1)
    }
}
}
"##;

fn convert() -> eldur_convert::Output {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    )
}

#[test]
fn percentage_width_controls_measure_as_in_chromium() {
    let out = convert();
    let dir = std::env::temp_dir().join(format!("eldur-percent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_percent.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_percent.qml"))
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
}
