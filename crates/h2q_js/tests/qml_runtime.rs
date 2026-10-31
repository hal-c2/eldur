use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use h2q_js::{
    DomQuery, DomResolver, ElementKey, JsOutput, QmlTarget, RUNTIME_JS, ScriptInput, ScriptKind,
    TargetKind, analyze, rewrite,
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

#[test]
fn qml6_assignment_expressions_return_dom_values_and_evaluate_operands_once() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 assignment-expression return value test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("assign_expr_values");
    let resolver = FakeResolver::default()
        .with_id("saveBtn", 1, "saveBtn", TargetKind::Button)
        .with_id("roleCombo", 2, "roleCombo", TargetKind::ComboBox)
        .with_id("cardBox", 3, "cardBox", TargetKind::Box)
        .with_id("htmlLabel", 4, "htmlLabel", TargetKind::Text)
        .with_id("nameInput", 5, "nameInput", TargetKind::TextField);

    let script = ScriptInput {
        name: "assign_expr_values.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r##"
let rhsCalls = 0;
let order = [];

function rhs(val) {
    rhsCalls += 1;
    return val;
}

function pickNode(tag, node) {
    order.push("recv:" + tag);
    return node;
}

function taggedRhs(tag, val) {
    order.push("rhs:" + tag);
    rhsCalls += 1;
    return val;
}

const save = document.getElementById("saveBtn");
const role = document.getElementById("roleCombo");
const card = document.getElementById("cardBox");
const htmlEl = document.getElementById("htmlLabel");
const nameEl = document.getElementById("nameInput");
document.title = "";

function runExprChecks() {
    // 1. document.title assignment expressions (logical, compound, simple)
    const t1 = (document.title ||= rhs("default")); // call 1 -> "default"
    const t2 = (document.title ||= rhs("ignored")); // short-circuit -> "default"
    const t3 = (document.title += rhs(":v2")); // call 2 -> "default:v2"
    const t4 = (document.title = rhs("Fresh")); // call 3 -> "Fresh"

    // 2. hidden and disabled assignment expressions return DOM bool, not inverted QML bool
    // Initially saveBtn.enabled = true (disabled = false), saveBtn.visible = false (hidden = true)
    const h1 = (save.hidden &&= rhs(false)); // call 4 -> false
    const h2 = (save.hidden &&= rhs(true)); // short-circuit -> false
    const d1 = (save.disabled = rhs(true)); // call 5 -> true
    const d2 = (save.disabled ||= rhs(false)); // short-circuit -> true
    const d3 = (save.disabled &&= rhs(false)); // call 6 -> false
    const d4 = (save.disabled |= rhs(true)); // call 7 -> true

    // 3. ComboBox value assignment expressions return DOM text, not currentIndex
    const r1 = (role.value ||= rhs("Editor")); // call 8 -> "Editor"
    const r2 = (role.value ||= rhs("Admin")); // short-circuit -> "Editor"
    const r3 = (role.value = rhs("Admin")); // call 9 -> "Admin"
    const r4 = (role.value = ""); // reset to "" -> ""
    const r5 = (role.value += rhs("Viewer")); // call 10 -> "Viewer"

    // 4. className, innerHTML, style.<prop>, style.display, textContent/value
    const c1 = (card.className = rhs("card")); // call 11 -> "card"
    const c2 = (card.className += rhs(" active")); // call 12 -> "card active"
    const c3 = (card.className ||= rhs("skip")); // short-circuit -> "card active"

    const m1 = (htmlEl.innerHTML = rhs("<b>A</b>")); // call 13 -> "<b>A</b>"
    const m2 = (htmlEl.innerHTML += rhs("<i>B</i>")); // call 14 -> "<b>A</b><i>B</i>"
    const m3 = (htmlEl.innerHTML ||= rhs("skip")); // short-circuit -> "<b>A</b><i>B</i>"

    const s1 = (card.style.color = rhs("#111")); // call 15 -> "#111"
    const s2 = (card.style.color += rhs("222")); // call 16 -> "#111222"
    const s3 = (card.style.color ||= rhs("#999")); // short-circuit -> "#111222"

    const disp1 = (card.style.display = rhs("none")); // call 17 -> "none"
    const disp2 = (card.style.display ||= rhs("flex")); // short-circuit ("none" is truthy) -> "none"
    const disp3 = (card.style.display &&= rhs("flex")); // call 18 -> "flex"

    const v1 = (nameEl.value ||= rhs("Ada")); // call 19 -> "Ada"
    const v2 = (nameEl.value ||= rhs("Skip")); // short-circuit -> "Ada"
    const v3 = (nameEl.value += rhs("!")); // call 20 -> "Ada!"

    // 5. Dynamic receiver with side effects + normal JS evaluation order (receiver before RHS, once each)
    const dynNode = document.getElementById("dynLabel");
    const dyn1 = (pickNode("1", dynNode).textContent = taggedRhs("1", "Alpha")); // call 21 -> "Alpha"
    const dyn2 = (pickNode("2", dynNode).textContent += taggedRhs("2", "+Beta")); // call 22 -> "Alpha+Beta"
    const dyn3 = (pickNode("3", dynNode).textContent ||= taggedRhs("3", "Skip")); // short-circuit -> "Alpha+Beta"
    const dyn4 = (pickNode("4", dynNode).innerHTML = taggedRhs("4", "<em>Z</em>")); // call 23 -> "<em>Z</em>"
    const dyn5 = (pickNode("5", dynNode).innerHTML += taggedRhs("5", "!")); // call 24 -> "<em>Z</em>!"

    return [
        t1, t2, t3, t4,
        h1, h2, d1, d2, d3, d4,
        r1, r2, r3, r4, r5, roleCombo.currentIndex,
        c1, c2, c3,
        m1, m2, m3,
        s1, s2, s3,
        disp1, disp2, disp3, cardBox.visible,
        v1, v2, v3,
        dyn1, dyn2, dyn3, dyn4, dyn5,
        order.join(","),
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
        property bool cls_active: false
        Text { id: cardBoxText; text: "" }
    }
    Text { id: htmlLabel; text: "" }
    TextField { id: nameInput; text: "" }
    Text { id: dynLabelItem; objectName: "dynLabel"; text: "" }

    Component.onCompleted: {
        Page.__h2q_init(root);
        var res = Page.runExprChecks();
        console.log("H2Q_OK_ASSIGN_EXPR", res);
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
            "H2Q_OK_ASSIGN_EXPR default|default|default:v2|Fresh|false|false|true|true|false|true|Editor|Editor|Admin||Viewer|1|card|card active|card active|<b>A</b>|<b>A</b><i>B</i>|<b>A</b><i>B</i>|#111|#111222|#111222|none|none|flex|true|Ada|Ada|Ada!|Alpha|Alpha+Beta|Alpha+Beta|<em>Z</em>|<em>Z</em>!|recv:1,rhs:1,recv:2,rhs:2,recv:3,recv:4,rhs:4,recv:5,rhs:5|24"
        ),
        "unexpected qml6 assignment-expression output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_executes_classic_block_nested_var_hoisting() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 classic block-nested var hoisting test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("classic_block_var_hoist");
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let s1 = ScriptInput {
        name: "s1.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
if (false) {
    var falseVar = "skipped";
}
var beforeTrue = (msg === undefined) && (falseVar === undefined);
if (true) {
    var msg = "ready", uninit;
}
for (var i = 0; i < 3; i++) {}
for (var item of ["x", "y"]) {}

function readFromTopFn() {
    return [beforeTrue, falseVar === undefined, msg, i, item, uninit === undefined].join(":");
}

function checkFunctionLocal() {
    if (true) {
        var innerOnly = "local";
    }
    for (var k = 0; k < 2; k++) {}
    return innerOnly + ":" + k;
}
"#
        .to_string(),
    };
    let s2 = ScriptInput {
        name: "s2.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
var out = document.getElementById("out");
var summary = [
    readFromTopFn(),
    msg,
    i,
    item,
    checkFunctionLocal(),
    typeof innerOnly
].join("|");
out.textContent = summary;
"#
        .to_string(),
    };

    let scripts = [s1, s2];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import "page.js" as Page

Item {
    id: root
    width: 200
    height: 100

    Text { id: outText; text: "" }

    Component.onCompleted: {
        Page.__h2q_init(root);
        console.log("H2Q_OK_BLOCK_VAR", outText.text, Page.readFromTopFn());
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
            "H2Q_OK_BLOCK_VAR true:true:ready:3:y:true|ready|3|y|local:2|undefined true:true:ready:3:y:true"
        ),
        "unexpected qml6 classic block-nested var output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_executes_colliding_modules_classic_globals_shared_imports_and_shadowing() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 colliding modules test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("colliding_modules");
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let shared_mod = ScriptInput {
        name: "src/shared.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
const step = 2;
export let ticks = 0;
export const order = [];
export function bump(tag) {
    ticks += step;
    order.push(tag + "@" + ticks);
    return ticks;
}
bump("S");
"#
        .to_string(),
    };

    let mod_a = ScriptInput {
        name: "src/mod_a.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
import { bump, ticks as seenTicks } from "./shared.js";
const factor = 2;
let offset = 10;
function helper(v) {
    return v * factor + offset;
}
class Box {
    static tag = "A";
    constructor(v) {
        this.v = helper(v);
    }
    read() {
        return Box.tag + ":" + this.v;
    }
}
export const token = factor + offset;
export function calc(x) {
    offset += 1;
    return new Box(x).read() + ":" + seenTicks;
}
export function shadowA(factor, offset) {
    const helper = (n) => n + factor + offset;
    return helper(3) + ":" + token;
}
bump("A");
"#
        .to_string(),
    };

    let mod_b = ScriptInput {
        name: "src/mod_b.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
import { bump, ticks as seenTicks } from "./shared.js";
const factor = 3;
let offset = 100;
function helper(v) {
    return v * factor + offset;
}
class Box {
    static tag = "B";
    constructor(v) {
        this.v = helper(v);
    }
    read() {
        return Box.tag + ":" + this.v;
    }
}
export const token = factor + offset;
export function calc(x) {
    offset += 5;
    return new Box(x).read() + ":" + seenTicks;
}
bump("B");
"#
        .to_string(),
    };

    let classic_script = ScriptInput {
        name: "src/classic.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
const factor = 1000;
let offset = 500;
var sharedName = "classic";
function helper(v) {
    return v + factor + offset;
}
class Box {
    constructor(v) {
        this.v = helper(v);
    }
    read() {
        return sharedName + ":" + this.v;
    }
}
function readClassic(v) {
    return new Box(v).read();
}
"#
        .to_string(),
    };

    let main_mod = ScriptInput {
        name: "src/main.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
import { calc as calcA, token as tokenA, shadowA } from "./mod_a.js";
import { calc as calcB, token as tokenB } from "./mod_b.js";
import { bump, order, ticks as seenTicks } from "./shared.js";

const factor = 7;
bump("M");
const summary = [
    calcA(4),
    calcB(4),
    shadowA(5, 6),
    readClassic(1),
    tokenA + "+" + tokenB + "+" + factor + "+" + seenTicks,
    order.join(",")
].join("|");
document.getElementById("out").textContent = summary;
"#
        .to_string(),
    };

    let scripts = [main_mod, mod_a, mod_b, shared_mod, classic_script];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import "page.js" as Page

Item {
    id: root
    width: 200
    height: 100

    Text { id: outText; text: "" }

    Component.onCompleted: {
        Page.__h2q_init(root);
        console.log("H2Q_OK_COLLIDING_MODULES", outText.text, Page.readClassic(2));
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
                "qmllint failed on {}: {}{}\npage.js:\n{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr),
                out.page_js
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains(
            "H2Q_OK_COLLIDING_MODULES A:19:8|B:117:8|14:12|classic:1501|12+103+7+8|S@2,A@4,B@6,M@8 classic:1502"
        ),
        "unexpected qml6 colliding modules output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_executes_top_level_destructured_var_across_scripts_in_statement_order() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 test: qml6 not found on PATH");
        return;
    }

    let dir = TempQmlDir::new("top_destructured_var");
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let s1 = ScriptInput {
        name: "s1.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
var trace = [];
var a = 1;
function step(label) {
    trace.push(label + ":" + a + ":" + (b === undefined ? "u" : b) + ":" + (c === undefined ? "u" : c));
    a = 99;
}
step("s1-pre");
var { a, nested: [b, { c = 30 }] } = { a: 2, nested: [20, {}] };
step("s1-mid");
var [a, b] = [3, 40];
step("s1-post");
"#
        .to_string(),
    };
    let s2 = ScriptInput {
        name: "s2.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
var a = 4;
step("s2-scalar");
var { b, c } = { b: 50, c: 60 };
var [a, d] = [5, 70];
function summary() {
    return trace.join("|") + "=>a=" + a + ",b=" + b + ",c=" + c + ",d=" + d;
}
document.getElementById("out").textContent = summary();
"#
        .to_string(),
    };

    let scripts = [s1, s2];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import "page.js" as Page

Item {
    id: root
    width: 200
    height: 100

    Text { id: outText; text: "" }

    Component.onCompleted: {
        Page.__h2q_init(root);
        console.log("H2Q_OK_DESTRUCTURED_VAR", outText.text);
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
                "qmllint failed on {}: {}{}\npage.js:\n{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr),
                out.page_js
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains(
            "H2Q_OK_DESTRUCTURED_VAR s1-pre:1:u:u|s1-mid:2:20:30|s1-post:3:40:30|s2-scalar:4:40:30=>a=5,b=50,c=60,d=70"
        ),
        "unexpected qml6 destructured var output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_executes_uninitialized_nested_var_in_control_flow_and_for_headers() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 test: qml6 not found on PATH");
        return;
    }

    let dir = TempQmlDir::new("uninit_nested_var");
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let s1 = ScriptInput {
        name: "s1.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
var trace = [];
if (false) var branchSingle;
trace.push("single:" + (branchSingle === undefined));
if (false)
    var branchNewline;
trace.push("newline:" + (branchNewline === undefined));
branchNewline = "set";
if (true) var thenOnly; else var elseOnly;
while (false) var whileOnly;
do var doOnly; while (false);
forHdr = 0;
for (var forHdr; forHdr < 2; forHdr++) var forBody;
for (var forOfHdr of ["m", "n"]) var forOfBody;
if (true) {
    var blockOnly;
}
function readAll() {
    var localUninit;
    if (false) var localBranch;
    for (var localIdx = 0; localIdx < 3; localIdx++) {}
    return [
        trace.join(","),
        branchNewline,
        forHdr,
        forOfHdr,
        blockOnly === undefined,
        localUninit === undefined,
        localBranch === undefined,
        localIdx
    ].join("|");
}
document.getElementById("out").textContent = readAll();
"#
        .to_string(),
    };

    let scripts = [s1];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import "page.js" as Page

Item {
    id: root
    width: 200
    height: 100

    Text { id: outText; text: "" }

    Component.onCompleted: {
        Page.__h2q_init(root);
        console.log("H2Q_OK_UNINIT_NESTED_VAR", outText.text);
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
                "qmllint failed on {}: {}{}\npage.js:\n{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr),
                out.page_js
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output
            .contains("H2Q_OK_UNINIT_NESTED_VAR single:true,newline:true|set|2|n|true|true|true|3"),
        "unexpected qml6 uninit nested var output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_unifies_mixed_onclick_and_add_event_listener_across_qml_and_dispatch_event() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 mixed onclick/addEventListener test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("mixed_on_and_listener");
    let resolver = FakeResolver::default()
        .with_id("btn", 1, "btnItem", TargetKind::Button)
        .with_id("dup", 2, "dupItem", TargetKind::Button);

    let script = ScriptInput {
        name: "mixed_events.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
let log = [];
let mode = "normal";

function f1(ev) {
    log.push("f1:" + ev.defaultPrevented);
    if (mode === "prevent-in-on") {
        ev.preventDefault();
    } else if (mode === "return-false-in-on") {
        return false;
    } else if (mode === "stop-in-on") {
        ev.stopImmediatePropagation();
    }
}

function f2(ev) {
    log.push("f2:" + ev.defaultPrevented);
    if (mode === "prevent-in-listener") {
        ev.preventDefault();
    }
}

const btn = document.getElementById("btn");
btn.onclick = f1;
btn.addEventListener("click", f2);

const dup = document.getElementById("dup");
let dupCount = 0;
function onDup() {
    dupCount += 1;
}
dup.addEventListener("click", onDup);
dup.addEventListener("click", onDup, { once: true });

function runProgChecks() {
    const b = document.getElementById("btn");
    const r1 = b.dispatchEvent(new Event("click", { cancelable: true }));

    mode = "prevent-in-on";
    const r2 = b.dispatchEvent(new Event("click", { cancelable: true }));

    mode = "return-false-in-on";
    const r3 = b.dispatchEvent(new Event("click", { cancelable: true }));

    mode = "stop-in-on";
    const r4 = b.dispatchEvent(new Event("click", { cancelable: true }));

    mode = "prevent-in-listener";
    const r5 = b.dispatchEvent(new Event("click", { cancelable: true }));

    mode = "normal";
    return [r1, r2, r3, r4, r5].join(",") + "|" + log.join(",");
}

function setModeForQml(m) {
    log = [];
    mode = m;
}

function takeLogAndClearOnclick() {
    const out = log.join(",");
    log = [];
    document.getElementById("btn").onclick = null;
    return out;
}

function checkDuplicateOnceUpgradeIgnored() {
    const d = document.getElementById("dup");
    d.dispatchEvent(new Event("click"));
    d.dispatchEvent(new Event("click"));
    return dupCount;
}
"#
        .to_string(),
    };

    let scripts = [script];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.bindings.len(), 2);
    assert_eq!(out.bindings[0].function, "__h2q_on_click_btnItem_0");
    assert_eq!(out.bindings[1].function, "__h2q_on_click_dupItem_1");

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        r#"import QtQuick
import QtQuick.Controls.Basic
import "page.js" as Page
import "h2q_runtime.js" as H2Q

Item {
    id: root
    width: 200
    height: 100

    Button { id: btnItem }
    Button { id: dupItem }

    Component.onCompleted: {
        Page.__h2q_init(root);
        var prog = Page.runProgChecks();

        Page.setModeForQml("prevent-in-on");
        var qmlEv1 = H2Q.event("click", btnItem, null);
        Page.__h2q_on_click_btnItem_0(qmlEv1);
        var qml1 = Page.takeLogAndClearOnclick() + ":" + qmlEv1.defaultPrevented;

        Page.setModeForQml("normal");
        var qmlEv2 = H2Q.event("click", btnItem, null);
        Page.__h2q_on_click_btnItem_0(qmlEv2);
        var qml2 = Page.takeLogAndClearOnclick() + ":" + qmlEv2.defaultPrevented;

        var dupHits = Page.checkDuplicateOnceUpgradeIgnored();
        Page.__h2q_on_click_dupItem_1(H2Q.event("click", dupItem, null));
        var dupHitsAfterQml = Page.checkDuplicateOnceUpgradeIgnored();

        console.log("H2Q_OK_MIXED_ON_LISTEN", prog + "||" + qml1 + "||" + qml2 + "||" + dupHits + ":" + dupHitsAfterQml);
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
                "qmllint failed on {}: {}{}\npage.js:\n{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr),
                out.page_js
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains(
            "H2Q_OK_MIXED_ON_LISTEN true,false,false,true,false|f1:false,f2:false,f1:false,f2:true,f1:false,f2:true,f1:false,f1:false,f2:false||f1:false,f2:true:true||f2:false:false||2:5"
        ),
        "unexpected qml6 mixed onclick/addEventListener output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_executes_shorthand_property_key_preservation_across_literals_bindings_and_destructuring() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 runtime test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("shorthand_keys");
    let resolver = FakeResolver::default()
        .with_id("save-btn", 1, "save_btn", TargetKind::Button)
        .with_id("status", 2, "status_txt", TargetKind::Text);

    let s_classic = ScriptInput {
        name: "app.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: r#"
const btn = document.getElementById("save-btn");
const statusEl = document.getElementById("status");
const count = 5;
const state = { btn, count, explicit: btn };
const { picked = btn, count: cVal = 0, ...restTop } = { count: 9, extra: "ok" };
let assigned;
let kept;
({ assigned = btn, kept } = { kept: "keptVal" });

function checkClassic(input) {
    const { localBtn = btn, tag, ...restLocal } = input;
    let localAssigned;
    ({ localAssigned = statusEl } = input);
    const snap = { btn, statusEl, count, tag, explicitStatus: statusEl, ...restLocal };
    return [
        state.btn === btn,
        typeof state.save_btn === "undefined",
        state.count === 5,
        state.explicit === btn,
        picked === btn,
        cVal === 9,
        restTop.extra === "ok",
        typeof restTop.count === "undefined",
        assigned === btn,
        kept === "keptVal",
        localBtn === btn,
        localAssigned === statusEl,
        snap.btn === btn,
        snap.statusEl === statusEl,
        typeof snap.save_btn === "undefined",
        typeof snap.status_txt === "undefined",
        snap.count === 5,
        snap.tag === "t1",
        snap.explicitStatus === statusEl,
        snap.more === 42
    ].every(Boolean);
}
"#
        .to_string(),
    };

    let m1 = ScriptInput {
        name: "m1.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
export const shared = "m1";
"#
        .to_string(),
    };

    let m2 = ScriptInput {
        name: "m2.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
const btn = document.getElementById("save-btn");
const shared = "m2";
const { shared: modShared = "fallback", modPick = btn, ...modRest } = { keepMod: 77 };
let modAssign;
({ modAssign = btn } = {});
const modObj = { btn, shared, modPick, modAssign };

export function verifyMod() {
    const expectedBtn = document.getElementById("save-btn");
    return [
        modObj.btn === expectedBtn,
        typeof modObj.save_btn === "undefined",
        modObj.shared === "m2",
        modObj.modPick === expectedBtn,
        modObj.modAssign === expectedBtn,
        modShared === "fallback",
        modRest.keepMod === 77
    ].every(Boolean);
}
"#
        .to_string(),
    };

    let scripts = [s_classic, m1, m2];
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
import "h2q_runtime.js" as H2Q

Item {
    id: root
    width: 200
    height: 100

    Button { id: save_btn; text: "Save" }
    Text { id: status_txt; text: "Ready" }

    Component.onCompleted: {
        Page.__h2q_init(root);
        var okClassic = Page.checkClassic({ tag: "t1", more: 42 });
        var okMod = Page.verifyMod();
        console.log("H2Q_OK_SHORTHAND_KEYS", okClassic + ":" + okMod);
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
                "qmllint failed on {}: {}{}\npage.js:\n{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr),
                out.page_js
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains("H2Q_OK_SHORTHAND_KEYS true:true"),
        "unexpected qml6 shorthand property key output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

#[test]
fn qml6_executes_module_exports_of_block_nested_var() {
    if !has_binary("qml6") {
        eprintln!("skipping qml6 module block-nested var export test: qml6 not on PATH");
        return;
    }

    let dir = TempQmlDir::new("module_block_var_export");
    let resolver = FakeResolver::default();

    let c = ScriptInput {
        name: "c.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        source: "var count = 'classic';\n".to_string(),
    };
    let a = ScriptInput {
        name: "a.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
{ var count = 10; }
if (false) { var skipped = 1; }
export { count, skipped as renamedSkipped };
"#
        .to_string(),
    };
    let b = ScriptInput {
        name: "b.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        source: r#"
import { count, renamedSkipped } from "./a.js";
import * as ns from "./a.js";
console.log("H2Q_OK_MOD_BLOCK_VAR", count, renamedSkipped, ns.count, typeof ns.renamedSkipped);
"#
        .to_string(),
    };

    let scripts = [c, a, b];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let runtime_path = dir.write_file("h2q_runtime.js", RUNTIME_JS);
    let page_path = dir.write_file("page.js", &out.page_js);
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
                "qmllint failed on {}: {}{}\npage.js:\n{}",
                path.display(),
                String::from_utf8_lossy(&lint.stdout),
                String::from_utf8_lossy(&lint.stderr),
                out.page_js
            );
        }
    }

    let output = run_qml6_offscreen(&main_qml, Duration::from_secs(3));
    assert!(
        output.contains("H2Q_OK_MOD_BLOCK_VAR 10 undefined 10 undefined"),
        "unexpected qml6 module block-nested var output:\n{output}\npage.js:\n{}",
        out.page_js
    );
}

/// Runs `script` against `main` (imports added) and returns qml6's output.
fn run_page_allowing_unsupported(
    resolver: &FakeResolver,
    script: &str,
    main: &str,
) -> (String, JsOutput) {
    let dir = TempQmlDir::new("controls");
    let scripts = [ScriptInput {
        source: script.to_string(),
        name: "inline script #1".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
    }];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, resolver);
    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.severity == h2q_js::Severity::Unsupported),
        "{:?}",
        out.diagnostics
    );
    dir.write_file("h2q_runtime.js", RUNTIME_JS);
    dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        &format!(
            "import QtQuick\nimport QtQuick.Controls.Basic\nimport \"page.js\" as Page\n\
             import \"h2q_runtime.js\" as H2Q\n\n{main}\n"
        ),
    );
    (run_qml6_offscreen(&main_qml, Duration::from_secs(3)), out)
}

fn run_page(resolver: &FakeResolver, script: &str, main: &str) -> (String, JsOutput) {
    let dir = TempQmlDir::new("controls");
    let scripts = [ScriptInput {
        source: script.to_string(),
        name: "inline script #1".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
    }];
    let analysis = analyze(&scripts);
    let out = rewrite(&scripts, &analysis, resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    dir.write_file("h2q_runtime.js", RUNTIME_JS);
    dir.write_file("page.js", &out.page_js);
    let main_qml = dir.write_file(
        "Main.qml",
        &format!(
            "import QtQuick\nimport QtQuick.Controls.Basic\nimport \"page.js\" as Page\n\
             import \"h2q_runtime.js\" as H2Q\n\n{main}\n"
        ),
    );
    (run_qml6_offscreen(&main_qml, Duration::from_secs(3)), out)
}

#[test]
fn qml6_connects_listeners_for_each_control_signal_once() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("c", 1, "check", TargetKind::CheckBox)
        .with_id("r", 2, "range", TargetKind::Range)
        .with_id("n", 3, "spin", TargetKind::Range)
        .with_id("s", 4, "combo", TargetKind::ComboBox);
    let (output, _) = run_page(
        &resolver,
        r#"
var seen = [];
function note(e) { seen.push(e.type + ":" + e.target.value); }
function add(el) { el.addEventListener("input", note); el.addEventListener("change", note); }
function start() {
    add(document.querySelector('#c'));
    add(document.querySelector('#r'));
    add(document.querySelector('#n'));
    add(document.querySelector('#s'));
}
function report() { return seen.join(","); }
"#,
        r#"Item {
    id: root
    CheckBox { id: check; objectName: "c"; property string value: "yes" }
    Slider { id: range; objectName: "r"; from: 0; to: 10; value: 3 }
    SpinBox { id: spin; objectName: "n"; value: 4 }
    ComboBox {
        id: combo; objectName: "s"; textRole: "text"; valueRole: "value"
        model: [{ text: "Alpha", value: "a" }]
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        Page.start();
        check.toggled();
        range.moved();
        range.pressedChanged();
        spin.valueModified();
        combo.activated(0);
        console.log("H2Q_OK_SIGNALS", Page.report());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_SIGNALS input:yes,change:yes,input:3,change:3,input:4,change:4,input:a,change:a"
        ),
        "{output}"
    );
    assert!(!output.contains("cannot bind"), "{output}");
}

#[test]
fn qml6_key_listener_on_a_known_element_needs_no_connection() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("t", 1, "field", TargetKind::TextField);
    let (output, out) = run_page(
        &resolver,
        r#"
var keys = [];
function note(e) { keys.push(e.key + "/" + e.code); e.preventDefault(); }
var field = document.getElementById("t");
field.addEventListener("keydown", note);
field.removeEventListener("keydown", note);
field.addEventListener("keydown", note);
function report() { return keys.join(","); }
"#,
        r#"Item {
    id: root
    TextField { id: field; objectName: "t" }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        var qt = { key: Qt.Key_Return, text: "\r", modifiers: 0, accepted: false };
        var dom = H2Q.event("keydown", field, qt);
        Page.__h2q_on_keydown_field_0(dom);
        console.log("H2Q_OK_KEYS", Page.report(), dom.defaultPrevented, qt.accepted);
        Qt.quit();
    }
}"#,
    );
    assert!(
        out.page_js.contains("H2Q.listenBound(field"),
        "{}",
        out.page_js
    );
    assert!(
        output.contains("H2Q_OK_KEYS Enter/Enter true true"),
        "{output}"
    );
    assert!(!output.contains("cannot bind"), "{output}");
}

