//! Accessible names (WAI-ARIA Accessible Name and Description Computation
//! 1.2, §4.3.2): what a screen reader says for an element, which is not
//! always what it shows. `aria-labelledby` wins over `aria-label`, which wins
//! over a native label or the element's own content; hidden content is no
//! part of a name, and content marked `aria-hidden` is drawn but not read.

use eldur_css::{Display, StyleMap, Visibility};
use eldur_dom::{Document, NodeData, NodeId, collapse_whitespace};

use super::controls::labeled_control;

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
    text_of(doc, styles, node, skip, reading, false)
}

/// The same, `traversal` when it is the content of an element
/// `aria-labelledby` points at: a node in it does not follow its own
/// `aria-labelledby` (step 2B), so a traversal never loops.
fn text_of(
    doc: &Document,
    styles: &StyleMap,
    node: NodeId,
    skip: Option<NodeId>,
    reading: Reading,
    traversal: bool,
) -> String {
    let mut out = String::new();
    collect(doc, styles, node, skip, reading, traversal, &mut out);
    collapse_whitespace(&out)
}

fn collect(
    doc: &Document,
    styles: &StyleMap,
    node: NodeId,
    skip: Option<NodeId>,
    reading: Reading,
    traversal: bool,
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
                // What a node in a name says for itself: what its own
                // `aria-labelledby` points at, outside a traversal (step 2B),
                // its `aria-label` (2C), an image's `alt` (2D); one that is
                // not shown says nothing, unless it is read because it is
                // referenced.
                let own = (reading != Reading::Drawn)
                    .then(|| {
                        (!traversal)
                            .then(|| labelledby(doc, styles, child))
                            .flatten()
                            .or_else(|| {
                                e.attr("aria-label")
                                    .map(collapse_whitespace)
                                    .filter(|t| !t.is_empty())
                            })
                            .or_else(|| {
                                (e.local_name() == "img")
                                    .then(|| e.attr("alt").unwrap_or_default().to_owned())
                            })
                    })
                    .flatten();
                if let Some(own) = own {
                    let shown = reading != Reading::Read
                        || style.is_none_or(|s| s.visibility == Visibility::Visible);
                    if shown {
                        out.push_str(&own);
                    }
                } else {
                    collect(doc, styles, child, skip, reading, traversal, out);
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
    labelledby(doc, styles, node).or_else(|| {
        element
            .attr("aria-label")
            .map(collapse_whitespace)
            .filter(|t| !t.is_empty())
    })
}

/// What `node`'s `aria-labelledby` names it, when it names anything.
fn labelledby(doc: &Document, styles: &StyleMap, node: NodeId) -> Option<String> {
    let ids = doc.element(node)?.attr("aria-labelledby")?;
    let parts: Vec<String> = ids
        .split_ascii_whitespace()
        .filter_map(|id| doc.element_by_id(id))
        .map(|by| referenced_name(doc, styles, by))
        .filter(|t| !t.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// The name `control`'s `<label>`s give it (AccName step 2D, HTML-AAM): what
/// they read in document order, joined, without the control itself (a
/// `<select>` inside its label would otherwise lend it every option's text).
/// A label that reads nothing gives way to the next (HTML-AAM). A hidden label
/// still names its control, read whole as a referenced node is; a shown one
/// leaves out what it hides inside. `traversal` as for [`text_of`].
pub(crate) fn label_name(
    doc: &Document,
    styles: &StyleMap,
    control: NodeId,
    traversal: bool,
) -> Option<String> {
    let parts: Vec<String> = doc
        .descendants(doc.root())
        .filter(|&n| labeled_control(doc, n) == Some(control))
        .map(|label| {
            let reading = if hidden(doc, styles, label) {
                Reading::Referenced
            } else {
                Reading::Read
            };
            text_of(doc, styles, label, Some(control), reading, traversal)
        })
        .filter(|t| !t.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// The name of an element `aria-labelledby` points at: its own `aria-label`
/// or `alt` first, then a control's `<label>`, then its content (steps 2C to
/// 2F). Its own
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
    if let Some(own) = own.or_else(|| label_name(doc, styles, by, true)) {
        return own;
    }
    let reading = if hidden(doc, styles, by) {
        Reading::Referenced
    } else {
        Reading::Read
    };
    text_of(doc, styles, by, None, reading, true)
}

/// Whether `node` is out of the accessibility tree: not displayed, invisible,
/// or under `aria-hidden`, itself or through an ancestor.
pub(crate) fn hidden(doc: &Document, styles: &StyleMap, node: NodeId) -> bool {
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
