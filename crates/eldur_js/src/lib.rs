//! JavaScript for Eldur: the page's scripts, parsed with oxc and
//! rewritten to run in QML's JavaScript engine.
//!
//! How the output runs: all classic scripts of a page become one JS
//! resource, `page.js`, imported by the generated `Main.qml` as
//! `import "page.js" as Page` (no `.pragma library`, so its code sees the
//! QML ids of the component that imports it). DOM look-ups that can be
//! resolved at conversion time become those ids; the rest go through a
//! small runtime, `eldur_runtime.js` (shipped by this crate, see
//! [`RUNTIME_JS`]), imported from `page.js` with `.import "eldur_runtime.js" as Eldur`.
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
pub const RUNTIME_JS: &str = include_str!("eldur_runtime.js");

/// How a script snippet was embedded in the HTML document.
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
/// `eldur_dom::NodeId` index, kept as a number so this crate does not depend
/// on the DOM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementKey(pub u32);

/// One script or inline event handler attribute extracted from a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptInput {
    /// Raw JavaScript source text.
    pub source: String,
    /// File path/URL or `inline script #N` / `onclick of <button id=x>`.
    pub name: String,
    /// Execution mode of the script.
    pub kind: ScriptKind,
    /// 1-based line in the HTML where inline code starts (for diagnostics).
    pub line_offset: u32,
}

/// A DOM query found in the scripts with a literal argument.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomQuery {
    /// `document.getElementById(id)`.
    ById(String),
    /// `querySelector` (first match).
    Selector(String),
    /// `querySelectorAll`, `getElementsByClassName`, `getElementsByTagName`
    /// (selector text built from the latter two).
    SelectorAll(String),
}

/// Whether a DOM event of this type goes on to the target's ancestors.
#[must_use]
pub fn bubbles(event: &str) -> bool {
    matches!(
        event,
        "click"
            | "dblclick"
            | "input"
            | "change"
            | "submit"
            | "reset"
            | "keydown"
            | "keyup"
            | "keypress"
            | "focusin"
            | "focusout"
            | "mousedown"
            | "mouseup"
            | "mousemove"
            | "mouseover"
            | "mouseout"
            | "pointerdown"
            | "pointerup"
            | "touchstart"
            | "touchend"
            | "contextmenu"
            | "wheel"
    )
}

/// Static summary of DOM queries, mutations, writes and event listeners found across scripts.
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
    /// Syntax errors and unsupported-feature warnings discovered during analysis.
    pub diagnostics: Vec<Diagnostic>,
    /// Every class write site, in source order (`docs/design/dynamic-styles.md` §9.1).
    pub class_writes: Vec<ClassWrite>,
    /// Every `document.createElement("<literal>")` site, and every element
    /// written literally in markup given to `innerHTML` or
    /// `insertAdjacentHTML`, in source order.
    pub created: Vec<CreatedElement>,
    /// Scripts reach elements by runtime selectors, traversal, `matches`/`closest`,
    /// event targets, or mutate the DOM: every element needs `eldurTag`/`eldurClasses`.
    pub runtime_dom: bool,
    /// Some handler can hear an event that started on another element, so
    /// every listener goes through the runtime's one dispatch path rather
    /// than a QML handler of its own. The converter sets this.
    pub dispatch_by_path: bool,
    /// A bubbling event is listened for on something no literal query names
    /// (`document`, `window`, `event.target`, a created element).
    pub listens_unresolved: bool,
    /// Elements whose children scripts insert, remove, replace or reorder
    /// (the receiver of `appendChild`, `insertBefore`, `removeChild`,
    /// `append`, `replaceChildren`, `innerHTML =`, `textContent =`, …), in
    /// first-seen order. Elements the analysis cannot follow are left out.
    pub mutated_parents: Vec<ElementRef>,
    /// Elements scripts remove or put siblings beside (`remove()`,
    /// `before`, `after`, `replaceWith`, `outerHTML =`): their parents'
    /// children change.
    pub mutated_siblings: Vec<ElementRef>,
    /// Queries whose answer can change while the page runs, so `rewrite` sends
    /// them to the runtime rather than answering from the original document.
    pub live_queries: Vec<DomQuery>,
    /// Elements whose text scripts assign (`textContent =`, `innerText =`).
    /// That removes an element's element children, which only the converter,
    /// holding the document, can see: it calls
    /// [`Analysis::mark_all_queries_live`] if one has any.
    pub text_writes: Vec<ElementRef>,
    /// Scripts can read attributes the markup wrote (`getAttribute`, `dataset`,
    /// `[attr]` selectors, `matches`): every element needs `eldurAttrs`.
    pub reads_attributes: bool,
}

