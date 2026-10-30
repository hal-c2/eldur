use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use h2q_js::{
    DomQuery, DomResolver, ElementKey, QmlTarget, RUNTIME_JS, ScriptInput, ScriptKind, TargetKind,
    analyze, rewrite,
};

#[derive(Default)]
struct FakeResolver {
    queries: HashMap<DomQuery, Vec<QmlTarget>>,
    elements: HashMap<ElementKey, QmlTarget>,
}

impl FakeResolver {
    fn target(key: u32, qml_id: &str, kind: TargetKind) -> QmlTarget {
        QmlTarget {
            key: ElementKey(key),
            qml_id: qml_id.to_string(),
            kind,
        }
    }

    fn with_id(mut self, html_id: &str, key: u32, qml_id: &str, kind: TargetKind) -> Self {
        let t = Self::target(key, qml_id, kind);
        self.queries
            .insert(DomQuery::ById(html_id.to_string()), vec![t.clone()]);
        self.queries
            .insert(DomQuery::Selector(format!("#{html_id}")), vec![t.clone()]);
        self.elements.insert(ElementKey(key), t);
        self
    }

    fn with_selector(mut self, sel: &str, targets: Vec<QmlTarget>) -> Self {
        if let Some(first) = targets.first().cloned() {
            self.queries
                .insert(DomQuery::Selector(sel.to_string()), vec![first]);
        }
        self.queries
            .insert(DomQuery::SelectorAll(sel.to_string()), targets);
        self
    }

    fn with_element(mut self, key: u32, qml_id: &str, kind: TargetKind) -> Self {
        self.elements
            .insert(ElementKey(key), Self::target(key, qml_id, kind));
        self
    }
}

impl DomResolver for FakeResolver {
    fn resolve(&self, query: &DomQuery) -> Vec<QmlTarget> {
        self.queries.get(query).cloned().unwrap_or_default()
    }

    fn element(&self, key: ElementKey) -> Option<QmlTarget> {
        self.elements.get(&key).cloned()
    }
}

fn has_binary(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

struct TempQmlDir {
    path: PathBuf,
}

impl TempQmlDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir_name = format!("h2q_js_{prefix}_{}_{nanos}_{seq}", std::process::id());
        let path = std::env::temp_dir().join(dir_name);
        fs::create_dir_all(&path).expect("failed to create temp qml test directory");
        Self { path }
    }

    fn write_file(&self, name: &str, contents: &str) -> PathBuf {
        let file_path = self.path.join(name);
        fs::write(&file_path, contents).expect("failed to write test file");
        file_path
    }
}

impl Drop for TempQmlDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run_qml6_offscreen(main_qml: &PathBuf, timeout: Duration) -> String {
    let mut child = Command::new("qml6")
        .arg(main_qml)
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn qml6");

    let start = Instant::now();
    loop {
        match child.try_wait().expect("failed polling qml6 process") {
            Some(_) => break,
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    let output = child
        .wait_with_output()
        .expect("failed reading qml6 output");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

#[test]
fn qmllint_accepts_runtime_and_rewritten_page_js_without_warnings() {
    if !has_binary("qmllint") {
        eprintln!("skipping qmllint test: qmllint not on PATH");
        return;
    }

    let dir = TempQmlDir::new("qmllint");
    let resolver = FakeResolver::default()
        .with_id("label", 1, "statusLabel", TargetKind::Text)
        .with_id("btn", 2, "actionBtn", TargetKind::Button);

    let script = ScriptInput {
        source: r#"
const greeting = "Ready";
let clicks = 0;
const btn = document.getElementById("btn");
btn.addEventListener("click", () => {
    clicks += 1;
    document.getElementById("label").textContent = greeting + ":" + clicks;
});
document.getElementById("label").textContent = greeting;
"#
        .to_string(),
        name: "app.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
    };

    let scripts = [script];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import QtQuick.Controls.Basic
import "page.js" as Page

Item {
    id: root
    width: 400
    height: 300

    Text {
        id: statusLabel
        text: ""
    }

    Button {
        id: actionBtn
        text: "Go"
        onClicked: Page.__h2q_on_click_actionBtn(Page.H2Q.event("click", actionBtn, null))
    }

    Component.onCompleted: {
        Page.__h2q_init(root)
    }
}
"#,
    );

    for path in [&runtime_path, &page_path, &main_qml] {
        let output = Command::new("qmllint")
            .arg(path)
            .output()
            .expect("failed to run qmllint");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.trim().is_empty() && stderr.trim().is_empty(),
            "qmllint failed or warned on {}:\nstatus: {}\nstdout: {}\nstderr: {}",
            path.display(),
            output.status,
            stdout,
            stderr
        );
    }
}

#[test]
fn qml6_executes_rewritten_dom_events_timers_storage_and_lowered_syntax() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 runtime test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("e2e");
    let resolver = FakeResolver::default()
        .with_id("title", 1, "titleText", TargetKind::Text)
        .with_id("card", 2, "cardBox", TargetKind::Box)
        .with_id("input", 3, "nameField", TargetKind::TextField)
        .with_id("btn", 4, "goBtn", TargetKind::Button)
        .with_element(5, "inlineBtn", TargetKind::Button)
        .with_selector(
            ".tag",
            vec![
                FakeResolver::target(6, "tag0", TargetKind::Text),
                FakeResolver::target(7, "tag1", TargetKind::Text),
            ],
        );

