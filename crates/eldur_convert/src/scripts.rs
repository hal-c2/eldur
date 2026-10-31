//! Steps 3 and 6: the page's scripts, the elements they address, and the
//! wiring of `page.js` into `Main.qml`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use eldur_dom::{Document, NodeId};
use eldur_js::{
    Analysis, DomQuery, DomResolver, ElementKey, ElementRef, JsOutput, QmlTarget, ScriptInput,
    ScriptKind, TargetKind,
};
use eldur_qml::{Member, Object};

use crate::Resources;
use crate::boxes::{describe, node_line};
use crate::diag::Diagnostics;
use crate::emit::{Form, Target};

/// The scripts of a page and what they refer to.
pub(crate) struct Scripts {
    pub inputs: Vec<ScriptInput>,
    pub analysis: Analysis,
    /// `ElementKey` → node (keys are node indices; `NodeId` has no public
    /// constructor).
    keys: HashMap<u32, NodeId>,
    matches: HashMap<DomQuery, Vec<NodeId>>,
    /// Every element a script can reach statically.
    pub wanted: HashSet<NodeId>,
    pub(crate) gains: Gains,
    /// The static tables a script can reach into. See [`live_tables`].
    pub(crate) live_tables: HashSet<NodeId>,
}

impl Scripts {
    pub(crate) fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// The elements a literal query finds.
    pub(crate) fn matches(&self, query: &DomQuery) -> &[NodeId] {
        self.matches.get(query).map_or(&[], Vec::as_slice)
    }

    /// The node of a key: `keys` holds handler-attribute elements, `wanted`
    /// everything scripts can reach.
    fn keys_to_node(&self, key: ElementKey) -> Option<NodeId> {
        self.wanted.iter().copied().find(|&n| key_of(n) == key)
    }
}

/// Collects `<script>`s and `on*` attributes in document order and runs
/// the analysis.
pub(crate) fn collect(
    doc: &Document,
    url: &str,
    resources: &mut dyn Resources,
    diags: &mut Diagnostics,
) -> Scripts {
    let mut inputs = Vec::new();
    let mut keys = HashMap::new();
    let mut inline = 0;
    for node in doc.descendants(doc.root()) {
        let Some(element) = doc.element(node) else {
            continue;
        };
        let line = node_line(doc, node);
        for attr in &element.attrs {
            let name: &str = &attr.name.local;
            if let Some(event) = name.strip_prefix("on")
                && !event.is_empty()
                && element.is_html()
            {
                let key = key_of(node);
                keys.insert(key.0, node);
                inputs.push(ScriptInput {
                    source: attr.value.clone(),
                    name: format!("{name} of {}", describe(doc, node)),
                    kind: ScriptKind::HandlerAttribute {
                        element: key,
                        event: event.to_owned(),
                    },
                    line_offset: line.unwrap_or(1),
                });
            }
        }
        if element.local_name() != "script" || !element.is_html() {
            continue;
        }
        let kind = match element.attr("type").map(|t| t.trim().to_ascii_lowercase()) {
            None => ScriptKind::Classic,
            Some(t) if t.is_empty() || is_javascript_mime(&t) => ScriptKind::Classic,
            Some(t) if t == "module" => ScriptKind::Module,
            // Data blocks (`application/json`, templates) are not run.
            Some(_) => continue,
        };
        match element.attr("src") {
            Some(src) => match resources.read(src, url) {
                Some(resource) => inputs.push(ScriptInput {
                    source: String::from_utf8_lossy(&resource.bytes).into_owned(),
                    name: resource.resolved,
                    kind,
                    line_offset: 1,
                }),
                None => diags.error(
                    "js",
                    line,
                    format!("script `{src}` could not be read; it is skipped"),
                ),
            },
            None => {
                inline += 1;
                inputs.push(ScriptInput {
                    source: doc.text_content(node),
                    name: format!("inline script #{inline}"),
                    kind,
                    line_offset: line.unwrap_or(1),
                });
            }
        }
    }

    let mut analysis = eldur_js::analyze(&inputs);
    let mut matches = HashMap::new();
    let mut wanted: HashSet<NodeId> = keys.values().copied().collect();
    for query in &analysis.queries {
        let found = resolve(doc, query, diags);
        wanted.extend(found.iter().copied());
        matches.insert(query.clone(), found);
    }
    // Assigning text removes what was inside, and with it what lookups find.
    let empties_elements = analysis.text_writes.iter().any(|write| match write {
        ElementRef::Query(query) => matches.get(query).is_some_and(|found: &Vec<NodeId>| {
            found
                .iter()
                .any(|&n| doc.element_children(n).next().is_some())
        }),
        _ => false,
    });
    if empties_elements {
        analysis.mark_all_queries_live();
    }
    if can_hear_another_element(doc, &analysis, &matches, &keys) {
        analysis.dispatch_by_path = true;
        analysis.runtime_dom = true;
    }
    let live_tables = live_tables(doc, &inputs, &wanted);
    for &table in &live_tables {
        wanted.extend(doc.descendants(table).filter(|&n| {
            doc.element(n)
                .is_some_and(|e| is_table_part(e.local_name()))
        }));
    }
    if !live_tables.is_empty() {
        analysis.runtime_dom = true;
        analysis.static_tables = true;
    }
    let gains = Gains::of(doc, &analysis, &matches, &wanted);
    Scripts {
        inputs,
        analysis,
        keys,
        matches,
        wanted,
        gains,
        live_tables,
    }
}

