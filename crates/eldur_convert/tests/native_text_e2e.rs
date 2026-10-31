//! Text the converted page makes at run time, in the converter's templates:
//! it asks for native glyphs, except under a scale or rotation (layout.md §1.2).
//! Whether a list's items are native depends on where the script inserts them,
//! and a template does not know that when it is emitted.

use std::process::Command;

use eldur_convert::{Input, NoResources, Options, RootKind};

/// `/usr/bin/qmltestrunner` can be Qt 5's.
const RUNNER: &str = "/usr/lib/qt6/bin/qmltestrunner";

const PAGE: &str = r#"<!doctype html><html><head><style>
.item { color: #123456; }
.label { color: #654321; }
#turned { transform: rotate(5deg); }
</style></head><body>
<ul id="plain"></ul>
<ul id="turned"></ul>
<script>
function add(list) {
    var li = document.createElement('li');
    li.className = 'item';
    var label = document.createElement('span');
    label.className = 'label';
    label.textContent = 'one';
    li.appendChild(label);
    li.appendChild(document.createTextNode(' by '));
    var name = document.createElement('strong');
    name.textContent = 'two';
    li.appendChild(name);
    list.appendChild(li);
}
add(document.getElementById('plain'));
add(document.getElementById('turned'));
</script></body></html>"#;

const TEST: &str = r#"
import QtQuick
import QtQuick.LocalStorage
import QtTest

Item {
    width: 1280
    height: 800
    id: host

TestCase {
    name: "NativeText"
    when: windowShown

    function items(item, out) {
        out.push(item)
        for (let i = 0; i < item.children.length; i++)
            items(item.children[i], out)
        return out
    }
    function test_templates() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        const page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        for (const [name, wanted] of [["plain", Text.NativeRendering], ["turned", Text.QtRendering]]) {
            const under = items(findChild(page, name), [])
            verify(under.some(i => typeof i.eldurTemplate === "string" && i.eldurTemplate !== ""),
                   name + ": no item from a template")
            const texts = under.filter(i => typeof i.text === "string" && typeof i.renderType === "number")
            compare(texts.map(t => t.text.replace(/ /g, " ").trim()).filter(t => t !== "").sort().join(), "by,one,two", name)
            for (const t of texts)
                compare(t.renderType, wanted, name + ": " + t.text)
        }
    }
}
}
"#;

#[test]
fn templates_and_text_runs_follow_where_they_are_inserted() {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    let out = eldur_convert::convert(
        &Input {
            html: PAGE.to_owned(),
            url: "native/index.html".to_owned(),
        },
        &options,
        &mut NoResources,
    );
    let dir = std::env::temp_dir().join(format!("eldur-native-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for file in &out.files {
        std::fs::write(dir.join(&file.path), &file.contents).expect("write output");
    }
    assert!(
        out.files.iter().any(|f| f.path.ends_with("Main.qml")
            && String::from_utf8_lossy(&f.contents).contains("eldurTextRun")),
        "the page should make a text run template"
    );
    std::fs::write(dir.join("tst_native.qml"), TEST).expect("write test");
    let output = Command::new(RUNNER)
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_native.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("XDG_DATA_HOME", dir.join("data"))
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
}