#[test]
fn qml6_generated_handler_defers_to_a_connected_signal() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("b", 1, "btn", TargetKind::Button);
    let (output, out) = run_page(
        &resolver,
        r#"
var hits = 0;
function hit() { hits += 1; }
var btn = document.getElementById("b");
btn.addEventListener("click", hit);
btn.removeEventListener("click", hit);
btn.addEventListener("click", hit);
function report() { return hits; }
"#,
        r#"Item {
    id: root
    Button { id: btn; objectName: "b" }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_CONNECTED", H2Q.connected(btn, "click"), H2Q.connected(btn, "input"));
        btn.clicked();
        console.log("H2Q_OK_HITS", Page.report());
        Qt.quit();
    }
}"#,
    );
    assert_eq!(out.redispatching_slots, ["__h2q_on_click_btn_0"]);
    assert!(output.contains("H2Q_OK_CONNECTED true false"), "{output}");
    assert!(output.contains("H2Q_OK_HITS 1"), "{output}");
}

#[test]
fn qml6_created_input_becomes_what_its_type_says_and_keeps_listeners() {
    if !has_binary("qml6") {
        return;
    }
    let (output, _) = run_page(
        &FakeResolver::default(),
        r#"
var seen = [];
function run() {
    var box = document.createElement("input");
    box.addEventListener("change", function () { seen.push("change:" + box.checked); });
    box.type = "checkbox";
    box.checked = true;
    box._item.toggled();
    var text = document.createElement("input");
    text.addEventListener("keydown", function (e) { seen.push("key:" + e.key); });
    text._item.h2qKeyDown({ key: Qt.Key_A, text: "a", modifiers: 0, accepted: false });
    text.value = "v";
    return seen.join(",") + "|" + box.type + "|" + text.value;
}
"#,
        r#"Item {
    id: root
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_CREATED", Page.run());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains("H2Q_OK_CREATED change:true,key:a|checkbox|v"),
        "{output}"
    );
}