fn is_table_part(tag: &str) -> bool {
    matches!(
        tag,
        "caption" | "colgroup" | "col" | "thead" | "tbody" | "tfoot" | "tr" | "td" | "th"
    )
}

/// The tables whose structure is made of objects, for scripts to walk and
/// move: those a query finds, or finds something in, and on a page whose
/// scripts talk about table parts at all, every one, since walking the tree or
/// an event's target reaches them with no query to see. The rest stay one
/// `GridLayout`, as a page that leaves its tables alone needs nothing more.
fn live_tables(
    doc: &Document,
    inputs: &[ScriptInput],
    wanted: &HashSet<NodeId>,
) -> HashSet<NodeId> {
    const WORDS: &[&str] = &[
        "tr",
        "td",
        "th",
        "tbody",
        "thead",
        "tfoot",
        "TR",
        "TD",
        "TH",
        "TBODY",
        "THEAD",
        "TFOOT",
        "rows",
        "cells",
        "tBodies",
        "tHead",
        "tFoot",
        "insertRow",
        "deleteRow",
        "insertCell",
        "deleteCell",
        "rowIndex",
        "sectionRowIndex",
        "cellIndex",
        "createTBody",
        "createTHead",
    ];
    let named = inputs.iter().any(|input| {
        input
            .source
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
            .any(|word| WORDS.contains(&word))
    });
    doc.descendants(doc.root())
        .filter(|&n| {
            doc.element(n).is_some_and(|e| e.local_name() == "table")
                && (named || doc.descendants(n).any(|d| wanted.contains(&d)))
        })
        .collect()
}

/// The boxes a script may put block children in. An element's form follows
/// what it holds in the document, so one that holds no blocks there needs a
/// layout of its own for them.
#[derive(Debug, Default)]
pub(crate) struct Gains {
    /// Named by a script's query, or looked up and handed on where analysis
    /// cannot follow.
    pub(crate) nodes: HashSet<NodeId>,
    /// A script inserts into something it finds at run time, which may be
    /// any empty element.
    pub(crate) any_empty: bool,
}

