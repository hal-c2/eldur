//! What the runtime reads about elements and the page
//! (`docs/design/dynamic-styles.md` §7, §8.1): `h2qTag`, `h2qClasses`,
//! `cls_<s>`, `root.h2qStyledClasses` and `root.baseUrl`.

use std::collections::{BTreeMap, BTreeSet};

use h2q_dom::{Document, NodeId};
use h2q_qml::{Member, Object, Value};

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

/// Puts `h2qTag`, `h2qClasses` and, for an element with an HTML id,
/// `objectName` on the object of `node`, ahead of its other members.
pub(crate) fn tag(obj: &mut Object, doc: &Document, node: NodeId) {
    let Some(element) = doc.element(node) else {
        return;
    };
    let mut decls = vec![decl(
        "string",
        "h2qTag",
        Value::String(element.local_name().to_ascii_lowercase()),
    )];
    if let Some(classes) = element.attr("class")
        && !classes.trim().is_empty()
    {
        decls.push(decl(
            "string",
            "h2qClasses",
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
        .take_while(|m| matches!(m, Member::PropertyDecl { name, .. } if name.starts_with("h2q")))
        .count();
    obj.members.splice(at..at, decls);
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

/// `root.h2qStyledClasses` (sorted, as a `BTreeSet` iterates).
pub(crate) fn styled_classes_decl(classes: &BTreeSet<String>) -> Member {
    let list: Vec<String> = classes.iter().map(|c| format!("{c:?}")).collect();
    decl(
        "var",
        "h2qStyledClasses",
        Value::Expr(format!("[{}]", list.join(", "))),
    )
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
        let (sheet, _) = h2q_css::Stylesheet::parse(css, h2q_css::Origin::Author, "test.css", None);
        h2q_css::styled_classes(&[sheet]).into_iter().collect()
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