    let s1 = ScriptInput {
        source: r#"
const PREFIX = "H2Q";
let count = 0;
const btn = document.getElementById("btn");
const card = document.getElementById("card");

btn.addEventListener("click", (ev) => {
    count += 1;
    const inputVal = document.getElementById("input").value;
    document.getElementById("title").textContent = PREFIX + ":" + inputVal + ":" + count + ":" + ev.type;
    card.textContent = "box:" + count;
    card.classList.add("active");
    card.dataset.clicks = String(count);
    card.style.display = "none";
});

document.addEventListener("DOMContentLoaded", () => {
    document.getElementById("input").value = "Alice";
    const tags = document.querySelectorAll(".tag");
    tags[0].textContent = "T0";
    tags[1].textContent = "T1";
});
"#
        .to_string(),
        name: "s1.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
    };

    let s2 = ScriptInput {
        source: r#"
async function verifyAsyncAndPolyfills() {
    const base = { a: 1, b: 2, c: 3 };
    const merged = { ...base, d: [1, [2, 3]].flat().at(-1) };
    const { a, ...rest } = merged;
    let label = null;
    label ??= "defaulted";

    localStorage.setItem("h2q_test_key", "stored_val");
    const roundTrip = localStorage.getItem("h2q_test_key");
    sessionStorage.setItem("sess_k", "sess_v");
    const sessVal = sessionStorage.getItem("sess_k");

    const dynNode = document.getElementById("dyn-item");
    dynNode.textContent = "dynamic-ok";

    const awaited = await Promise.resolve("async-ok").finally(() => {});
    const rep = "a-b-c".replaceAll("-", "_");
    return [a, rest.b, rest.c, rest.d, label, roundTrip, sessVal, dynNode.textContent, awaited, rep].join("|");
}

window.addEventListener("load", () => {
    verifyAsyncAndPolyfills().then((summary) => {
        setTimeout(() => {
            console.log("H2Q_OK_DOM",
                document.getElementById("title").textContent,
                card.textContent,
                card.classList.contains("active"),
                card.dataset.clicks,
                card.style.display,
                document.getElementById("inline-target").textContent
            );
            console.log("H2Q_OK_ASYNC", summary);
            Qt.quit();
        }, 10);
    });
});
"#
        .to_string(),
        name: "s2.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
    };

    let handler_attr = ScriptInput {
        source: r#"this.textContent = "inline:" + event.key + ":" + event.code; return false;"#
            .to_string(),
        name: "onkeydown of inlineBtn".to_string(),
        kind: ScriptKind::HandlerAttribute {
            element: ElementKey(5),
            event: "onkeydown".to_string(),
        },
        line_offset: 1,
    };

    let inputs = vec![s1, s2, handler_attr];
    let analysis = analyze(&inputs);
    let out = rewrite(&inputs, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));

    dir.write_file("h2q_runtime.js", RUNTIME_JS);
    dir.write_file("page.js", &out.page_js);

    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import QtQuick.Controls.Basic
import "page.js" as Page
import "h2q_runtime.js" as H2Q

