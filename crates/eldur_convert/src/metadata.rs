//! What the runtime reads about elements and the page
//! (`docs/design/dynamic-styles.md` §7, §8.1): `eldurTag`, `eldurClasses`,
//! `cls_<s>`, `root.eldurStyledClasses` and `root.baseUrl`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use eldur_dom::{Document, NodeId};
use eldur_qml::{Member, Object, Value};

/// `property <ty> <name>: <value>`.
fn decl(ty: &str, name: &str, value: Value) -> Member {
    Member::PropertyDecl {
        ty: ty.to_owned(),
        name: name.to_owned(),
        value: Some(value),
        readonly: false,
        required: false,
    }
}

/// The runtime's `_sanitizeClassProp`: `cls_` and the class with every
/// UTF-16 unit outside `[A-Za-z0-9_]` replaced by `_`.
pub(crate) fn class_prop(class: &str) -> String {
    let mut prop = String::from("cls_");
    for c in class.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            prop.push(c);
        } else {
            prop.extend(std::iter::repeat_n('_', c.len_utf16()));
        }
    }
    prop
}

/// Puts `eldurTag`, `eldurClasses` and, for an element with an HTML id,
/// `objectName` on the object of `node`, ahead of its other members.
/// Whitespace-only text between elements makes no item, but scripts read it
/// (`textContent`, `childNodes`): an element keeps what comes before it, and
/// what comes after its last child. See `docs/spec/dom-api.md`.
pub(crate) fn declare_whitespace(
    obj: &mut Object,
    doc: &Document,
    node: NodeId,
    live: &HashSet<NodeId>,
) {
    let mut before = whitespace_back(doc, doc.prev_siblings(node), live);
    let mut up = node;
    // First in a row that has no item: what is before the row counts too.
    while before.is_empty_run()
        && let Some(parent) = doc.parent_element(up)
        && has_no_item(doc, parent, live)
    {
        let more = whitespace_back(doc, doc.prev_siblings(parent), live);
        before.text.insert_str(0, &more.text);
        before.stopped = more.stopped;
        up = parent;
    }
    if !before.text.is_empty() {
        obj.members
            .push(decl("string", "eldurWs", Value::String(before.text)));
    }
    let end = whitespace_back(doc, doc.children(node).rev(), live);
    if !end.text.is_empty() && doc.element_children(node).next().is_some() {
        obj.members
            .push(decl("string", "eldurWsEnd", Value::String(end.text)));
    }
}

struct WhitespaceRun {
    text: String,
    /// An item or real text ended it, so nothing further back joins it.
    stopped: bool,
}

impl WhitespaceRun {
    fn is_empty_run(&self) -> bool {
        !self.stopped
    }
}

/// The whitespace in `nodes`, nearest first, up to an element with an item or
/// real text; an element without an item is looked through.
fn whitespace_back(
    doc: &Document,
    nodes: impl Iterator<Item = NodeId>,
    live: &HashSet<NodeId>,
) -> WhitespaceRun {
    let mut text = String::new();
    for node in nodes {
        if let Some(t) = doc.text(node) {
            if !t.trim().is_empty() {
                return WhitespaceRun {
                    text,
                    stopped: true,
                };
            }
            text.insert_str(0, t);
        } else if doc.element(node).is_some() {
            if !has_no_item(doc, node, live) {
                return WhitespaceRun {
                    text,
                    stopped: true,
                };
            }
            let inner = whitespace_back(doc, doc.children(node).rev(), live);
            text.insert_str(0, &inner.text);
            if inner.stopped {
                return WhitespaceRun {
                    text,
                    stopped: true,
                };
            }
        }
    }
    WhitespaceRun {
        text,
        stopped: false,
    }
}

/// A table's rows and the groups around them are cells of one grid; a script,
/// style and the like make no box at all.
fn has_no_item(doc: &Document, node: NodeId, live: &HashSet<NodeId>) -> bool {
    let Some(element) = doc.element(node) else {
        return false;
    };
    let in_grid = || !doc.ancestors(node).any(|a| live.contains(&a));
    match element.local_name() {
        "tr" => in_grid(),
        "thead" | "tbody" | "tfoot" => doc.element_children(node).next().is_some() && in_grid(),
        "script" | "style" | "link" | "meta" | "template" | "title" => true,
        _ => false,
    }
}

