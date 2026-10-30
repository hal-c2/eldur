//! Steps 3 and 6: the page's scripts, the elements they address, and the
//! wiring of `page.js` into `Main.qml`.

use std::collections::{BTreeMap, HashMap, HashSet};

use h2q_dom::{Document, NodeId};
use h2q_js::{
    Analysis, DomQuery, DomResolver, ElementKey, JsOutput, QmlTarget, ScriptInput, ScriptKind,
};
use h2q_qml::{Member, Object};

use crate::Resources;
use crate::boxes::{describe, node_line};
use crate::diag::Diagnostics;
use crate::emit::Target;

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
}

impl Scripts {
    pub(crate) fn is_empty(&self) -> bool {
        self.inputs.is_empty()
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

    let analysis = h2q_js::analyze(&inputs);
    let mut matches = HashMap::new();
    let mut wanted: HashSet<NodeId> = keys.values().copied().collect();
    for query in &analysis.queries {
        let found = resolve(doc, query, diags);
        wanted.extend(found.iter().copied());
        matches.insert(query.clone(), found);
    }
    Scripts {
        inputs,
        analysis,
        keys,
        matches,
        wanted,
    }
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
    let Some(mut found) = h2q_css::query_selector_all(doc, text) else {
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
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(scripts: &'a Scripts, targets: &'a BTreeMap<NodeId, Target>) -> Self {
        Self { scripts, targets }
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
        self.scripts.keys.get(&key.0).and_then(|&n| self.target(n))
    }
}

/// Reports elements scripts refer to that got no QML object.
pub(crate) fn report_missing(
    doc: &Document,
    scripts: &Scripts,
    targets: &BTreeMap<NodeId, Target>,
    diags: &mut Diagnostics,
) {
    let mut missing: Vec<NodeId> = scripts
        .wanted
        .iter()
        .copied()
        .filter(|n| !targets.contains_key(n))
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

/// Adds the signal handlers of `js.bindings` to the objects they target.
pub(crate) fn wire(root: &mut Object, js: &JsOutput, diags: &mut Diagnostics) {
    for binding in &js.bindings {
        let id = &binding.target.qml_id;
        let handler = match binding.event.as_str() {
            "click" => {
                let mut tap = Object::new("TapHandler");
                tap.members.push(Member::Handler {
                    name: "onTapped".to_owned(),
                    params: vec!["eventPoint".to_owned(), "button".to_owned()],
                    body: vec![format!(
                        "Page.{}(H2Q.event(\"click\", {id}, eventPoint));",
                        binding.function
                    )],
                });
                tap
            }
            other => {
                diags.warning(
                    "js",
                    None,
                    format!("`{other}` listeners on `{id}` are not wired to QML yet"),
                );
                continue;
            }
        };
        match find_mut(root, id) {
            Some(obj) => {
                obj.push_child(handler);
            }
            None => diags.error(
                "qml",
                None,
                format!("event binding targets `{id}`, which is not in Main.qml"),
            ),
        }
    }
}

fn find_mut<'o>(obj: &'o mut Object, id: &str) -> Option<&'o mut Object> {
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
