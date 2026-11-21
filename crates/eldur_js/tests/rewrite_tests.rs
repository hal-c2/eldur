use std::collections::HashMap;

use eldur_js::{
    DomQuery, DomResolver, ElementKey, EventBinding, JsOutput, QmlTarget, ScriptInput, ScriptKind,
    Severity, TargetKind, analyze, resolve_named_elements, rewrite,
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
        imports: Vec::new(),
    }
}

fn module_script(name: &str, source: &str) -> ScriptInput {
    ScriptInput {
        source: source.to_string(),
        name: name.to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
        imports: Vec::new(),
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
        imports: Vec::new(),
    }
}

fn run_rewrite(scripts: &[ScriptInput], resolver: &FakeResolver) -> JsOutput {
    let analysis = analyze(scripts);
    rewrite(scripts, &analysis, resolver)
}

/// The resolved lowering, which these tests pin, on a page whose writes would
/// otherwise send its lookups to the runtime. `live_lookups_*` below pin that
/// decision for the same kinds of write.
fn run_rewrite_resolved(scripts: &[ScriptInput], resolver: &FakeResolver) -> JsOutput {
    let mut analysis = analyze(scripts);
    analysis.live_queries.clear();
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
.import "eldur_runtime.js" as Eldur

function syncForm() {

    titleLabel.text = nameInput.text;
    cardBoxText.text = titleLabel.text;
    notesArea.placeholderText = "Enter notes";
    notesArea.readOnly = !agreeCheck.checked;
    optRadio.checked = true;
    const prevRole = roleCombo.currentValue;
    ((__v) => (roleCombo.currentIndex = roleCombo.indexOfValue(__v), __v))("Admin");
    roleCombo.currentIndex = 1;
    volSlider.value = 75;
    volSlider.value++;
    loadBar.value = Number(String(volSlider.value));
    avatarImg.source = "icon.png";
    !(saveBtn.enabled = false);
    Eldur.wrap(saveBtn).hidden = false;
    const isOff = ((!saveBtn.enabled)).toString();
    nameInput.forceActiveFocus();
    notesArea.focus = false;
    return [prevRole, isOff];
}
"###);
}