Item {
    id: root
    width: 640
    height: 480

    Text { id: titleText; text: "init" }

    Rectangle {
        id: cardBox
        property bool cls_active: false
        Text { id: cardBoxText; text: "" }
    }

    TextField { id: nameField; text: "" }
    Button { id: goBtn; text: "Go" }
    Button {
        id: inlineBtn
        objectName: "inline-target"
        text: "Inline"
    }
    Text { id: tag0; text: "" }
    Text { id: tag1; text: "" }
    Text {
        id: dynText
        objectName: "dyn-item"
        text: "before"
    }

    Component.onCompleted: {
        Page.__h2q_init(root);
        Page.__h2q_on_click_goBtn(H2Q.event("click", goBtn, { x: 5, y: 6, button: Qt.LeftButton }));
        var keyEv = H2Q.event("keydown", inlineBtn, { key: 0x41, text: "a", modifiers: 0, accepted: false });
        Page.__h2q_on_keydown_inlineBtn(keyEv);
        if (keyEv.defaultPrevented && cardBox.cls_active && tag0.text === "T0" && tag1.text === "T1") {
            console.log("H2Q_OK_SYNC", inlineBtn.text);
        }
    }
}
"#,
    );

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains("H2Q_OK_SYNC inline:a:KeyA"),
        "missing H2Q_OK_SYNC in qml6 output:\n{output}"
    );
    assert!(
        output.contains("H2Q_OK_DOM H2Q:Alice:1:click box:1 true 1 none inline:a:KeyA"),
        "missing H2Q_OK_DOM in qml6 output:\n{output}"
    );
    assert!(
        output
            .contains("H2Q_OK_ASYNC 1|2|3|3|defaulted|stored_val|sess_v|dynamic-ok|async-ok|a_b_c"),
        "missing H2Q_OK_ASYNC in qml6 output:\n{output}"
    );
}

#[test]
fn qml6_executes_controls_modules_classes_raf_and_dynamic_listen() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 controls/modules test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("controls_mod");
    let resolver = FakeResolver::default()
        .with_id("roleCombo", 1, "roleCombo", TargetKind::ComboBox)
        .with_id("agreeCheck", 2, "agreeCheck", TargetKind::CheckBox)
        .with_id("volSlider", 3, "volSlider", TargetKind::Range)
        .with_id("panelBox", 4, "panelBox", TargetKind::Box)
        .with_id("outLabel", 5, "outLabel", TargetKind::Text);

    let util_mod = ScriptInput {
        name: "src/util.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
export class Tracker {
    base = 10;
    static prefix = "TRK";

    constructor(step) {
        this.base += step;
    }

    async compute(mult) {
        const v = await Promise.resolve(mult * 3);
        return this.base + v;
    }
}

export default function fmt(tag, val) {
    return tag + "=" + val;
}
"#
        .to_string(),
    };

    let main_mod = ScriptInput {
        name: "src/main.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r##"
import fmt, { Tracker as Trk } from "./util.js";

const combo = document.getElementById("roleCombo");
const check = document.getElementById("agreeCheck");
const slider = document.getElementById("volSlider");
const panel = document.getElementById("panelBox");
const out = document.getElementById("outLabel");

combo.value = "Admin";
check.checked = true;
slider.value = 75;
panel.style.color = "#224466";
document.title = "ControlsMod";

async function finishChecks() {
    let dynClicks = 0;
    const dynBtn = document.getElementById("dynBtn");
    dynBtn.addEventListener("click", () => {
        dynClicks += 1;
    });
    dynBtn._item.clicked();

    const trk = new Trk(5);
    const total = await trk.compute(4);
    const settled = await Promise.allSettled([Promise.resolve(1), Promise.reject("x")]);
    const winner = await Promise.any([Promise.reject("a"), Promise.resolve("win")]);
    const fromEnt = Object.fromEntries([["ok", 9]]).ok;

    const summary = [
        fmt(Trk.prefix, total),
        combo.value,
        check.checked,
        slider.value,
        panel.style.color,
        dynClicks,
        settled.length,
        winner,
        fromEnt,
        document.title,
        window.innerWidth
    ].join("|");
    out.textContent = summary;

    requestAnimationFrame((ts) => {
        console.log("H2Q_OK_MOD_CONTROLS", summary, typeof ts === "number");
        Qt.quit();
    });
}

finishChecks();
"##
        .to_string(),
    };

    let scripts = [main_mod, util_mod];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import QtQuick.Controls.Basic
import "page.js" as Page