#[test]
fn qml6_empty_box_grows_with_appended_children_until_text_content_clears_them() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("l", 1, "list", TargetKind::Box);
    let (output, _) = run_page(
        &resolver,
        r#"
var list = document.getElementById("l");
function add() {
    var li = document.createElement("li");
    var label = document.createElement("span");
    label.textContent = "x";
    li.appendChild(label);
    list.appendChild(li);
}
function clear() {
    list.textContent = "";
}
"#,
        r#"Window {
    // Layouts settle their implicit size on the window's polish pass, so the
    // growth is only there to read after a frame.
    visible: true
    width: 200
    height: 200
    Item {
        id: root
        width: 200
        Rectangle {
            id: list
            objectName: "l"
            width: 200
            implicitHeight: listText.implicitHeight + 1
            Text { id: listText; text: ""; width: parent.width; y: 1 }
        }
    }
    Timer {
        id: settled
        interval: 100
        onTriggered: {
            var grownHeight = list.implicitHeight;
            var appended = H2Q.wrap(list).children.length;
            var textHidden = !listText.visible;
            Page.clear();
            var left = H2Q.wrap(list).children.length;
            cleared.report = [appended, textHidden, grownHeight > 1, left];
            cleared.start();
        }
    }
    Timer {
        id: cleared
        property var report
        interval: 100
        onTriggered: {
            console.log("H2Q_OK_APPEND", report.join(" "), list.implicitHeight);
            Qt.quit();
        }
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        Page.add();
        Page.add();
        settled.start();
    }
}"#,
    );
    assert!(output.contains("H2Q_OK_APPEND 2 true true 0 1"), "{output}");
}

#[test]
fn qml6_unmapped_dom_members_use_wrapper_and_skip_box_text_child() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("empty", 1, "emptyBox", TargetKind::Box)
        .with_id("list", 2, "listBox", TargetKind::Box)
        .with_id("item-1", 3, "item1", TargetKind::Box)
        .with_id("item-2", 4, "item2", TargetKind::Button);

    let (output, _) = run_page(
        &resolver,
        r##"
function verify() {
    const empty = document.getElementById("empty");
    const list = document.getElementById("list");
    const first = document.getElementById("item-1");
    const second = document.getElementById("item-2");

    list.setAttribute("data-role", "nav");
    const checks = [
        empty.children.length === 0,
        empty.childElementCount === 0,
        empty.firstElementChild === null,
        empty.lastElementChild === null,
        list.children.length === 2,
        list.childElementCount === 2,
        list.firstElementChild.id === "item-1",
        list.lastElementChild.id === "item-2",
        first.nextElementSibling.id === "item-2",
        first.previousElementSibling === null,
        second.previousElementSibling.id === "item-1",
        second.nextElementSibling === null,
        first.parentElement.id === "list",
        list.id === "list",
        list.tagName === "DIV",
        second.tagName === "BUTTON",
        list.getAttribute("data-role") === "nav"
    ];
    list.id = "renamed-list";
    checks.push(list.id === "renamed-list");
    checks.push(first.parentElement.id === "renamed-list");

    const dynA = document.createElement("div");
    dynA.id = "dyn-a";
    const dynB = document.createElement("span");
    dynB.id = "dyn-b";
    empty.appendChild(dynA);
    empty.appendChild(dynB);
    checks.push(empty.children.length === 2);
    checks.push(empty.firstElementChild.id === "dyn-a");
    checks.push(empty.lastElementChild.id === "dyn-b");
    checks.push(dynA.nextElementSibling.id === "dyn-b");
    checks.push(dynB.previousElementSibling.id === "dyn-a");
    checks.push(dynA.parentElement.id === "empty");
    return checks.map((c, idx) => c ? "ok" : ("fail@" + idx)).join(",");
}
"##,
        r#"Item {
    id: root
    Rectangle {
        id: emptyBox
        objectName: "empty"
        Text { id: emptyBoxText; text: "Only text" }
    }
    Rectangle {
        id: listBox
        objectName: "list"
        Text { id: listBoxText; text: "Header text" }
        Rectangle { id: item1; objectName: "item-1" }
        Button { id: item2; objectName: "item-2"; text: "Second" }
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_WRAP_MEMBERS", Page.verify());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains("H2Q_OK_WRAP_MEMBERS") && !output.contains("fail@"),
        "{output}"
    );
}

#[test]
fn qml6_range_and_spinbox_value_reads_as_string_and_increments_correctly() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("range", 1, "rangeSlider", TargetKind::Range)
        .with_id("spin", 2, "spinBox", TargetKind::Range)
        .with_id("prog", 3, "progBar", TargetKind::Progress);

    let (output, _) = run_page(
        &resolver,
        r##"
function verify() {
    const range = document.getElementById("range");
    const spin = document.getElementById("spin");
    const prog = document.getElementById("prog");
    const dynRange = document.querySelector("#range");
    const dynSpin = document.querySelector("#spin");

    const r1 = range.value + 1;
    const dr1 = dynRange.value + 1;
    range.value = 2.5;
    const r2 = range.value;
    const r2Concat = range.value + 1;
    const dr2 = dynRange.value;

    const sBefore = spin.value + 1;
    spin.value++;
    const sAfterInc = spin.value;
    const dsAfterInc = dynSpin.value;
    dynSpin.value++;
    const sAfterDynInc = spin.value;
    --spin.value;
    const sAfterDec = spin.value;

    const pType = typeof prog.value;
    const pAdd = prog.value + 1;

    range.value = 2;
    range.value += 1;
    const rCompound = range.value;

    spin.value = 2;
    spin.value += 1;
    const sCompound = spin.value;

    return [r1, dr1, r2, r2Concat, dr2, sBefore, sAfterInc, dsAfterInc, sAfterDynInc, sAfterDec, pType, pAdd, rCompound, sCompound].join("|");
}
"##,
        r#"Item {
    id: root
    Slider { id: rangeSlider; objectName: "range"; from: 0; to: 10; stepSize: 0.5; value: 2 }
    SpinBox { id: spinBox; objectName: "spin"; from: 0; to: 100; value: 2 }
    ProgressBar { id: progBar; objectName: "prog"; from: 0; to: 10; value: 4 }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_RANGE_STR", Page.verify());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains("H2Q_OK_RANGE_STR 21|21|2.5|2.51|2.5|21|3|3|4|3|number|5|10|21"),
        "{output}"
    );
}