impl Gains {
    fn of(
        doc: &Document,
        analysis: &eldur_js::Analysis,
        matches: &HashMap<DomQuery, Vec<NodeId>>,
        wanted: &HashSet<NodeId>,
    ) -> Self {
        let mut receivers: Vec<&ElementRef> = analysis
            .created
            .iter()
            .filter(|e| !crate::emit::templates::stays_inline(&e.tag))
            .flat_map(|e| &e.parents)
            .collect();
        receivers.extend(&analysis.opaque_inserts);
        let mut gains = Self::default();
        for receiver in receivers {
            match receiver {
                ElementRef::Query(query) => {
                    gains
                        .nodes
                        .extend(matches.get(query).into_iter().flatten().copied());
                }
                ElementRef::Unknown => gains.any_empty = true,
                ElementRef::Created(_) => {}
            }
        }
        if gains.any_empty {
            gains.nodes.extend(
                wanted
                    .iter()
                    .copied()
                    .filter(|&n| doc.element_children(n).next().is_none()),
            );
        }
        gains
    }
}

/// A listener on an element with elements inside hears events that started on
/// them; `submit` starts at the form that has the listener. A listener no query
/// names may be anywhere. A page with neither keeps its handlers in QML.
fn can_hear_another_element(
    doc: &Document,
    analysis: &eldur_js::Analysis,
    matches: &HashMap<DomQuery, Vec<NodeId>>,
    keys: &HashMap<u32, NodeId>,
) -> bool {
    let nests = |node: &NodeId| doc.element_children(*node).next().is_some();
    let delegates = |event: &str| event != "submit" && eldur_js::bubbles(event);
    analysis.listens_unresolved
        || analysis.listened.iter().any(|(query, event)| {
            delegates(event) && matches.get(query).is_some_and(|f| f.iter().any(nests))
        })
        || keys.values().any(|node| {
            nests(node)
                && doc.element(*node).is_some_and(|e| {
                    e.attrs.iter().any(|a| {
                        let name: &str = &a.name.local;
                        name.strip_prefix("on").is_some_and(delegates)
                    })
                })
        })
}

fn key_of(node: NodeId) -> ElementKey {
    ElementKey(u32::try_from(node.index()).unwrap_or(u32::MAX))
}

/// The JavaScript MIME type essence strings (HTML §8.1.1 "JavaScript MIME
/// type"), the only `type`s a browser runs as classic scripts.
fn is_javascript_mime(t: &str) -> bool {
    let essence = t.split(';').next().unwrap_or("").trim();
    matches!(
        essence,
        "text/javascript"
            | "application/javascript"
            | "application/ecmascript"
            | "application/x-ecmascript"
            | "application/x-javascript"
            | "text/ecmascript"
            | "text/javascript1.0"
            | "text/javascript1.1"
            | "text/javascript1.2"
            | "text/javascript1.3"
            | "text/javascript1.4"
            | "text/javascript1.5"
            | "text/jscript"
            | "text/livescript"
            | "text/x-ecmascript"
            | "text/x-javascript"
    )
}

fn resolve(doc: &Document, query: &DomQuery, diags: &mut Diagnostics) -> Vec<NodeId> {
    let (text, first_only) = match query {
        DomQuery::ById(id) => return doc.element_by_id(id).into_iter().collect(),
        DomQuery::Selector(s) => (s, true),
        DomQuery::SelectorAll(s) => (s, false),
    };
    let Some(mut found) = eldur_css::query_selector_all(doc, text) else {
        diags.warning(
            "js",
            None,
            format!("selector `{text}` does not parse; a browser would throw here"),
        );
        return Vec::new();
    };
    if first_only {
        found.truncate(1);
    }
    found
}

/// Maps queries to the ids the emitter gave their elements.
pub(crate) struct Resolver<'a> {
    scripts: &'a Scripts,
    targets: &'a BTreeMap<NodeId, Target>,
    inline_covered: &'a std::collections::BTreeSet<NodeId>,
}

/// The root property that holds an inline node with a handler attribute.
const INLINE_NODE_PREFIX: &str = "eldurNode";

impl<'a> Resolver<'a> {
    pub(crate) fn new(
        scripts: &'a Scripts,
        targets: &'a BTreeMap<NodeId, Target>,
        inline_covered: &'a std::collections::BTreeSet<NodeId>,
    ) -> Self {
        Self {
            scripts,
            targets,
            inline_covered,
        }
    }