Item {
    id: root
    width: 500
    height: 360
    property string title: ""

    ComboBox { id: roleCombo; model: ["Viewer", "Editor", "Admin"] }
    CheckBox { id: agreeCheck; checked: false }
    Slider { id: volSlider; from: 0; to: 100; value: 10 }
    Rectangle {
        id: panelBox
        Text { id: panelBoxText; text: "hello" }
    }
    Text { id: outLabel; text: "" }
    Button { id: dynBtnItem; objectName: "dynBtn"; text: "Dyn" }

    Component.onCompleted: Page.__h2q_init(root)
}
"#,
    );

    if has_binary("qmllint") {
        for path in [&runtime_path, &page_path, &main_qml] {
            let lint = Command::new("qmllint")
                .arg(path)
                .output()
                .expect("failed to run qmllint");
            assert!(
                lint.status.success(),
                "qmllint failed on {}: {}{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr)
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains(
            "H2Q_OK_MOD_CONTROLS TRK=27|Admin|true|75|#224466|1|2|win|9|ControlsMod|500 true"
        ),
        "unexpected qml6 controls/modules output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_unifies_resolved_target_add_remove_dispatch_and_dedup_listeners() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 resolved listener unification test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("unified_listen");
    let resolver = FakeResolver::default().with_id("box", 1, "boxItem", TargetKind::Box);

    let script = ScriptInput {
        name: "listeners.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
let hits = 0;
let log = [];

function onHit(ev) {
    hits += 1;
    log.push(ev.type + ":" + hits);
}

function onOnce(ev) {
    log.push("once:" + ev.type);
}

const topBox = document.getElementById("box");
topBox.addEventListener("click", onHit);
topBox.addEventListener("click", onOnce, { once: true });

function registerDuplicate() {
    const box = document.getElementById("box");
    box.addEventListener("click", onHit);
    box.addEventListener("click", onHit);
}

function dispatchClick() {
    const box = document.getElementById("box");
    box.dispatchEvent(new CustomEvent("click"));
}

function unregisterAndDispatch() {
    const box = document.getElementById("box");
    box.removeEventListener("click", onHit);
    box.dispatchEvent(new CustomEvent("click"));
    return hits + "|" + log.join(",");
}
"#
        .to_string(),
    };

    let scripts = [script];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.bindings.len(), 1);
    assert_eq!(out.bindings[0].function, "__h2q_on_click_boxItem_0");
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));

    dir.write_file("h2q_runtime.js", RUNTIME_JS);
    dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import "page.js" as Page
import "h2q_runtime.js" as H2Q

Item {
    id: root
    width: 200
    height: 200

    Rectangle {
        id: boxItem
        Text { id: boxItemText; text: "" }
    }

    Component.onCompleted: {
        Page.__h2q_init(root);
        Page.registerDuplicate();
        Page.dispatchClick();
        Page.__h2q_on_click_boxItem_0(H2Q.event("click", boxItem, null));
        var summary = Page.unregisterAndDispatch();
        Page.__h2q_on_click_boxItem_0(H2Q.event("click", boxItem, null));
        var finalSummary = Page.unregisterAndDispatch();
        console.log("H2Q_OK_UNIFIED_LISTEN", summary, finalSummary);
        Qt.quit();
    }
}
"#,
    );

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains(
            "H2Q_OK_UNIFIED_LISTEN 2|click:1,once:click,click:2 2|click:1,once:click,click:2"
        ),
        "unexpected qml6 unified listener output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_retains_uninitialized_top_level_let_with_script_order_reset() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 top-level let test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("top_let");
    let resolver = FakeResolver::default();

    let script = ScriptInput {
        name: "state.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
setState("before_decl");
console.log("boot");
let state;
function setState(v) { state = v; }
function getState() { return state; }
"#
        .to_string(),
    };

    let scripts = [script];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));

    dir.write_file("h2q_runtime.js", RUNTIME_JS);
    dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import "page.js" as Page