#[test]
fn qml6_reflected_attributes_and_control_name_property() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("seats", 1, "seatsInput", TargetKind::TextField)
        .with_id("link", 2, "navLink", TargetKind::Text)
        .with_id("pic", 3, "heroImg", TargetKind::Image);

    let (output, _) = run_page(
        &resolver,
        r##"
function verify() {
    const input = document.getElementById("seats");
    const a = document.getElementById("link");
    const img = document.getElementById("pic");

    const initialName = input.name;
    input.name = "passengers";
    const updatedName = input.name;

    const missingTitle = a.title;
    a.setAttribute("title", "Go home");
    const attrTitle = a.title;
    a.title = "Updated title";
    const getAttrTitle = a.getAttribute("title");

    const w = img.width;
    const h = img.height;

    return [initialName, updatedName, missingTitle, attrTitle, getAttrTitle, w, h].join("|");
}
"##,
        r#"Item {
    id: root
    TextField { id: seatsInput; objectName: "seats"; property string name: "seats" }
    Text { id: navLink; objectName: "link"; text: "Home" }
    Image { id: heroImg; objectName: "pic"; width: 120; height: 80 }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_REFLECTED", Page.verify());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains("H2Q_OK_REFLECTED seats|passengers||Go home|Updated title|120|80"),
        "{output}"
    );
}

#[test]
fn qml6_anchor_href_resolves_against_document_base_and_preserves_raw_attribute() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("abs", 1, "absLink", TargetKind::Text)
        .with_id("norm", 2, "normLink", TargetKind::Text)
        .with_id("rel", 3, "relLink", TargetKind::Text)
        .with_id("rich", 4, "richLink", TargetKind::Text);

    let (output, _) = run_page_allowing_unsupported(
        &resolver,
        r##"
function verifyBeforeBase() {
    const abs = document.getElementById("abs");
    const norm = document.getElementById("norm");
    const rel = document.getElementById("rel");

    abs.setAttribute("href", "https://example.com");
    norm.setAttribute("href", "https://example.com/a/b/../c?q=1#top");
    rel.setAttribute("href", "/docs/guide.html");

    // Before a document base URL is configured via init(root, baseUrl) or root.baseUrl,
    // relative URLs return the raw attribute. Assigning location.href warns and does
    // not configure the base URL.
    location.href = "https://ignored.example/should-not-become-base.html";
    const relWithoutBase = rel.href;
    const relRawBefore = rel.getAttribute("href");

    return [
        abs.href,
        abs.getAttribute("href"),
        norm.href,
        relWithoutBase,
        relRawBefore
    ].join("|");
}

function verifyWithBase() {
    const rel = document.getElementById("rel");
    const rich = document.getElementById("rich");

    const relRootResolved = rel.href;

    rel.href = "../about.html#team";
    const relDotDotResolved = rel.href;
    const relDotDotRaw = rel.getAttribute("href");

    rel.href = "?page=2";
    const relQueryResolved = rel.href;

    rel.href = "#section";
    const relHashResolved = rel.href;

    rel.href = "mailto:info@example.com";
    const mailtoHref = rel.href;

    return [
        relRootResolved,
        relDotDotResolved,
        relDotDotRaw,
        relQueryResolved,
        relHashResolved,
        mailtoHref,
        rich.getAttribute("href"),
        rich.href
    ].join("|");
}
"##,
        r#"Item {
    id: root
    property string baseUrl: ""
    Text { id: absLink; objectName: "abs"; text: "Abs" }
    Text { id: normLink; objectName: "norm"; text: "Norm" }
    Text { id: relLink; objectName: "rel"; text: "Rel" }
    Text { id: richLink; objectName: "rich"; text: "<a href=\"https://example.org\">Rich</a>" }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        const beforeBase = Page.verifyBeforeBase();
        root.baseUrl = "https://example.com/app/sub/index.html?v=1";
        H2Q.init(root, root.baseUrl);
        const withBase = Page.verifyWithBase();
        console.log("H2Q_OK_HREF", beforeBase + "|" + withBase);
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_HREF https://example.com/|https://example.com|https://example.com/a/c?q=1#top|/docs/guide.html|/docs/guide.html|https://example.com/docs/guide.html|https://example.com/app/about.html#team|../about.html#team|https://example.com/app/sub/index.html?page=2|https://example.com/app/sub/index.html?v=1#section|mailto:info@example.com|https://example.org|https://example.org/"
        ),
        "{output}"
    );
    assert!(
        output.contains("H2Q: location.href navigation is unsupported in QML: https://ignored.example/should-not-become-base.html"),
        "{output}"
    );
}

#[test]
fn qml6_slider_and_spinbox_min_max_step_update_qml_from_to_stepsize_and_attributes() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("rng", 1, "rngSlider", TargetKind::Range)
        .with_id("num", 2, "numSpin", TargetKind::TextField);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyRangeAndSpin() {
    const rng = document.getElementById("rng");
    const num = document.getElementById("num");

    rng.min = 10;
    rng.max = 200;
    rng.step = 5;
    rng.value = 25;

    num.min = "-50";
    num.max = "150";
    num.step = "10";

    const dynRange = document.createElement("input");
    dynRange.min = "20";
    dynRange.max = "80";
    dynRange.step = "4";
    dynRange.type = "range";
    dynRange.max = "90";

    const dynNum = document.createElement("input");
    dynNum.type = "number";
    dynNum.setAttribute("min", "5");
    dynNum.setAttribute("max", "55");
    dynNum.setAttribute("step", "5");

    return [
        rng.min, rng.max, rng.step,
        rng.getAttribute("min"), rng.getAttribute("max"), rng.getAttribute("step"),
        num.min, num.max, num.step,
        num.getAttribute("min"), num.getAttribute("max"), num.getAttribute("step"),
        dynRange._item.from, dynRange._item.to, dynRange._item.stepSize,
        dynNum._item.from, dynNum._item.to, dynNum._item.stepSize
    ].join("|");
}
"##,
        r#"Item {
    id: root
    Slider { id: rngSlider; objectName: "rng"; from: 0; to: 100; stepSize: 1; value: 50 }
    SpinBox { id: numSpin; objectName: "num"; from: 0; to: 99; stepSize: 1; value: 10 }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        const jsRes = Page.verifyRangeAndSpin();
        console.log(
            "H2Q_OK_MINMAXSTEP",
            [jsRes, rngSlider.from, rngSlider.to, rngSlider.stepSize, numSpin.from, numSpin.to, numSpin.stepSize].join("|")
        );
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_MINMAXSTEP 10|200|5|10|200|5|-50|150|10|-50|150|10|20|90|4|5|55|5|10|200|5|-50|150|10"
        ),
        "{output}"
    );
}

#[test]
fn qml6_selectors_closest_matches_contains_and_combinators() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("card", 1, "cardBox", TargetKind::Box)
        .with_id("saveBtn", 2, "saveBtn", TargetKind::Button)
        .with_id("cancelBtn", 3, "cancelBtn", TargetKind::Button);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifySelectors() {
    const card = document.getElementById("card");
    const save = document.getElementById("saveBtn");
    const cancel = document.getElementById("cancelBtn");

    card.setAttribute("data-section", "billing");
    save.setAttribute("data-action", "save");
    cancel.setAttribute("data-action", "cancel");

    const m1 = save.matches(".btn.primary");
    const m2 = save.matches("button[data-action=\"save\"]");
    const m3 = save.matches(".card .btn");
    const m4 = save.matches(".card > .btn");
    const m5 = save.matches(".nonexistent");

    const cSelf = save.closest("button");
    const cCard = save.closest(".card[data-section=\"billing\"]");
    const cNone = save.closest(".missing");

    const hasSave = card.contains(save);
    const hasSelf = card.contains(card);
    const revContains = save.contains(card);

    const qsAttr = document.querySelector(".card button[data-action=\"cancel\"]");
    const qsaList = document.querySelectorAll("#saveBtn, [data-action=\"cancel\"]");
    const byCls = card.getElementsByClassName("btn primary");
    const byTag = card.getElementsByTagName("button");

    return [
        m1, m2, m3, m4, m5,
        cSelf ? cSelf.id : "null",
        cCard ? cCard.id : "null",
        cNone === null,
        hasSave, hasSelf, revContains,
        qsAttr ? qsAttr.id : "null",
        qsaList.length,
        byCls.length,
        byTag.length
    ].join("|");
}
"##,
        r#"Item {
    id: root
    Rectangle {
        id: cardBox
        objectName: "card"
        property string h2qTag: "div"
        property string h2qClasses: "card panel"
        Text { id: cardBoxText; text: "" }
        Column {
            Button {
                id: saveBtn
                objectName: "saveBtn"
                property string h2qTag: "button"
                property string h2qClasses: "btn primary"
                text: "Save"
            }
            Button {
                id: cancelBtn
                objectName: "cancelBtn"
                property string h2qTag: "button"
                property string h2qClasses: "btn secondary"
                text: "Cancel"
            }
        }
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_SELECTORS", Page.verifySelectors());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_SELECTORS true|true|true|true|false|saveBtn|card|true|true|true|false|cancelBtn|2|1|2"
        ),
        "{output}"
    );
}

#[test]
fn qml6_dom_mutations_insert_adjacent_replace_append_prepend_before_after_clone() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("list", 1, "listBox", TargetKind::Box);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyMutations() {
    const list = document.getElementById("list");

    const mid = document.createElement("span");
    mid.id = "mid";
    mid.className = "item";
    mid.dataset.idx = "2";
    mid.textContent = "Middle";
    list.appendChild(mid);

    mid.insertAdjacentHTML("beforebegin", "<span id=\"first\" class=\"item\" data-idx=\"1\">First</span>");
    mid.insertAdjacentHTML("afterend", "<span id=\"last\" class=\"item\" data-idx=\"3\">Last</span>");

    const cloned = mid.cloneNode(true);
    cloned.id = "cloned";
    list.append(cloned);

    const head = document.createElement("span");
    head.id = "head";
    head.textContent = "Head";
    list.prepend(head);

    const beforeMid = document.createElement("span");
    beforeMid.id = "bmid";
    mid.before(beforeMid);

    const afterMid = document.createElement("span");
    afterMid.id = "amid";
    mid.after(afterMid);

    const rep = document.createElement("span");
    rep.id = "rep";
    afterMid.replaceWith(rep);

    const ids1 = Array.from(list.children).map((c) => c.id).join(",");
    const sibs = [
        list.firstChild ? list.firstChild.id : "",
        list.lastChild ? list.lastChild.id : "",
        mid.previousSibling ? mid.previousSibling.id : "",
        mid.nextSibling ? mid.nextSibling.id : ""
    ].join(",");

    const frag = document.createDocumentFragment();
    const f1 = document.createElement("span");
    f1.id = "f1";
    const f2 = document.createElement("span");
    f2.id = "f2";
    frag.appendChild(f1);
    frag.appendChild(f2);
    list.replaceChildren(frag);
    const ids2 = Array.from(list.children).map((c) => c.id).join(",");

    const f3 = document.createElement("span");
    f3.id = "f3";
    list.replaceChild(f3, f1);
    const ids3 = Array.from(list.children).map((c) => c.id).join(",");

    return [ids1, sibs, cloned.dataset.idx, cloned.textContent, ids2, ids3].join("|");
}
"##,
        r#"Item {
    id: root
    Rectangle {
        id: listBox
        objectName: "list"
        implicitHeight: 20
        Text { id: listBoxText; text: "" }
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_MUTATIONS", Page.verifyMutations());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_MUTATIONS head,first,bmid,mid,rep,last,cloned|head,cloned,bmid,rep|2|Middle|f1,f2|f3,f2"
        ),
        "{output}"
    );
}

