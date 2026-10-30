use std::collections::HashMap;

use h2q_js::{
    DomQuery, DomResolver, ElementKey, EventBinding, JsOutput, QmlTarget, ScriptInput, ScriptKind,
    Severity, TargetKind, analyze, rewrite,
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

fn classic(name: &str, source: &str) -> ScriptInput {
    ScriptInput {
        source: source.to_string(),
        name: name.to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
    }
}

fn module_script(name: &str, source: &str) -> ScriptInput {
    ScriptInput {
        source: source.to_string(),
        name: name.to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
    }
}

fn handler(key: u32, event: &str, source: &str) -> ScriptInput {
    ScriptInput {
        source: source.to_string(),
        name: format!("{event} of element #{key}"),
        kind: ScriptKind::HandlerAttribute {
            element: ElementKey(key),
            event: event.to_string(),
        },
        line_offset: 1,
    }
}

fn run_rewrite(scripts: &[ScriptInput], resolver: &FakeResolver) -> JsOutput {
    let analysis = analyze(scripts);
    rewrite(scripts, &analysis, resolver)
}

#[test]
fn maps_properties_by_target_kind_for_text_box_and_controls() {
    let resolver = FakeResolver::default()
        .with_id("title", 1, "titleLabel", TargetKind::Text)
        .with_id("card", 2, "cardBox", TargetKind::Box)
        .with_id("name", 3, "nameInput", TargetKind::TextField)
        .with_id("notes", 4, "notesArea", TargetKind::TextArea)
        .with_id("agree", 5, "agreeCheck", TargetKind::CheckBox)
        .with_id("opt-a", 6, "optRadio", TargetKind::RadioButton)
        .with_id("role", 7, "roleCombo", TargetKind::ComboBox)
        .with_id("vol", 8, "volSlider", TargetKind::Range)
        .with_id("prog", 9, "loadBar", TargetKind::Progress)
        .with_id("avatar", 10, "avatarImg", TargetKind::Image)
        .with_id("save", 11, "saveBtn", TargetKind::Button);

    let script = classic(
        "controls.js",
        r#"
function syncForm() {
    const title = document.getElementById("title");
    const card = document.getElementById("card");
    const name = document.getElementById("name");
    const notes = document.getElementById("notes");
    const agree = document.getElementById("agree");
    const radio = document.getElementById("opt-a");
    const role = document.getElementById("role");
    const vol = document.getElementById("vol");
    const prog = document.getElementById("prog");
    const avatar = document.getElementById("avatar");
    const save = document.getElementById("save");

    title.textContent = name.value;
    card.innerText = title.textContent;
    notes.placeholder = "Enter notes";
    notes.readOnly = !agree.checked;
    radio.checked = true;
    const prevRole = role.value;
    role.value = "Admin";
    role.selectedIndex = 1;
    vol.value = 75;
    vol.value++;
    prog.value = vol.value;
    avatar.src = "icon.png";
    save.disabled = true;
    save.hidden = false;
    const isOff = (save.disabled).toString();
    name.focus();
    notes.blur();
    return [prevRole, isOff];
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function, None);
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function syncForm() {

    titleLabel.text = nameInput.text;
    cardBoxText.text = titleLabel.text;
    notesArea.placeholderText = "Enter notes";
    notesArea.readOnly = !agreeCheck.checked;
    optRadio.checked = true;
    const prevRole = roleCombo.currentText;
    ((__v) => (roleCombo.currentIndex = roleCombo.find(__v), __v))("Admin");
    roleCombo.currentIndex = 1;
    volSlider.value = 75;
    volSlider.value++;
    loadBar.value = volSlider.value;
    avatarImg.source = "icon.png";
    !(saveBtn.enabled = false);
    !(saveBtn.visible = true);
    const isOff = ((!saveBtn.enabled)).toString();
    nameInput.forceActiveFocus();
    notesArea.focus = false;
    return [prevRole, isOff];
}
"###);
}

#[test]
fn maps_style_classlist_dataset_attributes_and_innerhtml() {
    let resolver = FakeResolver::default()
        .with_id("panel", 1, "panel", TargetKind::Box)
        .with_selector(
            ".chip",
            vec![
                FakeResolver::target(2, "chip0", TargetKind::Button),
                FakeResolver::target(3, "chip1", TargetKind::Button),
            ],
        );

    let script = classic(
        "dom_helpers.js",
        r##"
const panel = document.getElementById("panel");
panel.style.display = "none";
panel.style.display = "flex";
panel.style.color = "#ff0000";
panel.style["backgroundColor"] = "navy";
const col = panel.style.color;
const disp = panel.style.display;
panel.classList.add("open");
panel.classList.toggle("active");
panel.dataset.tabId = "settings";
delete panel.dataset.oldKey;
panel.setAttribute("aria-expanded", "true");
const expanded = panel.getAttribute("aria-expanded");
panel.innerHTML = "<b>Hello</b>";
const chips = document.querySelectorAll(".chip");
"##,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

let col;
let disp;
let expanded;
let chips;

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    ((__v) => (panel.visible = __v !== "none", __v))("none");
    ((__v) => (panel.visible = __v !== "none", __v))("flex");
    ((__v) => (H2Q.setStyle(panel, "color", __v), __v))("#ff0000");
    ((__v) => (H2Q.setStyle(panel, "backgroundColor", __v), __v))("navy");
    col = H2Q.getStyle(panel, "color");
    disp = (panel.visible ? "" : "none");
    H2Q.classList(panel).add("open");
    H2Q.classList(panel).toggle("active");
    H2Q.dataset(panel).tabId = "settings";
    delete H2Q.dataset(panel).oldKey;
    H2Q.setAttribute(panel, "aria-expanded", "true");
    expanded = H2Q.getAttribute(panel, "aria-expanded");
    ((__v) => (H2Q.setInnerHTML(panel, __v), __v))("<b>Hello</b>");
    chips = [chip0, chip1];
}
"###);
}

#[test]
fn avoids_shadowing_or_tdz_when_js_var_matches_qml_id() {
    let resolver = FakeResolver::default().with_id("btn", 1, "btn", TargetKind::Button);

    let s1 = classic(
        "s1.js",
        r#"
const btn = document.getElementById("btn");
btn.textContent = "Top";
"#,
    );
    let s2 = classic(
        "s2.js",
        r#"
function relabel() {
    const btn = document.getElementById("btn");
    btn.textContent = "Clicked";
}
"#,
    );

    let out = run_rewrite(&[s1, s2], &resolver);
    assert!(out.diagnostics.is_empty());
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function relabel() {
    btn.text = "Clicked";
}

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    btn.text = "Top";
}
"###);
}

#[test]
fn falls_back_to_runtime_for_unresolved_queries_and_unknown_receivers() {
    let resolver = FakeResolver::default();
    let script = classic(
        "dynamic.js",
        r#"
function updateDynamic(id, node) {
    const el = document.getElementById(id);
    const missing = document.getElementById("missing");
    node.textContent = "updated";
    node.innerHTML += "!";
    node.addEventListener("click", onHit);
    return [el, missing, node.textContent, node.classList, node.dataset];
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty());
    assert_eq!(out.init_function, None);
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function updateDynamic(id, node) {
    const el = H2Q.document.getElementById(id);
    const missing = H2Q.document.getElementById("missing");
    ((__v) => (H2Q.setText(node, __v), __v))("updated");
    ((__v) => (H2Q.setInnerHTML(node, __v), __v))(H2Q.getInnerHTML(node) + ("!"));
    H2Q.listen(node, "click", onHit);
    return [el, missing, H2Q.getText(node), H2Q.classList(node), H2Q.dataset(node)];
}
"###);
}

#[test]
fn hoists_side_effect_initializers_and_orders_lifecycle_hooks_in_init() {
    let resolver = FakeResolver::default().with_id("out", 1, "outLabel", TargetKind::Text);

    let s1 = classic(
        "s1.js",
        r#"
const APP_NAME = "Demo";
let counter = 0;
const saved = localStorage.getItem("saved") ?? "none";
const { a, b } = parsePair(saved);
document.addEventListener("DOMContentLoaded", () => {
    counter += 1;
});
window.onload = function() {
    counter += 100;
};
"#,
    );
    let s2 = classic(
        "s2.js",
        r#"
window.addEventListener("load", onReady);
window.onload = function() {
    document.getElementById("out").textContent = APP_NAME + ":" + counter;
};
console.log("top-level tail", a, b);
function onReady() {
    counter += 10;
}
"#,
    );

    let out = run_rewrite(&[s1, s2], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

const APP_NAME = "Demo";
let counter = 0;
let saved;
let a, b;
function onReady() {
    counter += 10;
}

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    saved = H2Q.localStorage.getItem("saved") ?? "none";
    ({ a, b } = parsePair(saved));
    console.log("top-level tail", a, b);
    (() => {
        counter += 1;
    })(H2Q.event("DOMContentLoaded", H2Q.document, null));
    (function() {
        outLabel.text = APP_NAME + ":" + counter;
    })(H2Q.event("load", H2Q.window, null));
    onReady(H2Q.event("load", H2Q.window, null));
}
"###);
}

#[test]
fn handles_classic_script_var_and_function_clashes_and_rejects_let_clash() {
    let resolver = FakeResolver::default();

    // Pure declarations with no intervening statements: later wins cleanly at top level.
    let pure1 = classic(
        "p1.js",
        r#"
var theme = "light";
function version() { return 1; }
"#,
    );
    let pure2 = classic(
        "p2.js",
        r#"
var theme = "dark";
function version() { return 2; }
"#,
    );
    let pure_out = run_rewrite(&[pure1, pure2], &resolver);
    assert!(pure_out.diagnostics.is_empty());
    assert_eq!(pure_out.init_function, None);
    insta::assert_snapshot!(pure_out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

var theme = "dark";
function version() { return 2; }
"###);

    // `let` redeclaration across classic scripts reports Error and skips offending script.
    let l1 = classic("l1.js", "let limit = 5;");
    let l2 = classic("l2.js", "let limit = 10;\nconsole.log(limit);");
    let lex_out = run_rewrite(&[l1, l2], &resolver);
    assert_eq!(lex_out.diagnostics.len(), 1);
    assert_eq!(lex_out.diagnostics[0].severity, Severity::Error);
    assert_eq!(lex_out.diagnostics[0].source, "l2.js");
    assert_eq!(lex_out.init_function, None);
    insta::assert_snapshot!(lex_out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

let limit = 5;
"###);
}

#[test]
fn rewrites_inline_handler_attributes_and_static_event_bindings() {
    let resolver = FakeResolver::default()
        .with_id("go", 1, "goBtn", TargetKind::Button)
        .with_id("once", 2, "onceBtn", TargetKind::Button)
        .with_element(3, "linkItem", TargetKind::Text);

    let main_js = classic(
        "events.js",
        r#"
function handleGo(e) {
    console.log(e.type);
}
const go = document.getElementById("go");
go.addEventListener("click", handleGo);
go.onmouseenter = function(e) {
    this.textContent = e.type;
};
document.getElementById("once").addEventListener("click", () => {
    go.disabled = false;
}, { once: true });
"#,
    );
    let attr = handler(
        3,
        "onclick",
        "if (!ready) return false;\nnavigate(this, event);\nreturn false;",
    );

    let out = run_rewrite(&[main_js, attr], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function, None);
    assert_eq!(
        out.bindings,
        vec![
            EventBinding {
                target: FakeResolver::target(1, "goBtn", TargetKind::Button),
                event: "click".to_string(),
                function: "__h2q_on_click_goBtn".to_string(),
            },
            EventBinding {
                target: FakeResolver::target(1, "goBtn", TargetKind::Button),
                event: "mouseenter".to_string(),
                function: "__h2q_on_mouseenter_goBtn".to_string(),
            },
            EventBinding {
                target: FakeResolver::target(2, "onceBtn", TargetKind::Button),
                event: "click".to_string(),
                function: "__h2q_on_click_onceBtn".to_string(),
            },
            EventBinding {
                target: FakeResolver::target(3, "linkItem", TargetKind::Text),
                event: "click".to_string(),
                function: "__h2q_on_click_linkItem".to_string(),
            },
        ]
    );
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function handleGo(e) {
    console.log(e.type);
}

var __h2q_once_0 = false;

function __h2q_on_click_goBtn(event) {
    handleGo.call(goBtn, event);
}

function __h2q_on_mouseenter_goBtn(event) {
    (function(e) {
        goBtn.text = e.type;
    })(event);
}

function __h2q_on_click_onceBtn(event) {
    if (__h2q_once_0) {
        return;
    }
    __h2q_once_0 = true;
    (() => {
        !(goBtn.enabled = true);
    })(event);
}

function __h2q_on_click_linkItem(event) {
    if (!ready) { event.preventDefault(); return; }
    navigate(linkItem, event);
    event.preventDefault();
}
"###);
}

#[test]
fn supports_reassigned_on_property_handlers_via_slot() {
    let resolver = FakeResolver::default().with_id("btn", 1, "btn", TargetKind::Button);
    let script = classic(
        "reassign.js",
        r#"
const btn = document.getElementById("btn");
btn.onclick = () => console.log("first");
function disableClick() {
    btn.onclick = null;
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty());
    assert_eq!(out.bindings.len(), 1);
    assert_eq!(out.bindings[0].event, "click");
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function disableClick() {
    __h2q_prop_click_btn = null;
}

var __h2q_prop_click_btn = null;
function __h2q_on_click_btn_prop(event) {
    if (__h2q_prop_click_btn) {
        __h2q_prop_click_btn.call(btn, event);
    }
}

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    __h2q_prop_click_btn = () => console.log("first");
}
"###);
}

#[test]
fn preserves_shadowed_globals_and_shadowed_dom_variables() {
    let resolver = FakeResolver::default().with_id("hdr", 1, "hdrText", TargetKind::Text);
    let script = classic(
        "shadow.js",
        r#"
const hdr = document.getElementById("hdr");
function isolated(document, window, setTimeout, localStorage, fetch, hdr) {
    setTimeout(() => {}, 10);
    localStorage.setItem("k", "v");
    fetch("/api");
    hdr.textContent = "local";
    return [document.title, window.innerWidth];
}
function realAccess() {
    hdr.textContent = "global";
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty());
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function isolated(document, window, setTimeout, localStorage, fetch, hdr) {
    setTimeout(() => {}, 10);
    localStorage.setItem("k", "v");
    fetch("/api");
    ((__v) => (H2Q.setText(hdr, __v), __v))("local");
    return [document.title, window.innerWidth];
}
function realAccess() {
    hdrText.text = "global";
}
"###);
}

#[test]
fn rewrites_browser_globals_timers_storage_fetch_title_and_stubs() {
    let resolver = FakeResolver::default();
    let script = classic(
        "globals.js",
        r#"
function runGlobals() {
    const id = setTimeout(() => alert("tick"), 50);
    clearTimeout(id);
    const raf = requestAnimationFrame(() => {});
    cancelAnimationFrame(raf);
    localStorage.setItem("a", "1");
    sessionStorage.setItem("b", "2");
    document.title = "Dashboard";
    const t = document.title;
    const size = [window.innerWidth, window.innerHeight];
    console.table([{ t }]);
    const href = location.href;
    return fetch("/status.json").then((r) => r.json()).then((d) => [d, size, href]);
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert_eq!(out.diagnostics.len(), 1);
    assert_eq!(out.diagnostics[0].severity, Severity::Unsupported);
    assert!(out.diagnostics[0].message.contains("location"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function runGlobals() {
    const id = H2Q.setTimeout(() => H2Q.alert("tick"), 50);
    H2Q.clearTimeout(id);
    const raf = H2Q.requestAnimationFrame(() => {});
    H2Q.cancelAnimationFrame(raf);
    H2Q.localStorage.setItem("a", "1");
    H2Q.sessionStorage.setItem("b", "2");
    ((__v) => (H2Q.setTitle(__v), __v))("Dashboard");
    const t = H2Q.getTitle();
    const size = [H2Q.window.innerWidth, H2Q.window.innerHeight];
    H2Q.consoleTable([{ t }]);
    const href = H2Q.location.href;
    return H2Q.fetch("/status.json").then((r) => r.json()).then((d) => [d, size, href]);
}
"###);
}

#[test]
fn lowers_missing_qt611_syntax_spread_rest_logical_assign_catch_classes_and_async() {
    let resolver = FakeResolver::default().with_id("msg", 1, "msgText", TargetKind::Text);
    let script = classic(
        "modern.js",
        r#"
class Base {
    kind = "base";
}

class Counter extends Base {
    count = 0;
    static label = "clicks";

    constructor(start) {
        super();
        this.count = start;
    }

    async bump(step) {
        const delta = await Promise.resolve(step);
        this.count += delta;
        return this.count;
    }
}

async function loadGreeting(raw) {
    const merged = { ...raw, extra: true };
    const { extra, ...rest } = merged;
    let status = rest.status;
    status ??= "ok";
    try {
        JSON.parse(status);
    } catch {
        status = "fallback";
    }
    const compute = async (x) => (await Promise.resolve(x)) + 1;
    const n = await compute(2);
    document.getElementById("msg").textContent = status + ":" + n + ":" + extra;
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function, None);
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

class Base {
    constructor() { this.kind = "base"; }
}
class Counter extends Base {

    constructor(start) {
        super(); this.count = 0;
        this.count = start;
    }

    bump(step) {
    return H2Q.asyncRun(this, function*() {
        const delta = (yield (Promise.resolve(step)));
        this.count += delta;
        return this.count;
    });
}
}
Counter.label = "clicks";
function loadGreeting(raw) {
    return H2Q.asyncRun(this, function*() {
    const merged = Object.assign({}, raw, { extra: true });
    const __h2q_obj_0 = merged, { extra } = __h2q_obj_0, rest = H2Q.objectRest(__h2q_obj_0, ["extra"]);
    let status = rest.status;
    status ?? (status = "ok");
    try {
        JSON.parse(status);
    } catch (__h2q_err) {
        status = "fallback";
    }
    const compute = (x) => H2Q.asyncRun(this, function*() { return ((yield (Promise.resolve(x)))) + 1; });
    const n = (yield (compute(2)));
    msgText.text = status + ":" + n + ":" + extra;
});
}
"###);
}

#[test]
fn inlines_local_es_modules_strips_exports_and_diagnoses_external_imports() {
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let math_mod = module_script(
        "js/math.js",
        r#"
export const UNIT = "px";
export function double(n) {
    return n * 2;
}
export default function formatPx(n) {
    return double(n) + UNIT;
}
"#,
    );
    let main_mod = module_script(
        "js/app.js",
        r#"
import fmt, { double as mul2 } from "./math.js";
import * as MathNs from "./math.js";
import { externalThing } from "https://example.com/ext.js";

export const VERSION = 1;
document.getElementById("out").textContent = fmt(mul2(3)) + ":" + MathNs.UNIT;
"#,
    );

    // Pass `main_mod` before `math_mod` to verify dependency-first single inlining!
    let out = run_rewrite(&[main_mod, math_mod], &resolver);
    assert_eq!(out.diagnostics.len(), 1);
    assert_eq!(out.diagnostics[0].severity, Severity::Unsupported);
    assert!(
        out.diagnostics[0]
            .message
            .contains("https://example.com/ext.js")
    );
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

const UNIT = "px";
function double(n) {
    return n * 2;
}
function formatPx(n) {
    return double(n) + UNIT;
}
const fmt = formatPx;
const mul2 = double;
const MathNs = { UNIT: UNIT, double: double, default: formatPx };
const VERSION = 1;

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    outText.text = fmt(mul2(3)) + ":" + MathNs.UNIT;
}
"###);
}

#[test]
fn preserves_execution_order_when_var_or_function_clashes_follow_statements() {
    let resolver = FakeResolver::default();
    let s1 = classic(
        "s1.js",
        r#"
var stage = 1;
function current() { return "one"; }
console.log(stage, current());
"#,
    );
    let s2 = classic(
        "s2.js",
        r#"
var stage = 2;
function current() { return "two"; }
console.log(stage, current());
"#,
    );

    let out = run_rewrite(&[s1, s2], &resolver);
    assert!(out.diagnostics.is_empty());
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

var stage = 1;
var current = function current() { return "one"; };

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    console.log(stage, current());
    stage = 2;
    current = function current() { return "two"; };
    console.log(stage, current());
}
"###);
}

#[test]
fn hoists_top_level_object_rest_destructuring_into_init() {
    let resolver = FakeResolver::default();
    let script = classic(
        "rest_top.js",
        r#"
const { mode, ...extra } = getSettings();
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty());
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

let mode, extra;

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    { const __h2q_obj_0 = getSettings(); ({ mode } = __h2q_obj_0); extra = H2Q.objectRest(__h2q_obj_0, ["mode"]); }
}
"###);
}

#[test]
fn wires_nested_add_event_listener_and_dom_methods_on_resolved_targets() {
    let resolver = FakeResolver::default().with_id("box", 1, "boxItem", TargetKind::Box);
    let script = classic(
        "nested_listen.js",
        r#"
function setup(cb) {
    const box = document.getElementById("box");
    box.addEventListener("click", cb);
    box.removeEventListener("click", cb);
    box.dispatchEvent(new CustomEvent("ping"));
    return box.getBoundingClientRect();
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty());
    assert_eq!(out.bindings.len(), 1);
    assert_eq!(out.bindings[0].function, "__h2q_on_click_boxItem_0");
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function setup(cb) {
    H2Q.listen(boxItem, "click", cb);
    H2Q.unlisten(boxItem, "click", cb);
    H2Q.dispatchEvent(boxItem, new H2Q.CustomEvent("ping"));
    return H2Q.getBoundingClientRect(boxItem);
}

function __h2q_on_click_boxItem_0(event) {
    H2Q.dispatchEvent(boxItem, event);
}
"###);
}

#[test]
fn unifies_resolved_target_add_remove_dispatch_and_duplicate_listeners() {
    let resolver = FakeResolver::default().with_id("box", 1, "boxItem", TargetKind::Box);
    let script = classic(
        "unified_listeners.js",
        r#"
const topBox = document.getElementById("box");
topBox.addEventListener("click", onTop);
topBox.addEventListener("click", onOnce, { once: true });

function bindAndExercise(cb) {
    const box = document.getElementById("box");
    box.addEventListener("click", cb);
    box.addEventListener("click", cb);
    box.dispatchEvent(new CustomEvent("click"));
    box.removeEventListener("click", cb);
    box.dispatchEvent(new CustomEvent("click"));
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.bindings.len(), 1);
    assert_eq!(out.bindings[0].event, "click");
    assert_eq!(out.bindings[0].function, "__h2q_on_click_boxItem_0");
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));
    assert!(!out.page_js.contains("__h2q_listeners_"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function bindAndExercise(cb) {
    H2Q.listen(boxItem, "click", cb);
    H2Q.listen(boxItem, "click", cb);
    H2Q.dispatchEvent(boxItem, new H2Q.CustomEvent("click"));
    H2Q.unlisten(boxItem, "click", cb);
    H2Q.dispatchEvent(boxItem, new H2Q.CustomEvent("click"));
}

function __h2q_on_click_boxItem_0(event) {
    H2Q.dispatchEvent(boxItem, event);
}

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    H2Q.listen(boxItem, "click", onTop);
    H2Q.listen(boxItem, "click", onOnce, { once: true });
}
"###);
}

#[test]
fn retains_uninitialized_top_level_let_after_init_stmts_with_reset_semantics() {
    let resolver = FakeResolver::default();
    let script = classic(
        "top_let.js",
        r#"
console.log("boot");
let state;
function setState(v) { state = v; }
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

let state;
function setState(v) { state = v; }

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    console.log("boot");
    state = undefined;
}
"###);
}

#[test]
fn resolves_document_body_and_document_element_when_mapped() {
    let resolver = FakeResolver::default()
        .with_selector(
            "body",
            vec![FakeResolver::target(1, "bodyBox", TargetKind::Box)],
        )
        .with_selector(
            "html",
            vec![FakeResolver::target(2, "htmlRoot", TargetKind::Box)],
        );

    let script = classic(
        "doc_roots.js",
        r#"
document.body.classList.add("dark");
document.documentElement.dataset.theme = "night";
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty());
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function __h2q_init(rootItem) {
    H2Q.init(rootItem || (typeof root !== "undefined" ? root : null));
    H2Q.classList(bodyBox).add("dark");
    H2Q.dataset(htmlRoot).theme = "night";
}
"###);
}

#[test]
fn reports_unresolved_handler_attribute_and_unlowerable_async_arguments() {
    let resolver = FakeResolver::default();
    let orphan_handler = handler(999, "onclick", "doThing(this);");
    let bad_async = classic(
        "bad_async.js",
        r#"
async function usesArguments() {
    return await Promise.resolve(arguments.length);
}
"#,
    );

    let out = run_rewrite(&[orphan_handler, bad_async], &resolver);
    assert_eq!(out.diagnostics.len(), 2);
    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.severity == Severity::Unsupported)
    );
    assert!(out.diagnostics[0].message.contains("ElementKey(999)"));
    assert!(out.diagnostics[1].message.contains("arguments"));
}

#[test]
fn empty_scripts_slice_produces_minimal_valid_header_with_no_init() {
    let resolver = FakeResolver::default();
    let out = run_rewrite(&[], &resolver);
    assert!(out.diagnostics.is_empty());
    assert!(out.bindings.is_empty());
    assert_eq!(out.init_function, None);
    assert_eq!(out.page_js, ".import \"h2q_runtime.js\" as H2Q\n");
}

#[test]
fn rewrites_compound_and_logical_assignments_across_resolved_dom_properties() {
    let resolver = FakeResolver::default()
        .with_id("title", 1, "titleLabel", TargetKind::Text)
        .with_id("card", 2, "cardBox", TargetKind::Box)
        .with_id("name", 3, "nameInput", TargetKind::TextField)
        .with_id("vol", 4, "volSlider", TargetKind::Range)
        .with_id("agree", 5, "agreeCheck", TargetKind::CheckBox)
        .with_id("role", 6, "roleCombo", TargetKind::ComboBox)
        .with_id("avatar", 7, "avatarImg", TargetKind::Image)
        .with_id("save", 8, "saveBtn", TargetKind::Button);

    let script = classic(
        "compound_logical_dom.js",
        r##"
function updateDom(suffix, extraClass, htmlChunk, fallbackRole, node, state) {
    const title = document.getElementById("title");
    const card = document.getElementById("card");
    const name = document.getElementById("name");
    const vol = document.getElementById("vol");
    const agree = document.getElementById("agree");
    const role = document.getElementById("role");
    const avatar = document.getElementById("avatar");
    const save = document.getElementById("save");

    document.title += suffix;
    document.title ||= "Untitled";
    document.title &&= "Prefixed: " + suffix;
    document.title ??= "DefaultTitle";

    save.disabled ||= true;
    save.disabled &&= false;
    save.disabled ??= true;
    save.hidden &&= false;
    save.hidden ||= true;
    save.hidden ??= false;
    save.disabled |= true;
    save.hidden &= false;

    role.value ||= fallbackRole;
    role.value &&= "Admin";
    role.value ??= "Viewer";
    role.value += suffix;

    card.className += extraClass;
    card.className ||= "card-default";
    card.className &&= "card-active";
    card.className ??= "card-fallback";

    card.innerHTML += htmlChunk;
    card.innerHTML ||= "<b>empty</b>";
    card.innerHTML &&= "<i>ready</i>";
    card.innerHTML ??= "<u>init</u>";

    title.textContent += suffix;
    title.textContent ||= "N/A";
    card.textContent += suffix;
    name.value += suffix;
    name.value ||= "anon";
    name.placeholder ??= "Enter name";
    vol.value -= 5;
    agree.checked &&= true;
    avatar.src ||= "qrc:/fallback.png";

    card.style.color += "00";
    card.style.color ||= "#112233";
    card.style.color &&= "#445566";
    card.style.color ??= "#778899";
    card.style.display ||= "none";
    card.style.display &&= "flex";

    node.innerHTML += htmlChunk;
    node.innerHTML ||= "<span>node</span>";
    node.textContent ||= "plain";

    state.count += 1;
    state.label ||= "default";
    state.active &&= false;
    state.meta ??= "none";
}
"##,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(
        !out.page_js.contains(".innerHTML"),
        "raw .innerHTML access leaked into page_js:\n{}",
        out.page_js
    );
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function updateDom(suffix, extraClass, htmlChunk, fallbackRole, node, state) {

    ((__v) => (H2Q.setTitle(__v), __v))(H2Q.getTitle() + (suffix));
    H2Q.getTitle() || ((__v) => (H2Q.setTitle(__v), __v))("Untitled");
    H2Q.getTitle() && ((__v) => (H2Q.setTitle(__v), __v))("Prefixed: " + suffix);
    H2Q.getTitle() ?? ((__v) => (H2Q.setTitle(__v), __v))("DefaultTitle");

    !saveBtn.enabled || !(saveBtn.enabled = false);
    !saveBtn.enabled && !(saveBtn.enabled = true);
    (!saveBtn.enabled) ?? !(saveBtn.enabled = false);
    !saveBtn.visible && !(saveBtn.visible = true);
    !saveBtn.visible || !(saveBtn.visible = false);
    (!saveBtn.visible) ?? !(saveBtn.visible = true);
    !(saveBtn.enabled = !((!saveBtn.enabled) | (true)));
    !(saveBtn.visible = !((!saveBtn.visible) & (false)));

    roleCombo.currentText || ((__v) => (roleCombo.currentIndex = roleCombo.find(__v), __v))(fallbackRole);
    roleCombo.currentText && ((__v) => (roleCombo.currentIndex = roleCombo.find(__v), __v))("Admin");
    roleCombo.currentText ?? ((__v) => (roleCombo.currentIndex = roleCombo.find(__v), __v))("Viewer");
    ((__v) => (roleCombo.currentIndex = roleCombo.find(__v), __v))(roleCombo.currentText + (suffix));

    ((__v) => (H2Q.setClassName(cardBox, __v), __v))(H2Q.getClassName(cardBox) + (extraClass));
    H2Q.getClassName(cardBox) || ((__v) => (H2Q.setClassName(cardBox, __v), __v))("card-default");
    H2Q.getClassName(cardBox) && ((__v) => (H2Q.setClassName(cardBox, __v), __v))("card-active");
    H2Q.getClassName(cardBox) ?? ((__v) => (H2Q.setClassName(cardBox, __v), __v))("card-fallback");

    ((__v) => (H2Q.setInnerHTML(cardBox, __v), __v))(H2Q.getInnerHTML(cardBox) + (htmlChunk));
    H2Q.getInnerHTML(cardBox) || ((__v) => (H2Q.setInnerHTML(cardBox, __v), __v))("<b>empty</b>");
    H2Q.getInnerHTML(cardBox) && ((__v) => (H2Q.setInnerHTML(cardBox, __v), __v))("<i>ready</i>");
    H2Q.getInnerHTML(cardBox) ?? ((__v) => (H2Q.setInnerHTML(cardBox, __v), __v))("<u>init</u>");

    titleLabel.text += suffix;
    titleLabel.text || (titleLabel.text = "N/A");
    cardBoxText.text += suffix;
    nameInput.text += suffix;
    nameInput.text || (nameInput.text = "anon");
    nameInput.placeholderText ?? (nameInput.placeholderText = "Enter name");
    volSlider.value -= 5;
    agreeCheck.checked && (agreeCheck.checked = true);
    avatarImg.source || (avatarImg.source = "qrc:/fallback.png");

    ((__v) => (H2Q.setStyle(cardBox, "color", __v), __v))(H2Q.getStyle(cardBox, "color") + ("00"));
    H2Q.getStyle(cardBox, "color") || ((__v) => (H2Q.setStyle(cardBox, "color", __v), __v))("#112233");
    H2Q.getStyle(cardBox, "color") && ((__v) => (H2Q.setStyle(cardBox, "color", __v), __v))("#445566");
    H2Q.getStyle(cardBox, "color") ?? ((__v) => (H2Q.setStyle(cardBox, "color", __v), __v))("#778899");
    (cardBox.visible ? "" : "none") || ((__v) => (cardBox.visible = __v !== "none", __v))("none");
    (cardBox.visible ? "" : "none") && ((__v) => (cardBox.visible = __v !== "none", __v))("flex");

    ((__v) => (H2Q.setInnerHTML(node, __v), __v))(H2Q.getInnerHTML(node) + (htmlChunk));
    H2Q.getInnerHTML(node) || ((__v) => (H2Q.setInnerHTML(node, __v), __v))("<span>node</span>");
    H2Q.getText(node) || ((__v) => (H2Q.setText(node, __v), __v))("plain");

    state.count += 1;
    state.label || (state.label = "default");
    state.active && (state.active = false);
    state.meta ?? (state.meta = "none");
}
"###);
}

#[test]
fn rewrites_assignment_expressions_and_dynamic_receivers_with_dom_values() {
    let resolver = FakeResolver::default()
        .with_id("save", 1, "saveBtn", TargetKind::Button)
        .with_id("role", 2, "roleCombo", TargetKind::ComboBox)
        .with_id("card", 3, "cardBox", TargetKind::Box);

    let script = classic(
        "assign_exprs.js",
        r#"
function captureAssigned(rhs, pickNode) {
    const save = document.getElementById("save");
    const role = document.getElementById("role");
    const card = document.getElementById("card");

    const a = (document.title ||= "default");
    const b = (document.title += rhs());
    const c = (save.hidden &&= false);
    const d = (role.value ||= rhs());
    const e = (card.style.display = "none");
    const f = (pickNode(0).textContent = rhs());
    const g = (pickNode(1).textContent += rhs());
    const h = (pickNode(2).innerHTML ||= rhs());
    return [a, b, c, d, e, f, g, h];
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function, None);
    insta::assert_snapshot!(out.page_js, @r###"
.import "h2q_runtime.js" as H2Q

function captureAssigned(rhs, pickNode) {

    const a = (H2Q.getTitle() || ((__v) => (H2Q.setTitle(__v), __v))("default"));
    const b = (((__v) => (H2Q.setTitle(__v), __v))(H2Q.getTitle() + (rhs())));
    const c = (!saveBtn.visible && !(saveBtn.visible = true));
    const d = (roleCombo.currentText || ((__v) => (roleCombo.currentIndex = roleCombo.find(__v), __v))(rhs()));
    const e = (((__v) => (cardBox.visible = __v !== "none", __v))("none"));
    const f = (((__o, __v) => (H2Q.setText(__o, __v), __v))(pickNode(0), rhs()));
    const g = (((__o) => ((__v) => (H2Q.setText(__o, __v), __v))(H2Q.getText(__o) + (rhs())))(pickNode(1)));
    const h = (((__o) => H2Q.getInnerHTML(__o) || ((__v) => (H2Q.setInnerHTML(__o, __v), __v))(rhs()))(pickNode(2)));
    return [a, b, c, d, e, f, g, h];
}
"###);
}

#[test]
fn rejects_cross_script_block_nested_var_conflict_with_lexical_binding() {
    let resolver = FakeResolver::default();
    let s1 = classic("s1.js", "let x = 1;");
    let s2 = classic("s2.js", "if (true) { var x = 2; }\nconsole.log(x);");
    let out1 = run_rewrite(&[s1, s2], &resolver);
    assert_eq!(out1.diagnostics.len(), 1);
    assert_eq!(out1.diagnostics[0].severity, Severity::Error);
    assert_eq!(out1.diagnostics[0].source, "s2.js");
    assert_eq!(out1.init_function, None);
    assert_eq!(
        out1.page_js,
        ".import \"h2q_runtime.js\" as H2Q\n\nlet x = 1;\n"
    );

    let s3 = classic("s3.js", "if (true) { var y = 1; }");
    let s4 = classic("s4.js", "let y = 2;");
    let out2 = run_rewrite(&[s3, s4], &resolver);
    assert_eq!(out2.diagnostics.len(), 1);
    assert_eq!(out2.diagnostics[0].severity, Severity::Error);
    assert_eq!(out2.diagnostics[0].source, "s4.js");
    assert_eq!(out2.init_function.as_deref(), Some("__h2q_init"));
}

#[test]
fn hoists_classic_block_nested_var_and_preserves_function_local_var() {
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let s1 = classic(
        "s1.js",
        r#"
if (false) {
    var falseBranch = "never";
}
var seenBeforeTrue = (trueBranch === undefined);
if (true) {
    var trueBranch = "ready", uninitVar;
}
for (var i = 0; i < 3; i++) {
    trueBranch += ":" + i;
}
for (var item of ["a", "b"]) {}
function readFromTopFn() {
    return [falseBranch === undefined, seenBeforeTrue, trueBranch, i, item, uninitVar === undefined].join("|");
}
function keepLocal() {
    if (true) {
        var fnLocalOnly = "local";
    }
    for (var j = 0; j < 2; j++) {}
    return fnLocalOnly + ":" + j;
}
"#,
    );
    let s2 = classic(
        "s2.js",
        r#"
document.getElementById("out").textContent = readFromTopFn() + "|" + trueBranch + "|" + i + "|" + item + "|" + keepLocal();
"#,
    );

    let out = run_rewrite(&[s1, s2], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__h2q_init"));
    for hoisted in [
        "var falseBranch;",
        "var trueBranch;",
        "var uninitVar;",
        "var i;",
        "var item;",
    ] {
        assert!(
            out.page_js.contains(hoisted),
            "expected `{hoisted}` at file scope in page_js:\n{}",
            out.page_js
        );
    }
    assert!(
        !out.page_js.contains("var fnLocalOnly;") && !out.page_js.contains("var j;"),
        "function-local `var` should not be hoisted to file scope:\n{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("var fnLocalOnly = \"local\";")
            && out.page_js.contains("for (var j = 0; j < 2; j++)"),
        "function-local `var` should remain inside its function:\n{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("seenBeforeTrue = (trueBranch === undefined);\n    if (true) {\n        trueBranch = \"ready\";\n    }"),
        "true-branch assignment order inside __h2q_init was not preserved:\n{}",
        out.page_js
    );
    assert!(
        !out.page_js.contains("    var trueBranch")
            && !out.page_js.contains("for (var i")
            && !out.page_js.contains("for (var item"),
        "`var` keyword remained inside __h2q_init, shadowing file-scope var:\n{}",
        out.page_js
    );
}

#[test]
fn diagnoses_window_qualified_unsupported_features_without_duplicates_and_keeps_supported_window_clean()
 {
    let resolver = FakeResolver::default();
    let unsupported_script = classic(
        "win_unsupported.js",
        r#"
function runUnsupported() {
    const a = window.BigInt(1);
    const b = window.eval("1 + 1");
    const c = window.confirm("ok?");
    const d = window.prompt("name?");
    const obs = new window.MutationObserver(() => {});
    const ro = new window.ResizeObserver(() => {});
    const worker = new window.Worker("w.js");
    const fnRef = window.eval;
    window.setTimeout(() => window.location.href, 10);
    return [a, b, c, d, obs, ro, worker, fnRef];
}
"#,
    );
    let out = run_rewrite(&[unsupported_script], &resolver);
    assert_eq!(
        out.diagnostics.len(),
        9,
        "expected 9 unsupported diagnostics without duplicates, got: {:#?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.severity == Severity::Unsupported)
    );

    let clean_script = classic(
        "win_clean.js",
        r#"
function shadowed(window) {
    return [
        window.BigInt(1),
        window.eval("1"),
        window.confirm("ok"),
        new window.MutationObserver(() => {}),
        new window.ResizeObserver(() => {}),
        new window.Worker("w.js"),
        window.location.href,
    ];
}
function supported() {
    const id = window.setTimeout(() => window.alert("hi"), 10);
    window.clearTimeout(id);
    window.localStorage.setItem("k", "v");
    return [window.innerWidth, window.performance.now()];
}
"#,
    );
    let clean_out = run_rewrite(&[clean_script], &resolver);
    assert!(
        clean_out.diagnostics.is_empty(),
        "expected no diagnostics for shadowed or supported window uses, got: {:#?}",
        clean_out.diagnostics
    );
}