#[test]
fn a_page_that_keeps_attributes_reads_disabled_from_them() {
    let resolver = FakeResolver::default().with_id("save", 1, "save", TargetKind::Button);
    let scripts = [classic(
        "app.js",
        "function f() { return document.getElementById(\"save\").disabled; }",
    )];
    let mut analysis = analyze(&scripts);
    analysis.reads_attributes = true;
    analysis.live_queries.clear();
    let out = rewrite(&scripts, &analysis, &resolver);
    assert!(
        out.page_js.contains("Eldur.wrap(save).disabled"),
        "{}",
        out.page_js
    );
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

    let out = run_rewrite_resolved(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function.as_deref(), Some("__eldur_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

let panel;
let col;
let disp;
let expanded;
let chips;

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
    panel = Eldur.document.getElementById("panel");
    panel.style.display = "none";
    panel.style.display = "flex";
    panel.style.color = "#ff0000";
    panel.style["backgroundColor"] = "navy";
    col = panel.style.color;
    disp = panel.style.display;
    Eldur.classList(panel).add("open");
    Eldur.classList(panel).toggle("active");
    Eldur.dataset(panel).tabId = "settings";
    delete Eldur.dataset(panel).oldKey;
    Eldur.setAttribute(panel, "aria-expanded", "true");
    expanded = Eldur.getAttribute(panel, "aria-expanded");
    ((__v) => (Eldur.setInnerHTML(panel, __v), __v))("<b>Hello</b>");
    chips = Eldur.document.querySelectorAll(".chip");
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
.import "eldur_runtime.js" as Eldur

function relabel() {
    btn.text = "Clicked";
}

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
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
.import "eldur_runtime.js" as Eldur

function updateDynamic(id, node) {
    const el = Eldur.document.getElementById(id);
    const missing = Eldur.document.getElementById("missing");
    ((__v) => (Eldur.setText(node, __v), __v))("updated");
    ((__v) => (Eldur.setInnerHTML(node, __v), __v))(Eldur.getInnerHTML(node) + ("!"));
    Eldur.listen(node, "click", onHit);
    return [el, missing, Eldur.getText(node), Eldur.classList(node), Eldur.dataset(node)];
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
    assert_eq!(out.init_function.as_deref(), Some("__eldur_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

const APP_NAME = "Demo";
let counter = 0;
let saved;
let a, b;
var __eldur_ready_listeners = [];
var __eldur_load_listeners = [];
function __eldur_call_window_onload() {
    var handler = __eldur_call_window_onload.handler;
    if (typeof handler === "function") {
        handler.call(Eldur.window, Eldur.event("load", Eldur.window, null));
    }
}
function onReady() {
    counter += 10;
}

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
    saved = Eldur.localStorage.getItem("saved") ?? "none";
    ({ a, b } = parsePair(saved));
    Eldur.queueListener(__eldur_ready_listeners, Eldur.document, "DOMContentLoaded", () => {
        counter += 1;
    });
    Eldur.setOnload(__eldur_load_listeners, __eldur_call_window_onload, function() {
        counter += 100;
    });
    Eldur.queueListener(__eldur_load_listeners, Eldur.window, "load", onReady);
    Eldur.setOnload(__eldur_load_listeners, __eldur_call_window_onload, function() {
        outLabel.text = APP_NAME + ":" + counter;
    });
    console.log("top-level tail", a, b);
    Eldur.fireListeners(__eldur_ready_listeners);
    Eldur.fireListeners(__eldur_load_listeners);
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
.import "eldur_runtime.js" as Eldur

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
.import "eldur_runtime.js" as Eldur

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

    let out = run_rewrite_resolved(&[main_js, attr], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function, None);
    assert_eq!(
        out.bindings,
        vec![
            EventBinding {
                target: FakeResolver::target(1, "goBtn", TargetKind::Button),
                event: "click".to_string(),
                function: "__eldur_on_click_goBtn".to_string(),
            },
            EventBinding {
                target: FakeResolver::target(1, "goBtn", TargetKind::Button),
                event: "mouseenter".to_string(),
                function: "__eldur_on_mouseenter_goBtn".to_string(),
            },
            EventBinding {
                target: FakeResolver::target(2, "onceBtn", TargetKind::Button),
                event: "click".to_string(),
                function: "__eldur_on_click_onceBtn".to_string(),
            },
            EventBinding {
                target: FakeResolver::target(3, "linkItem", TargetKind::Text),
                event: "click".to_string(),
                function: "__eldur_on_click_linkItem".to_string(),
            },
        ]
    );
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

function handleGo(e) {
    console.log(e.type);
}

var __eldur_once_0 = false;

function __eldur_on_click_goBtn(event) {
    handleGo.call(Eldur.wrap(goBtn), event);
}

function __eldur_on_mouseenter_goBtn(event) {
    (function(e) {
        goBtn.text = e.type;
    })(event);
}

function __eldur_on_click_onceBtn(event) {
    if (__eldur_once_0) {
        return;
    }
    __eldur_once_0 = true;
    (() => {
        !(goBtn.enabled = true);
    })(event);
}

function __eldur_on_click_linkItem(event) {
    if (!ready) { event.preventDefault(); return; }
    navigate(Eldur.wrap(linkItem), event);
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
.import "eldur_runtime.js" as Eldur

function disableClick() {
    __eldur_prop_click_btn = null;
}

var __eldur_prop_click_btn = null;
function __eldur_on_click_btn_prop(event) {
    if (__eldur_prop_click_btn) {
        __eldur_prop_click_btn.call(Eldur.wrap(btn), event);
    }
}

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
    __eldur_prop_click_btn = () => console.log("first");
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

    let out = run_rewrite_resolved(&[script], &resolver);
    assert!(out.diagnostics.is_empty());
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

function isolated(document, window, setTimeout, localStorage, fetch, hdr) {
    setTimeout(() => {}, 10);
    localStorage.setItem("k", "v");
    fetch("/api");
    ((__v) => (Eldur.setText(hdr, __v), __v))("local");
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
.import "eldur_runtime.js" as Eldur

function runGlobals() {
    const id = Eldur.setTimeout(() => Eldur.alert("tick"), 50);
    Eldur.clearTimeout(id);
    const raf = Eldur.requestAnimationFrame(() => {});
    Eldur.cancelAnimationFrame(raf);
    Eldur.localStorage.setItem("a", "1");
    Eldur.sessionStorage.setItem("b", "2");
    ((__v) => (Eldur.setTitle(__v), __v))("Dashboard");
    const t = Eldur.getTitle();
    const size = [Eldur.window.innerWidth, Eldur.window.innerHeight];
    Eldur.consoleTable([{ t }]);
    const href = Eldur.location.href;
    return Eldur.fetch("/status.json").then((r) => r.json()).then((d) => [d, size, href]);
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
.import "eldur_runtime.js" as Eldur

class Base {
    constructor() { this.kind = "base"; }
}
class Counter extends Base {

    constructor(start) {
        super(); this.count = 0;
        this.count = start;
    }

    bump(step) {
    return Eldur.asyncRun(this, function*() {
        const delta = (yield (Eldur.Promise.resolve(step)));
        this.count += delta;
        return this.count;
    });
}
}
Counter.label = "clicks";
function loadGreeting(raw) {
    return Eldur.asyncRun(this, function*() {
    const merged = Object.assign({}, raw, { extra: true });
    const __eldur_obj_0 = merged, { extra } = __eldur_obj_0, rest = Eldur.objectRest(__eldur_obj_0, ["extra"]);
    let status = rest.status;
    status ?? (status = "ok");
    try {
        JSON.parse(status);
    } catch (__eldur_err) {
        status = "fallback";
    }
    const compute = (x) => Eldur.asyncRun(this, function*() { return ((yield (Eldur.Promise.resolve(x)))) + 1; });
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
    assert_eq!(out.init_function.as_deref(), Some("__eldur_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

const UNIT = "px";
function double(n) {
    return n * 2;
}
function formatPx(n) {
    return double(n) + UNIT;
}
const fmt = formatPx;
const mul2 = double;
const MathNs = { get UNIT() { return UNIT; }, get default() { return formatPx; }, get double() { return double; } };
const VERSION = 1;

function __eldur_run_m0() {
    if (__eldur_run_m0.state === 2) throw __eldur_run_m0.error;
    if (__eldur_run_m0.state) return;
    __eldur_run_m0.state = 3;
    try {
        outText.text = fmt(mul2(3)) + ":" + MathNs.UNIT;
    } catch (err) {
        __eldur_run_m0.state = 2;
        __eldur_run_m0.error = err;
        throw err;
    }
    __eldur_run_m0.state = 1;
}

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
    try {
        __eldur_run_m0();
    } catch (err) {
        Eldur.reportError(err);
    }
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
    assert_eq!(out.init_function.as_deref(), Some("__eldur_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

var stage = 1;
var current = function current() { return "one"; };

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
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
.import "eldur_runtime.js" as Eldur

let mode, extra;

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
    { const __eldur_obj_0 = getSettings(); ({ mode } = __eldur_obj_0); extra = Eldur.objectRest(__eldur_obj_0, ["mode"]); }
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
    assert_eq!(out.bindings[0].function, "__eldur_on_click_boxItem_0");
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

function setup(cb) {
    Eldur.listen(boxItem, "click", cb);
    Eldur.unlisten(boxItem, "click", cb);
    Eldur.dispatchEvent(boxItem, new Eldur.CustomEvent("ping"));
    return Eldur.getBoundingClientRect(boxItem);
}

function __eldur_on_click_boxItem_0(event) {
    Eldur.dispatchEvent(boxItem, event);
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
    assert_eq!(out.bindings[0].function, "__eldur_on_click_boxItem_0");
    assert_eq!(out.init_function.as_deref(), Some("__eldur_init"));
    assert!(!out.page_js.contains("__eldur_listeners_"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

function bindAndExercise(cb) {
    Eldur.listen(boxItem, "click", cb);
    Eldur.listen(boxItem, "click", cb);
    Eldur.dispatchEvent(boxItem, new Eldur.CustomEvent("click"));
    Eldur.unlisten(boxItem, "click", cb);
    Eldur.dispatchEvent(boxItem, new Eldur.CustomEvent("click"));
}

function __eldur_on_click_boxItem_0(event) {
    Eldur.dispatchEvent(boxItem, event);
}

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
    Eldur.listen(boxItem, "click", onTop);
    Eldur.listen(boxItem, "click", onOnce, { once: true });
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
    assert_eq!(out.init_function.as_deref(), Some("__eldur_init"));
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

let state;
function setState(v) { state = v; }

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
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
.import "eldur_runtime.js" as Eldur

function __eldur_init(rootItem) {
    Eldur.init(rootItem || (typeof root !== "undefined" ? root : null));
    Eldur.classList(bodyBox).add("dark");
    Eldur.dataset(htmlRoot).theme = "night";
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
    assert_eq!(out.page_js, ".import \"eldur_runtime.js\" as Eldur\n");
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

    let out = run_rewrite_resolved(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(
        !out.page_js.contains(".innerHTML"),
        "raw .innerHTML access leaked into page_js:\n{}",
        out.page_js
    );
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

function updateDom(suffix, extraClass, htmlChunk, fallbackRole, node, state) {

    ((__v) => (Eldur.setTitle(__v), __v))(Eldur.getTitle() + (suffix));
    Eldur.getTitle() || ((__v) => (Eldur.setTitle(__v), __v))("Untitled");
    Eldur.getTitle() && ((__v) => (Eldur.setTitle(__v), __v))("Prefixed: " + suffix);
    Eldur.getTitle() ?? ((__v) => (Eldur.setTitle(__v), __v))("DefaultTitle");

    !saveBtn.enabled || !(saveBtn.enabled = false);
    !saveBtn.enabled && !(saveBtn.enabled = true);
    (!saveBtn.enabled) ?? !(saveBtn.enabled = false);
    Eldur.wrap(saveBtn).hidden && (Eldur.wrap(saveBtn).hidden = false);
    Eldur.wrap(saveBtn).hidden || (Eldur.wrap(saveBtn).hidden = true);
    Eldur.wrap(saveBtn).hidden ?? (Eldur.wrap(saveBtn).hidden = false);
    !(saveBtn.enabled = !((!saveBtn.enabled) | (true)));
    Eldur.wrap(saveBtn).hidden &= false;

    roleCombo.currentValue || ((__v) => (roleCombo.currentIndex = roleCombo.indexOfValue(__v), __v))(fallbackRole);
    roleCombo.currentValue && ((__v) => (roleCombo.currentIndex = roleCombo.indexOfValue(__v), __v))("Admin");
    roleCombo.currentValue ?? ((__v) => (roleCombo.currentIndex = roleCombo.indexOfValue(__v), __v))("Viewer");
    ((__v) => (roleCombo.currentIndex = roleCombo.indexOfValue(__v), __v))(roleCombo.currentValue + (suffix));

    ((__v) => (Eldur.setClassName(cardBox, __v), __v))(Eldur.getClassName(cardBox) + (extraClass));
    Eldur.getClassName(cardBox) || ((__v) => (Eldur.setClassName(cardBox, __v), __v))("card-default");
    Eldur.getClassName(cardBox) && ((__v) => (Eldur.setClassName(cardBox, __v), __v))("card-active");
    Eldur.getClassName(cardBox) ?? ((__v) => (Eldur.setClassName(cardBox, __v), __v))("card-fallback");

    ((__v) => (Eldur.setInnerHTML(cardBox, __v), __v))(Eldur.getInnerHTML(cardBox) + (htmlChunk));
    Eldur.getInnerHTML(cardBox) || ((__v) => (Eldur.setInnerHTML(cardBox, __v), __v))("<b>empty</b>");
    Eldur.getInnerHTML(cardBox) && ((__v) => (Eldur.setInnerHTML(cardBox, __v), __v))("<i>ready</i>");
    Eldur.getInnerHTML(cardBox) ?? ((__v) => (Eldur.setInnerHTML(cardBox, __v), __v))("<u>init</u>");

    titleLabel.text += suffix;
    titleLabel.text || (titleLabel.text = "N/A");
    cardBoxText.text += suffix;
    nameInput.text += suffix;
    nameInput.text || (nameInput.text = "anon");
    nameInput.placeholderText ?? (nameInput.placeholderText = "Enter name");
    volSlider.value -= 5;
    agreeCheck.checked && (agreeCheck.checked = true);
    avatarImg.source || (avatarImg.source = "qrc:/fallback.png");

    ((__v) => (Eldur.setStyle(cardBox, "color", __v), __v))(Eldur.getStyle(cardBox, "color") + ("00"));
    Eldur.getStyle(cardBox, "color") || ((__v) => (Eldur.setStyle(cardBox, "color", __v), __v))("#112233");
    Eldur.getStyle(cardBox, "color") && ((__v) => (Eldur.setStyle(cardBox, "color", __v), __v))("#445566");
    Eldur.getStyle(cardBox, "color") ?? ((__v) => (Eldur.setStyle(cardBox, "color", __v), __v))("#778899");
    Eldur.getStyle(cardBox, "display") || ((__v) => (Eldur.setStyle(cardBox, "display", __v), __v))("none");
    Eldur.getStyle(cardBox, "display") && ((__v) => (Eldur.setStyle(cardBox, "display", __v), __v))("flex");

    ((__v) => (Eldur.setInnerHTML(node, __v), __v))(Eldur.getInnerHTML(node) + (htmlChunk));
    Eldur.getInnerHTML(node) || ((__v) => (Eldur.setInnerHTML(node, __v), __v))("<span>node</span>");
    Eldur.getText(node) || ((__v) => (Eldur.setText(node, __v), __v))("plain");

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

    let out = run_rewrite_resolved(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(out.init_function, None);
    insta::assert_snapshot!(out.page_js, @r###"
.import "eldur_runtime.js" as Eldur

function captureAssigned(rhs, pickNode) {

    const a = (Eldur.getTitle() || ((__v) => (Eldur.setTitle(__v), __v))("default"));
    const b = (((__v) => (Eldur.setTitle(__v), __v))(Eldur.getTitle() + (rhs())));
    const c = (Eldur.wrap(saveBtn).hidden && (Eldur.wrap(saveBtn).hidden = false));
    const d = (roleCombo.currentValue || ((__v) => (roleCombo.currentIndex = roleCombo.indexOfValue(__v), __v))(rhs()));
    const e = (((__v) => (Eldur.setStyle(cardBox, "display", __v), __v))("none"));
    const f = (((__o, __v) => (Eldur.setText(__o, __v), __v))(pickNode(0), rhs()));
    const g = (((__o) => ((__v) => (Eldur.setText(__o, __v), __v))(Eldur.getText(__o) + (rhs())))(pickNode(1)));
    const h = (((__o) => Eldur.getInnerHTML(__o) || ((__v) => (Eldur.setInnerHTML(__o, __v), __v))(rhs()))(pickNode(2)));
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
        ".import \"eldur_runtime.js\" as Eldur\n\nlet x = 1;\n"
    );

    let s3 = classic("s3.js", "if (true) { var y = 1; }");
    let s4 = classic("s4.js", "let y = 2;");
    let out2 = run_rewrite(&[s3, s4], &resolver);
    assert_eq!(out2.diagnostics.len(), 1);
    assert_eq!(out2.diagnostics[0].severity, Severity::Error);
    assert_eq!(out2.diagnostics[0].source, "s4.js");
    assert_eq!(out2.init_function.as_deref(), Some("__eldur_init"));
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
    assert_eq!(out.init_function.as_deref(), Some("__eldur_init"));
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
        "true-branch assignment order inside __eldur_init was not preserved:\n{}",
        out.page_js
    );
    assert!(
        !out.page_js.contains("    var trueBranch")
            && !out.page_js.contains("for (var i")
            && !out.page_js.contains("for (var item"),
        "`var` keyword remained inside __eldur_init, shadowing file-scope var:\n{}",
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
    // The two observers are supported, so they make no diagnostic.
    assert_eq!(
        out.diagnostics.len(),
        7,
        "expected 7 unsupported diagnostics without duplicates, got: {:#?}",
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

#[test]
fn isolates_colliding_module_private_const_let_function_and_class_bindings() {
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let mod_a = module_script(
        "js/a.js",
        r#"
const factor = 2;
let offset = 10;
function helper(n) {
    return n * factor + offset;
}
class Box {
    static tag = "A";
    constructor(v) {
        this.value = helper(v);
    }
    describe() {
        return Box.tag + ":" + this.value;
    }
}
export function computeA(x) {
    offset += 1;
    return new Box(x).describe();
}
"#,
    );

    let mod_b = module_script(
        "js/b.js",
        r#"
const factor = 3;
let offset = 100;
function helper(n) {
    return n * factor + offset;
}
class Box {
    static tag = "B";
    constructor(v) {
        this.value = helper(v);
    }
    describe() {
        return Box.tag + ":" + this.value;
    }
}
export function computeB(x) {
    offset += 5;
    return new Box(x).describe();
}
"#,
    );

    let main_mod = module_script(
        "js/main.js",
        r#"
import { computeA } from "./a.js";
import { computeB } from "./b.js";
document.getElementById("out").textContent = computeA(4) + "|" + computeB(4);
"#,
    );

    let out = run_rewrite(&[main_mod, mod_a, mod_b], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(
        !out.page_js.contains("const factor =")
            && !out.page_js.contains("let offset =")
            && !out.page_js.contains("function helper(")
            && !out.page_js.contains("class Box {"),
        "module-private bindings should be hygienically renamed:\n{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("const __eldur_m1_factor = 2;")
            && out.page_js.contains("const __eldur_m2_factor = 3;")
            && out.page_js.contains("let __eldur_m1_offset = 10;")
            && out.page_js.contains("let __eldur_m2_offset = 100;")
            && out.page_js.contains("function __eldur_m1_helper(n)")
            && out.page_js.contains("function __eldur_m2_helper(n)")
            && out.page_js.contains("class __eldur_m1_Box")
            && out.page_js.contains("class __eldur_m2_Box")
            && out.page_js.contains("__eldur_m1_Box.tag = \"A\";")
            && out.page_js.contains("__eldur_m2_Box.tag = \"B\";"),
        "expected isolated module-private bindings for both modules:\n{}",
        out.page_js
    );
}

#[test]
fn isolates_module_private_bindings_from_classic_script_declarations() {
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let widget_mod = module_script(
        "js/widget.js",
        r#"
const factor = 2;
let offset = 3;
var sharedState = "module";
function helper(v) {
    return v * factor + offset;
}
class Box {
    read(v) {
        return sharedState + ":" + helper(v);
    }
}
export function readWidget(v) {
    return new Box().read(v);
}
"#,
    );

    let classic_before = classic(
        "js/classic_before.js",
        r#"
const factor = 100;
let offset = 50;
"#,
    );

    let classic_after = classic(
        "js/classic_after.js",
        r#"
var sharedState = "classic";
function helper(v) {
    return v + factor + offset;
}
class Box {
    read(v) {
        return sharedState + ":" + helper(v);
    }
}
document.getElementById("out").textContent = new Box().read(1);
"#,
    );

    let out = run_rewrite(&[widget_mod, classic_before, classic_after], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(
        out.page_js.matches("const factor =").count(),
        1,
        "{}",
        out.page_js
    );
    assert_eq!(
        out.page_js.matches("let offset =").count(),
        1,
        "{}",
        out.page_js
    );
    assert_eq!(
        out.page_js.matches("var sharedState =").count(),
        1,
        "{}",
        out.page_js
    );
    assert_eq!(
        out.page_js.matches("function helper(").count(),
        1,
        "{}",
        out.page_js
    );
    assert_eq!(
        out.page_js.matches("class Box ").count(),
        1,
        "{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("const factor = 100;")
            && out.page_js.contains("let offset = 50;")
            && out.page_js.contains("var sharedState = \"classic\";")
            && out.page_js.contains("const __eldur_m0_factor = 2;")
            && out.page_js.contains("let __eldur_m0_offset = 3;")
            && out
                .page_js
                .contains("var __eldur_m0_sharedState = \"module\";")
            && out.page_js.contains("function __eldur_m0_helper(v)")
            && out.page_js.contains("class __eldur_m0_Box"),
        "expected classic declarations to keep global names and module-private declarations to be isolated:\n{}",
        out.page_js
    );
}

#[test]
fn connects_imported_bindings_and_references_to_correct_exporter_under_collisions() {
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let exp_a = module_script(
        "js/exp_a.js",
        r#"
const secret = 10;
const alias = 999;
export const token = secret + 1;
export function compute(x) {
    return x + token + (alias - 999);
}
"#,
    );

    let exp_b = module_script(
        "js/exp_b.js",
        r#"
const secret = 20;
export const token = secret + 2;
export function compute(x) {
    return x * token;
}
"#,
    );

    let use_a = module_script(
        "js/use_a.js",
        r#"
import { token, compute as alias } from "./exp_a.js";
const localBias = 1;
export function fromA(v) {
    return alias(v) + token + localBias;
}
"#,
    );

    let use_b = module_script(
        "js/use_b.js",
        r#"
import { token, compute as alias } from "./exp_b.js";
const localBias = 2;
export function fromB(v) {
    return alias(v) + token + localBias;
}
"#,
    );

    let main_mod = module_script(
        "js/main.js",
        r#"
import { fromA } from "./use_a.js";
import { fromB } from "./use_b.js";
document.getElementById("out").textContent = fromA(5) + ":" + fromB(5);
"#,
    );

    let out = run_rewrite(&[main_mod, use_a, use_b, exp_a, exp_b], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert_eq!(
        out.page_js.matches("const token =").count(),
        1,
        "{}",
        out.page_js
    );
    assert_eq!(
        out.page_js.matches("function compute(").count(),
        1,
        "{}",
        out.page_js
    );
    assert_eq!(
        out.page_js.matches("const alias =").count(),
        1,
        "{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("function __eldur_m4_compute(x)")
            && out
                .page_js
                .contains("const __eldur_m4_token = __eldur_m4_secret + 2;")
            && out.page_js.contains(
                "return __eldur_m4_compute(v) + __eldur_m4_token + __eldur_m2_localBias;"
            ),
        "expected second importer to stay connected to second exporter's renamed bindings:\n{}",
        out.page_js
    );
}

#[test]
fn preserves_execution_order_and_single_evaluation_for_repeated_shared_imports_and_keeps_function_shadowing_untouched()
 {
    let resolver = FakeResolver::default();

    let shared_mod = module_script(
        "js/shared.js",
        r#"
const step = 2;
export let ticks = 0;
export function record(tag) {
    ticks += step;
    return tag + "@" + ticks;
}
console.log("init:shared:" + record("S"));
"#,
    );

    let left_mod = module_script(
        "js/left.js",
        r#"
import { record, ticks as seenTicks } from "./shared.js";
const label = "L";
export function checkShadow(label, step) {
    const ticks = 100;
    {
        const label = "block";
        console.log(label);
    }
    return [label, step, ticks, seenTicks, record("call")].join(":");
}
console.log("init:left:" + record(label) + ":" + seenTicks);
"#,
    );

    let right_mod = module_script(
        "js/right.js",
        r#"
import { record, ticks as seenTicks } from "./shared.js";
const label = "R";
console.log("init:right:" + record(label) + ":" + seenTicks);
"#,
    );

    let main_mod = module_script(
        "js/main.js",
        r#"
import { checkShadow } from "./left.js";
import "./right.js";
import { record, ticks as seenTicks } from "./shared.js";
console.log("init:main:" + record("M") + ":" + seenTicks + ":" + checkShadow("arg", 7));
"#,
    );

    let out = run_rewrite(&[main_mod, left_mod, right_mod, shared_mod], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    // 1. Shared module side effect is emitted exactly once and before left, right, main.
    assert_eq!(
        out.page_js.matches("init:shared:").count(),
        1,
        "{}",
        out.page_js
    );
    let pos_shared = out.page_js.find("init:shared:").expect("shared init");
    let pos_left = out.page_js.find("init:left:").expect("left init");
    let pos_right = out.page_js.find("init:right:").expect("right init");
    let pos_main = out.page_js.find("init:main:").expect("main init");
    assert!(
        pos_shared < pos_left && pos_left < pos_right && pos_right < pos_main,
        "expected shared -> left -> right -> main execution order in page_js:\n{}",
        out.page_js
    );

    // 2. Lexical shadowing inside `checkShadow` remains untouched while module-scope `seenTicks` and `record` resolve to `shared.js`.
    assert!(
        out.page_js.contains("function checkShadow(label, step)")
            && out.page_js.contains("const ticks = 100;")
            && out.page_js.contains("const label = \"block\";")
            && out
                .page_js
                .contains("return [label, step, ticks, ticks, record(\"call\")].join(\":\");"),
        "expected function-scoped shadowed bindings to remain untouched and imported live binding to resolve to exporter:\n{}",
        out.page_js
    );
}

#[test]
fn deduplicates_top_level_destructured_var_slots_and_preserves_init_order() {
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let s1 = classic(
        "s1.js",
        r#"
var log = [];
var a = 1;
function sideEffect(tag) {
    log.push(tag + ":" + a + ":" + typeof b + ":" + b + ":" + typeof c + ":" + c);
    a = 99;
}
sideEffect("before");
var { a, b: [c, { d = 4 }] } = { a: 2, b: [3, {}] };
sideEffect("mid");
var [a, e] = [5, 6];
"#,
    );
    let s2 = classic(
        "s2.js",
        r#"
var a = 7;
sideEffect("s2-after-scalar");
var { c, e } = { c: 8, e: 9 };
var [d, f] = [10, 11];
document.getElementById("out").textContent = log.join("|") + "|" + [a, c, d, e, f].join(",");
"#,
    );

    let out = run_rewrite(&[s1, s2], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let init_pos = out.page_js.find("function __eldur_init").expect("init fn");
    let top_scope = &out.page_js[..init_pos];
    let init_scope = &out.page_js[init_pos..];

    assert_eq!(
        top_scope.matches("var a").count(),
        1,
        "expected a single top-level `var a` declaration:\n{}",
        out.page_js
    );
    for name in ["c", "d", "e", "f"] {
        let pat = format!("var {name};");
        assert_eq!(
            top_scope.matches(&pat).count(),
            1,
            "expected a single `{pat}` in top_scope:\n{}",
            out.page_js
        );
    }
    assert!(
        !top_scope.contains("var {") && !top_scope.contains("var ["),
        "destructured `var` should not be emitted directly in top_decls:\n{}",
        out.page_js
    );

    let p_before = init_scope
        .find("sideEffect(\"before\");")
        .expect("before call");
    let p_obj1 = init_scope
        .find("({ a, b: [c, { d = 4 }] } = { a: 2, b: [3, {}] });")
        .expect("first object destructuring in init_stmts");
    let p_mid = init_scope.find("sideEffect(\"mid\");").expect("mid call");
    let p_arr1 = init_scope
        .find("[a, e] = [5, 6];")
        .expect("first array destructuring in init_stmts");
    let p_scalar = init_scope
        .find("a = 7;")
        .expect("later-script scalar var reuse in init_stmts");
    let p_s2_call = init_scope
        .find("sideEffect(\"s2-after-scalar\");")
        .expect("s2 call");
    let p_obj2 = init_scope
        .find("({ c, e } = { c: 8, e: 9 });")
        .expect("second object destructuring in init_stmts");
    let p_arr2 = init_scope
        .find("[d, f] = [10, 11];")
        .expect("second array destructuring in init_stmts");

    assert!(
        p_before < p_obj1
            && p_obj1 < p_mid
            && p_mid < p_arr1
            && p_arr1 < p_scalar
            && p_scalar < p_s2_call
            && p_s2_call < p_obj2
            && p_obj2 < p_arr2,
        "expected destructuring and intervening statements to preserve execution order:\n{}",
        out.page_js
    );
}

#[test]
fn preserves_empty_statement_for_hoisted_uninitialized_nested_var_and_for_headers() {
    let resolver = FakeResolver::default().with_id("out", 1, "outText", TargetKind::Text);

    let s1 = classic(
        "s1.js",
        r#"
var log = [];
if (false) var branchSingle;
log.push("after-single");
if (false)
    var branchNewline;
log.push("after-newline");
if (true) var thenOnly; else var elseOnly;
while (false) var whileOnly;
do var doOnly; while (false);
for (var forHdr; false; ) var forBody;
for (var forOfHdr of ["x", "y"]) var forOfBody;
if (true) {
    var blockOnly;
}
function keepLocal() {
    var localUninit;
    if (false) var localBranch;
    for (var localIdx = 0; localIdx < 2; localIdx++) {}
    return [typeof localUninit, typeof localBranch, localIdx].join(":");
}
document.getElementById("out").textContent = log.join("|") + "|" + forOfHdr + "|" + keepLocal();
"#,
    );

    let out = run_rewrite(&[s1], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    let init_pos = out.page_js.find("function __eldur_init").expect("init fn");
    let top_scope = &out.page_js[..init_pos];
    let init_scope = &out.page_js[init_pos..];

    for name in [
        "branchSingle",
        "branchNewline",
        "thenOnly",
        "elseOnly",
        "whileOnly",
        "doOnly",
        "forHdr",
        "forBody",
        "forOfHdr",
        "forOfBody",
        "blockOnly",
    ] {
        let decl = format!("var {name};");
        assert!(
            top_scope.contains(&decl),
            "expected `{decl}` hoisted to file scope:\n{}",
            out.page_js
        );
    }
    let keep_local_pos = top_scope
        .find("function keepLocal()")
        .expect("keepLocal fn");
    let file_var_scope = &top_scope[..keep_local_pos];
    for local_name in ["localUninit", "localBranch", "localIdx"] {
        let decl = format!("var {local_name}");
        assert!(
            !file_var_scope.contains(&decl),
            "function-local `{decl}` should not be hoisted to file scope:\n{}",
            out.page_js
        );
    }
    assert!(
        top_scope.contains("var localUninit;")
            && top_scope.contains("if (false) var localBranch;")
            && top_scope.contains("for (var localIdx = 0; localIdx < 2; localIdx++)"),
        "function-local vars should remain untouched inside keepLocal:\n{}",
        out.page_js
    );
    assert!(
        !init_scope.contains("var "),
        "no `var` keyword should remain inside __eldur_init:\n{}",
        out.page_js
    );
    assert!(
        init_scope.contains("if (false) ;")
            && init_scope.contains("if (true) ; else ;")
            && init_scope.contains("while (false) ;")
            && init_scope.contains("do ; while (false);")
            && init_scope.contains("for (; false; ) ;")
            && init_scope.contains("for (forOfHdr of [\"x\", \"y\"]) ;"),
        "expected valid empty statements and for-header handling in __eldur_init:\n{}",
        out.page_js
    );
}

#[test]
fn unifies_mixed_on_property_and_add_event_listener_onto_dynamic_event_path() {
    let resolver = FakeResolver::default()
        .with_id("btn", 1, "btn", TargetKind::Button)
        .with_id("solo", 2, "solo", TargetKind::Button);

    let s = classic(
        "s.js",
        r#"
function f1(ev) {
    if (ev.defaultPrevented) return;
}
function f2(ev) {
    ev.stopImmediatePropagation();
}
const btn = document.getElementById("btn");
btn.onclick = f1;
btn.addEventListener("click", f2);

const solo = document.getElementById("solo");
solo.onclick = f1;

function trigger() {
    btn.dispatchEvent(new Event("click"));
}
"#,
    );

    let out = run_rewrite(&[s], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    // `btn` has both `onclick` and `addEventListener("click")` (plus `dispatchEvent`),
    // so both must share the dynamic `Eldur.setOn` / `Eldur.listen` / `Eldur.dispatchEvent` path
    // with a single forwarding `EventBinding` (`__eldur_on_click_btn_0`), while `solo` stays
    // on the static fast path (`__eldur_on_click_solo`).
    assert_eq!(
        out.bindings,
        vec![
            EventBinding {
                target: FakeResolver::target(1, "btn", TargetKind::Button),
                event: "click".to_string(),
                function: "__eldur_on_click_btn_0".to_string(),
            },
            EventBinding {
                target: FakeResolver::target(2, "solo", TargetKind::Button),
                event: "click".to_string(),
                function: "__eldur_on_click_solo".to_string(),
            },
        ]
    );
    assert!(
        out.page_js.contains("Eldur.setOn(btn, \"click\", f1);")
            && out.page_js.contains("Eldur.listen(btn, \"click\", f2);")
            && out.page_js.contains(
                "function __eldur_on_click_btn_0(event) {\n    Eldur.dispatchEvent(btn, event);\n}"
            ),
        "expected unified dynamic Eldur.setOn + Eldur.listen + Eldur.dispatchEvent path for btn:\n{}",
        out.page_js
    );
    assert!(
        !out.page_js.contains("function __eldur_on_click_btn(")
            && !out.page_js.contains("__eldur_prop_click_btn"),
        "btn.onclick must not stay on a separate static or prop-slot EventBinding:\n{}",
        out.page_js
    );
}

#[test]
fn preserves_shorthand_property_keys_for_elided_dom_bindings_and_module_renames() {
    let resolver = FakeResolver::default()
        .with_id("save-btn", 1, "save_btn", TargetKind::Button)
        .with_id("hdr", 2, "main_hdr", TargetKind::Text);

    let s1 = classic(
        "s1.js",
        r#"
const btn = document.getElementById("save-btn");
const hdr = document.getElementById("hdr");
const count = 5;
const state = { btn, count, explicit: btn };
const { picked = btn, count: renamedCount = 0, ...restTop } = { count: 7, keep: 9 };
let assigned;
let plainAssign;
({ assigned = btn, plainAssign } = { plainAssign: 11 });

function buildSnapshot(extra) {
    const { localPick = hdr, plainLocal, ...restLocal } = extra;
    let fnAssigned;
    ({ fnAssigned = btn } = extra);
    return { btn, hdr, count, localPick, plainLocal, fnAssigned, explicitHdr: hdr, ...restLocal };
}
"#,
    );

    let m1 = module_script(
        "m1.js",
        r#"
export const shared = 10;
"#,
    );

    let m2 = module_script(
        "m2.js",
        r#"
const btn = document.getElementById("save-btn");
const shared = 20;
const plainMod = 30;
const { modPick = btn, shared: modShared = 0, ...modRest } = { extraMod: 1 };
export const modState = { btn, shared, plainMod, explicitBtn: btn };
export function readMod(src) {
    const { shared: aliased = btn, fallback = btn, ...restMod } = src;
    return { btn, shared, aliased, fallback, restMod, modPick, modShared, modRest };
}
let modTarget = 0;
({ modTarget = btn } = {});
"#,
    );

    let out = run_rewrite_resolved(&[s1, m1, m2], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);

    // 1. Elided DOM binding shorthand in classic top-level and function scope preserves `btn: save_btn` and `hdr: main_hdr`
    assert!(
        out.page_js.contains(
            "state = { btn: Eldur.wrap(save_btn), count, explicit: Eldur.wrap(save_btn) };"
        ),
        "expected classic top-level shorthand `{{ btn }}` to preserve key `btn: save_btn` and leave `count` / `explicit: save_btn` intact:\n{}",
        out.page_js
    );
    assert!(
        !out.page_js.contains("{ save_btn") && !out.page_js.contains(", Eldur.wrap(save_btn),"),
        "elided DOM shorthand must not collapse `{{ btn }}` into `{{ save_btn }}`:\n{}",
        out.page_js
    );
    assert!(
        out.page_js.contains(
            "Object.assign({ btn: Eldur.wrap(save_btn), hdr: Eldur.wrap(main_hdr), count, localPick, plainLocal, fnAssigned, explicitHdr: Eldur.wrap(main_hdr) }, restLocal)"
        ),
        "expected function-scope object literal with spread to preserve shorthand keys for `btn` and `hdr` while keeping ordinary shorthand unchanged:\n{}",
        out.page_js
    );

    // 2. Shorthand binding declarations + rest + defaults with elided DOM bindings
    assert!(
        out.page_js.contains("({ picked = Eldur.wrap(save_btn), count: renamedCount = 0 } = __eldur_obj_0); restTop = Eldur.objectRest(__eldur_obj_0, [\"picked\", \"count\"]);"),
        "expected top-level destructuring with default and object rest to preserve shorthand key `picked = save_btn`:\n{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("{ localPick = Eldur.wrap(main_hdr), plainLocal } = __eldur_obj_1, restLocal = Eldur.objectRest(__eldur_obj_1, [\"localPick\", \"plainLocal\"])"),
        "expected function-local destructuring with default and object rest to preserve `localPick = main_hdr` and `plainLocal`:\n{}",
        out.page_js
    );

    // 3. Shorthand destructuring assignment with default referencing elided DOM binding
    assert!(
        out.page_js
            .contains("({ assigned = Eldur.wrap(save_btn), plainAssign } = { plainAssign: 11 });")
            && out
                .page_js
                .contains("({ fnAssigned = Eldur.wrap(save_btn) } = extra);"),
        "expected shorthand destructuring assignments to preserve key and rewrite default to `save_btn`:\n{}",
        out.page_js
    );

    // 4. Module symbol_renames alongside elided DOM bindings in m2.js
    assert!(
        out.page_js.contains("modState = { btn: Eldur.wrap(save_btn), shared: __eldur_m2_shared, plainMod: __eldur_m2_plainMod, explicitBtn: Eldur.wrap(save_btn) };"),
        "expected module object literal to preserve keys for both elided DOM `btn: save_btn` and renamed module symbols:\n{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("({ modPick: __eldur_m2_modPick = Eldur.wrap(save_btn), shared: __eldur_m2_modShared = 0 } = __eldur_obj_2); __eldur_m2_modRest = Eldur.objectRest(__eldur_obj_2, [\"modPick\", \"shared\"]);") && out.page_js.contains("return { btn: Eldur.wrap(save_btn), shared: __eldur_m2_shared, aliased, fallback, restMod, modPick: __eldur_m2_modPick, modShared: __eldur_m2_modShared, modRest: __eldur_m2_modRest };"),
        "expected module function return object to preserve `btn: save_btn` and `shared: __eldur_m2_shared` while keeping local shorthand unchanged:\n{}",
        out.page_js
    );
    assert!(
        out.page_js
            .contains("({ modTarget: __eldur_m2_modTarget = Eldur.wrap(save_btn) } = {});"),
        "expected module destructuring assignment to preserve `modTarget: __eldur_m2_modTarget = save_btn`:\n{}",
        out.page_js
    );
}

#[test]
fn registers_block_nested_module_var_in_exports_with_hygienic_rename() {
    let resolver = FakeResolver::default();
    let classic_script = classic("c.js", "var count = 1;\n");
    let a = module_script(
        "a.js",
        r#"
{ var count = 10; }
if (false) { var skipped = 1; }
function local() { if (true) { var inner = 1; } return inner; }
export { count, skipped as renamedSkipped };
"#,
    );
    let b = module_script(
        "b.js",
        r#"
import { count, renamedSkipped } from "./a.js";
import * as ns from "./a.js";
console.log(count, renamedSkipped, ns.count);
"#,
    );

    let out = run_rewrite(&[classic_script, a, b], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let js = &out.page_js;
    assert!(js.contains("var __eldur_m1_count;"), "{js}");
    assert!(js.contains("var __eldur_m1_skipped;"), "{js}");
    assert!(js.contains("__eldur_m1_count = 10;"), "{js}");
    assert!(
        js.contains("renamedSkipped = __eldur_m1_skipped;")
            && js.contains("console.log(__eldur_m1_count, renamedSkipped, ns.count);"),
        "importer must alias the hoisted exports:\n{js}"
    );
    assert!(
        js.contains(
            "get count() { return __eldur_m1_count; }, get renamedSkipped() { return __eldur_m1_skipped; }"
        ),
        "namespace object must read the hoisted exports live:\n{js}"
    );
    assert!(
        js.contains("var inner = 1;"),
        "function-local var untouched:\n{js}"
    );
    assert!(!js.contains("__eldur_m1_inner"), "{js}");
}

#[test]
fn routes_unmapped_dom_members_through_wrapper_and_coerces_range_value_to_string() {
    let resolver = FakeResolver::default()
        .with_id("list", 1, "list_", TargetKind::Box)
        .with_id("vol", 2, "volSlider", TargetKind::Range)
        .with_id("spin", 3, "spinBox", TargetKind::Range)
        .with_id("prog", 4, "loadBar", TargetKind::Progress)
        .with_id("field", 5, "fieldInput", TargetKind::TextField);

    let script = classic(
        "members_and_range.js",
        r#"
function inspectDom() {
    const list = document.getElementById("list");
    const vol = document.getElementById("vol");
    const spin = document.getElementById("spin");
    const prog = document.getElementById("prog");
    const field = document.getElementById("field");

    const len = list.children.length;
    const count = list.childElementCount;
    const first = list.firstElementChild;
    const last = list.lastElementChild;
    const parent = list.parentElement;
    const next = list.nextElementSibling;
    const prev = list.previousElementSibling;
    const curId = list.id;
    const tag = list.tagName;
    const attr = list.getAttribute("data-role");

    list.id = "main-list";
    field.type = "checkbox";
    list.customProp ||= 1;
    list.customProp++;

    const rangeConcat = vol.value + 1;
    const progAdd = prog.value + 1;
    spin.value++;
    --spin.value;
    return [len, count, first, last, parent, next, prev, curId, tag, attr, rangeConcat, progAdd, spin.value];
}
"#,
    );

    let out = run_rewrite_resolved(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let js = &out.page_js;
    assert!(
        js.contains("const len = Eldur.wrap(list_).children.length;"),
        "{js}"
    );
    assert!(
        js.contains("const count = Eldur.wrap(list_).childElementCount;"),
        "{js}"
    );
    assert!(
        js.contains("const first = Eldur.wrap(list_).firstElementChild;"),
        "{js}"
    );
    assert!(
        js.contains("const last = Eldur.wrap(list_).lastElementChild;"),
        "{js}"
    );
    assert!(
        js.contains("const parent = Eldur.wrap(list_).parentElement;"),
        "{js}"
    );
    assert!(
        js.contains("const next = Eldur.wrap(list_).nextElementSibling;"),
        "{js}"
    );
    assert!(
        js.contains("const prev = Eldur.wrap(list_).previousElementSibling;"),
        "{js}"
    );
    assert!(js.contains("const curId = Eldur.wrap(list_).id;"), "{js}");
    assert!(
        js.contains("const tag = Eldur.wrap(list_).tagName;"),
        "{js}"
    );
    assert!(
        js.contains("const attr = Eldur.getAttribute(list_, \"data-role\");"),
        "{js}"
    );
    assert!(js.contains("Eldur.wrap(list_).id = \"main-list\";"), "{js}");
    assert!(
        js.contains("Eldur.wrap(fieldInput).type = \"checkbox\";"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(list_).customProp || (Eldur.wrap(list_).customProp = 1);"),
        "{js}"
    );
    assert!(js.contains("Eldur.wrap(list_).customProp++;"), "{js}");
    assert!(
        js.contains("const rangeConcat = String(volSlider.value) + 1;"),
        "{js}"
    );
    assert!(js.contains("const progAdd = loadBar.value + 1;"), "{js}");
    assert!(js.contains("spinBox.value++;"), "{js}");
    assert!(js.contains("--spinBox.value;"), "{js}");
    assert!(js.contains("String(spinBox.value)]"), "{js}");
}

#[test]
fn rewrites_extended_dom_methods_style_helpers_and_browser_globals() {
    let resolver = FakeResolver::default()
        .with_id("card", 1, "cardBox", TargetKind::Box)
        .with_id("btn", 2, "actBtn", TargetKind::Button);

    let script = classic(
        "extended_dom.js",
        r##"
function testExtended(dynEl) {
    const card = document.getElementById("card");
    const btn = document.getElementById("btn");

    card.toggleAttribute("data-open");
    dynEl.toggleAttribute("data-open", true);

    card.style.setProperty("--gap", "8px");
    const gap = card.style.getPropertyValue("--gap");
    card.style.removeProperty("--gap");
    const cs = getComputedStyle(card);

    const m = btn.matches(".primary");
    const c = btn.closest("#card");
    const has = card.contains(btn);
    const byCls = card.getElementsByClassName("item");
    const byTag = card.getElementsByTagName("button");

    card.insertAdjacentHTML("beforeend", "<span>hi</span>");
    card.insertAdjacentText("afterbegin", "lead");
    card.insertAdjacentElement("beforebegin", btn);
    card.append(btn);
    card.prepend(btn);
    card.before(btn);
    card.after(btn);
    card.replaceWith(btn);
    card.replaceChildren(btn);
    card.replaceChild(btn, dynEl);
    const cloned = card.cloneNode(true);
    card.scrollIntoView();
    card.scrollTo(0, 10);
    card.scrollBy(0, 5);
    btn.click();

    const dynClosest = dynEl.closest(".card");
    const dynMatches = dynEl.matches("div");
    dynEl.insertAdjacentHTML("afterend", "<p>ok</p>");

    scrollTo(10, 20);
    scrollBy(1, 2);
    window.scrollTo(30, 40);
    window.scrollBy(3, 4);
    const bus = new EventTarget();
    const kev = new KeyboardEvent("keydown", { key: "a" });
    const mev = new MouseEvent("click", { clientX: 5 });
    return [gap, cs, m, c, has, byCls, byTag, cloned, dynClosest, dynMatches, bus, kev, mev, pageXOffset, pageYOffset, devicePixelRatio];
}
"##,
    );

    let out = run_rewrite_resolved(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let js = &out.page_js;
    assert!(
        js.contains("Eldur.toggleAttribute(cardBox, \"data-open\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.toggleAttribute(dynEl, \"data-open\", true);"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).style.setProperty(\"--gap\", \"8px\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).style.getPropertyValue(\"--gap\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).style.removeProperty(\"--gap\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.getComputedStyle(Eldur.wrap(cardBox));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(actBtn).matches(\".primary\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(actBtn).closest(\"#card\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).contains(Eldur.wrap(actBtn));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).getElementsByClassName(\"item\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).getElementsByTagName(\"button\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).insertAdjacentHTML(\"beforeend\", \"<span>hi</span>\");"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).insertAdjacentText(\"afterbegin\", \"lead\");"),
        "{js}"
    );
    assert!(
        js.contains(
            "Eldur.wrap(cardBox).insertAdjacentElement(\"beforebegin\", Eldur.wrap(actBtn));"
        ),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).append(Eldur.wrap(actBtn));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).prepend(Eldur.wrap(actBtn));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).before(Eldur.wrap(actBtn));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).after(Eldur.wrap(actBtn));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).replaceWith(Eldur.wrap(actBtn));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).replaceChildren(Eldur.wrap(actBtn));"),
        "{js}"
    );
    assert!(
        js.contains("Eldur.wrap(cardBox).replaceChild(Eldur.wrap(actBtn), dynEl);"),
        "{js}"
    );
    assert!(js.contains("Eldur.wrap(cardBox).cloneNode(true);"), "{js}");
    assert!(js.contains("Eldur.wrap(cardBox).scrollIntoView();"), "{js}");
    assert!(js.contains("Eldur.wrap(cardBox).scrollTo(0, 10);"), "{js}");
    assert!(js.contains("Eldur.wrap(cardBox).scrollBy(0, 5);"), "{js}");
    assert!(js.contains("Eldur.wrap(actBtn).click();"), "{js}");
    assert!(js.contains("Eldur.wrap(dynEl).closest(\".card\");"), "{js}");
    assert!(js.contains("Eldur.wrap(dynEl).matches(\"div\");"), "{js}");
    assert!(
        js.contains("Eldur.wrap(dynEl).insertAdjacentHTML(\"afterend\", \"<p>ok</p>\");"),
        "{js}"
    );
    assert!(js.contains("Eldur.scrollTo(10, 20);"), "{js}");
    assert!(js.contains("Eldur.scrollBy(1, 2);"), "{js}");
    assert!(js.contains("Eldur.scrollTo(30, 40);"), "{js}");
    assert!(js.contains("Eldur.scrollBy(3, 4);"), "{js}");
    assert!(js.contains("new Eldur.EventTarget()"), "{js}");
    assert!(js.contains("new Eldur.KeyboardEvent("), "{js}");
    assert!(js.contains("new Eldur.MouseEvent("), "{js}");
    assert!(js.contains("Eldur.window.pageXOffset"), "{js}");
    assert!(js.contains("Eldur.window.pageYOffset"), "{js}");
    assert!(js.contains("Eldur.window.devicePixelRatio"), "{js}");
}

#[test]
fn preserves_user_object_and_class_methods_without_dom_hijacking() {
    let resolver = FakeResolver::default();
    let script = classic(
        "user_objects.js",
        r#"
class CustomModel {
    matches(q) { return "cls-matches:" + q; }
    closest(q) { return "cls-closest:" + q; }
    cloneNode(deep) { return "cls-clone:" + Boolean(deep); }
    toggleAttribute(k, f) { return "cls-toggle:" + k + ":" + Boolean(f); }
    addEventListener(t, fn) { return "cls-listen:" + t + ":" + fn(); }
}

function testUserObjects(dynObj) {
    const model = new CustomModel();
    const plain = {
        textContent: "initial",
        matches(q) { return "obj-matches:" + q; },
        closest(q) { return "obj-closest:" + q; },
        cloneNode(deep) { return "obj-clone:" + Boolean(deep); },
        toggleAttribute(k) { return "obj-toggle:" + k; },
        addEventListener(t, fn) { return "obj-listen:" + t + ":" + fn(); }
    };
    plain.textContent = "updated";
    const r1 = model.matches("a");
    const r2 = model.closest("b");
    const r3 = model.cloneNode(true);
    const r4 = model.toggleAttribute("flag", true);
    const r5 = model.addEventListener("ping", () => "ok");
    const r6 = plain.matches("c");
    const r7 = plain.closest("d");
    const r8 = plain.cloneNode(false);
    const r9 = plain.toggleAttribute("flag");
    const r10 = plain.addEventListener("pong", () => "done");
    const d1 = dynObj.matches("x");
    const d2 = dynObj.toggleAttribute("y", false);
    const d3 = dynObj.addEventListener("z", () => "dyn");
    return [plain.textContent, r1, r2, r3, r4, r5, r6, r7, r8, r9, r10, d1, d2, d3];
}
"#,
    );

    let out = run_rewrite(&[script], &resolver);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let js = &out.page_js;
    // Known non-DOM bindings (`model` from `new CustomModel()`, `plain` from `{ ... }`)
    // are not wrapped in `Eldur.wrap(...)` or rewritten to `Eldur.listen` / `Eldur.toggleAttribute`.
    assert!(js.contains("plain.textContent = \"updated\";"), "{js}");
    assert!(js.contains("const r1 = model.matches(\"a\");"), "{js}");
    assert!(js.contains("const r2 = model.closest(\"b\");"), "{js}");
    assert!(js.contains("const r3 = model.cloneNode(true);"), "{js}");
    assert!(
        js.contains("const r4 = model.toggleAttribute(\"flag\", true);"),
        "{js}"
    );
    assert!(
        js.contains("const r5 = model.addEventListener(\"ping\", () => \"ok\");"),
        "{js}"
    );
    assert!(js.contains("const r6 = plain.matches(\"c\");"), "{js}");
    assert!(js.contains("const r7 = plain.closest(\"d\");"), "{js}");
    assert!(js.contains("const r8 = plain.cloneNode(false);"), "{js}");
    assert!(
        js.contains("const r9 = plain.toggleAttribute(\"flag\");"),
        "{js}"
    );
    assert!(
        js.contains("const r10 = plain.addEventListener(\"pong\", () => \"done\");"),
        "{js}"
    );
    // Unknown parameter `dynObj` goes through Eldur helpers (which delegate at runtime for non-DOM objects).
    assert!(
        js.contains("const d1 = Eldur.wrap(dynObj).matches(\"x\");"),
        "{js}"
    );
    assert!(
        js.contains("const d2 = Eldur.toggleAttribute(dynObj, \"y\", false);"),
        "{js}"
    );
    assert!(
        js.contains("const d3 = Eldur.listen(dynObj, \"z\", () => \"dyn\");"),
        "{js}"
    );
}

#[test]
fn compares_a_resolved_element_with_a_dom_read_as_wrappers() {
    let resolver = FakeResolver::default().with_id("host", 1, "hostBox", TargetKind::Box);
    let out = run_rewrite(
        &[classic(
            "a.js",
            "var h = document.getElementById('host'); function f(m) { return m.parentNode === h; }",
        )],
        &resolver,
    );
    assert!(
        out.page_js
            .contains("Eldur.wrap(m.parentNode) === Eldur.wrap(hostBox)"),
        "{}",
        out.page_js
    );
}

fn live_page(source: &str) -> String {
    let resolver = FakeResolver::default()
        .with_id("a", 1, "aBox", TargetKind::Box)
        .with_id("b", 2, "bBox", TargetKind::Box)
        .with_selector(".x", vec![FakeResolver::target(1, "aBox", TargetKind::Box)]);
    run_rewrite(&[classic("live.js", source)], &resolver).page_js
}

#[test]
fn live_lookups_stay_resolved_when_nothing_can_change_their_answer() {
    let js = live_page(
        r#"const a = document.getElementById("a"); a.style.color = "red"; console.log(a.id);"#,
    );
    assert!(!js.contains("getElementById"), "{js}");
    assert!(js.contains("aBox"), "{js}");
}

#[test]
fn live_lookups_go_through_the_runtime_for_each_write_that_can_change_an_id_lookup() {
    for write in [
        r#"a.remove();"#,
        r#"a.id = "b";"#,
        r#"a.setAttribute("id", "b");"#,
        r#"a.innerHTML = "<div id=b></div>";"#,
        r#"a.parentNode.removeChild(a);"#,
        r#"a.parentNode.appendChild(a);"#,
        r#"function w(n) { n.textContent = ""; }"#,
    ] {
        let js = live_page(&format!(
            r#"const a = document.getElementById("a"); {write}"#
        ));
        assert!(
            js.contains(r#"Eldur.document.getElementById("a")"#),
            "{write}: {js}"
        );
    }
}

#[test]
fn live_lookups_keep_an_id_lookup_resolved_when_only_new_idless_elements_arrive() {
    let js = live_page(
        r#"const a = document.getElementById("a");
           a.appendChild(document.createElement("div"));"#,
    );
    assert!(!js.contains("getElementById"), "{js}");
}

#[test]
fn live_lookups_send_selectors_to_the_runtime_when_the_tree_or_their_class_changes() {
    for write in [
        r#"document.body.appendChild(document.createElement("div"));"#,
        r#"document.getElementById("b").classList.add("x");"#,
        r#"document.getElementById("b").className = "y";"#,
    ] {
        let js = live_page(&format!(
            r#"console.log(document.querySelectorAll(".x").length); {write}"#
        ));
        assert!(
            js.contains(r#"Eldur.document.querySelectorAll(".x")"#),
            "{write}: {js}"
        );
    }
    let js = live_page(
        r#"console.log(document.querySelectorAll(".x").length);
           document.getElementById("b").classList.add("other");"#,
    );
    assert!(!js.contains("Eldur.document.querySelectorAll"), "{js}");
}

#[test]
fn live_lookups_hand_scripts_a_wrapper_wherever_a_resolved_element_escapes() {
    let js = live_page(
        r#"const a = document.getElementById("a");
           const o = { a };
           function f(x) { return x; }
           f(a);
           a.onclick = function () { f(this); };"#,
    );
    assert!(js.contains("a: Eldur.wrap(aBox)"), "{js}");
    assert!(js.contains("f(Eldur.wrap(aBox))"), "{js}");
    assert!(
        js.contains("(function () { f(Eldur.wrap(aBox)); })"),
        "{js}"
    );
}

#[test]
fn a_sort_goes_through_the_runtime_as_the_engines_is_not_stable() {
    let script = classic(
        "sort.js",
        "function by(list) { return list.map(f).sort(function (a, b) { return a - b; }).slice(1); }\nvar plain = names.sort();\nvar chosen = maybe?.sort();",
    );
    let js = run_rewrite(&[script], &FakeResolver::default()).page_js;
    assert!(
        js.contains("Eldur.sort(list.map(f), function (a, b) { return a - b; }).slice(1)"),
        "{js}"
    );
    assert!(js.contains("plain = Eldur.sort(names)"), "{js}");
    // Short-circuits on the object, which a plain call would not.
    assert!(js.contains("maybe?.sort()"), "{js}");
}

/// A QML id written into a script means whatever the script's own name of that
/// spelling means there, so an element sharing a name with a page variable or
/// parameter has to be looked up.
#[test]
fn a_page_name_spelled_like_an_id_does_not_stand_for_the_element() {
    let resolver = FakeResolver::default()
        .with_id("order", 1, "order", TargetKind::Text)
        .with_id("total", 2, "total", TargetKind::Text)
        .with_id("plain", 3, "plain", TargetKind::Text);
    let scripts = [classic(
        "page.js",
        "var order = [1, 2];\n\
         var plain = document.getElementById('plain');\n\
         function show(total) {\n\
           document.getElementById('order').title = order.join();\n\
           document.getElementById('total').title = total;\n\
           plain.title = 'x';\n\
         }\n",
    )];
    let out = run_rewrite_resolved(&scripts, &resolver);
    assert!(
        out.page_js.contains("getElementById(\"order\")")
            || out.page_js.contains("getElementById('order')"),
        "{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("getElementById(\"total\")")
            || out.page_js.contains("getElementById('total')"),
        "{}",
        out.page_js
    );
    // Bound to its own element, the name still is the element.
    assert!(
        !out.page_js.contains("getElementById(\"plain\")"),
        "{}",
        out.page_js
    );
    assert!(
        !out.page_js.contains("getElementById('plain')"),
        "{}",
        out.page_js
    );
}

/// A bundler's lazy chunk (`() => import("./chunk.js")`) inside an inlined
/// module and in an inline handler: every `import(` leaves page.js, because
/// QML refuses the whole file at that token, and each call is diagnosed once.
#[test]
fn lowers_dynamic_import_in_modules_and_handlers_and_diagnoses_each_once() {
    let resolver = FakeResolver::default().with_element(1, "btn", TargetKind::Button);
    let lazy = module_script(
        "_astro/page.js",
        r#"
const loaders = { counter: () => import("./counter.js"), named: (n) => import(`./${n}.js`) };
export function load(name) { return loaders[name](); }
"#,
    );
    let main = module_script(
        "_astro/main.js",
        r#"
import { load } from "./page.js";
load("counter").catch(() => {});
"#,
    );
    let click = handler(1, "click", r#"import("./on-click.js")"#);
    let out = run_rewrite(&[lazy, main, click], &resolver);
    assert!(!out.page_js.contains("import("), "{}", out.page_js);
    assert!(
        out.page_js
            .contains(r#"counter: () => Eldur.dynamicImport("./counter.js")"#),
        "{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("Eldur.dynamicImport(`./${n}.js`)"),
        "{}",
        out.page_js
    );
    let messages: Vec<&str> = out
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("dynamic import"))
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        vec![
            "dynamic import('./counter.js') cannot load a module in QML: the promise rejects as for a missing module",
            "dynamic import() of a computed specifier cannot load a module in QML: the promise rejects as for a missing module",
            "dynamic import('./on-click.js') cannot load a module in QML: the promise rejects as for a missing module",
        ],
        "{:?}",
        out.diagnostics
    );
    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.severity == Severity::Unsupported),
        "{:?}",
        out.diagnostics
    );
}

/// A bundler imports a shared chunk statically and lazily. The lazy import of a
/// module this page holds resolves to that module's namespace: no rejection,
/// no "missing module" diagnostic, one namespace for every importer.
#[test]
fn dynamic_import_of_an_inlined_module_resolves_to_its_namespace() {
    let resolver = FakeResolver::default().with_element(1, "btn", TargetKind::Button);
    let chunk = module_script(
        "_astro/chunk.js",
        "export let count = 1;\nexport function bump() { count += 1; return count; }\nexport default \"chunk\";\n",
    );
    let main = module_script(
        "_astro/main.js",
        r#"
import { bump } from "./chunk.js";
bump();
export const lazy = () => import("./chunk.js");
"#,
    );
    // Imported only lazily: it stays at its own place on the page, as
    // `import()` never runs a module ahead of its importer (the order itself
    // is `a_lazily_imported_module_runs_at_its_own_place_on_the_page`).
    let late = module_script("_astro/late.js", "export const late = 1;\n");
    let other = module_script(
        "_astro/other.js",
        r#"
import("./chunk.js").then((m) => m.bump());
import("./late.js");
import("./missing.js");
"#,
    );
    let click = handler(1, "click", r#"import("/_astro/chunk.js")"#);
    let out = run_rewrite(&[chunk, main, other, late, click], &resolver);
    assert!(!out.page_js.contains("import("), "{}", out.page_js);
    assert!(
        !out.page_js.contains(r#"Eldur.dynamicImport("./chunk.js")"#)
            && !out.page_js.contains(r#"Eldur.dynamicImport("./late.js")"#)
            && !out
                .page_js
                .contains(r#"Eldur.dynamicImport("/_astro/chunk.js")"#),
        "{}",
        out.page_js
    );
    assert!(
        out.page_js
            .contains(r#"Eldur.dynamicImport("./missing.js")"#),
        "{}",
        out.page_js
    );
    // Through the module's run function, which records whether it has run;
    // null for one with nothing to run. The runtime makes
    // the page's own promise, which `instanceof Promise` on the page names.
    assert!(
        out.page_js
            .contains("Eldur.importEager(null, __eldur_module_namespace)"),
        "{}",
        out.page_js
    );
    // One namespace object per module, made on the first call, whose getters
    // read the live bindings, keys sorted as a namespace's are.
    let made = out.page_js.matches("get bump()").count();
    assert_eq!(made, 1, "{}", out.page_js);
    let bump = out.page_js.find("get bump()").expect("bump getter");
    let count = out.page_js.find("get count()").expect("count getter");
    let default = out.page_js.find("get default()").expect("default getter");
    assert!(bump < count && count < default, "{}", out.page_js);
    assert_eq!(
        out.page_js.matches("late = 1").count(),
        1,
        "{}",
        out.page_js
    );
    assert!(
        out.page_js
            .contains("Eldur.importEager(__eldur_run_m3, __eldur_module_namespace_2)")
            && out.page_js.contains("get late() { return late; }"),
        "{}",
        out.page_js
    );
    let messages: Vec<&str> = out
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("dynamic import"))
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        vec![
            "dynamic import('./missing.js') cannot load a module in QML: the promise rejects as for a missing module",
        ],
        "{:?}",
        out.diagnostics
    );
}

/// A module that imports, statically, the module that imports it lazily. The
/// lazy import names a module emitted later on the page, which is fine: its
/// namespace is read when the promise is made, after the page has run.
#[test]
fn dynamic_import_of_a_module_that_imports_this_one_resolves_to_its_namespace() {
    let resolver = FakeResolver::default();
    let a = module_script(
        "a.js",
        "import { b } from \"./b.js\";\nexport const a = b;\n",
    );
    let b = module_script(
        "b.js",
        "export const b = 1;\nexport const back = () => import(\"./a.js\");\n",
    );
    let out = run_rewrite(&[a, b], &resolver);
    assert!(!out.page_js.contains("dynamicImport"), "{}", out.page_js);
    assert!(
        out.page_js
            .contains("Eldur.importEager(__eldur_run_m0, __eldur_module_namespace)"),
        "{}",
        out.page_js
    );
    assert!(out.page_js.contains("get a()"), "{}", out.page_js);
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("dynamic import")),
        "{:?}",
        out.diagnostics
    );
}

/// Classic `app.js` holds an `import()` of a page module, then classic
/// `later.js`, then that module. Chromium logs classic, later, chunk: the
/// lazy import leaves the module's top-level code where the page has it.
#[test]
fn a_lazily_imported_module_runs_at_its_own_place_on_the_page() {
    let resolver = FakeResolver::default();
    let app = classic(
        "app.js",
        "var cfg = 7;\nconsole.log('classic');\nfunction go() { return import(\"./chunk.js\"); }\n",
    );
    let later = classic("later.js", "console.log('later');\n");
    let chunk = module_script(
        "chunk.js",
        "console.log('chunk', cfg);\nexport const x = 1;\n",
    );
    let out = run_rewrite(&[app, later, chunk], &resolver);
    let classic_at = out.page_js.find("console.log('classic')").expect("app.js");
    let later_at = out.page_js.find("console.log('later')").expect("later.js");
    // Its code is in its run function, called at its place.
    assert!(
        out.page_js.contains("console.log('chunk'"),
        "{}",
        out.page_js
    );
    let chunk_at = out
        .page_js
        .find("try {\n        __eldur_run_m2();")
        .expect("chunk.js");
    assert!(
        classic_at < later_at && later_at < chunk_at,
        "{}",
        out.page_js
    );
    assert!(
        out.page_js
            .contains("Eldur.importEager(__eldur_run_m2, __eldur_module_namespace)"),
        "{}",
        out.page_js
    );
}

fn imported_module(name: &str, source: &str) -> ScriptInput {
    ScriptInput {
        source: source.to_string(),
        name: name.to_string(),
        kind: ScriptKind::ImportedModule,
        line_offset: 1,
        imports: Vec::new(),
    }
}

/// The modules a page's module script reaches (hal-c2/eldur#22): one it imports
/// statically runs before it, at load, found by the path its specifier makes
/// beside the importer; one only `import()` names runs on that import, in a
/// function of its own, and never at a place on the page.
#[test]
fn an_imported_module_runs_before_its_importer_and_a_lazy_one_on_its_first_import() {
    let resolver = FakeResolver::default();
    let main = module_script(
        "/site/js/main.js",
        "import { label } from \"./lib/../helper.js\";\nconsole.log('main', label(2));\nimport(`./chunk.js`).then((m) => console.log(m.word));\n",
    );
    let helper = imported_module(
        "/site/js/helper.js",
        "console.log('helper');\nexport function label(n) { return 'n=' + n; }\n",
    );
    let decoy = imported_module(
        "/site/js/lib/helper.js",
        "export function label() { return 'no'; }\n",
    );
    let chunk = imported_module(
        "/site/js/chunk.js",
        "console.log('chunk');\nwindow.addEventListener('load', () => console.log('never'));\nexport const word = 'chunk';\n",
    );
    let out = run_rewrite(&[main, helper, decoy, chunk], &resolver);
    let init = out.page_js.find("function __eldur_init").expect("init");
    // Each runs in its own run function: main's runs helper's first, and the
    // page runs main's at its place.
    let helper_fn = out
        .page_js
        .find("function __eldur_run_m1()")
        .expect("helper");
    let helper_at = out.page_js.find("console.log('helper')").expect("helper");
    let main_fn = out.page_js.find("function __eldur_run_m0()").expect("main");
    let helper_call = main_fn
        + out.page_js[main_fn..]
            .find("__eldur_run_m1();")
            .expect("call");
    let main_at = out.page_js.find("console.log('main'").expect("main");
    assert!(
        helper_fn < helper_at && helper_at < main_fn && helper_call < main_at,
        "{}",
        out.page_js
    );
    assert!(
        out.page_js[init..].contains("__eldur_run_m0();"),
        "{}",
        out.page_js
    );
    assert!(!out.page_js.contains("'no'"), "{}", out.page_js);
    let run = out
        .page_js
        .find("function __eldur_run_m3()")
        .expect("chunk's run function");
    let chunk_at = out.page_js.find("console.log('chunk')").expect("chunk");
    assert!(run < chunk_at && chunk_at < init, "{}", out.page_js);
    // Its load listener goes through the runtime, which has fired load by then.
    assert!(!out.page_js[init..].contains("never"), "{}", out.page_js);
    assert!(
        out.page_js
            .contains("Eldur.importModule(__eldur_module_namespace)")
            && out
                .page_js
                .contains("__eldur_run_m3(); return __eldur_module_namespace.value ||"),
        "{}",
        out.page_js
    );
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("import") || d.message.contains("module")),
        "{:?}",
        out.diagnostics
    );
}

/// A lazy module's own static imports that are lazy too run, in order, when it
/// does; the module an inline handler's `import()` names is found from the page.
#[test]
fn a_lazy_modules_lazy_dependencies_run_first_and_a_handler_imports_from_the_page() {
    let resolver = FakeResolver::default().with_element(1, "go", TargetKind::Button);
    let main = module_script(
        "/order/main.js",
        "export const later = () => import('./lazy.js');\n",
    );
    let lazy = imported_module(
        "/order/lazy.js",
        "import { only } from './lazydep.js';\nconsole.log('lazy', only);\nexport const word = 'chunk';\n",
    );
    let dep = imported_module(
        "/order/lazydep.js",
        "console.log('lazydep');\nexport const only = 'dep';\n",
    );
    let click = imported_module("/order/click.js", "export const word = 'clicked';\n");
    let go = handler(
        1,
        "click",
        "import('./click.js').then((m) => console.log(m.word))",
    );
    let out = run_rewrite(&[main, lazy, dep, click, go], &resolver);
    let dep_run = out
        .page_js
        .find("function __eldur_run_m2()")
        .expect("lazydep runs");
    let lazy_run = out
        .page_js
        .find("function __eldur_run_m1()")
        .expect("lazy runs");
    let calls_dep = out.page_js[lazy_run..]
        .find("__eldur_run_m2();")
        .expect("lazy runs lazydep")
        + lazy_run;
    let lazy_log = out.page_js.find("console.log('lazy'").expect("lazy");
    assert!(
        dep_run < lazy_run && calls_dep < lazy_log,
        "{}",
        out.page_js
    );
    assert!(
        out.page_js.contains("function __eldur_run_m3()"),
        "{}",
        out.page_js
    );
    assert!(!out.page_js.contains("dynamicImport"), "{}", out.page_js);
}

/// A cycle: `a` imports `b`, which imports `a` back. Chromium runs `b` first,
/// and `b` already sees `a`'s hoisted function; so it does here, through the
/// name `a`'s export ends up with.
#[test]
fn a_module_in_a_cycle_reads_the_bindings_of_the_one_that_imports_it() {
    let resolver = FakeResolver::default();
    let main = module_script(
        "/m/main.js",
        "import { ping } from './a.js';\nconsole.log(ping());\n",
    );
    let a = imported_module(
        "/m/a.js",
        "import { pong } from './b.js';\nexport function ping() { return 'ping' + pong(); }\n",
    );
    let b = imported_module(
        "/m/b.js",
        "import { ping } from './a.js';\nexport { ping as again } from './a.js';\nconsole.log('b', typeof ping);\nexport function pong() { return 'pong'; }\n",
    );
    let out = run_rewrite(&[main, a, b], &resolver);
    assert!(!out.page_js.contains("__eldur_cycle"), "{}", out.page_js);
    assert!(
        out.page_js.contains("console.log('b', typeof ping)"),
        "{}",
        out.page_js
    );
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("circular") || d.message.contains("cycle")),
        "{:?}",
        out.diagnostics
    );
}

/// What a cycle cannot give here is said: `export *` from a module further up
/// it re-exports nothing.
#[test]
fn export_star_from_a_module_that_imports_this_one_is_reported() {
    let resolver = FakeResolver::default();
    let a = module_script(
        "/m/a.js",
        "import { b } from './b.js';\nexport const a = 1;\nconsole.log(b);\n",
    );
    let b = imported_module("/m/b.js", "export * from './a.js';\nexport const b = 2;\n");
    let out = run_rewrite(&[a, b], &resolver);
    assert!(
        out.diagnostics.iter().any(|d| d
            .message
            .starts_with("export * from './a.js', a module importing this one")),
        "{:?}",
        out.diagnostics
    );
}

/// In a browser a property set on the window is a global binding: after
/// `window.X = v`, a bare `X` in any script or handler is that property, read
/// or written. A name a classic script declares is its own binding, a
/// parameter is the function's, and a name QML's global object has (`Map`,
/// polyfilled or not) stays what it is.
#[test]
fn a_bare_name_a_script_sets_on_the_window_is_that_window_property() {
    let resolver = FakeResolver::default().with_element(1, "btn", TargetKind::Button);
    let head = classic(
        "head.js",
        r#"
window.Provider = { n: 1 };
globalThis["Other"] = "o";
function later() { return typeof Provider + Late; }
self.Late = "late";
var Local = 1;
window.Local = 2;
window.Map = window.Map || function () {};
"#,
    );
    let body = classic(
        "body.js",
        r#"
Provider.n += 1;
Provider = { n: 10 };
console.log(Other, Local, Missing, { Provider }, new Map());
function shadow(Provider) { return Provider; }
"#,
    );
    let click = handler(1, "click", "Provider.n++");
    let out = run_rewrite(&[head, body, click], &resolver);
    let js = &out.page_js;
    for want in [
        "typeof Eldur.window.Provider + Eldur.window.Late",
        "Eldur.window.Provider.n += 1",
        "Eldur.window.Provider = { n: 10 }",
        "console.log(Eldur.window.Other, Local, Missing, { Provider: Eldur.window.Provider }, new Map())",
        "function shadow(Provider) { return Provider; }",
        "Eldur.window.Provider.n++",
    ] {
        assert!(js.contains(want), "{want} missing:\n{js}");
    }
}

/// A polyfill sets a browser name the QML global object lacks, then calls it
/// bare: the bare name is that window property, or it throws ReferenceError.
/// A name QML's global object has (`JSON`) keeps its own binding.
#[test]
fn a_polyfilled_browser_global_called_bare_is_the_window_property() {
    let resolver = FakeResolver::default();
    let head = classic(
        "polyfill.js",
        r#"
window.requestIdleCallback = window.requestIdleCallback || function (cb) { return setTimeout(cb, 1); };
window.Intl = window.Intl || {};
window.JSON = window.JSON || {};
"#,
    );
    let body = classic(
        "app.js",
        "requestIdleCallback(function () {});\nconsole.log(typeof Intl, JSON.stringify([1]));\n",
    );
    let out = run_rewrite(&[head, body], &resolver);
    let js = &out.page_js;
    for want in [
        "Eldur.window.requestIdleCallback(function () {})",
        "console.log(typeof Eldur.window.Intl, JSON.stringify([1]))",
    ] {
        assert!(js.contains(want), "{want} missing:\n{js}");
    }
}

/// HTML's named access comes after the window's own properties: a name a
/// script sets on the window is that property, not the element with that id.
#[test]
fn a_name_set_on_the_window_is_not_the_element_with_that_id() {
    let mut scripts = [
        classic("a.js", "window.panel = { open: true };\n"),
        classic("b.js", "console.log(panel.open, card.id);\n"),
    ];
    let ids = ["panel", "card"].map(str::to_owned).into_iter().collect();
    let analysis = analyze(&scripts);
    assert!(resolve_named_elements(&mut scripts, &ids, &analysis));
    assert_eq!(
        scripts[1].source,
        "console.log(panel.open, document.getElementById(\"card\").id);\n"
    );
}

/// A lazily imported module's `let`, `const` and `class` bindings hold a
/// sentinel until their declarations run: a read of one checks it and throws
/// a `ReferenceError`, a write to a `const` throws a `TypeError`, and a
/// destructuring target is a setter that checks as an assignment does. Code at the
/// module's top level after a declaration reads it unchecked, as it always
/// finds it initialized.
#[test]
fn a_lazy_modules_lexical_bindings_are_checked_until_initialized() {
    let resolver = FakeResolver::default();
    let main = module_script("main.js", "import(\"./lazy.js\");\n");
    let lazy = imported_module(
        "lazy.js",
        "export const K = 1;\nexport let n = 2;\nexport class Box {}\n\
         export function read() { return K + n; }\n\
         export function write() { K = 3; n = 4; }\n\
         export function destructure(v) { [n] = [v]; ({ K } = { K: v }); }\n\
         export const after = K;\n",
    );
    let out = run_rewrite(&[main, lazy], &resolver);
    let js = &out.page_js;
    for decl in [
        "var K = __eldur_tdz;",
        "var n = __eldur_tdz;",
        "var Box = __eldur_tdz;",
        "var after = __eldur_tdz;",
        "function __eldur_tdz() {}",
    ] {
        assert!(js.contains(decl), "{decl}\n{js}");
    }
    for checked in [
        r#"return (K === __eldur_tdz ? __eldur_tdz_error("K") : K) + (n === __eldur_tdz ? __eldur_tdz_error("n") : n);"#,
        r#"(3, __eldur_const_write(K === __eldur_tdz, "K"));"#,
        r#"n = __eldur_tdz_set(4, n, "n");"#,
        "after = K;",
        r#"[({ set v(__eldur_value) { n = __eldur_tdz_set(__eldur_value, n, "n"); } }).v] = [v];"#,
        r#"({ K: ({ set v(__eldur_value) { (__eldur_value, __eldur_const_write(K === __eldur_tdz, "K")); } }).v } = { K: v });"#,
    ] {
        assert!(js.contains(checked), "{checked}\n{js}");
    }
}

/// A module run as the page loads keeps its `let` and `const` bindings as
/// they are: no sentinel, no check.
#[test]
fn an_eager_modules_lexical_bindings_are_not_checked() {
    let resolver = FakeResolver::default();
    let main = module_script(
        "main.js",
        "import { n } from \"./dep.js\";\nexport const K = n + 1;\nexport function read() { return K + n; }\n",
    );
    let dep = module_script("dep.js", "export let n = 1;\nexport class Box {}\n");
    let out = run_rewrite(&[main, dep], &resolver);
    assert!(!out.page_js.contains("__eldur_tdz"), "{}", out.page_js);
    assert!(out.page_js.contains("return K + n;"), "{}", out.page_js);
}

/// In a lazily imported module a write that must throw first does what a
/// browser does before it throws: a compound assignment to a `const` runs
/// its operation, an update coerces as `++` does (a BigInt too). Every
/// import is read-only, a live one to a `var` included. A destructuring
/// setter's parameter never hides the binding it writes, a name outside
/// ASCII gets its sentinel too, and an import's copy exists before the
/// module's first statement runs.
#[test]
fn a_lazy_modules_writes_and_imports_behave_as_a_browsers() {
    let resolver = FakeResolver::default();
    let main = module_script("main.js", "import(\"./lazy.js\");\n");
    let dep = imported_module(
        "dep.js",
        "export var v = 1;\nexport const x = 1;\nexport function bump() { v++; }\n",
    );
    let lazy = imported_module(
        "lazy.js",
        "console.log(y, café);\n\
         import { v, x as y, bump } from \"./dep.js\";\n\
         export const B = 1n;\n\
         export let __eldur_value = 0;\n\
         export let café = 1;\n\
         export function compound() { B -= 1; }\n\
         export function update() { B++; }\n\
         export function destructure() { [__eldur_value] = [7]; }\n\
         export function writeImport() { v = 2; bump(); }\n",
    );
    let out = run_rewrite(&[main, lazy, dep], &resolver);
    let js = &out.page_js;
    for expected in [
        r#"(B === __eldur_tdz ? __eldur_tdz_error("B") : B) - (1), __eldur_const_write(false, "B"));"#,
        r#"(((__eldur_value) => __eldur_value++)((B === __eldur_tdz ? __eldur_tdz_error("B") : B)), __eldur_const_write(false, "B"));"#,
        r#"({ set v(__eldur_value_2) { __eldur_value = __eldur_tdz_set(__eldur_value_2, __eldur_value, "__eldur_value"); } }).v] = [7];"#,
        r#"(2, __eldur_const_write(false, "v"));"#,
        "café = __eldur_tdz",
    ] {
        assert!(js.contains(expected), "{expected}\n{js}");
    }
    let copy = js
        .find("y = (x === __eldur_tdz")
        .or_else(|| js.find("y = x;"))
        .unwrap_or_else(|| panic!("the copy of x\n{js}"));
    let first = js
        .find("console.log(y, ")
        .unwrap_or_else(|| panic!("console.log\n{js}"));
    assert!(copy < first, "{js}");
}

/// A lazy module's `const` bound to an element is looked up when the module
/// runs, as in a browser: it is a variable, checked until its declaration
/// runs, not the element wired in while converting.
#[test]
fn a_lazy_modules_element_const_is_looked_up_when_it_runs() {
    let resolver = FakeResolver::default().with_id("save", 1, "save", TargetKind::Button);
    let main = module_script("main.js", "import(\"./lazy.js\");\n");
    let lazy = imported_module(
        "lazy.js",
        "function early() { return btn.disabled; }\n\
         early();\n\
         const btn = document.getElementById(\"save\");\n\
         function late() { const copy = btn; return copy.disabled; }\n",
    );
    let out = run_rewrite(&[main, lazy], &resolver);
    let js = &out.page_js;
    let checked = r#"(__eldur_m1_btn === __eldur_tdz ? __eldur_tdz_error("btn") : __eldur_m1_btn)"#;
    for expected in [
        "var __eldur_m1_btn = __eldur_tdz;".to_string(),
        format!("{checked}.disabled"),
        format!("const copy = {checked};"),
    ] {
        assert!(js.contains(&expected), "{expected}\n{js}");
    }
    // The lookup itself runs inside the module, after the call that reads
    // too early.
    let run = js.find("function __eldur_run_m1()").expect("run function");
    let early = js[run..].find("__eldur_m1_early();").expect("early call") + run;
    let lookup = js[run..]
        .find("__eldur_m1_btn = Eldur.wrap(save);")
        .unwrap_or_else(|| panic!("lookup inside the run function\n{js}"))
        + run;
    assert!(early < lookup, "{js}");
}

/// A module that runs at load records whether it threw. The scripts after it
/// go on; a module importing it throws the same error; a module script whose
/// graph threw reports it; `import()` of it reads the record.
#[test]
fn an_eager_modules_error_stops_its_importers_only() {
    let resolver = FakeResolver::default();
    let first = module_script("first.js", "import(\"./boom.js\");\n");
    let boom = module_script(
        "boom.js",
        "export const x = 1;\nthrow new Error(\"boom\");\n",
    );
    let after = module_script(
        "after.js",
        "import { x } from \"./boom.js\";\nconsole.log(x);\n",
    );
    let out = run_rewrite(&[first, boom, after], &resolver);
    let js = &out.page_js;
    let run = "__eldur_run_m1";
    for expected in [
        format!("Eldur.importEager({run}, "),
        format!("function {run}() {{\n    if ({run}.state === 2) throw {run}.error;\n    if ({run}.state) return;\n    {run}.state = 3;"),
        format!("{run}.state = 2;\n        {run}.error = err;"),
        format!("try {{\n        {run}();\n    }} catch (err) {{\n        Eldur.reportError(err);\n    }}"),
        "try {\n        __eldur_run_m2();\n    } catch (err) {\n        Eldur.reportError(err);\n    }".to_string(),
    ] {
        assert!(js.contains(&expected), "{expected}\n{js}");
    }
    // Its importer runs it first, and never runs again once it has thrown.
    let importer = js.find("function __eldur_run_m2()").expect("after");
    let call = importer + js[importer..].find(&format!("{run}();")).expect("call");
    let logged = js.find("console.log(x)").expect("logged");
    assert!(call < logged, "{js}");
}

#[test]
fn a_throwing_module_fails_every_module_of_its_cycle() {
    let resolver = FakeResolver::default();
    let ca = module_script(
        "ca.js",
        "import { b } from \"./cb.js\";\nconsole.log(b);\nthrow new Error(\"ca\");\n",
    );
    let cb = module_script(
        "cb.js",
        "import \"./ca.js\";\nexport const b = \"b\";\nconsole.log(\"cb\");\n",
    );
    let out = run_rewrite(&[ca, cb], &resolver);
    let js = &out.page_js;
    let ca_run = js.find("function __eldur_run_m0()").expect("ca");
    let marked = ca_run
        + js[ca_run..]
            .find("__eldur_run_m1.state = 2; __eldur_run_m1.error = err;")
            .expect("cb failed with ca");
    let rethrown = ca_run + js[ca_run..].find("throw err;").expect("rethrown");
    assert!(marked < rethrown, "{js}");
    assert!(js.contains("function __eldur_run_m1()"), "{js}");
}

/// The body of the function `name` declares in `js`.
fn function_body<'s>(js: &'s str, name: &str) -> &'s str {
    let start = js
        .find(&format!("function {name}("))
        .unwrap_or_else(|| panic!("no {name}:\n{js}"));
    let end = start + js[start..].find("\n}").expect("closed");
    &js[start..end]
}

/// A module of a cycle that ran to its end is done only once its cycle is: an
/// `import()` of it before then waits, and rejects if the cycle fails. Which
/// module ends the cycle is known only at runtime: the cycle may be entered at
/// either module, as an importer that stopped at an earlier throw leaves it to
/// the other module's script.
#[test]
fn a_cycle_member_is_done_only_with_its_cycle() {
    let resolver = FakeResolver::default();
    let wa = module_script(
        "wa.js",
        "import { b } from \"./wb.js\";\nimport(\"./wb.js\");\nthrow new Error(\"wa\");\n",
    );
    let wb = module_script(
        "wb.js",
        "import \"./wa.js\";\nexport const b = \"b\";\nconsole.log(\"wb\");\n",
    );
    let out = run_rewrite(&[wa, wb], &resolver);
    let js = &out.page_js;
    for (me, other) in [
        ("__eldur_run_m0", "__eldur_run_m1"),
        ("__eldur_run_m1", "__eldur_run_m0"),
    ] {
        let run = function_body(js, me);
        assert!(run.contains(&format!("{me}.state = 3;")), "{js}");
        let done = format!(
            "if ({other}.state === 3) {{ {me}.state = 4; }} else {{ {me}.state = 1; if ({other}.state === 4) {other}.state = 1; }}"
        );
        assert!(run.contains(&done), "{me} waits for {other}:\n{js}");
        assert!(!run.contains(&format!("\n    {me}.state = 1;")), "{js}");
    }
}

/// The namespace an `import()` resolves to is handed out only after the
/// module's run, so a module that failed rejects each time.
#[test]
fn a_namespace_runs_its_module_first() {
    let resolver = FakeResolver::default();
    let first = module_script(
        "first.js",
        "import(\"./late.js\").then(() => {}, () => {});\n",
    );
    let late = module_script("late.js", "export const x = 1;\nconsole.log(x);\n");
    let out = run_rewrite(&[first, late], &resolver);
    let js = &out.page_js;
    let ns = js
        .find("function __eldur_module_namespace")
        .unwrap_or_else(|| panic!("no namespace:\n{js}"));
    let line = js[ns..].lines().next().expect("namespace line");
    assert!(line.contains("() { __eldur_run_m1(); return "), "{line}");
}

#[test]
fn a_module_reaching_a_running_cycle_joins_it() {
    // xa → xb → xa is a cycle; xc imports xb after xb ran, while xa still runs.
    let resolver = FakeResolver::default();
    let xa = module_script(
        "xa.js",
        "import { b } from \"./xb.js\";\nimport { c } from \"./xc.js\";\nconsole.log(b + c);\nthrow new Error(\"xa\");\n",
    );
    let xb = module_script(
        "xb.js",
        "import \"./xa.js\";\nexport const b = \"b\";\nconsole.log(\"xb\");\n",
    );
    let xc = module_script(
        "xc.js",
        "import { b } from \"./xb.js\";\nexport const c = \"c\";\nconsole.log(b);\n",
    );
    let out = run_rewrite(&[xa, xb, xc], &resolver);
    let js = &out.page_js;
    let xa_run = js.find("function __eldur_run_m0()").expect("xa");
    let rethrown = xa_run + js[xa_run..].find("throw err;").expect("rethrown");
    for other in ["__eldur_run_m1", "__eldur_run_m2"] {
        let marked = xa_run
            + js[xa_run..]
                .find(&format!("{other}.state = 2; {other}.error = err;"))
                .unwrap_or_else(|| panic!("{other} failed with xa:\n{js}"));
        assert!(marked < rethrown, "{js}");
    }
}

#[test]
fn a_module_reaching_a_finished_cycle_keeps_its_own_state() {
    // ya ⇄ yb finish before yc, a later script, imports yb.
    let resolver = FakeResolver::default();
    let ya = module_script("ya.js", "import \"./yb.js\";\nconsole.log(\"ya\");\n");
    let yb = module_script("yb.js", "import \"./ya.js\";\nconsole.log(\"yb\");\n");
    let yc = module_script(
        "yc.js",
        "import \"./yb.js\";\nconsole.log(\"yc\");\nthrow new Error(\"yc\");\n",
    );
    let out = run_rewrite(&[ya, yb, yc], &resolver);
    let js = &out.page_js;
    let yc_run = js.find("function __eldur_run_m2()").expect("yc");
    let yc_fn = &js[yc_run..yc_run + js[yc_run..].find("\n}\n").expect("end")];
    assert!(!yc_fn.contains("__eldur_run_m0.state = 2"), "{yc_fn}");
    assert!(!yc_fn.contains("__eldur_run_m1.state = 2"), "{yc_fn}");
}

#[test]
fn an_initializer_that_may_throw_runs_inside_its_module() {
    let resolver = FakeResolver::default();
    // Each source, and what of its last initializer must be worked out inside
    // the module's run function.
    let cases = [
        ("export const q = 1n / 0n;\n", "1n / 0n"),
        ("const big = 1n;\nexport const q = big + 1;\n", "big + 1"),
        ("export const q = +1n;\n", "+1n"),
        (
            "const shown = { toString() { throw new Error(\"shown\"); } };\nexport const q = `${shown}`;\n",
            "`${",
        ),
        (
            "const shown = { valueOf() { throw new Error(\"shown\"); } };\nexport const q = shown * 2;\n",
            "shown * 2",
        ),
        ("const n = 5;\nexport const q = [...n];\n", "[..."),
        (
            "const nothing = null;\nexport const { q } = nothing;\n",
            "q } = ",
        ),
        (
            "const f = function () {};\nexport const q = f < 1;\n",
            "f < 1",
        ),
        // Destructuring a class or a function reads its own properties, and a
        // static getter, or `caller` of a strict function, may throw.
        (
            "class C { static get a() { throw new Error(\"g\"); } }\nexport const { a: q } = C;\n",
            "{ a: ",
        ),
        (
            "function f() {}\nexport const { caller: q } = f;\n",
            "{ caller: ",
        ),
        (
            "const g = () => 1;\nexport const { caller: q } = g;\n",
            "{ caller: ",
        ),
        // `__proto__:` makes the class the literal's prototype.
        (
            "class C { static get a() { throw new Error(\"g\"); } }\nconst o = { __proto__: C };\nexport const { a: q } = o;\n",
            "{ a: ",
        ),
        // A statement that ran first may have changed what a binding holds.
        (
            "const source = {};\nObject.defineProperty(source, \"x\", { get() { throw new Error(\"g\"); } });\nexport const { x: q } = source;\n",
            "{ x: ",
        ),
        (
            "const list = [1];\nlist.push(2);\nexport const q = [...list];\n",
            "[...",
        ),
        // A name that is not the literal's own is read from its prototype,
        // whose accessors the page may define; an object bound earlier may be
        // changed by then.
        ("export const { b: q } = { a: 1 };\n", "{ b: "),
        (
            "const o = { a: 1 };\nexport const { a: q } = o;\n",
            "{ a: q }",
        ),
        // An array pattern runs `Array.prototype[Symbol.iterator]`.
        ("export const [q] = [1];\n", "] = [1]"),
    ];
    for (src, init) in cases {
        let m = module_script("m.js", &format!("{src}console.log(\"m\");\n"));
        let out = run_rewrite(&[m], &resolver);
        let js = &out.page_js;
        let run = js
            .find("function __eldur_run_m0()")
            .unwrap_or_else(|| panic!("no run function for {src}:\n{js}"));
        let at = js
            .find(init)
            .unwrap_or_else(|| panic!("{init} missing:\n{js}"));
        assert!(at > run, "{src} worked out outside its run:\n{js}");
    }
}

#[test]
fn an_initializer_that_cannot_throw_stays_hoisted() {
    let resolver = FakeResolver::default();
    let m = module_script(
        "m.js",
        "const a = 2;\nconst b = a * 3 + 1;\nconst s = `n${b}`;\nconst big = 2n * 3n;\nconst list = [...[a, b]];\nconst o = { a, ...{ b } };\nexport const all = [s, big, list, o, typeof o, !o, o === a];\n",
    );
    let out = run_rewrite(&[m], &resolver);
    let js = &out.page_js;
    assert!(!js.contains("function __eldur_run_m0()"), "{js}");
    assert!(
        js.contains("const __eldur_m0_b = __eldur_m0_a * 3 + 1;"),
        "{js}"
    );
    assert!(js.contains("const __eldur_m0_big = 2n * 3n;"), "{js}");

    // Each name the pattern reads is one of the literal's own data properties.
    let m = module_script(
        "m.js",
        "const b = 2;\nexport const { a: one, b: two = 3 } = { a: 1, b };\n",
    );
    let js = run_rewrite(&[m], &resolver).page_js;
    assert!(!js.contains("function __eldur_run_m0()"), "{js}");
    assert!(js.contains("{ a: 1, b: __eldur_m0_b }"), "{js}");
}

/// A listener a module wires to an element at conversion is heard only once
/// its module reaches the call that adds it, so not after the module threw.
#[test]
fn an_element_listener_waits_for_its_module_to_reach_it() {
    let resolver = FakeResolver::default().with_id("go", 1, "goBtn", TargetKind::Button);
    for adds in [
        "btn.addEventListener(\"click\", () => console.log(\"heard\"));",
        "btn.addEventListener(\"click\", () => console.log(\"heard\"), { once: true });",
        "btn.onclick = () => console.log(\"onclick\");",
    ] {
        let m = module_script(
            "m.js",
            &format!(
                "const btn = document.getElementById(\"go\");\nfail();\n{adds}\nfunction fail() {{ throw new Error(\"bad\"); }}\n"
            ),
        );
        let out = run_rewrite(&[m], &resolver);
        let js = &out.page_js;
        assert_eq!(out.bindings.len(), 1, "{adds}:\n{js}");
        let body = function_body(js, &out.bindings[0].function);
        let flag = body
            .split_once("{\n")
            .map(|(_, inner)| inner.trim_start())
            .and_then(|inner| inner.strip_prefix("if (!"))
            .and_then(|rest| rest.split_once(')'))
            .map(|(flag, _)| flag)
            .unwrap_or_else(|| panic!("{adds}: no guard first:\n{js}"));
        let run = function_body(js, "__eldur_run_m0");
        let set = run
            .find(&format!("{flag} = true;"))
            .unwrap_or_else(|| panic!("{flag} never set:\n{js}"));
        assert!(
            set > run.find("fail()").expect("fail call"),
            "{flag} set before the throw:\n{js}"
        );
    }
}

/// `window.onload` is a slot each assignment sets where it runs, `null`
/// included, and the slot's value is called at load.
#[test]
fn window_onload_is_set_where_it_runs() {
    let resolver = FakeResolver::default();
    let first = classic(
        "first.js",
        "window.onload = function () { console.log(\"first\"); };\n",
    );
    let m = module_script(
        "m.js",
        "fail();\nwindow.onload = null;\nfunction fail() { throw new Error(\"bad\"); }\n",
    );
    let out = run_rewrite(&[first, m], &resolver);
    let js = &out.page_js;
    let run = function_body(js, "__eldur_run_m1");
    assert!(
        run.find("Eldur.setOnload(__eldur_load_listeners, __eldur_call_window_onload, null);")
            .expect("null set in its module")
            > run.find("fail()").expect("fail call"),
        "{js}"
    );
    let init = function_body(js, "__eldur_init");
    assert!(
        init.contains(
            "Eldur.setOnload(__eldur_load_listeners, __eldur_call_window_onload, function () {"
        ),
        "{js}"
    );
    assert!(
        function_body(js, "__eldur_call_window_onload")
            .contains("var handler = __eldur_call_window_onload.handler;"),
        "{js}"
    );
}

/// A read of `window.onload` at load is what `Eldur.setOnload` kept, at the
/// top level and in a function alike, so a value that is not an object reads
/// as `null`. The function that calls the handler is declared for a read alone.
#[test]
fn window_onload_reads_what_set_onload_kept() {
    let resolver = FakeResolver::default();
    let first = classic(
        "first.js",
        "console.log(window.onload);\nfunction later() { return window.onload; }\nwindow.onload = \"x\";\n",
    );
    let out = run_rewrite(&[first], &resolver);
    let js = &out.page_js;
    assert_eq!(
        js.matches("Eldur.getOnload(__eldur_call_window_onload)")
            .count(),
        2,
        "{js}"
    );
    assert!(!js.contains("Eldur.window.onload"), "{js}");
    assert!(js.contains("function __eldur_call_window_onload()"), "{js}");
}

/// A `load` listener and `window.onload` each take their place among the
/// `load` listeners where they run: the listener is queued where its module
/// adds it, and each assignment goes through `Eldur.setOnload`, which adds the
/// handler at the end, keeps its place while replaced, and takes it out on
/// `null`. Nothing is put in place at conversion.
#[test]
fn load_listeners_take_their_place_where_they_run() {
    let resolver = FakeResolver::default();
    let handler = module_script(
        "handler.js",
        "fail();\nwindow.onload = () => console.log(\"h1\");\nfunction fail() { throw new Error(\"bad\"); }\n",
    );
    let listen = module_script(
        "listen.js",
        "window.addEventListener(\"load\", () => console.log(\"l\"));\n",
    );
    let reset = module_script(
        "reset.js",
        "window.onload = null;\nwindow.onload = () => console.log(\"h2\");\n",
    );
    let out = run_rewrite(&[handler, listen, reset], &resolver);
    let js = &out.page_js;
    let set = "Eldur.setOnload(__eldur_load_listeners, __eldur_call_window_onload, ";
    let run0 = function_body(js, "__eldur_run_m0");
    assert!(
        run0.find(set).expect("h1 set in its module") > run0.find("fail()").expect("fail call"),
        "{js}"
    );
    let run1 = function_body(js, "__eldur_run_m1");
    assert!(
        run1.contains("Eldur.queueListener(__eldur_load_listeners, Eldur.window, \"load\", () =>"),
        "{js}"
    );
    let run2 = function_body(js, "__eldur_run_m2");
    let null_at = run2.find(&format!("{set}null);")).expect("null set");
    assert!(run2[null_at + 1..].contains(set), "{js}");
    let init = function_body(js, "__eldur_init");
    assert!(
        init.trim_end()
            .ends_with("Eldur.fireListeners(__eldur_load_listeners);"),
        "{js}"
    );
    assert!(!init.contains("Eldur.setOnload("), "{js}");
}

/// A `DOMContentLoaded` or `load` listener given by name is the value the
/// name holds where `addEventListener` runs, as a browser takes it, so a later
/// assignment to the name changes nothing.
#[test]
fn a_lifecycle_listener_is_the_value_at_the_call() {
    let resolver = FakeResolver::default();
    let m = module_script(
        "m.js",
        "let h = () => console.log(\"a\");\nwindow.addEventListener(\"load\", h);\ndocument.addEventListener(\"DOMContentLoaded\", h);\nh = () => console.log(\"b\");\n",
    );
    let out = run_rewrite(&[m], &resolver);
    let js = &out.page_js;
    let run = function_body(js, "__eldur_run_m0");
    let load = run
        .find("Eldur.queueListener(__eldur_load_listeners, Eldur.window, \"load\", __eldur_m0_h);")
        .expect("load listener queued with its value");
    let ready = run
        .find("Eldur.queueListener(__eldur_ready_listeners, Eldur.document, \"DOMContentLoaded\", __eldur_m0_h);")
        .expect("ready listener queued with its value");
    let reassigned = run.find("__eldur_m0_h = () =>").expect("reassigned");
    assert!(load < reassigned && ready < reassigned, "{js}");
    let init = function_body(js, "__eldur_init");
    let fire_ready = init
        .find("Eldur.fireListeners(__eldur_ready_listeners);")
        .expect("ready fired");
    let fire_load = init
        .find("Eldur.fireListeners(__eldur_load_listeners);")
        .expect("load fired");
    assert!(fire_ready < fire_load, "{js}");
}

/// A lifecycle listener's options are passed where the call runs, so the
/// runtime can tell a listener added again (same callback and capture) from
/// a capture listener of the same callback.
#[test]
fn a_lifecycle_listener_keeps_its_options() {
    let resolver = FakeResolver::default();
    let m = module_script(
        "m.js",
        "const h = () => console.log(\"a\");\nwindow.addEventListener(\"load\", h);\nwindow.addEventListener(\"load\", h, { capture: true });\ndocument.addEventListener(\"DOMContentLoaded\", h, true);\n",
    );
    let out = run_rewrite(&[m], &resolver);
    let js = &out.page_js;
    let run = function_body(js, "__eldur_run_m0");
    let plain = run
        .find("Eldur.queueListener(__eldur_load_listeners, Eldur.window, \"load\", __eldur_m0_h);")
        .expect("load listener without options");
    let capture = run
        .find("Eldur.queueListener(__eldur_load_listeners, Eldur.window, \"load\", __eldur_m0_h, { capture: true });")
        .expect("load listener with its options");
    let ready = run
        .find("Eldur.queueListener(__eldur_ready_listeners, Eldur.document, \"DOMContentLoaded\", __eldur_m0_h, true);")
        .expect("ready listener with its capture flag");
    assert!(plain < capture && capture < ready, "{js}");
}

/// Whatever runs at load, inside `__eldur_init` or a module's run function,
/// only assigns: each top-level binding is declared at the top of page.js, so
/// it is not local to the function. The rewrite asserts this in debug builds;
/// this page has every kind of top-level declaration, run where it can throw.
#[test]
fn no_declaration_is_local_to_the_code_run_at_load() {
    let resolver = FakeResolver::default();
    let decls = "var v = f();\nlet l = f();\nconst c = f();\nconst { d } = f();\nlet [e] = f();\nvar { g } = f(), [h] = f();\nfunction fn1() {}\nfunction* gen() {}\nasync function af() {}\nclass K { static s = f(); }\nlet u;\nfor (let i = 0; i < 1; i++) { const inner = i; }\nif (v) { let scoped = 1; }\n";
    let module = module_script(
        "m.js",
        &format!(
            "function f() {{ return [1]; }}\n{decls}export const ex = f();\nexport let el = f();\nexport class EK {{ static s = f(); }}\nexport default f();\n"
        ),
    );
    let lazy = module_script(
        "lazy.js",
        &format!("function f() {{ return [1]; }}\n{decls}export const lx = f();\n"),
    );
    let importer = module_script("i.js", "import(\"./lazy.js\");\n");
    let classic = ScriptInput {
        source: format!("function f() {{ return [1]; }}\n{decls}"),
        name: "classic.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
        imports: Vec::new(),
    };
    let out = run_rewrite(&[module, importer, lazy, classic], &resolver);
    let js = &out.page_js;
    for name in ["__eldur_run_m0", "__eldur_init"] {
        let at = js
            .find(&format!("function {name}("))
            .unwrap_or_else(|| panic!("{name}:\n{js}"));
        let body = &js[at..at + js[at..].find("\n}\n").expect("end")];
        for line in body.lines() {
            let t = line.trim_start();
            let depth = line.len() - t.len();
            // The function's own statements, and those of its `try`.
            if depth == 4 || depth == 8 {
                for kw in [
                    "var ",
                    "let ",
                    "const ",
                    "function ",
                    "function*",
                    "async function",
                    "class ",
                ] {
                    assert!(!t.starts_with(kw), "{name} declares `{t}`:\n{js}");
                }
            }
        }
    }
}