#[test]
fn qml6_focus_blur_scroll_computed_style_and_attribute_helpers() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("field", 1, "fieldInput", TargetKind::TextField)
        .with_id("box", 2, "boxRect", TargetKind::Box)
        .with_id("sel", 3, "selCombo", TargetKind::ComboBox);

    let (output, _) = run_page_allowing_unsupported(
        &resolver,
        r##"
function verifyFocusScrollStyle() {
    const field = document.getElementById("field");
    const box = document.getElementById("box");
    const sel = document.getElementById("sel");

    const focusLog = [];
    field.addEventListener("focus", () => focusLog.push("focus"));
    field.addEventListener("blur", () => focusLog.push("blur"));
    field.focus();
    const activeId = document.activeElement ? document.activeElement.id : "";
    field.blur();

    field.readOnly = true;
    field.placeholder = "Enter name";
    const toggle1 = field.toggleAttribute("data-flag");
    const toggle2 = field.toggleAttribute("data-flag");
    const toggle3 = field.toggleAttribute("data-flag", true);

    box.classList.add("alpha", "beta");
    box.classList.remove("alpha");
    const repOk = box.classList.replace("beta", "gamma");
    const togForce = box.classList.toggle("gamma", true);

    box.style.setProperty("--accent", "#ff5500");
    const customVal = box.style.getPropertyValue("--accent");
    box.style.removeProperty("--accent");
    const removedVal = box.style.getPropertyValue("--accent");
    box.style.cssText = "width: 140px; height: 60px; opacity: 0.75; border-radius: 8px";
    const cs = getComputedStyle(box);

    box.scrollIntoView();
    window.scrollTo(15, 40);
    window.scrollBy({ left: 5, top: 10 });
    box.scrollTop = 25;

    sel.selectedIndex = 1;
    const rect = box.getBoundingClientRect();

    return [
        focusLog.join(","),
        activeId,
        field.readOnly,
        field.placeholder,
        toggle1, toggle2, toggle3,
        box.className, repOk, togForce,
        customVal, removedVal,
        cs.width, cs.height, cs.opacity,
        window.scrollX, window.scrollY, window.pageXOffset, window.pageYOffset,
        box.scrollTop,
        sel.selectedIndex, sel.value,
        rect.width, rect.height, box.offsetWidth, box.clientHeight
    ].join("|");
}
"##,
        r#"Item {
    id: root
    width: 800
    height: 600
    focus: true
    TextField { id: fieldInput; objectName: "field" }
    Rectangle {
        id: boxRect
        objectName: "box"
        width: 100
        height: 50
        Text { id: boxRectText; text: "Box" }
    }
    ComboBox {
        id: selCombo
        objectName: "sel"
        model: ["One", "Two", "Three"]
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_FOCUS_SCROLL_STYLE", Page.verifyFocusScrollStyle());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_FOCUS_SCROLL_STYLE focus,blur|field|true|Enter name|true|false|true|gamma|true|true|#ff5500||140px|60px|0.75|0|0|0|0|0|1|Two|140|60|140|60"
        ),
        "{output}"
    );
}

#[test]
fn qml6_browser_globals_timers_events_and_observers() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("btn", 1, "actBtn", TargetKind::Button);

    let (output, _) = run_page_allowing_unsupported(
        &resolver,
        r##"
function verifyGlobalsAndEvents(done) {
    const btn = document.getElementById("btn");
    const log = [];

    // EventTarget + typed Event constructors + stopImmediatePropagation
    const bus = new EventTarget();
    bus.addEventListener("ping", (e) => {
        log.push("bus:" + e.detail);
        e.stopImmediatePropagation();
    });
    bus.addEventListener("ping", () => log.push("bus:second"));
    bus.dispatchEvent(new CustomEvent("ping", { detail: 42 }));

    btn.addEventListener("keydown", (e) => {
        log.push("key:" + e.key + ":" + e.keyCode + ":" + e.ctrlKey);
    });
    btn.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, ctrlKey: true }));

    btn.addEventListener("click", (e) => {
        log.push("click:" + e.clientX + ":" + e.clientY + ":" + (e.currentTarget === btn || e.currentTarget._item === btn));
        e.preventDefault();
    });
    const clickRes = btn.dispatchEvent(new MouseEvent("click", { clientX: 12, clientY: 34, cancelable: true }));
    log.push("clickRes:" + clickRes);

    // hashchange & popstate
    window.addEventListener("hashchange", () => log.push("hash:" + location.hash));
    window.addEventListener("popstate", (e) => log.push("pop:" + (e.state && e.state.p)));
    location.hash = "tab1";
    history.replaceState({ p: 7 }, "", "/step7");
    history.pushState({ p: 8 }, "", "/step8");
    history.back();

    // Observer stubs
    const mo = new MutationObserver(() => {});
    const ro = new ResizeObserver(() => {});
    const io = new IntersectionObserver(() => {});
    mo.observe(btn);
    ro.observe(btn);
    io.observe(btn);
    mo.disconnect();

    // Globals
    const cloned = structuredClone({ a: [1, 2] });
    const b64 = btoa("hi");
    const dec = atob(b64);
    const mq = matchMedia("(min-width: 400px)");
    const t0 = performance.now();
    const vis = document.visibilityState + ":" + document.hidden + ":" + window.devicePixelRatio;

    // setInterval + clearInterval + clipboard
    let ticks = 0;
    const cancelledId = setTimeout(() => { ticks += 100; }, 5);
    clearTimeout(cancelledId);

    navigator.clipboard.writeText("copied-text").then(() => navigator.clipboard.readText()).then((clip) => {
        const iv = setInterval(() => {
            ticks += 1;
            if (ticks === 2) {
                clearInterval(iv);
                done([
                    log.join(","),
                    cloned.a.join("+"),
                    dec,
                    mq.matches,
                    t0 >= 0,
                    vis,
                    clip,
                    ticks
                ].join("|"));
            }
        }, 5);
    });
}
"##,
        r#"Item {
    id: root
    width: 500
    height: 400
    Button { id: actBtn; objectName: "btn"; text: "Act" }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        Page.verifyGlobalsAndEvents(function(res) {
            console.log("H2Q_OK_GLOBALS_EVENTS", res);
            Qt.quit();
        });
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_GLOBALS_EVENTS bus:42,key:Enter:13:true,click:12:34:true,clickRes:false,hash:#tab1,pop:7|1+2|hi|true|true|visible:false:1|copied-text|2"
        ),
        "{output}"
    );
}

#[test]
fn qml6_user_object_and_class_methods_are_not_hijacked_by_h2q() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default();
    let (output, _) = run_page(
        &resolver,
        r##"
class CustomWidget {
    constructor(label) {
        this.label = label;
        this.events = [];
        this.flags = {};
    }
    matches(q) { return this.label + ":matches:" + q; }
    closest(q) { return this.label + ":closest:" + q; }
    cloneNode(deep) { return this.label + ":clone:" + Boolean(deep); }
    toggleAttribute(k, force) {
        const next = force !== undefined ? Boolean(force) : !this.flags[k];
        this.flags[k] = next;
        return this.label + ":toggle:" + k + ":" + next;
    }
    addEventListener(type, fn) {
        this.events.push(type + ":" + fn());
        return this.label + ":listen:" + type;
    }
}

function invokeThroughUnknownReceiver(obj) {
    return [
        obj.matches(".item"),
        obj.closest("#root"),
        obj.cloneNode(true),
        obj.toggleAttribute("active", true),
        obj.addEventListener("custom", () => "fired")
    ].join(",");
}

function verifyUserObjects() {
    const inst = new CustomWidget("cls");
    const directRes = [
        inst.matches("a"),
        inst.closest("b"),
        inst.cloneNode(false),
        inst.toggleAttribute("pin"),
        inst.addEventListener("ready", () => "go")
    ].join(",");

    const plain = {
        matches: (q) => "plain:matches:" + q,
        closest: (q) => "plain:closest:" + q,
        cloneNode: (d) => "plain:clone:" + Boolean(d),
        toggleAttribute: (k, f) => "plain:toggle:" + k + ":" + Boolean(f),
        addEventListener: (t, fn) => "plain:listen:" + t + ":" + fn()
    };

    const dynCls = invokeThroughUnknownReceiver(inst);
    const dynPlain = invokeThroughUnknownReceiver(plain);
    return [directRes, dynCls, dynPlain, inst.events.join("+")].join("|");
}
"##,
        r#"Item {
    id: root
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_USER_OBJECTS", Page.verifyUserObjects());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_USER_OBJECTS cls:matches:a,cls:closest:b,cls:clone:false,cls:toggle:pin:true,cls:listen:ready|cls:matches:.item,cls:closest:#root,cls:clone:true,cls:toggle:active:true,cls:listen:custom|plain:matches:.item,plain:closest:#root,plain:clone:true,plain:toggle:active:true,plain:listen:custom:fired|ready:go+custom:fired"
        ),
        "{output}"
    );
}

#[test]
fn qml6_overflow_scroll_container_scrolltop_scrollleft_scrollto_scrollby() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("scroller", 1, "scrollBox", TargetKind::Box)
        .with_id("plainScroller", 2, "plainScrollItem", TargetKind::Box);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyElementScroll() {
    const scroller = document.getElementById("scroller");
    const plain = document.getElementById("plainScroller");
    let scrollEvents = 0;
    scroller.addEventListener("scroll", () => { scrollEvents += 1; });

    const sh = scroller.scrollHeight;
    const sw = scroller.scrollWidth;
    const ch = scroller.clientHeight;
    const cw = scroller.clientWidth;

    scroller.scrollTop = 35;
    const top1 = scroller.scrollTop;

    scroller.scrollBy({ left: 25, top: 20 });
    const top2 = scroller.scrollTop;
    const left2 = scroller.scrollLeft;

    // Clamping past max scroll (scrollHeight 160 - clientHeight 60 = 100, scrollWidth 220 - clientWidth 100 = 120)
    scroller.scrollTo(500, 500);
    const clampedTop = scroller.scrollTop;
    const clampedLeft = scroller.scrollLeft;

    // Plain clipped Item wrapper (emitted by h2q_convert when overflow: auto has no background)
    plain.scrollTo({ top: 30 });
    const plainTop = plain.scrollTop;
    const plainKidsCount = plain.children.length;

    return [
        sh, sw, ch, cw,
        top1, top2, left2,
        clampedTop, clampedLeft,
        scrollEvents,
        plainTop, plainKidsCount
    ].join("|");
}
"##,
        r#"import QtQuick
import QtQuick.Layouts

Item {
    id: root
    Rectangle {
        id: scrollBox
        objectName: "scroller"
        width: 100
        height: 60
        implicitHeight: scrollInner.implicitHeight
        clip: true
        ColumnLayout {
            id: scrollInner
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            implicitWidth: 220
            implicitHeight: 160
            spacing: 0
            Text { text: "Row 1"; Layout.preferredWidth: 220; Layout.preferredHeight: 80 }
            Text { text: "Row 2"; Layout.preferredHeight: 80 }
        }
    }
    Item {
        id: plainScrollItem
        objectName: "plainScroller"
        width: 100
        height: 50
        implicitHeight: plainInner.implicitHeight
        clip: true
        ColumnLayout {
            id: plainInner
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            implicitHeight: 120
            spacing: 0
            Rectangle { objectName: "c1"; implicitHeight: 60; Text { text: "A" } }
            Rectangle { objectName: "c2"; implicitHeight: 60; Text { text: "B" } }
        }
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_EL_SCROLL", Page.verifyElementScroll(), scrollInner.anchors.topMargin, scrollInner.anchors.leftMargin, plainInner.anchors.topMargin);
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains("H2Q_OK_EL_SCROLL 160|220|60|100|35|55|25|100|120|5|30|2 -100 -120 -30"),
        "{output}"
    );
}

