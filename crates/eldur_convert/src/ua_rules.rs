//! The browser's own rules a page's runtime needs: those for elements the page
//! has or its scripts can make. The UA sheet has rules for every HTML element;
//! a page with a form and three paragraphs needs a dozen of them.

use std::collections::BTreeSet;

use eldur_dom::{Document, NodeId};

/// Element names that may be in the page while it runs, or `None` when any may.
pub(crate) struct PossibleTags(Option<BTreeSet<String>>);

impl PossibleTags {
    pub(crate) fn new(doc: &Document, analysis: &eldur_js::Analysis) -> Self {
        // Markup the analysis cannot read, or a tag it cannot name, can make anything.
        if analysis.open_elements || !analysis.opaque_inserts.is_empty() {
            return Self(None);
        }
        let mut tags = BTreeSet::new();
        add_tree(doc, doc.root(), &mut tags);
        tags.extend(analysis.created.iter().map(|e| e.tag.to_ascii_lowercase()));
        // What HTML makes without a script naming it: a table's implied rows and
        // sections (`insertRow`, the parser's `tbody`), `new Option()`, `new Image()`.
        if tags.contains("table") {
            for implied in [
                "caption", "col", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr",
            ] {
                tags.insert(implied.to_owned());
            }
        }
        for implied in ["html", "body", "option", "img", "audio"] {
            tags.insert(implied.to_owned());
        }
        Self(Some(tags))
    }

    /// Whether an element `selector` matches may be in the page. Only the
    /// selector's subject is read, and only its element name: a selector this
    /// does not follow may match.
    pub(crate) fn may_match(&self, selector: &str) -> bool {
        let Some(tags) = &self.0 else {
            return true;
        };
        split_top(selector, ',').iter().any(|one| {
            subject_names(subject(one)).is_none_or(|names| names.iter().any(|n| tags.contains(n)))
        })
    }

    /// Drops the UA rules whose element no page element can be.
    pub(crate) fn retain(&self, rules: &mut Vec<eldur_css::RuntimeRule>) {
        rules.retain(|r| !r.user_agent || self.may_match(&r.selector));
    }
}

fn add_tree(doc: &Document, from: NodeId, tags: &mut BTreeSet<String>) {
    for node in doc.descendants(from) {
        if let Some(element) = doc.element(node) {
            tags.insert(element.local_name().to_ascii_lowercase());
            // A template's contents are a fragment of their own, cloned at run time.
            if let Some(fragment) = element.template_contents {
                add_tree(doc, fragment, tags);
            }
        }
    }
}

/// Splits at `sep` outside brackets, parentheses and quotes.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote = None;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth -= 1,
            (None, c) if c == sep && depth == 0 => {
                parts.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// The last compound selector: what the selector matches.
fn subject(selector: &str) -> &str {
    let selector = selector.trim();
    let mut depth = 0i32;
    let mut quote = None;
    let mut start = 0;
    for (i, c) in selector.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') => depth -= 1,
            (None, ' ' | '>' | '+' | '~') if depth == 0 => start = i + 1,
            _ => {}
        }
    }
    &selector[start..]
}

/// The element names a compound can match, or `None` when it names none.
fn subject_names(compound: &str) -> Option<Vec<String>> {
    let name: String = compound
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if !name.is_empty() {
        return Some(vec![name.to_ascii_lowercase()]);
    }
    // `:is(a, b)` and `:where(a, b)` at the start name what their list does.
    let inner = compound
        .strip_prefix(":is(")
        .or_else(|| compound.strip_prefix(":where("))?;
    let close = matching_paren(inner)?;
    let mut names = Vec::new();
    for one in split_top(&inner[..close], ',') {
        names.extend(subject_names(subject(one))?);
    }
    Some(names)
}

fn matching_paren(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(i),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(names: &[&str]) -> PossibleTags {
        PossibleTags(Some(names.iter().map(|s| (*s).to_owned()).collect()))
    }

    #[test]
    fn a_rule_for_an_element_the_page_lacks_goes() {
        let page = tags(&["p", "summary", "ul"]);
        assert!(page.may_match("p"));
        assert!(!page.may_match("area"));
        assert!(!page.may_match("input[type=\"hidden\" i]"));
        assert!(page.may_match("details > summary:first-of-type"));
        assert!(!page.may_match("q::before"));
        assert!(page.may_match(":is(dir, dl, menu, ol, ul) :is(dir, menu, ol, ul)"));
        assert!(!page.may_match(":is(dir, dl) :is(dir, menu)"));
        assert!(page.may_match("[hidden]:not([hidden=\"until-found\" i])"));
        assert!(page.may_match("*"));
        assert!(page.may_match("area, p"));
    }

    #[test]
    fn any_tag_keeps_every_rule() {
        assert!(PossibleTags(None).may_match("area"));
    }

    #[test]
    fn tags_come_from_the_page_its_templates_and_its_scripts() {
        let doc = Document::parse("<p>a</p><template><table></table></template>");
        let mut analysis = eldur_js::Analysis::default();
        let page = PossibleTags::new(&doc, &analysis);
        assert!(page.may_match("p") && page.may_match("table") && page.may_match("td"));
        assert!(!page.may_match("dialog"));
        analysis.open_elements = true;
        assert!(PossibleTags::new(&doc, &analysis).may_match("dialog"));
    }
}