    fn target(&self, node: NodeId) -> Option<QmlTarget> {
        self.targets.get(&node).map(|t| QmlTarget {
            key: key_of(node),
            qml_id: t.qml_id.clone(),
            kind: t.kind,
        })
    }
}

impl DomResolver for Resolver<'_> {
    fn resolve(&self, query: &DomQuery) -> Vec<QmlTarget> {
        self.scripts
            .matches
            .get(query)
            .map(|nodes| nodes.iter().filter_map(|&n| self.target(n)).collect())
            .unwrap_or_default()
    }

    fn element(&self, key: ElementKey) -> Option<QmlTarget> {
        let node = *self.scripts.keys.get(&key.0)?;
        self.target(node).or_else(|| {
            // No object of its own: its node is found when the page starts.
            (self.scripts.analysis.dispatch_by_path && self.inline_covered.contains(&node)).then(
                || QmlTarget {
                    key,
                    qml_id: format!("{INLINE_NODE_PREFIX}{}", key.0),
                    kind: TargetKind::Box,
                },
            )
        })
    }
}

/// Reports elements scripts refer to that got no QML object.
pub(crate) fn report_missing(
    doc: &Document,
    scripts: &Scripts,
    targets: &BTreeMap<NodeId, Target>,
    inline_covered: &std::collections::BTreeSet<NodeId>,
    diags: &mut Diagnostics,
) {
    let mut missing: Vec<NodeId> = scripts
        .wanted
        .iter()
        .copied()
        .filter(|n| !targets.contains_key(n) && !inline_covered.contains(n))
        .collect();
    missing.sort_by_key(|n| n.index());
    for node in missing {
        diags.warning(
            "js",
            node_line(doc, node),
            format!(
                "{} is used by a script but has no QML object of its own (inline, hidden or not converted); script access to it does nothing",
                describe(doc, node)
            ),
        );
    }
}

/// Declares `cls_<s>` on each static element a script writes classes on,
/// so the runtime's class changes reach QML, and reports classes that share
/// one property.
pub(crate) fn declare_classes(
    root: &mut Object,
    doc: &Document,
    scripts: &Scripts,
    targets: &BTreeMap<NodeId, Target>,
    diags: &mut Diagnostics,
) {
    let writes = &scripts.analysis.class_writes;
    let mut by_node: BTreeMap<NodeId, BTreeSet<String>> = BTreeMap::new();
    for write in writes {
        let ElementRef::Query(query) = &write.target else {
            // Created elements get theirs in their templates; elements a
            // script reaches without a literal query are found from the
            // style sheet once classes are styled (dynamic-styles §5.1).
            continue;
        };
        for node in scripts.matches.get(query).into_iter().flatten() {
            by_node
                .entry(*node)
                .or_default()
                .extend(write.names.iter().cloned());
        }
    }
    for (node, classes) in &by_node {
        let Some(target) = targets.get(node) else {
            continue;
        };
        if let Some(obj) = find_mut(root, &target.qml_id) {
            crate::metadata::declare_classes(obj, doc, *node, classes);
        }
    }
    let names = writes
        .iter()
        .flat_map(|w| w.names.iter().map(String::as_str));
    for (prop, classes) in crate::metadata::collisions(names) {
        let list: Vec<String> = classes.iter().map(|c| format!("`{c}`")).collect();
        let line = writes
            .iter()
            .find(|w| w.names.iter().any(|n| classes.contains(n.as_str())))
            .and_then(|w| w.line);
        diags.info(
            "js",
            line,
            format!(
                "classes {} all map to `{prop}`, so QML cannot tell them apart",
                list.join(" and ")
            ),
        );
    }
}