#[test]
fn qml6_init_base_url_resolves_relative_anchor_href_and_img_src() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("navLink", 1, "navLinkText", TargetKind::Text)
        .with_id("heroImg", 2, "heroImage", TargetKind::Image);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyBaseUrlResolution() {
    const link = document.getElementById("navLink");
    const img = document.getElementById("heroImg");

    const hrefProp = link.href;
    const hrefAttr = link.getAttribute("href");

    img.setAttribute("src", "../assets/logo.png");
    const srcProp1 = img.src;
    const srcAttr1 = img.getAttribute("src");

    img.src = "icons/check.svg";
    const srcProp2 = img.src;
    const srcAttr2 = img.getAttribute("src");

    return [hrefProp, hrefAttr, srcProp1, srcAttr1, srcProp2, srcAttr2].join("|");
}
"##,
        r#"Item {
    id: root
    Text {
        id: navLinkText
        objectName: "navLink"
        property string h2qTag: "a"
        textFormat: Text.RichText
        text: "<a href=\"docs/guide.html\">Guide</a>"
    }
    Image {
        id: heroImage
        objectName: "heroImg"
    }
    Component.onCompleted: {
        H2Q.init(root, "https://example.org/app/sub/index.html");
        if (Page.__h2q_init) Page.__h2q_init(root);
        console.log("H2Q_OK_BASE_URL", Page.verifyBaseUrlResolution());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_BASE_URL https://example.org/app/sub/docs/guide.html|docs/guide.html|https://example.org/app/assets/logo.png|../assets/logo.png|https://example.org/app/sub/icons/check.svg|icons/check.svg"
        ),
        "{output}"
    );
}

#[test]
fn qml6_html_collection_vs_node_list_methods_and_named_item() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("box", 1, "boxRect", TargetKind::Box)
        .with_id("firstItem", 2, "firstBtn", TargetKind::Button)
        .with_id("secondItem", 3, "secondBtn", TargetKind::Button);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyCollections() {
    const box = document.getElementById("box");
    const second = document.getElementById("secondItem");
    second.setAttribute("name", "namedSecond");

    const childrenCol = box.children;
    const byClassCol = box.getElementsByClassName("act");
    const byTagCol = box.getElementsByTagName("button");
    const qsaList = box.querySelectorAll(".act");
    const childNodesList = box.childNodes;

    const htmlCollectionHasNoForEach =
        childrenCol.forEach === undefined &&
        byClassCol.forEach === undefined &&
        byTagCol.forEach === undefined;

    const nodeListHasForEach =
        typeof qsaList.forEach === "function" &&
        typeof childNodesList.forEach === "function" &&
        typeof qsaList.item === "function" &&
        typeof childNodesList.item === "function";

    const item0 = childrenCol.item(0) ? childrenCol.item(0).id : "null";
    const itemOob = childrenCol.item(99) === null;
    const namedById = childrenCol.namedItem("firstItem") ? childrenCol.namedItem("firstItem").id : "null";
    const namedByName = byTagCol.namedItem("namedSecond") ? byTagCol.namedItem("namedSecond").id : "null";
    const propById = childrenCol.firstItem ? childrenCol.firstItem.id : "null";

    const qsaIds = [];
    qsaList.forEach((el) => qsaIds.push(el.id));
    const qsaItem1 = qsaList.item(1) ? qsaList.item(1).id : "null";

    return [
        htmlCollectionHasNoForEach,
        nodeListHasForEach,
        item0,
        itemOob,
        namedById,
        namedByName,
        propById,
        qsaIds.join(","),
        qsaItem1
    ].join("|");
}
"##,
        r#"Item {
    id: root
    Rectangle {
        id: boxRect
        objectName: "box"
        Text { id: boxRectText; text: "" }
        Column {
            Button {
                id: firstBtn
                objectName: "firstItem"
                property string h2qTag: "button"
                property string h2qClasses: "act primary"
                text: "First"
            }
            Button {
                id: secondBtn
                objectName: "secondItem"
                property string h2qTag: "button"
                property string h2qClasses: "act secondary"
                text: "Second"
            }
        }
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_COLLECTIONS", Page.verifyCollections());
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains(
            "H2Q_OK_COLLECTIONS true|true|firstItem|true|firstItem|secondItem|firstItem|firstItem,secondItem|secondItem"
        ),
        "{output}"
    );
}

#[test]
fn qml6_window_scroll_drives_root_flickable_with_chromium_clamping() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("target", 1, "targetBox", TargetKind::Box);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyWindowScroll() {
    let scrollEvents = 0;
    window.addEventListener("scroll", () => { scrollEvents += 1; });

    window.scrollTo(15, 40);
    const s1 = window.scrollX + "," + window.scrollY;

    window.scrollBy({ left: 5, top: 10 });
    const s2 = [window.scrollX, window.scrollY, window.pageXOffset, window.pageYOffset].join(",");

    window.scrollTo(5000, 5000);
    const sMax = window.scrollX + "," + window.scrollY;

    window.scrollTo(-50, -50);
    const sMin = window.scrollX + "," + window.scrollY;

    document.getElementById("target").scrollIntoView();
    const sInto = window.scrollX + "," + window.scrollY;

    return [s1, s2, sMax, sMin, sInto, scrollEvents].join("|");
}
"##,
        r#"import QtQuick

Item {
    id: root
    width: 800
    height: 600
    Flickable {
        id: flickable
        anchors.fill: parent
        contentWidth: page.implicitWidth
        contentHeight: page.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        Item {
            id: page
            implicitWidth: 1200
            implicitHeight: 1500
            Rectangle {
                id: targetBox
                objectName: "target"
                x: 120
                y: 340
                width: 100
                height: 50
                Text { id: targetBoxText; text: "Target" }
            }
        }
    }
    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_WIN_SCROLL", Page.verifyWindowScroll(), flickable.contentX, flickable.contentY);
        Qt.quit();
    }
}"#,
    );
    assert!(
        output.contains("H2Q_OK_WIN_SCROLL 15,40|20,50,20,50|400,900|0,0|0,340|5 0 340"),
        "{output}"
    );
}

#[test]
fn qml6_template_lookup_and_fallback_chain_with_single_warning() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("todo-list", 1, "todoList", TargetKind::Box)
        .with_id("other-box", 2, "otherBox", TargetKind::Box);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyTemplateLookup() {
    const todoList = document.getElementById("todo-list");
    const otherBox = document.getElementById("other-box");

    // Step 1 exact hit: classes filtered by h2qStyledClasses ("extra" ignored) and sorted ("done", "todo-item")
    const li = document.createElement("li");
    li.className = "todo-item extra done";
    const cb = document.createElement("input");
    cb.setAttribute("type", "checkbox");
    cb.className = "toggle";
    li.appendChild(cb);
    todoList.appendChild(li);

    // Step 2 fallback (tag.classes@*): span.badge@todoList -> span.badge@*
    const badge1 = document.createElement("span");
    badge1.className = "badge";
    todoList.appendChild(badge1);
    // Second miss of the same exact key must NOT warn a second time.
    const badge2 = document.createElement("span");
    badge2.className = "badge";
    todoList.appendChild(badge2);

    // Step 3 fallback (tag@context): span.unknown-styled@todoList -> span@todoList
    const sCtx = document.createElement("span");
    sCtx.className = "unknown-styled";
    todoList.appendChild(sCtx);

    // Step 4 fallback (tag@*): p.unknown-styled@otherBox -> p@*
    const pStar = document.createElement("p");
    pStar.className = "unknown-styled";
    otherBox.appendChild(pStar);

    // Step 5 fallback (generic source): div.unknown-styled@todoList -> generic
    const divGen1 = document.createElement("div");
    divGen1.className = "unknown-styled";
    todoList.appendChild(divGen1);
    const divGen2 = document.createElement("div");
    divGen2.className = "unknown-styled";
    todoList.appendChild(divGen2);

    return [
        li._item.h2qTemplate,
        li._item.h2qClasses,
        cb._item.h2qTemplate,
        badge1._item.h2qTemplate,
        badge2._item.h2qTemplate,
        sCtx._item.h2qTemplate,
        pStar._item.h2qTemplate,
        divGen1._item.h2qTemplate || "generic",
        divGen1.tagName
    ].join("|");
}
"##,
        r#"import QtQuick
import QtQuick.Layouts

Item {
    id: root
    readonly property var h2qStyledClasses: ["badge", "done", "todo-item", "toggle", "unknown-styled"]
    readonly property var h2qTemplates: ({
        "li.done.todo-item@todoList": h2qT_li_exact,
        "input[type=checkbox].toggle@h2qT_li_exact": h2qT_input_cb,
        "span.badge@*": h2qT_span_star,
        "span@todoList": h2qT_span_ctx,
        "p@*": h2qT_p_star
    })

    Component {
        id: h2qT_li_exact
        RowLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_exact"
            property bool cls_done: false
        }
    }
    Component {
        id: h2qT_input_cb
        CheckBox {
            property string h2qTag: "input"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_input_cb"
        }
    }
    Component {
        id: h2qT_span_star
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_star"
        }
    }
    Component {
        id: h2qT_span_ctx
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_ctx"
        }
    }
    Component {
        id: h2qT_p_star
        Text {
            property string h2qTag: "p"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_p_star"
        }
    }

    ColumnLayout {
        id: todoList
        objectName: "todo-list"
        property string h2qTag: "ul"
        property string h2qKey: "todoList"
    }
    ColumnLayout {
        id: otherBox
        objectName: "other-box"
        property string h2qTag: "div"
        property string h2qKey: "otherBox"
    }

    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_LOOKUP", Page.verifyTemplateLookup());
        Qt.quit();
    }
}"#,
    );

    assert!(
        output.contains("H2Q_OK_LOOKUP h2qT_li_exact|todo-item extra done|h2qT_input_cb|h2qT_span_star|h2qT_span_star|h2qT_span_ctx|h2qT_p_star|generic|DIV"),
        "{output}"
    );
    assert_eq!(
        output
            .matches("no style template for li.done.todo-item@todoList")
            .count(),
        0,
        "{output}"
    );
    assert_eq!(
        output
            .matches("h2q: no style template for span.badge@todoList; using span.badge@*")
            .count(),
        1,
        "{output}"
    );
    assert_eq!(
        output
            .matches("h2q: no style template for span.unknown-styled@todoList; using span@todoList")
            .count(),
        1,
        "{output}"
    );
    assert_eq!(
        output
            .matches("h2q: no style template for p.unknown-styled@otherBox; using p@*")
            .count(),
        1,
        "{output}"
    );
    assert_eq!(
        output
            .matches("h2q: no style template for div.unknown-styled@todoList; using generic")
            .count(),
        1,
        "{output}"
    );
}