pub(crate) fn tag(obj: &mut Object, doc: &Document, node: NodeId) {
    let Some(element) = doc.element(node) else {
        return;
    };
    let mut decls = vec![decl(
        "string",
        "eldurTag",
        Value::String(element.local_name().to_ascii_lowercase()),
    )];
    if element.local_name() == "details" && element.has_attr("open") {
        decls.push(decl("bool", "eldurOpen", Value::Bool(true)));
    }
    if let Some(classes) = element.attr("class")
        && !classes.trim().is_empty()
    {
        decls.push(decl(
            "string",
            "eldurClasses",
            Value::String(classes.to_owned()),
        ));
    }
    obj.members.splice(0..0, decls);
    // Runtime `getElementById` and `#id` selectors find elements by it.
    if let Some(id) = element.id()
        && obj.get("objectName").is_none()
    {
        obj.set("objectName", Value::String(id.to_owned()));
    }
}

/// `pointer-events: none`, computed: what the runtime's hit test looks through.
pub(crate) fn declare_no_hit(obj: &mut Object) {
    obj.members
        .insert(0, decl("bool", "eldurNoHit", Value::Bool(true)));
}

/// `eldurStyle`: a `style` attribute as written, which the cascade of what a
/// script makes inherits from.
pub(crate) fn declare_style(obj: &mut Object, doc: &Document, node: NodeId) {
    let Some(style) = doc.element(node).and_then(|e| e.attr("style")) else {
        return;
    };
    if !style.trim().is_empty() {
        obj.members
            .insert(0, decl("string", "eldurStyle", Value::string(style)));
    }
}

/// An element whose attributes the runtime acts on when it hears the person
/// itself: what a control validates by, what a link follows, what takes a stop
/// in the focus order or opens a dialog.
pub(crate) fn acts_on_attrs(doc: &Document, node: NodeId) -> bool {
    doc.element(node).is_some_and(|e| {
        matches!(
            e.local_name(),
            "form"
                | "input"
                | "select"
                | "textarea"
                | "button"
                | "label"
                | "fieldset"
                | "a"
                | "dialog"
                | "details"
        ) || e.attr("tabindex").is_some()
            || e.attr("popover").is_some()
    })
}

/// `eldurAttrs`: what the markup says, for scripts that ask. `id` and `class`
/// have their own properties, `style` was applied, `on*` became handlers.
pub(crate) fn declare_attrs(obj: &mut Object, doc: &Document, node: NodeId) {
    let Some(element) = doc.element(node) else {
        return;
    };
    let pairs: Vec<String> = element
        .attrs
        .iter()
        .filter(|a| {
            let name: &str = &a.name.local;
            !matches!(name, "id" | "class" | "style") && !name.starts_with("on")
        })
        .map(|a| {
            let name: &str = &a.name.local;
            format!("{}: {}", js_string(name), js_string(&a.value))
        })
        .collect();
    if pairs.is_empty() {
        return;
    }
    obj.members.insert(
        0,
        decl(
            "var",
            "eldurAttrs",
            Value::Expr(format!("({{ {} }})", pairs.join(", "))),
        ),
    );
}