/// Adds the signal handlers of `js.bindings` to the objects they target.
pub(crate) fn wire(
    root: &mut Object,
    js: &JsOutput,
    forms: &BTreeMap<NodeId, Form>,
    scripts: &Scripts,
    diags: &mut Diagnostics,
) -> Vec<String> {
    if scripts.analysis.dispatch_by_path {
        return wire_by_path(root, js, forms, scripts);
    }
    for binding in &js.bindings {
        let id = &binding.target.qml_id;
        let call = |event: &str, arg: &str| {
            let fire = format!(
                "Page.{}(Eldur.event({event:?}, {id}, {arg}));",
                binding.function
            );
            if js.redispatching_slots.contains(&binding.function) {
                format!("if (!Eldur.connected({id}, {event:?})) {{ {fire} }}")
            } else {
                fire
            }
        };
        let event = binding.event.as_str();
        if event == "submit" {
            let form = scripts
                .keys_to_node(binding.target.key)
                .and_then(|n| forms.get(&n));
            let submitters = form.map_or(0, |f| f.accepting.len() + f.submitters.len());
            if submitters == 0 {
                diags.warning(
                    "js",
                    None,
                    format!("`submit` listeners on `{id}` have no field or button to fire them"),
                );
            }
            if let Some(form) = form {
                for (ids, signal) in [
                    (&form.accepting, "onAccepted"),
                    (&form.submitters, "onClicked"),
                ] {
                    for submitter in ids {
                        if let Some(obj) = find_mut(root, submitter) {
                            add_handler(obj, signal, &[], vec![call("submit", "null")]);
                        }
                    }
                }
            }
            continue;
        }
        let Some(obj) = find_mut(root, id) else {
            diags.error(
                "qml",
                None,
                format!("event binding targets `{id}`, which is not in Main.qml"),
            );
            continue;
        };
        let handler = signal_for(&obj.type_name, event);
        match (event, handler) {
            ("click", None) => {
                let mut tap = Object::new("TapHandler");
                tap.members.push(Member::Handler {
                    name: "onTapped".to_owned(),
                    params: vec!["eventPoint".to_owned(), "button".to_owned()],
                    body: vec![call("click", "eventPoint")],
                });
                obj.push_child(tap);
            }
            ("keydown" | "keyup", _) => {
                let name = if event == "keydown" {
                    "Keys.onPressed"
                } else {
                    "Keys.onReleased"
                };
                // Qt accepts a key event with a handler unless told not to,
                // and an accepted key never reaches the field's own editing.
                add_handler(
                    obj,
                    name,
                    &["event"],
                    vec![
                        format!("const dom = Eldur.event({event:?}, {id}, event);"),
                        format!("Page.{}(dom);", binding.function),
                        "event.accepted = dom.defaultPrevented;".to_owned(),
                    ],
                );
            }
            ("focus" | "focusin" | "blur" | "focusout", _) => {
                let body = if matches!(event, "focus" | "focusin") {
                    format!("if (activeFocus) {{ {} }}", call(event, "null"))
                } else {
                    format!(
                        "if (!activeFocus) {{ Eldur.whenBlurred({id}, {event:?}, function() {{ {} }}); }}",
                        call(event, "null")
                    )
                };
                add_handler(obj, "onActiveFocusChanged", &[], vec![body]);
            }
            (_, Some(signal)) => {
                let mut body = call(event, "null");
                // TextArea has no "edited by the user" signal.
                if signal == "onTextChanged" {
                    body = format!("if (activeFocus) {{ {body} }}");
                }
                if signal == "onPressedChanged" {
                    body = format!("if (!pressed) {{ {body} }}");
                }
                add_handler(obj, signal, &[], vec![body]);
            }
            (other, None) => diags.warning(
                "js",
                None,
                format!("`{other}` listeners on `{id}` are not wired to QML yet"),
            ),
        }
    }
    Vec::new()
}