#[test]
fn qml6_template_rematerialisation_preserves_state_children_listeners_and_order() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("todo-list", 1, "todoList", TargetKind::Box);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyRematerialisation() {
    const todoList = document.getElementById("todo-list");
    const events = [];

    const first = document.createElement("li");
    first.id = "first";
    first.className = "todo-item";

    const li = document.createElement("li");
    li.className = "todo-item";
    li.id = "item-1";
    li.dataset.index = "42";
    li.style.opacity = "0.85";
    li.addEventListener("click", () => { events.push("li:" + li._item.h2qTemplate); });

    const cb = document.createElement("input");
    cb.addEventListener("change", () => { events.push("cb:" + cb.checked); });
    cb.type = "checkbox";
    cb.checked = true;
    cb.disabled = true;

    const label = document.createElement("span");
    label.className = "todo-text";
    label.textContent = "Buy oat milk";
    label.style.width = "140px";
    label.addEventListener("click", () => { events.push("label:" + label.textContent); });

    const note = document.createElement("input");
    note.value = "urgent";
    note.readOnly = true;

    li.appendChild(cb);
    li.appendChild(note);
    li.insertBefore(label, note);

    const last = document.createElement("li");
    last.id = "last";
    last.className = "todo-item";

    todoList.appendChild(first);
    todoList.appendChild(li);
    todoList.appendChild(last);

    const initialTemplates = [
        li._item.h2qTemplate,
        cb._item.h2qTemplate,
        label._item.h2qTemplate,
        note._item.h2qTemplate
    ].join(",");

    const childTagsBefore = Array.from(li.children).map((c) => c.tagName).join(",");
    const wrapperStable = (todoList.children[1] === li) && (li.children[0] === cb) && (li.children[1] === label) && (li.children[2] === note);

    // Fire listeners after initial connection rematerialisation
    li.click();
    cb._item.toggled();
    label.click();

    // Same-template class change: updates cls_done and h2qClasses without swapping _item
    const itemBeforeSame = li._item;
    li.classList.add("done");
    const sameItemKept = (li._item === itemBeforeSame);
    const clsAfterAdd = li.className + ":" + li._item.h2qClasses + ":" + li._item.cls_done;

    // Form-change class change: switches from h2qT_li_0 to h2qT_li_card in place
    li.classList.replace("todo-item", "card-item");
    const afterSwapTemplate = li._item.h2qTemplate;
    const clsAfterReplace = li.className + ":" + li._item.h2qClasses + ":" + li._item.cls_done;
    const childTagsAfter = Array.from(li.children).map((c) => c.tagName).join(",");
    const childTemplatesAfter = [cb._item.h2qTemplate, label._item.h2qTemplate, note._item.h2qTemplate].join(",");
    const listOrder = Array.from(todoList.children).map((c) => c.id).join(",");

    // Fire listener again on the newly swapped card item
    li.click();

    const stateCheck = [
        li.id,
        li.dataset.index,
        Math.round(li._item.opacity * 100),
        cb.checked,
        cb.disabled,
        label.textContent,
        label._item.width,
        note.value,
        note.readOnly
    ].join(",");

    return [
        initialTemplates,
        childTagsBefore,
        wrapperStable,
        sameItemKept,
        clsAfterAdd,
        afterSwapTemplate,
        clsAfterReplace,
        childTagsAfter,
        childTemplatesAfter,
        listOrder,
        stateCheck,
        events.join(";")
    ].join("|");
}
"##,
        r#"import QtQuick
import QtQuick.Layouts

Item {
    id: root
    readonly property var h2qStyledClasses: ["card-item", "done", "todo-item", "todo-text"]
    readonly property var h2qTemplates: ({
        "li.todo-item@todoList": h2qT_li_0,
        "li.done.todo-item@todoList": h2qT_li_0,
        "li.card-item.done@todoList": h2qT_li_card,
        "input[type=checkbox]@h2qT_li_0": h2qT_input_cb,
        "input[type=checkbox]@h2qT_li_card": h2qT_input_cb,
        "span.todo-text@h2qT_li_0": h2qT_span_text,
        "span.todo-text@h2qT_li_card": h2qT_span_text_card,
        "input@h2qT_li_0": h2qT_input_text,
        "input@h2qT_li_card": h2qT_input_text
    })

    Component {
        id: h2qT_li_0
        RowLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_0"
            property bool cls_done: false
        }
    }
    Component {
        id: h2qT_li_card
        ColumnLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_card"
            property bool cls_done: false
        }
    }
    Component {
        id: h2qT_input_cb
        CheckBox {
            property string h2qTag: "input"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_input_cb"
        }
    }
    Component {
        id: h2qT_span_text
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_text"
        }
    }
    Component {
        id: h2qT_span_text_card
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_text_card"
        }
    }
    Component {
        id: h2qT_input_text
        TextField {
            property string h2qTag: "input"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_input_text"
        }
    }

    ColumnLayout {
        id: todoList
        objectName: "todo-list"
        property string h2qTag: "ul"
        property string h2qKey: "todoList"
    }

    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_REMAT", Page.verifyRematerialisation());
        Qt.quit();
    }
}"#,
    );

    assert!(
        output.contains(
            "H2Q_OK_REMAT h2qT_li_0,h2qT_input_cb,h2qT_span_text,h2qT_input_text|INPUT,SPAN,INPUT|true|true|todo-item done:todo-item done:true|h2qT_li_card|card-item done:card-item done:true|INPUT,SPAN,INPUT|h2qT_input_cb,h2qT_span_text_card,h2qT_input_text|first,item-1,last|item-1,42,85,true,true,Buy oat milk,140,urgent,true|li:h2qT_li_0;cb:true;label:Buy oat milk;li:h2qT_li_card"
        ),
        "{output}"
    );
}

#[test]
fn qml6_template_h2q_up_n_tracks_ancestor_classes_and_parent_rematerialisation() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default()
        .with_id("todo-list", 1, "todoList", TargetKind::Box)
        .with_id("archive-list", 2, "archiveList", TargetKind::Box);

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyUpPointers() {
    const todoList = document.getElementById("todo-list");
    const archiveList = document.getElementById("archive-list");

    const li = document.createElement("li");
    li.className = "todo-item";
    const wrapDiv = document.createElement("div");
    wrapDiv.className = "wrap";
    const leaf = document.createElement("span");
    leaf.className = "todo-text";
    leaf.textContent = "Nested item";

    wrapDiv.appendChild(leaf);
    li.appendChild(wrapDiv);
    todoList.appendChild(li);

    const upInitialOk = (wrapDiv._item.h2qUp1 === li._item)
        && (leaf._item.h2qUp1 === wrapDiv._item)
        && (leaf._item.h2qUp2 === li._item);
    const strike0 = leaf._item.font.strikeout;
    const bold0 = leaf._item.font.bold;

    // Toggling class on grandparent (h2qUp2) and parent (h2qUp1)
    li.classList.toggle("done");
    wrapDiv.classList.toggle("highlighted");
    const strike1 = leaf._item.font.strikeout;
    const bold1 = leaf._item.font.bold;

    // Form change on grandparent li swaps li._item; h2qUp1 and h2qUp2 must update to new li._item
    li.classList.replace("todo-item", "card-item");
    const upAfterSwapOk = (wrapDiv._item.h2qUp1 === li._item)
        && (leaf._item.h2qUp1 === wrapDiv._item)
        && (leaf._item.h2qUp2 === li._item)
        && (li._item.h2qTemplate === "h2qT_li_card");
    li.classList.toggle("done");
    const strike2 = leaf._item.font.strikeout;

    // Detaching does not rematerialise; reinserting into archiveList rematerialises for new context
    li.remove();
    const whileDetached = li._item.h2qTemplate;
    archiveList.appendChild(li);
    const afterReinsert = li._item.h2qTemplate;
    const upAfterReinsertOk = (wrapDiv._item.h2qUp1 === li._item) && (leaf._item.h2qUp2 === li._item);

    return [
        upInitialOk,
        strike0,
        bold0,
        strike1,
        bold1,
        upAfterSwapOk,
        strike2,
        whileDetached,
        afterReinsert,
        upAfterReinsertOk
    ].join("|");
}
"##,
        r#"import QtQuick
import QtQuick.Layouts

Item {
    id: root
    readonly property var h2qStyledClasses: ["card-item", "done", "highlighted", "todo-item", "todo-text", "wrap"]
    readonly property var h2qTemplates: ({
        "li.todo-item@todoList": h2qT_li_0,
        "li.done.todo-item@todoList": h2qT_li_0,
        "li.card-item@todoList": h2qT_li_card,
        "li.card-item.done@todoList": h2qT_li_card,
        "li.card-item@archiveList": h2qT_li_archive,
        "div.wrap@*": h2qT_div_wrap,
        "div.highlighted.wrap@*": h2qT_div_wrap,
        "span.todo-text@h2qT_div_wrap": h2qT_span_leaf
    })

    Component {
        id: h2qT_li_0
        RowLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_0"
            property bool cls_done: false
        }
    }
    Component {
        id: h2qT_li_card
        ColumnLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_card"
            property bool cls_done: false
        }
    }
    Component {
        id: h2qT_li_archive
        RowLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_archive"
            property bool cls_done: false
        }
    }
    Component {
        id: h2qT_div_wrap
        RowLayout {
            property string h2qTag: "div"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_div_wrap"
            property bool cls_highlighted: false
            property Item h2qUp1: null
        }
    }
    Component {
        id: h2qT_span_leaf
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_leaf"
            property Item h2qUp1: null
            property Item h2qUp2: null
            font.bold: Boolean(h2qUp1 && h2qUp1.cls_highlighted)
            font.strikeout: Boolean(h2qUp2 && h2qUp2.cls_done)
        }
    }

    ColumnLayout {
        id: todoList
        objectName: "todo-list"
        property string h2qTag: "ul"
        property string h2qKey: "todoList"
    }
    ColumnLayout {
        id: archiveList
        objectName: "archive-list"
        property string h2qTag: "ul"
        property string h2qKey: "archiveList"
    }

    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_UP_N", Page.verifyUpPointers());
        Qt.quit();
    }
}"#,
    );

    assert!(
        output.contains(
            "H2Q_OK_UP_N true|false|false|true|true|true|false|h2qT_li_card|h2qT_li_archive|true"
        ),
        "{output}"
    );
}

#[test]
fn qml6_template_no_behavior_on_insertion_and_behavior_animates_on_toggle() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("todo-list", 1, "todoList", TargetKind::Box);

    let (output, _) = run_page(
        &resolver,
        r##"
function startBehaviorTest() {
    const todoList = document.getElementById("todo-list");
    const li = document.createElement("li");
    li.className = "todo-item done";
    todoList.appendChild(li);

    // On insertion, initial properties passed to createObject must NOT trigger Behavior:
    // opacity is immediately 0.4, not 1.0.
    const opacityOnInsert = li._item.opacity.toFixed(2);

    // Toggling class after insertion MUST trigger Behavior:
    // synchronously after toggle, opacity is still at 0.4 (animating towards 1.0),
    // and reaches 1.0 after the 100ms animation finishes.
    li.classList.remove("done");
    const opacitySyncAfterToggle = li._item.opacity.toFixed(2);

    setTimeout(() => {
        const opacityAfterAnim = li._item.opacity.toFixed(2);
        console.log("H2Q_OK_BEHAVIOR", [opacityOnInsert, opacitySyncAfterToggle, opacityAfterAnim].join("|"));
        Qt.quit();
    }, 220);
}
"##,
        r#"import QtQuick
import QtQuick.Layouts

Item {
    id: root
    readonly property var h2qStyledClasses: ["done", "todo-item"]
    readonly property var h2qTemplates: ({
        "li.todo-item@todoList": h2qT_li_anim,
        "li.done.todo-item@todoList": h2qT_li_anim
    })

    Component {
        id: h2qT_li_anim
        RowLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_anim"
            property bool cls_done: false
            opacity: cls_done ? 0.4 : 1.0
            Behavior on opacity { NumberAnimation { duration: 100 } }
        }
    }

    ColumnLayout {
        id: todoList
        objectName: "todo-list"
        property string h2qTag: "ul"
        property string h2qKey: "todoList"
    }

    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        Page.startBehaviorTest();
    }
}"#,
    );

    assert!(
        output.contains("H2Q_OK_BEHAVIOR 0.40|0.40|1.00"),
        "{output}"
    );
}