fn js_string(text: &str) -> String {
    let mut out = String::from("\"");
    for unit in text.chars() {
        match unit {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 || matches!(c, '\u{2028}' | '\u{2029}') => {
                out.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Declares `cls_<s>` on `obj` for each of `classes` (the names scripts
/// write on it), true when the element has the class in the document.
pub(crate) fn declare_classes(
    obj: &mut Object,
    doc: &Document,
    node: NodeId,
    classes: &BTreeSet<String>,
) {
    let has: BTreeSet<String> = doc
        .element(node)
        .map(|e| e.classes().map(class_prop).collect())
        .unwrap_or_default();
    // A class a condition binds to may be declared already.
    let declared: BTreeSet<&str> = obj
        .members
        .iter()
        .filter_map(|m| match m {
            Member::PropertyDecl { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    let props: BTreeSet<String> = classes
        .iter()
        .map(|c| class_prop(c))
        .filter(|p| !declared.contains(p.as_str()))
        .collect();
    let decls = props.into_iter().map(|prop| {
        let initial = has.contains(&prop);
        decl("bool", &prop, initial.into())
    });
    let at = obj
        .members
        .iter()
        .take_while(|m| matches!(m, Member::PropertyDecl { name, .. } if name.starts_with("eldur")))
        .count();
    obj.members.splice(at..at, decls);
}

/// `eldurIndex`/`eldurCount` (and, with `of_type`, the same-tag
/// `eldurTypeIndex`/`eldurTypeCount`): `node`'s 1-based place among its element
/// siblings, which the runtime keeps up to date as scripts change them.
pub(crate) fn declare_position(obj: &mut Object, doc: &Document, node: NodeId, of_type: bool) {
    let (place, type_place) = position(doc, node);
    let mut wanted = vec![("eldurIndex", place.0), ("eldurCount", place.1)];
    if of_type {
        wanted.extend([
            ("eldurTypeIndex", type_place.0),
            ("eldurTypeCount", type_place.1),
        ]);
    }
    let declared: BTreeSet<String> = obj
        .members
        .iter()
        .filter_map(|m| match m {
            Member::PropertyDecl { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let decls: Vec<Member> = wanted
        .into_iter()
        .filter(|(name, _)| !declared.contains(*name))
        .map(|(name, value)| Member::PropertyDecl {
            ty: "int".to_owned(),
            name: name.to_owned(),
            value: Some(Value::Number(f64::from(value))),
            readonly: false,
            required: false,
        })
        .collect();
    let at = obj
        .members
        .iter()
        .take_while(|m| matches!(m, Member::PropertyDecl { name, .. } if name.starts_with("eldur")))
        .count();
    obj.members.splice(at..at, decls);
}

/// `node`'s (index, count) among its parent's element children, and among
/// those with its tag.
pub(crate) fn position(doc: &Document, node: NodeId) -> ((u32, u32), (u32, u32)) {
    let tag = |n: NodeId| doc.element(n).map(|e| e.local_name().to_ascii_lowercase());
    let Some(parent) = doc.parent(node) else {
        return ((1, 1), (1, 1));
    };
    let kids: Vec<NodeId> = doc.element_children(parent).collect();
    let own = tag(node);
    let count = |list: &[NodeId]| u32::try_from(list.len()).unwrap_or(u32::MAX);
    let at = kids.iter().position(|&k| k == node).unwrap_or(0);
    let same: Vec<NodeId> = kids.iter().copied().filter(|&k| tag(k) == own).collect();
    let type_at = same.iter().position(|&k| k == node).unwrap_or(0);
    (
        (count(&kids[..=at]), count(&kids)),
        (count(&same[..=type_at]), count(&same)),
    )
}

/// Class names that share one `cls_` property, by property.
pub(crate) fn collisions<'c>(
    classes: impl IntoIterator<Item = &'c str>,
) -> BTreeMap<String, BTreeSet<&'c str>> {
    let mut by_prop: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for class in classes {
        by_prop.entry(class_prop(class)).or_default().insert(class);
    }
    by_prop.retain(|_, names| names.len() > 1);
    by_prop
}

/// `root.eldurStyledClasses` (sorted, as a `BTreeSet` iterates).
pub(crate) fn styled_classes_decl(classes: &BTreeSet<String>) -> Member {
    let list: Vec<String> = classes.iter().map(|c| format!("{c:?}")).collect();
    decl(
        "var",
        "eldurStyledClasses",
        Value::Expr(format!("[{}]", list.join(", "))),
    )
}

/// `root.eldurOpenElements`: the runtime styles created elements from the
/// page's rules, because analysis cannot name them all.
pub(crate) fn open_elements_decl() -> Member {
    decl("bool", "eldurOpenElements", Value::Bool(true))
}

/// `root.eldurNoMotion`: converted without animations, so the runtime starts no
/// transition and no CSS animation either.
pub(crate) fn no_motion_decl() -> Member {
    decl("bool", "eldurNoMotion", Value::Bool(true))
}

/// `root.eldurControlFonts`: what sizes a created control, the converter's table.
pub(crate) fn control_fonts_decl() -> Member {
    decl(
        "string",
        "eldurControlFonts",
        Value::string(crate::emit::control_fonts_json()),
    )
}

/// `root.eldurLiteralClasses`: the classes a script names outright, which the
/// converter gave variants. One it composes at run time has none it can trust.
pub(crate) fn literal_classes_decl(classes: &BTreeSet<&str>) -> Member {
    let list: Vec<String> = classes.iter().map(|c| format!("{c:?}")).collect();
    decl(
        "var",
        "eldurLiteralClasses",
        Value::Expr(format!("[{}]", list.join(", "))),
    )
}

/// `root.eldurInlineRules`: what the runtime re-evaluates on inline elements a
/// script changes. A string, parsed once, as the page may have many rules.
pub(crate) fn inline_rules_decl(rules: &[eldur_css::InlineRule]) -> Member {
    let list: Vec<serde_json::Value> = rules
        .iter()
        .map(|r| {
            serde_json::json!({
                "sel": r.selector,
                "spec": [r.specificity.0, r.specificity.1, r.specificity.2],
                "ua": r.user_agent,
                "decls": r.declarations,
            })
        })
        .collect();
    decl(
        "string",
        "eldurInlineRules",
        Value::String(serde_json::Value::Array(list).to_string()),
    )
}

/// `root.eldurPseudoRules`: what `::before` and `::after` generate, for the
/// runtime to follow classes a script sets.
pub(crate) fn pseudo_rules_decl(rules: &[eldur_css::PseudoRule]) -> Member {
    let list: Vec<serde_json::Value> = rules
        .iter()
        .map(|r| {
            let parts: Vec<serde_json::Value> = r
                .content
                .iter()
                .map(|p| match p {
                    eldur_css::PseudoPart::Text(t) => serde_json::json!(t),
                    eldur_css::PseudoPart::Attr(a) => serde_json::json!({ "attr": a }),
                })
                .collect();
            serde_json::json!({
                "sel": r.selector,
                "after": r.after,
                "spec": [r.specificity.0, r.specificity.1, r.specificity.2],
                "parts": parts,
            })
        })
        .collect();
    decl(
        "string",
        "eldurPseudoRules",
        Value::String(serde_json::Value::Array(list).to_string()),
    )
}

/// The same property with all a created element can use (`eldur_runtime.js`
/// reads either).
pub(crate) fn runtime_rules_decl(rules: &[eldur_css::RuntimeRule]) -> Member {
    decl(
        "string",
        "eldurInlineRules",
        Value::String(runtime_rules_json(rules)),
    )
}

/// `root.eldurHtmlTemplates`: each `<template id>`'s contents as markup. Nothing
/// else in the output keeps them: they make no box.
pub(crate) fn html_templates_decl(doc: &eldur_dom::Document) -> Option<Member> {
    let mut templates = serde_json::Map::new();
    for node in doc.descendants(doc.root()) {
        if let Some(id) = doc.element(node).and_then(eldur_dom::Element::id)
            && let Some(markup) = doc.template_markup(node)
        {
            templates.insert(id.to_owned(), markup.into());
        }
    }
    (!templates.is_empty()).then(|| {
        decl(
            "string",
            "eldurHtmlTemplates",
            Value::String(serde_json::Value::Object(templates).to_string()),
        )
    })
}

/// `root.eldurCascadeRules`: the same on a page whose elements all have
/// templates, for what the runtime computes without restyling any: margins
/// around created elements and `getComputedStyle`.
pub(crate) fn cascade_rules_decl(rules: &[eldur_css::RuntimeRule]) -> Member {
    decl(
        "string",
        "eldurCascadeRules",
        Value::String(runtime_rules_json(rules)),
    )
}

/// `root.eldurKeyframes`: what each `@keyframes` says, for the runtime's clock.
pub(crate) fn keyframes_decl(
    keyframes: &std::collections::BTreeMap<String, Vec<eldur_css::Keyframe>>,
) -> Member {
    let rules: serde_json::Map<String, serde_json::Value> = keyframes
        .iter()
        .map(|(name, frames)| {
            let frames = frames
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "o": f.offset,
                        "d": f.declarations,
                        "e": f.easing.as_ref().map(easing_text),
                    })
                })
                .collect();
            (name.clone(), serde_json::Value::Array(frames))
        })
        .collect();
    decl(
        "string",
        "eldurKeyframes",
        Value::String(serde_json::Value::Object(rules).to_string()),
    )
}

fn easing_text(easing: &eldur_css::Easing) -> String {
    match easing {
        eldur_css::Easing::CubicBezier(a, b, c, d) => format!("cubic-bezier({a}, {b}, {c}, {d})"),
        eldur_css::Easing::Steps(n, true) => format!("steps({n}, start)"),
        eldur_css::Easing::Steps(n, false) => format!("steps({n}, end)"),
        eldur_css::Easing::Linear => "linear".to_owned(),
    }
}

fn runtime_rules_json(rules: &[eldur_css::RuntimeRule]) -> String {
    let list: Vec<serde_json::Value> = rules
        .iter()
        .map(|r| {
            let mut rule = serde_json::json!({
                "sel": r.selector,
                "spec": [r.specificity.0, r.specificity.1, r.specificity.2],
                "ua": r.user_agent,
                "decls": r.declarations,
            });
            if let Some(layer) = r.layer {
                rule["layer"] = layer.into();
            }
            rule
        })
        .collect();
    serde_json::Value::Array(list).to_string()
}

/// `root.baseUrl` (dynamic-styles §14, decision 2): an `http(s)` page's
/// own URL, a local page's converted `Main.qml`, and `<base href>`
/// resolved against either.
pub(crate) fn base_url(doc: &Document, page_url: &str) -> Value {
    let page = url::Url::parse(page_url)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"));
    // HTML §2.5.1: the first `base` element with an `href`.
    let href = doc
        .descendants(doc.root())
        .filter_map(|n| doc.element(n))
        .find(|e| e.is_html() && e.local_name() == "base" && e.attr("href").is_some())
        .and_then(|e| e.attr("href"))
        .map(str::trim);
    match (page, href) {
        (Some(page), Some(href)) => match page.join(href) {
            Ok(resolved) => Value::String(resolved.into()),
            Err(_) => Value::String(page.into()),
        },
        (Some(page), None) => Value::String(page.into()),
        (None, Some(href)) => match url::Url::parse(href) {
            Ok(absolute) => Value::String(absolute.into()),
            // The converted app stands where the page did, so a relative
            // base resolves next to `Main.qml`.
            Err(_) => Value::Expr(format!("Qt.resolvedUrl({href:?})")),
        },
        (None, None) => Value::Expr("Qt.resolvedUrl(\"Main.qml\")".to_owned()),
    }
}

pub(crate) fn base_url_decl(doc: &Document, page_url: &str) -> Member {
    decl("string", "baseUrl", base_url(doc, page_url))
}

/// `root.documentUrl`, what `location` reports: an `http(s)` page's own
/// URL, a local page's converted `Main.qml`. Unlike `baseUrl`, `<base href>`
/// does not move it: it sets a document's base URL, not its URL.
pub(crate) fn document_url(page_url: &str) -> Value {
    match url::Url::parse(page_url) {
        Ok(page) if matches!(page.scheme(), "http" | "https") => Value::String(page.into()),
        _ => Value::Expr("Qt.resolvedUrl(\"Main.qml\")".to_owned()),
    }
}

pub(crate) fn document_url_decl(page_url: &str) -> Member {
    decl("string", "documentUrl", document_url(page_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(css: &str) -> Vec<String> {
        let (sheet, _) =
            eldur_css::Stylesheet::parse(css, eldur_css::Origin::Author, "test.css", None);
        eldur_css::styled_classes(&[sheet]).into_iter().collect()
    }

    #[test]
    fn selectors_name_classes_and_blocks_do_not() {
        assert_eq!(
            classes(
                ".a, div.b > .c:not(.d) { margin: .5em; background: url(x.y.png) }\
                 /* .no */ @media (min-width: 1.5em) { .e:hover { color: red } }\
                 [title='.no'] .f { color: red }\
                 @font-face { src: url(a.woff) }\
                 @keyframes k { 0% { opacity: .5 } }\
                 .g\\:h { color: red }"
            ),
            ["a", "b", "c", "d", "e", "f", "g:h"]
        );
        // The parser drops a rule without declarations: it styles nothing.
        assert!(classes(".empty { }").is_empty());
    }

    #[test]
    fn class_props_follow_the_runtime() {
        assert_eq!(class_prop("is-done"), "cls_is_done");
        assert_eq!(class_prop("a_b9"), "cls_a_b9");
        assert_eq!(class_prop("md:flex"), "cls_md_flex");
        // JavaScript replaces each UTF-16 unit.
        assert_eq!(class_prop("é😀"), "cls____");
    }

    #[test]
    fn colliding_classes_are_grouped() {
        let found = collisions(["a-b", "a_b", "c", "a-b"]);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found["cls_a_b"].iter().copied().collect::<Vec<_>>(),
            ["a-b", "a_b"]
        );
    }

    #[test]
    fn the_document_url_ignores_base_and_local_pages_are_the_app() {
        assert_eq!(
            document_url("https://example.com/docs/guide/index.html"),
            Value::String("https://example.com/docs/guide/index.html".to_owned())
        );
        assert_eq!(
            document_url("index.html"),
            Value::Expr("Qt.resolvedUrl(\"Main.qml\")".to_owned())
        );
        assert_eq!(
            document_url("file:///home/me/page/index.html"),
            Value::Expr("Qt.resolvedUrl(\"Main.qml\")".to_owned())
        );
    }
}