/// `wire` for a page whose listeners all go through the runtime's dispatch:
/// `Eldur.listen` connects what it can, so only keys and a form's submit, which
/// it cannot, get a QML handler. Attribute handlers join as `onX` before any
/// script runs; returns those lines, for the root's `Component.onCompleted`.
fn wire_by_path(
    root: &mut Object,
    js: &JsOutput,
    forms: &BTreeMap<NodeId, Form>,
    scripts: &Scripts,
) -> Vec<String> {
    let mut start = Vec::new();
    let mut keys: BTreeSet<(String, &str)> = BTreeSet::new();
    let mut inline_nodes = BTreeSet::new();
    for binding in &js.bindings {
        let id = &binding.target.qml_id;
        let event = binding.event.as_str();
        if let Some(key) = id.strip_prefix(INLINE_NODE_PREFIX)
            && inline_nodes.insert(id.clone())
        {
            root.members.push(Member::PropertyDecl {
                ty: "var".to_owned(),
                name: id.clone(),
                value: None,
                readonly: false,
                required: false,
            });
            start.push(format!("{id} = Eldur.inlineNode({key});"));
        }
        let bound = matches!(event, "keydown" | "keyup" | "submit");
        if !js.redispatching_slots.contains(&binding.function) {
            let setter = if bound { "setOnBound" } else { "setOn" };
            start.push(format!(
                "Eldur.{setter}({id}, {event:?}, Page.{});",
                binding.function
            ));
        }
        if event == "submit" {
            let form = scripts
                .keys_to_node(binding.target.key)
                .and_then(|n| forms.get(&n));
            if let Some(form) = form {
                for (ids, signal) in [
                    (&form.accepting, "onAccepted"),
                    (&form.submitters, "onClicked"),
                ] {
                    for submitter in ids {
                        if let Some(obj) = find_mut(root, submitter) {
                            let fire = format!("Eldur.fire({id}, \"submit\", null);");
                            add_handler(obj, signal, &[], vec![fire]);
                        }
                    }
                }
            }
        } else if bound {
            keys.insert((id.clone(), event));
        }
    }
    let page = &js.page_js;
    // A form with no listener of its own starts the path of its submit too.
    if page.contains("submit") {
        let heard: BTreeSet<NodeId> = js
            .bindings
            .iter()
            .filter(|b| b.event == "submit")
            .filter_map(|b| scripts.keys_to_node(b.target.key))
            .collect();
        for (_, form) in forms.iter().filter(|(n, _)| !heard.contains(n)) {
            for (ids, signal) in [
                (&form.accepting, "onAccepted"),
                (&form.submitters, "onClicked"),
            ] {
                for submitter in ids {
                    if let Some(obj) = find_mut(root, submitter) {
                        let fire = format!("Eldur.fireSubmit({submitter});");
                        add_handler(obj, signal, &[], vec![fire]);
                    }
                }
            }
        }
    }
    // A control with no listener of its own starts the key's path too.
    for (event, quoted) in [("keydown", "keydown"), ("keyup", "keyup")] {
        if page.contains(quoted) {
            let mut ids = Vec::new();
            control_ids(root, &mut ids);
            keys.extend(ids.into_iter().map(|id| (id, event)));
        }
    }
    for (id, event) in keys {
        let Some(obj) = find_mut(root, &id) else {
            continue;
        };
        let name = if event == "keydown" {
            "Keys.onPressed"
        } else {
            "Keys.onReleased"
        };
        add_handler(
            obj,
            name,
            &["event"],
            vec![format!(
                "event.accepted = !Eldur.fire({id}, {event:?}, event);"
            )],
        );
    }
    if !start.is_empty() {
        start.insert(0, format!("Eldur.init({});", crate::emit::units::ROOT_ID));
    }
    start
}

fn control_ids(obj: &Object, out: &mut Vec<String>) {
    let control = matches!(
        obj.type_name.as_str(),
        "Button"
            | "CheckBox"
            | "RadioButton"
            | "TextField"
            | "TextArea"
            | "ComboBox"
            | "Slider"
            | "SpinBox"
    );
    if control && let Some(id) = &obj.id {
        out.push(id.clone());
    }
    for member in &obj.members {
        if let Member::Child(child) = member {
            control_ids(child, out);
        }
    }
}

