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
    let props: BTreeSet<String> = classes.iter().map(|c| class_prop(c)).collect();
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

/// Classes some selector in `css` names.
///
/// TODO: switch to `h2q_css::styled_classes` once it lands (dynamic-styles
/// §9.2); this scans the text because `Stylesheet` keeps its selectors
/// private. It reads every rule's prelude, outside comments and strings, and
/// takes each `.ident` there. Declaration blocks are skipped, so `.5em` and
/// `url(a.png)` are never read as classes.
pub(crate) fn styled_classes<'t>(css: impl IntoIterator<Item = &'t str>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for text in css {
        scan(text, &mut found);
    }
    found
}

fn scan(text: &str, found: &mut BTreeSet<String>) {
    let chars: Vec<char> = text.chars().collect();
    // Whether each open block holds rules (the top level, `@media`, …)
    // rather than declarations.
    let mut blocks: Vec<bool> = Vec::new();
    let mut prelude = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
                prelude.push(' ');
                continue;
            }
            '"' | '\'' => {
                i += 1;
                while i < chars.len() && chars[i] != c {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                prelude.push(' ');
            }
            '\\' => {
                prelude.push(c);
                if let Some(&next) = chars.get(i + 1) {
                    prelude.push(next);
                }
                i += 1;
            }
            '{' => {
                let in_rules = blocks.last().copied().unwrap_or(true);
                let at_rule = prelude.trim_start().starts_with('@');
                if in_rules && !at_rule {
                    classes_in(&prelude, found);
                }
                // `@media`, `@supports`, `@layer`, `@container` and
                // `@document` hold rules; `@font-face`, `@page` and style
                // rules hold declarations (`@keyframes` selectors name no
                // classes either way).
                blocks.push(in_rules && at_rule && !is_declaration_at_rule(&prelude));
                prelude.clear();
            }
            '}' => {
                blocks.pop();
                prelude.clear();
            }
            ';' => prelude.clear(),
            _ => prelude.push(c),
        }
        i += 1;
    }
}

fn is_declaration_at_rule(prelude: &str) -> bool {
    let name: String = prelude
        .trim_start()
        .trim_start_matches('@')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    matches!(
        name.to_ascii_lowercase().as_str(),
        "font-face" | "page" | "property" | "counter-style" | "font-feature-values"
    )
}

/// Each `.ident` in a selector list (CSS Syntax 3 §4.3.11 ident code
/// points; escapes are decoded only for the common `\<char>` form).
fn classes_in(selectors: &str, found: &mut BTreeSet<String>) {
    let chars: Vec<char> = selectors.chars().collect();
    let ident_char = |c: char| c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '.' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut name = String::new();
        while j < chars.len() {
            if chars[j] == '\\' && j + 1 < chars.len() {
                name.push(chars[j + 1]);
                j += 2;
            } else if ident_char(chars[j]) {
                name.push(chars[j]);
                j += 1;
            } else {
                break;
            }
        }
        let starts_ok = name.chars().next().is_some_and(|c| !c.is_ascii_digit())
            && !(name.starts_with('-') && name.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));
        if starts_ok {
            found.insert(name);
        }
        i = j.max(i + 1);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(css: &str) -> Vec<String> {
        styled_classes([css]).into_iter().collect()
    }

    #[test]
    fn selectors_name_classes_and_blocks_do_not() {
        assert_eq!(
            classes(
                ".a, div.b > .c:not(.d) { margin: .5em; background: url(x.y.png) }\
                 /* .no */ @media (min-width: 1.5em) { .e:hover { color: red } }\
                 [title='.no'] .f { }\
                 @font-face { src: url(a.woff) }\
                 @keyframes k { 0% { opacity: .5 } }\
                 .g\\:h { }"
            ),
            ["a", "b", "c", "d", "e", "f", "g:h"]
        );
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
}