impl Analysis {
    /// Every query becomes live: the converter found a mutation the script
    /// alone does not show.
    pub fn mark_all_queries_live(&mut self) {
        self.live_queries.clone_from(&self.queries);
        self.runtime_dom |= !self.live_queries.is_empty();
    }
}

/// A created element (`document.createElement` call site) as analysis predicts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CreatedId(pub u32);

/// An element a script refers to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementRef {
    /// A static element found by a literal query.
    Query(DomQuery),
    /// An element created by script at a known site.
    Created(CreatedId),
    /// `event.target`, `this`, untracked variables.
    Unknown,
}

/// Classes a script adds, removes or assigns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassWrite {
    /// The element whose classes change.
    pub target: ElementRef,
    /// Literal class names involved (each may end up present or absent), sorted.
    pub names: Vec<String>,
    /// Part of the value is not a literal (`'chat ' + role`, `` `toast ${t}` ``, a variable).
    pub open: bool,
    /// `className =` / `setAttribute("class")`: replaces the whole set.
    pub assigns: bool,
    /// Originating [`ScriptInput::name`].
    pub source: String,
    /// 1-based line in the HTML.
    pub line: Option<u32>,
}

/// An element a script creates, with what it is like when inserted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedElement {
    /// The call site's id; [`ElementRef::Created`] refers to it.
    pub id: CreatedId,
    /// Lower-case tag name.
    pub tag: String,
    /// `<input>` type set by the script, if literal.
    pub input_type: Option<String>,
    /// Every complete class set the element can have when inserted (sorted, deduplicated).
    pub class_sets: Vec<Vec<String>>,
    /// Part of some class value is not a literal.
    pub open_classes: bool,
    /// Where it is inserted.
    pub parents: Vec<ElementRef>,
    /// Originating [`ScriptInput::name`].
    pub source: String,
    /// 1-based line in the HTML of the `createElement` call.
    pub line: Option<u32>,
}

/// What the converter made of an element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QmlTarget {
    /// Converter's element key.
    pub key: ElementKey,
    /// Sanitized QML `id` emitted in `Main.qml`.
    pub qml_id: String,
    /// QML component family chosen for the element.
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
    /// What a query matches in the document as written, even when `resolve`
    /// declines to answer because the query is live. For wiring that has to
    /// exist before the page runs, such as a generated event handler.
    fn original(&self, query: &DomQuery) -> Vec<QmlTarget> {
        self.resolve(query)
    }
}

/// A function in `page.js` the converter wires to a QML signal handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBinding {
    /// Resolved QML element receiving the signal handler.
    pub target: QmlTarget,
    /// DOM event type: `click`, `input`, `change`, `keydown`, ….
    pub event: String,
    /// Function in `page.js` taking one argument, a DOM-like event object
    /// built by the QML handler with `Eldur.event(...)`; call it as
    /// `Page.<function>(Eldur.event("click", <qml_id>, mouse))`.
    pub function: String,
}

/// Rewritten `page.js` output along with initialization hook, static event bindings, and diagnostics.
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
    /// The `bindings` functions that hand the event to listeners added with
    /// `Eldur.listen`, which can also connect the signal itself: their QML
    /// handler must check `Eldur.connected(<qml_id>, <event>)`.
    pub redispatching_slots: Vec<String>,
    /// Syntax errors and unsupported-feature warnings encountered during rewrite.
    pub diagnostics: Vec<Diagnostic>,
}

/// Severity of a JavaScript conversion diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Valid JS that will not work (or only partly) in QML.
    Unsupported,
    /// A syntax error; that script is skipped.
    Error,
}

/// A diagnostic emitted while analyzing or rewriting page scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Error or unsupported-feature warning level.
    pub severity: Severity,
    /// Originating [`ScriptInput::name`].
    pub source: String,
    /// 1-based line number adjusted by [`ScriptInput::line_offset`].
    pub line: Option<u32>,
    /// Human-readable description of the issue.
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
mod edit;
mod lower;
mod parse;
mod resolve;
mod rewrite;