/// The signal of a Qt Quick Control that is the DOM's `event` (dom-api.md §4).
fn signal_for(type_name: &str, event: &str) -> Option<&'static str> {
    Some(match (type_name, event) {
        ("Button" | "CheckBox" | "RadioButton", "click") => "onClicked",
        ("TextField", "input") => "onTextEdited",
        ("TextArea", "input") => "onTextChanged",
        ("TextField" | "TextArea", "change") => "onEditingFinished",
        ("CheckBox" | "RadioButton", "input" | "change") => "onToggled",
        ("ComboBox", "input" | "change") => "onActivated",
        ("Slider", "input") => "onMoved",
        ("Slider", "change") => "onPressedChanged",
        ("SpinBox", "input" | "change") => "onValueModified",
        _ => return None,
    })
}

/// Appends to `name`'s handler: QML allows one per signal.
pub(crate) fn add_handler(obj: &mut Object, name: &str, params: &[&str], body: Vec<String>) {
    for member in &mut obj.members {
        if let Member::Handler {
            name: existing,
            body: lines,
            ..
        } = member
            && existing == name
        {
            lines.extend(body);
            return;
        }
    }
    obj.members.push(Member::Handler {
        name: name.to_owned(),
        params: params.iter().map(|p| (*p).to_owned()).collect(),
        body,
    });
}

pub(crate) fn find_mut<'o>(obj: &'o mut Object, id: &str) -> Option<&'o mut Object> {
    if obj.id.as_deref() == Some(id) {
        return Some(obj);
    }
    for member in &mut obj.members {
        let found = match member {
            Member::Child(child) => find_mut(child, id),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn select(html: &str, selector: &str) -> Vec<String> {
        let doc = Document::parse(html);
        let mut diags = Diagnostics::new("t");
        resolve(
            &doc,
            &DomQuery::SelectorAll(selector.to_owned()),
            &mut diags,
        )
        .into_iter()
        .map(|n| describe(&doc, n))
        .collect()
    }

    #[test]
    fn simple_selectors_resolve() {
        let html = r#"<div id="a" class="x"><p class="x y">1</p><span><p>2</p></span></div><p data-k="v">3</p>"#;
        assert_eq!(select(html, "p.x"), [r#"<p class="x y">"#]);
        assert_eq!(select(html, "#a > p"), [r#"<p class="x y">"#]);
        assert_eq!(select(html, "#a p").len(), 2);
        assert_eq!(
            select(html, ".x"),
            [r#"<div id="a" class="x">"#, r#"<p class="x y">"#]
        );
        assert_eq!(select(html, "[data-k=v]"), ["<p>"]);
        assert_eq!(select(html, "div, span").len(), 2);
    }

    #[test]
    fn sibling_combinators_and_pseudo_classes_resolve() {
        let html = "<ul><li>1</li><li>2</li><li>3</li></ul>";
        assert_eq!(select(html, "li:first-child").len(), 1);
        assert_eq!(select(html, "li + li").len(), 2);
        assert_eq!(select(html, "li ~ li:last-child").len(), 1);
        assert_eq!(select(html, "li:not(:first-child)").len(), 2);
    }

    #[test]
    fn first_only_query_takes_the_first_match() {
        let doc = Document::parse("<p>1</p><p>2</p>");
        let mut diags = Diagnostics::new("t");
        let found = resolve(&doc, &DomQuery::Selector("p".to_owned()), &mut diags);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn invalid_selectors_are_reported_and_match_nothing() {
        let doc = Document::parse("<p>1</p>");
        let mut diags = Diagnostics::new("t");
        let found = resolve(&doc, &DomQuery::SelectorAll("p[".to_owned()), &mut diags);
        assert!(found.is_empty());
        assert_eq!(diags.into_vec().len(), 1);
    }
}
