//! JavaScript for html-2-qml: the page's scripts, parsed with oxc and
//! rewritten to run in QML's JavaScript engine.
//!
//! How the output runs: all classic scripts of a page become one JS
//! resource, `page.js`, imported by the generated `Main.qml` as
//! `import "page.js" as Page` (no `.pragma library`, so its code sees the
//! QML ids of the component that imports it). DOM look-ups that can be
//! resolved at conversion time become those ids; the rest go through a
//! small runtime, `h2q_runtime.js` (shipped by this crate, see
//! [`RUNTIME_JS`]), imported from `page.js` with `.import "h2q_runtime.js" as H2Q`.
//!
//! Two passes, because the converter must know which elements scripts
//! touch (to give them ids and the right QML type) before it builds QML,
//! and the rewrite needs those ids:
//! 1. [`analyze`] finds every DOM query and event binding.
//! 2. [`rewrite`] produces `page.js` given a [`DomResolver`].
//!
//! The public API in this file is the contract the converter is written
//! against. Add to it freely; change or remove items only with the lead's
//! agreement.

/// The runtime every generated `page.js` imports: timers, a minimal
/// `Element` wrapper over QML items for what cannot be resolved statically,
/// `console`, `localStorage`, events. Written to the output directory next
/// to `page.js`.
pub const RUNTIME_JS: &str = include_str!("h2q_runtime.js");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptKind {
    /// `<script>` / `<script src>` without `type=module`.
    Classic,
    /// `<script type="module">`: imports are inlined when they resolve
    /// to files given to the converter, else reported.
    Module,
    /// An event handler attribute: `<button onclick="go(this)">`.
    /// `element` is the converter's key for the element (its QML id once
    /// resolved through [`DomResolver::element`]).
    HandlerAttribute { element: ElementKey, event: String },
}

/// Opaque key the converter uses for a DOM element (it is the
/// `h2q_dom::NodeId` index, kept as a number so this crate does not depend
/// on the DOM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementKey(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInput {
    pub source: String,
    /// File path/URL or `inline script #N` / `onclick of <button id=x>`.
    pub name: String,
    pub kind: ScriptKind,
    /// 1-based line in the HTML where inline code starts (for diagnostics).
    pub line_offset: u32,
}

/// A DOM query found in the scripts with a literal argument.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomQuery {
    ById(String),
    /// `querySelector` (first match).
    Selector(String),
    /// `querySelectorAll`, `getElementsByClassName`, `getElementsByTagName`
    /// (selector text built from the latter two).
    SelectorAll(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Analysis {
    /// Every literal DOM query, deduplicated, in first-seen order.
    pub queries: Vec<DomQuery>,
    /// Event types listened for, per query (`addEventListener`, `onX =`).
    pub listened: Vec<(DomQuery, String)>,
    /// Scripts create or move elements (`createElement`, `innerHTML =`,
    /// `appendChild`, …): the converter must give the affected containers a
    /// runtime-mutable form (see `docs/spec/dom-api.md`).
    pub mutates_dom: bool,
    /// Element properties written per query (`textContent`, `value`,
    /// `style.color`, `classList`, `hidden`, …), so the converter can make
    /// the corresponding QML properties writable/bound.
    pub writes: Vec<(DomQuery, String)>,
    pub diagnostics: Vec<Diagnostic>,
}

/// What the converter made of an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QmlTarget {
    pub key: ElementKey,
    pub qml_id: String,
    pub kind: TargetKind,
}

/// The QML type family of a converted element; decides how `textContent`,
/// `value`, `checked`, `style.*` map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetKind {
    /// `Text` (text property `text`).
    Text,
    /// A box (`Rectangle`/`Item`/layout) whose text content is one child
    /// `Text` with id `<qml_id>Text` when it has text.
    Box,
    /// `TextField` (`text`, `placeholderText`, `readOnly`, `enabled`).
    TextField,
    /// `TextArea`.
    TextArea,
    /// `CheckBox` (`checked`).
    CheckBox,
    /// `RadioButton` (`checked`).
    RadioButton,
    /// `ComboBox` (`currentIndex`, `currentText`, model of option texts).
    ComboBox,
    /// `Slider` / `SpinBox` (`value`, `from`, `to`, `stepSize`).
    Range,
    /// `Button` (`text`, `enabled`, `onClicked`).
    Button,
    /// `Image` (`source`).
    Image,
    /// `ProgressBar` (`value`, `from`, `to`).
    Progress,
}

/// Lets the rewrite map DOM queries to QML ids. Implemented by the
/// converter.
pub trait DomResolver {
    /// All elements a query matches, in document order.
    fn resolve(&self, query: &DomQuery) -> Vec<QmlTarget>;
    /// The target for a handler attribute's element (`this` in handler code).
    fn element(&self, key: ElementKey) -> Option<QmlTarget>;
}

/// A function in `page.js` the converter wires to a QML signal handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBinding {
    pub target: QmlTarget,
    /// DOM event type: `click`, `input`, `change`, `keydown`, ….
    pub event: String,
    /// Function in `page.js` taking one argument, a DOM-like event object
    /// built by the QML handler with `H2Q.event(...)`; call it as
    /// `Page.<function>(H2Q.event("click", <qml_id>, mouse))`.
    pub function: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JsOutput {
    /// Contents of `page.js`.
    pub page_js: String,
    /// Function in `page.js` to call from `Component.onCompleted` of the
    /// root (runs top-level statements, `DOMContentLoaded` and `load`
    /// listeners in order). `None` when there is nothing to run.
    pub init_function: Option<String>,
    /// Static event bindings (handler attributes, `addEventListener` and
    /// `onX =` on resolved targets).
    pub bindings: Vec<EventBinding>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Valid JS that will not work (or only partly) in QML.
    Unsupported,
    /// A syntax error; that script is skipped.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub source: String,
    pub line: Option<u32>,
    pub message: String,
}

/// Pass 1: parse every script with oxc and find DOM queries and bindings.
#[must_use]
pub fn analyze(scripts: &[ScriptInput]) -> Analysis {
    analyze::analyze(scripts)
}

/// Pass 2: rewrite all scripts into one `page.js`.
#[must_use]
pub fn rewrite(
    scripts: &[ScriptInput],
    analysis: &Analysis,
    resolver: &dyn DomResolver,
) -> JsOutput {
    rewrite::rewrite(scripts, analysis, resolver)
}

mod analyze;
mod rewrite;
