//! Accessible names (WAI-ARIA Accessible Name and Description Computation
//! 1.2, §4.3.2): what a screen reader says for an element, which is not
//! always what it shows. `aria-labelledby` wins over `aria-label`, which wins
//! over a native label or the element's own content; hidden content is no
//! part of a name, and content marked `aria-hidden` is drawn but not read.

use eldur_css::{Display, StyleMap, Visibility};
use eldur_dom::{Document, NodeData, NodeId, collapse_whitespace};

/// Which content counts.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reading {
    /// What is drawn: everything but `display: none` and the elements that
    /// never render. A control's text is this.
    Drawn,
    /// What is read out: also without `aria-hidden="true"` and
    /// `visibility: hidden`, as AccName step 2A skips hidden nodes.
    Read,
    /// Everything but the elements that never render: a hidden element that
    /// `aria-labelledby` names is read whole, hidden parts and all (step 2A).
    Referenced,
}

/// `node`'s text as `reading` sees it, with `skip`'s subtree left out (the
/// control a label names is not part of its own name), whitespace collapsed.
pub(crate) fn content_text(
    doc: &Document,
    styles: &StyleMap,
    node: NodeId,
    skip: Option<NodeId>,
    reading: Reading,
) -> String {
    let mut out = String::new();
    collect(doc, styles, node, skip, reading, &mut out);
    collapse_whitespace(&out)
}

fn collect(
    doc: &Document,
    styles: &StyleMap,
    node: NodeId,
    skip: Option<NodeId>,
    reading: Reading,
    out: &mut String,
) {
    for child in doc.children(node) {
        if Some(child) == skip {
            continue;
        }
        match &doc.node(child).data {
            NodeData::Text(t) => {
                let shown = reading != Reading::Read
                    || styles
                        .computed(node)
                        .is_none_or(|c| c.style.visibility == Visibility::Visible);
                if shown {
                    out.push_str(t);
                }
            }
            NodeData::Element(e) => {
                if matches!(
                    e.local_name(),
                    "script" | "style" | "template" | "noscript" | "head" | "title"
                ) {
                    continue;
                }
                let style = styles.computed(child).map(|c| &c.style);
                if reading != Reading::Referenced
                    && style.is_some_and(|s| s.display == Display::None)
                {
                    continue;
                }
                if reading == Reading::Read && e.attr("aria-hidden") == Some("true") {
                    continue;
                }
                // A block-level box, a flex or grid item, or a line break
                // stands apart from its neighbours: "Go" and "bold" in
                // their own boxes read as "Go bold".
                let apart = e.local_name() == "br"
                    || style
                        .is_some_and(|s| !matches!(s.display, Display::Inline | Display::Contents))
                    || styles.computed(node).is_some_and(|c| {
                        matches!(
                            c.style.display,
                            Display::Flex
                                | Display::InlineFlex
                                | Display::Grid
                                | Display::InlineGrid
                        )
                    });
                if apart {
                    out.push(' ');
                }
                if reading != Reading::Drawn
                    && e.local_name() == "img"
                    && let Some(alt) = e.attr("alt")
                {
                    out.push_str(alt);
                } else {
                    collect(doc, styles, child, skip, reading, out);
                }
                if apart {
                    out.push(' ');
                }
            }
            _ => {}
        }
    }
}

/// The name `aria-labelledby` or `aria-label` gives `node`, before any native
/// label or content (AccName steps 2B and 2C).
pub(crate) fn aria_name(doc: &Document, styles: &StyleMap, node: NodeId) -> Option<String> {
    let element = doc.element(node)?;
    if let Some(ids) = element.attr("aria-labelledby") {
        let parts: Vec<String> = ids
            .split_ascii_whitespace()
            .filter_map(|id| doc.element_by_id(id))
            .map(|by| referenced_name(doc, styles, by))
            .filter(|t| !t.is_empty())
            .collect();
        if !parts.is_empty() {
            return Some(parts.join(" "));
        }
    }
    element
        .attr("aria-label")
        .map(collapse_whitespace)
        .filter(|t| !t.is_empty())
}

/// The name of an element `aria-labelledby` points at: its own `aria-label`
/// or `alt` first, then its content (steps 2C to 2F). Its own
/// `aria-labelledby` is not followed, so a traversal never loops (step 2B).
/// A hidden one still names, with what it hides inside; a shown one leaves
/// its hidden parts out.
fn referenced_name(doc: &Document, styles: &StyleMap, by: NodeId) -> String {
    let element = doc.element(by);
    let own = element
        .and_then(|e| {
            e.attr("aria-label")
                .filter(|t| !t.trim().is_empty())
                .or_else(|| (e.local_name() == "img").then(|| e.attr("alt")).flatten())
        })
        .map(collapse_whitespace);
    if let Some(own) = own {
        return own;
    }
    let reading = if hidden(doc, styles, by) {
        Reading::Referenced
    } else {
        Reading::Read
    };
    content_text(doc, styles, by, None, reading)
}

/// Whether `node` is out of the accessibility tree: not displayed, invisible,
/// or under `aria-hidden`, itself or through an ancestor.
fn hidden(doc: &Document, styles: &StyleMap, node: NodeId) -> bool {
    if styles
        .computed(node)
        .is_some_and(|c| c.style.visibility != Visibility::Visible)
    {
        return true;
    }
    std::iter::once(node).chain(doc.ancestors(node)).any(|n| {
        styles
            .computed(n)
            .is_some_and(|c| c.style.display == Display::None)
            || doc
                .element(n)
                .is_some_and(|e| e.attr("aria-hidden") == Some("true"))
    })
}