Item {
    id: root
    width: 100
    height: 100

    Component.onCompleted: {
        Page.__h2q_init(root);
        var afterInit = String(Page.getState());
        Page.setState("ready");
        var afterSet = String(Page.getState());
        Page.__h2q_init(root);
        var afterReinit = String(Page.getState());
        console.log("H2Q_OK_TOP_LET", afterInit + "|" + afterSet + "|" + afterReinit);
        Qt.quit();
    }
}
"#,
    );

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains("H2Q_OK_TOP_LET undefined|ready|undefined"),
        "unexpected qml6 top-level let output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_executes_compound_and_logical_dom_assignments_with_short_circuiting() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 compound/logical assignment test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("compound_logical");
    let resolver = FakeResolver::default()
        .with_id("saveBtn", 1, "saveBtn", TargetKind::Button)
        .with_id("roleCombo", 2, "roleCombo", TargetKind::ComboBox)
        .with_id("cardBox", 3, "cardBox", TargetKind::Box)
        .with_id("htmlLabel", 4, "htmlLabel", TargetKind::Text)
        .with_id("nameInput", 5, "nameInput", TargetKind::TextField);

    let script = ScriptInput {
        name: "compound_logical.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r##"
let rhsCalls = 0;
function rhs(val) {
    rhsCalls += 1;
    return val;
}

const save = document.getElementById("saveBtn");
const role = document.getElementById("roleCombo");
const card = document.getElementById("cardBox");
const htmlEl = document.getElementById("htmlLabel");
const nameEl = document.getElementById("nameInput");
document.title = "Doc";

function runChecks() {
    // 1. document.title += and logical operators with short-circuiting
    document.title += rhs(":v1"); // evaluates rhs (1) -> "Doc:v1"
    document.title ||= rhs("unused-title"); // short-circuits!
    document.title ??= rhs("unused-nullish"); // short-circuits!

    // 2. disabled ||= true and hidden &&= false with inversion + short-circuiting
    // Initially saveBtn.enabled = true (disabled = false), saveBtn.visible = false (hidden = true)
    save.disabled ||= rhs(true); // evaluates rhs (2) -> enabled becomes false
    save.disabled ||= rhs(false); // short-circuits! (disabled is already true)
    save.hidden &&= rhs(false); // evaluates rhs (3) -> visible becomes true
    save.hidden &&= rhs(true); // short-circuits! (hidden is now false)

    // 3. ComboBox value ||= text only assigns currentIndex/find when falsy
    // Initially roleCombo index 0 is "" (falsy)
    role.value &&= rhs("Admin"); // short-circuits! (currentText is "")
    role.value ||= rhs("Editor"); // evaluates rhs (4) -> selects "Editor" (index 2)
    role.value ||= rhs("Admin"); // short-circuits! (currentText is "Editor")
    role.value ??= rhs("Viewer"); // short-circuits!

    // 4. className += appends rather than replacing, logical ops short-circuit
    card.className = "card";
    card.className += rhs(" primary"); // evaluates rhs (5) -> "card primary"
    card.className ||= rhs("fallback"); // short-circuits!

    // 5. resolved innerHTML += and ||= &&= ??=
    htmlEl.innerHTML = "<b>Hi</b>";
    htmlEl.innerHTML += rhs("<i>!</i>"); // evaluates rhs (6) -> "<b>Hi</b><i>!</i>"
    htmlEl.innerHTML ||= rhs("<b>skip</b>"); // short-circuits!
    htmlEl.innerHTML ??= rhs("<b>skip2</b>"); // short-circuits!
    htmlEl.innerHTML &&= rhs("<u>OK</u>"); // evaluates rhs (7) -> "<u>OK</u>"

    // 6. mapped text/value/style and ordinary JS object logical assignments
    nameEl.value ||= rhs("Alice"); // evaluates rhs (8)
    nameEl.value ||= rhs("Bob"); // short-circuits!
    nameEl.value += rhs("!"); // evaluates rhs (9) -> "Alice!"
    card.style.color ||= rhs("#123456"); // evaluates rhs (10)
    card.style.color ||= rhs("#654321"); // short-circuits!

    const obj = { count: 2, tag: "" };
    obj.count += 3;
    obj.tag ||= rhs("tagged"); // evaluates rhs (11)
    obj.tag ||= rhs("ignored"); // short-circuits!

    return [
        document.title,
        save.disabled,
        saveBtn.enabled,
        save.hidden,
        saveBtn.visible,
        role.value,
        roleCombo.currentIndex,
        card.className,
        cardBox.cls_card,
        cardBox.cls_primary,
        htmlEl.innerHTML,
        nameEl.value,
        card.style.color,
        obj.count,
        obj.tag,
        rhsCalls
    ].join("|");
}
"##
        .to_string(),
    };

    let scripts = [script];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import QtQuick.Controls.Basic
import "page.js" as Page

Item {
    id: root
    width: 400
    height: 300
    property string title: ""

    Button { id: saveBtn; enabled: true; visible: false }
    ComboBox { id: roleCombo; model: ["", "Viewer", "Editor", "Admin"]; currentIndex: 0 }
    Rectangle {
        id: cardBox
        property bool cls_card: false
        property bool cls_primary: false
        Text { id: cardBoxText; text: "" }
    }
    Text { id: htmlLabel; text: "" }
    TextField { id: nameInput; text: "" }

    Component.onCompleted: {
        Page.__h2q_init(root);
        var res = Page.runChecks();
        console.log("H2Q_OK_COMPOUND_LOGICAL", res);
        Qt.quit();
    }
}
"#,
    );

    if has_binary("qmllint") {
        for path in [&runtime_path, &page_path, &main_qml] {
            let lint = Command::new("qmllint")
                .arg(path)
                .output()
                .expect("failed to run qmllint");
            assert!(
                lint.status.success(),
                "qmllint failed on {}: {}{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr)
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains(
            "H2Q_OK_COMPOUND_LOGICAL Doc:v1|true|false|false|true|Editor|2|card primary|true|true|<u>OK</u>|Alice!|#123456|5|tagged|11"
        ),
        "unexpected qml6 compound/logical output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}
