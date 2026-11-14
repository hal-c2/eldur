//! An element whose item a settle swaps, time after time, keeps answering reads
//! made through what a script still holds, and the runtime keeps nothing of
//! the items it swapped out.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r##"<!DOCTYPE html><html><head><style>
body{margin:8px;font:14px Arial}
.out{position:absolute;left:30px;top:40px}
</style></head><body><div id="host"></div><script>
var d = document.createElement("div");
d.id = "made";
d.textContent = "made";
document.getElementById("host").appendChild(d);
document.getElementById("host").onclick = function() { d.classList.toggle("out"); };
</script></body></html>"##;

const TEST: &str = r##"
import QtQuick
import QtTest
import "eldur_runtime.js" as Eldur

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "ReplacedChurn"
    when: windowShown

    property var page

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
    }

    // Leaving the flow and coming back each swap the element's item.
    function test_a_churn_of_swaps_keeps_nothing() {
        const made = Eldur.document.getElementById("made")
        const held = Eldur.getComputedStyle(made)
        compare(held.position, "static")
        const records = Eldur._itemMeta.size
        let swaps = 0
        for (let i = 0; i < 2000; i++) {
            const before = Eldur._unwrap(made)
            made.classList.toggle("out")
            made.getBoundingClientRect()
            if (Eldur._unwrap(made) !== before)
                ++swaps
            // Qt destroys what it was asked to between events.
            if (i % 50 === 49)
                wait(0)
        }
        compare(swaps, 2000)
        verify(Eldur._gReplaced.size < 10, "kept " + Eldur._gReplaced.size + " swapped items")
        verify(Eldur._itemMeta.size < records + 300, "kept " + (Eldur._itemMeta.size - records) + " more records")
        made.classList.toggle("out")
        compare(held.position, "absolute")
        const box = made.getBoundingClientRect()
        compare([box.left, box.top], [30, 40])
    }
}
}
"##;

#[test]
fn swapped_items_are_not_kept() {
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "/page/index.html".to_owned(),
        },
        &Options {
            root: RootKind::Item,
            // What the test asks the runtime itself.
            runtime_uses: [
                "document",
                "getComputedStyle",
                "_gReplaced",
                "_itemMeta",
                "_unwrap",
            ]
            .map(String::from)
            .to_vec(),
            ..Options::default()
        },
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-replaced-churn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    std::fs::write(dir.join("tst_churn.qml"), TEST).expect("write test");
    let run = Command::new(RUNNER)
        .args(["-input", "tst_churn.qml"])
        .current_dir(&dir)
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_FONT_DPI", "96")
        .output()
        .expect("qmltestrunner");
    let log = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{log}");
}