// Expected values recorded from headless Chromium (Playwright) serving:
//   <!DOCTYPE html><html><body>
//     <div id="sec-a" class="panel item"><p id="p-1" class="item lead">Alpha</p><button id="btn-1" class="item action">One</button></div>
//     <div id="sec-b" class="panel"><p id="p-2" class="item">Beta</p></div>
//   </body></html>
// at https://example.com/page1.html:
//   querySelectorAll(".item").length === 4
//   Array.from(querySelectorAll(".item"), el => el.id) === ["sec-a", "p-1", "btn-1", "p-2"]
//   querySelectorAll(".panel .item").length === 3
//   Array.from(querySelectorAll(".panel .item"), el => el.id) === ["p-1", "btn-1", "p-2"]
//   getElementsByClassName("item").length === 4
//   Array.from(getElementsByClassName("item"), el => el.id) === ["sec-a", "p-1", "btn-1", "p-2"]
//   getElementsByClassName("panel").length === 2
//   getElementsByTagName("p").length === 2
//   Array.from(getElementsByTagName("p"), el => el.id) === ["p-1", "p-2"]
//   getElementsByTagName("div").length === 2
//   getElementsByTagName("button").length === 1
//   document.body.children.length === 2
//   getElementById("sec-a").nextElementSibling.id === "sec-b"
//   getElementById("sec-b").previousElementSibling.id === "sec-a"
//   getElementById("sec-a").textContent === "AlphaOne"
#[test]
fn qml6_flickable_root_query_selector_all_and_tag_class_collections_visit_once() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default();

    let (output, _) = run_page(
        &resolver,
        r##"
function verifyFlickableTraversal() {
    const dot = ".";
    const qsaItem = document.querySelectorAll(dot + "item");
    const qsaNested = document.querySelectorAll(dot + "panel " + dot + "item");
    const byClassItem = document.getElementsByClassName("item");
    const byClassPanel = document.getElementsByClassName("panel");
    const byTagP = document.getElementsByTagName("p");
    const byTagDiv = document.getElementsByTagName("div");
    const byTagBtn = document.getElementsByTagName("button");
    const secA = document.getElementById("sec-a");
    const secB = document.getElementById("sec-b");

    return [
        qsaItem.length,
        Array.from(qsaItem, (el) => el.id).join(","),
        qsaNested.length,
        Array.from(qsaNested, (el) => el.id).join(","),
        byClassItem.length,
        Array.from(byClassItem, (el) => el.id).join(","),
        byClassPanel.length,
        byTagP.length,
        Array.from(byTagP, (el) => el.id).join(","),
        byTagDiv.length,
        byTagBtn.length,
        document.body.children.length,
        secA.nextElementSibling.id,
        secB.previousElementSibling.id,
        document.getElementById("p-1").textContent
    ].join("|");
}
"##,
        r#"import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Flickable {
    id: root
    width: 800
    height: 600
    contentWidth: col.implicitWidth
    contentHeight: col.implicitHeight

    ColumnLayout {
        id: col
        Rectangle {
            id: secA
            objectName: "sec-a"
            property string h2qTag: "div"
            property string h2qClasses: "panel item"
            ColumnLayout {
                Text {
                    id: p1
                    objectName: "p-1"
                    property string h2qTag: "p"
                    property string h2qClasses: "item lead"
                    text: "Alpha"
                }
                Button {
                    id: btn1
                    objectName: "btn-1"
                    property string h2qTag: "button"
                    property string h2qClasses: "item action"
                    text: "One"
                }
            }
        }
        Rectangle {
            id: secB
            objectName: "sec-b"
            property string h2qTag: "div"
            property string h2qClasses: "panel"
            ColumnLayout {
                Text {
                    id: p2
                    objectName: "p-2"
                    property string h2qTag: "p"
                    property string h2qClasses: "item"
                    text: "Beta"
                }
            }
        }
    }

    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_FLICKABLE_WALK", Page.verifyFlickableTraversal());
        Qt.quit();
    }
}"#,
    );

    assert!(
        output.contains(
            "H2Q_OK_FLICKABLE_WALK 4|sec-a,p-1,btn-1,p-2|3|p-1,btn-1,p-2|4|sec-a,p-1,btn-1,p-2|2|2|p-1,p-2|2|1|2|sec-b|sec-a|Alpha"
        ),
        "{output}"
    );
}

// Expected values recorded from headless Chromium (Playwright) serving:
//   <!DOCTYPE html><html><head><base href="../api/v2/"></head><body>
//     <a id="rel" href="users/list.json">Users</a>
//     <a id="up" href="../v1/ping">Ping</a>
//     <a id="root" href="/status">Status</a>
//   </body></html>
// at https://example.com/docs/guide/index.html:
//   location.href === "https://example.com/docs/guide/index.html"
//   location.pathname === "/docs/guide/index.html"
//   document.baseURI === "https://example.com/docs/api/v2/"
//   rel.href === "https://example.com/docs/api/v2/users/list.json"
//   rel.getAttribute("href") === "users/list.json"
//   up.href === "https://example.com/docs/api/v1/ping"
//   root.href === "https://example.com/status"
//   new URL("items/42", document.baseURI).href === "https://example.com/docs/api/v2/items/42"
#[test]
fn qml6_base_href_keeps_location_href_as_document_url_and_resolves_base_uri_and_relative_anchor() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default();

    let (output, _) = run_page_allowing_unsupported(
        &resolver,
        r##"
function verifyBaseAndLocation() {
    const rel = document.createElement("a");
    rel.setAttribute("href", "users/list.json");
    const up = document.createElement("a");
    up.setAttribute("href", "../v1/ping");
    const rootLink = document.createElement("a");
    rootLink.setAttribute("href", "/status");
    const viaUrlCtor = new URL("items/42", document.baseURI).href;

    return [
        location.href,
        location.pathname,
        document.baseURI,
        rel.href,
        rel.getAttribute("href"),
        up.href,
        rootLink.href,
        viaUrlCtor
    ].join("|");
}
"##,
        r#"import QtQuick

Item {
    id: root
    property string documentUrl: ""
    property string baseUrl: "https://example.com/docs/guide/index.html"

    Item {
        id: baseElem
        property string h2qTag: "base"
        property string href: "../api/v2/"
    }

    Component.onCompleted: {
        // 1. With a <base href="../api/v2/"> element in the tree and baseUrl as the document URL
        (Page.__h2q_init || H2Q.init)(root);
        const withBaseElem = Page.verifyBaseAndLocation();

        // 2. With root.documentUrl and root.baseUrl kept apart on root (without <base> element)
        baseElem.parent = null;
        root.documentUrl = "https://example.com/docs/guide/index.html";
        root.baseUrl = "https://example.com/docs/api/v2/";
        H2Q.init(root);
        const withRootProps = Page.verifyBaseAndLocation();

        console.log("H2Q_OK_BASE_URL", withBaseElem + "||" + (withBaseElem === withRootProps));
        Qt.quit();
    }
}"#,
    );

    assert!(
        output.contains(
            "H2Q_OK_BASE_URL https://example.com/docs/guide/index.html|/docs/guide/index.html|https://example.com/docs/api/v2/|https://example.com/docs/api/v2/users/list.json|users/list.json|https://example.com/docs/api/v1/ping|https://example.com/status|https://example.com/docs/api/v2/items/42||true"
        ),
        "{output}"
    );
}

/// `innerHTML` and `insertAdjacentHTML` on a box make their elements the way
/// `createElement` does, so they go through the registry (§6.7): nested
/// elements take the template of their context, `innerHTML` replaces the
/// children, and a class change rematerialises as for created elements.
#[test]
fn qml6_inner_html_and_insert_adjacent_html_materialise_through_templates() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("list", 1, "list", TargetKind::Box);

    let (output, out) = run_page(
        &resolver,
        r##"
function summary() {
    const items = document.getElementById("list").children;
    return Array.prototype.map.call(items, function (li) {
        const span = li.querySelector("span");
        return li._item.h2qTemplate + ":" + li.className + ":"
            + (span ? span._item.h2qTemplate + "=" + span.textContent : "-");
    }).join(",");
}
function verifyMarkup() {
    const list = document.getElementById("list");
    const old = document.createElement("li");
    old.className = "item";
    list.appendChild(old);
    list.innerHTML = '<li class="item"><span class="label">One</span></li>'
        + '<li class="item done"><span class="label">Two &amp; more</span></li>';
    const first = summary();
    list.insertAdjacentHTML("beforeend", '<li class="item"><span class="label">Three</span></li>');
    list.insertAdjacentHTML("afterbegin", '<li class="item"><span class="label">Zero</span></li>');
    const second = summary();
    list.children[0].classList.add("done");
    const third = summary();
    list.innerHTML = "";
    return [first, second, third, list.children.length].join("|");
}
"##,
        r#"import QtQuick
import QtQuick.Layouts

Item {
    id: root
    readonly property var h2qStyledClasses: ["done", "item", "label"]
    readonly property var h2qTemplates: ({
        "li.item@list": h2qT_li_0,
        "li.done.item@list": h2qT_li_1,
        "span.label@h2qT_li_0": h2qT_span_2,
        "span.label@h2qT_li_1": h2qT_span_3
    })

    Component {
        id: h2qT_li_0
        RowLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_0"
        }
    }
    Component {
        id: h2qT_li_1
        RowLayout {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_1"
        }
    }
    Component {
        id: h2qT_span_2
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_2"
        }
    }
    Component {
        id: h2qT_span_3
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_3"
        }
    }

    ColumnLayout {
        id: list
        objectName: "list"
        property string h2qTag: "ul"
        property string h2qKey: "list"
    }

    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_MARKUP", Page.verifyMarkup());
        Qt.quit();
    }
}"#,
    );

    assert!(
        output.contains(
            "H2Q_OK_MARKUP \
             h2qT_li_0:item:h2qT_span_2=One,h2qT_li_1:item done:h2qT_span_3=Two & more|\
             h2qT_li_0:item:h2qT_span_2=Zero,h2qT_li_0:item:h2qT_span_2=One,\
             h2qT_li_1:item done:h2qT_span_3=Two & more,h2qT_li_0:item:h2qT_span_2=Three|\
             h2qT_li_1:item done:h2qT_span_3=Zero,h2qT_li_0:item:h2qT_span_2=One,\
             h2qT_li_1:item done:h2qT_span_3=Two & more,h2qT_li_0:item:h2qT_span_2=Three|0"
        ),
        "{output}\n{}",
        out.page_js
    );
    assert!(!output.contains("no style template"), "{output}");
    assert!(!output.contains("unsupported"), "{output}");
}

#[test]
fn qml6_created_text_nodes_take_the_line_text_run() {
    if !has_binary("qml6") {
        return;
    }
    let resolver = FakeResolver::default().with_id("list", 1, "list", TargetKind::Box);

    let (output, out) = run_page(
        &resolver,
        r##"
function describe(item) {
    return (item.h2qTag || "run") + "=" + JSON.stringify(item.text) + "/" + item.color;
}
function verifyRuns() {
    const list = document.getElementById("list");
    const li = document.createElement("li");
    li.className = "item";
    const label = document.createElement("span");
    label.className = "label";
    label.textContent = "First";
    li.appendChild(label);
    const by = document.createTextNode(" by ");
    li.appendChild(by);
    const who = document.createElement("strong");
    who.textContent = "Ada";
    li.appendChild(who);
    list.appendChild(li);
    const line = li._item.children[0];
    const first = Array.prototype.map.call(line.children, describe).join(",");
    const read = by.textContent;
    by.textContent = " and ";
    const second = describe(line.children[1]) + "|" + by.textContent;
    return [first, read, second, line.children.length].join("|");
}
"##,
        r##"import QtQuick

Item {
    id: root
    readonly property var h2qStyledClasses: ["item", "label"]
    readonly property var h2qTemplates: ({
        "li.item@list": h2qT_li_0,
        "span.label@h2qT_li_0": h2qT_span_1,
        "strong@h2qT_li_0": h2qT_strong_2
    })

    Component {
        id: h2qT_li_0
        Item {
            property string h2qTag: "li"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_li_0"
            width: 400
            height: line.height
            Flow {
                id: line
                readonly property Component h2qTextRun: lineRun
                width: parent.width
                spacing: 0
                Component {
                    id: lineRun
                    Text { color: "#868e96" }
                }
            }
        }
    }
    Component {
        id: h2qT_span_1
        Text {
            property string h2qTag: "span"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_span_1"
            color: "#2b8a3e"
        }
    }
    Component {
        id: h2qT_strong_2
        Text {
            property string h2qTag: "strong"
            property string h2qClasses: ""
            property string h2qTemplate: "h2qT_strong_2"
            color: "#1f2430"
            font.weight: 700
        }
    }

    Column {
        id: list
        objectName: "list"
        property string h2qTag: "ul"
        property string h2qKey: "list"
    }

    Component.onCompleted: {
        (Page.__h2q_init || H2Q.init)(root);
        console.log("H2Q_OK_RUNS", Page.verifyRuns());
        Qt.quit();
    }
}"##,
    );

    assert!(
        output.contains(
            "H2Q_OK_RUNS \
             span=\"First\"/#2b8a3e,run=\"\u{a0}by\u{a0}\"/#868e96,strong=\"Ada\"/#1f2430| by |\
             run=\"\u{a0}and\u{a0}\"/#868e96| and |3"
        ),
        "{output}\n{}",
        out.page_js
    );
    assert!(!output.contains("no style template"), "{output}");
    assert!(!output.contains("unsupported"), "{output}");
}
