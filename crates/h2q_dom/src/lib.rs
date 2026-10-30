//! HTML5 documents parsed by html5ever into an arena DOM.
//!
//! html5ever drives a [`TreeSink`]; the tree builder needs handles that own
//! their element names (`elem_name` borrows from the handle), so parsing
//! builds a small `Rc` tree and [`Document::parse`] flattens it into an arena
//! of [`Node`]s addressed by [`NodeId`]. Everything downstream (CSS matching,
//! conversion) works on the arena: cheap `Copy` ids, no `RefCell`.

mod mutate;
mod sink;

use std::fmt;

pub use mutate::DomError;

pub use html5ever::{LocalName, Namespace, QualName, ns};

static ROOT_SIBLINGS: [NodeId; 1] = [NodeId(0)];

/// Index of a node in its [`Document`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }

    fn new(index: usize) -> Self {
        Self(u32::try_from(index).expect("more than u32::MAX DOM nodes"))
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// An attribute as written in the source, name lower-cased by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: QualName,
    pub value: String,
}

/// An element: its name, attributes and where it started in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: QualName,
    pub attrs: Vec<Attribute>,
    /// 1-based line of the start tag.
    pub line: u64,
    /// For `<template>`: the document fragment holding its contents.
    pub template_contents: Option<NodeId>,
}

impl Element {
    /// Local name, lower case for HTML elements (`div`, `svg`, `foreignObject`).
    #[must_use]
    pub fn local_name(&self) -> &str {
        &self.name.local
    }

    #[must_use]
    pub fn is_html(&self) -> bool {
        self.name.ns == ns!(html)
    }

    #[must_use]
    pub fn is_svg(&self) -> bool {
        self.name.ns == ns!(svg)
    }

    /// Value of the attribute with this local name (no namespace), if any.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name.ns == ns!() && &*a.name.local == name)
            .map(|a| a.value.as_str())
    }

    #[must_use]
    pub fn has_attr(&self, name: &str) -> bool {
        self.attr(name).is_some()
    }

    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.attr("id").filter(|id| !id.is_empty())
    }

    /// The `class` attribute split on ASCII whitespace.
    pub fn classes(&self) -> impl Iterator<Item = &str> {
        self.attr("class").unwrap_or("").split_ascii_whitespace()
    }

    #[must_use]
    pub fn has_class(&self, class: &str) -> bool {
        self.classes().any(|c| c == class)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeData {
    Document,
    /// A `<template>`'s contents.
    Fragment,
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    Element(Element),
    Text(String),
    Comment(String),
    ProcessingInstruction {
        target: String,
        data: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub data: NodeData,
}

/// Quirks mode as the parser decided it from the doctype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuirksMode {
    #[default]
    NoQuirks,
    LimitedQuirks,
    Quirks,
}

/// A parse error html5ever reported. HTML parsing never fails; these are
/// what a validator would complain about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: u64,
    pub message: String,
}

/// A parsed HTML document. Node 0 is the document node.
#[derive(Debug, Clone)]
pub struct Document {
    nodes: Vec<Node>,
    pub quirks_mode: QuirksMode,
    pub errors: Vec<ParseError>,
}

impl Document {
    /// Parses a full document, as a browser would (implied `<html>`,
    /// `<head>`, `<body>`, misnested tags fixed, and so on).
    #[must_use]
    pub fn parse(html: &str) -> Self {
        sink::parse_document(html)
    }

    /// Parses a fragment as the children of a `<body>`. The returned
    /// document's root is the document node; its only element child is an
    /// `<html>` element holding the fragment's nodes (html5ever's shape).
    #[must_use]
    pub fn parse_fragment(html: &str) -> Self {
        sink::parse_fragment(html)
    }

    #[must_use]
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    #[must_use]
    pub fn element(&self, id: NodeId) -> Option<&Element> {
        match &self.node(id).data {
            NodeData::Element(e) => Some(e),
            _ => None,
        }
    }

    #[must_use]
    pub fn text(&self, id: NodeId) -> Option<&str> {
        match &self.node(id).data {
            NodeData::Text(t) => Some(t),
            _ => None,
        }
    }

    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent
    }

    /// The nearest ancestor that is an element.
    #[must_use]
    pub fn parent_element(&self, id: NodeId) -> Option<NodeId> {
        self.ancestors(id).find(|&a| self.element(a).is_some())
    }

    pub fn children(&self, id: NodeId) -> impl DoubleEndedIterator<Item = NodeId> + '_ {
        self.node(id).children.iter().copied()
    }

    pub fn element_children(&self, id: NodeId) -> impl DoubleEndedIterator<Item = NodeId> + '_ {
        self.children(id).filter(|&c| self.element(c).is_some())
    }

    /// Strict ancestors, nearest first.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(self.parent(id), |&p| self.parent(p))
    }

    /// `id` and everything under it, in document (pre-)order.
    pub fn descendants(&self, id: NodeId) -> Descendants<'_> {
        Descendants {
            doc: self,
            stack: vec![id],
        }
    }

    /// Siblings before `id`, nearest first.
    pub fn prev_siblings(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let (siblings, at) = self.siblings_and_index(id);
        siblings[..at].iter().rev().copied()
    }

    /// Siblings after `id`, nearest first.
    pub fn next_siblings(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let (siblings, at) = self.siblings_and_index(id);
        siblings[at + 1..].iter().copied()
    }

    #[must_use]
    pub fn prev_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.prev_siblings(id).find(|&s| self.element(s).is_some())
    }

    #[must_use]
    pub fn next_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.next_siblings(id).find(|&s| self.element(s).is_some())
    }

    fn siblings_and_index(&self, id: NodeId) -> (&[NodeId], usize) {
        match self.parent(id) {
            Some(p) => {
                let siblings = &self.node(p).children;
                let at = siblings
                    .iter()
                    .position(|&s| s == id)
                    .expect("a child is among its parent's children");
                (siblings, at)
            }
            // Only the document node has no parent; it is its own only sibling.
            None => (&ROOT_SIBLINGS, 0),
        }
    }

    /// First element in document order with this local name (HTML namespace).
    #[must_use]
    pub fn first_element_by_name(&self, local: &str) -> Option<NodeId> {
        self.descendants(self.root()).find(|&n| {
            self.element(n)
                .is_some_and(|e| e.is_html() && e.local_name() == local)
        })
    }

    #[must_use]
    pub fn element_by_id(&self, id: &str) -> Option<NodeId> {
        self.descendants(self.root())
            .find(|&n| self.element(n).and_then(Element::id) == Some(id))
    }

    #[must_use]
    pub fn html(&self) -> Option<NodeId> {
        self.first_element_by_name("html")
    }

    #[must_use]
    pub fn head(&self) -> Option<NodeId> {
        self.first_element_by_name("head")
    }

    #[must_use]
    pub fn body(&self) -> Option<NodeId> {
        self.first_element_by_name("body")
    }

    /// The `<title>` text, whitespace collapsed.
    #[must_use]
    pub fn title(&self) -> Option<String> {
        let t = self.first_element_by_name("title")?;
        Some(collapse_whitespace(&self.text_content(t)))
    }

    /// Concatenated text of `id` and its descendants (DOM `textContent`).
    #[must_use]
    pub fn text_content(&self, id: NodeId) -> String {
        let mut out = String::new();
        for n in self.descendants(id) {
            if let NodeData::Text(t) = &self.node(n).data {
                out.push_str(t);
            }
        }
        out
    }
}

/// Pre-order traversal; see [`Document::descendants`].
pub struct Descendants<'a> {
    doc: &'a Document,
    stack: Vec<NodeId>,
}

impl Iterator for Descendants<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.stack.pop()?;
        self.stack
            .extend(self.doc.node(id).children.iter().rev().copied());
        Some(id)
    }
}

/// Collapses runs of ASCII whitespace to one space and trims the ends, as
/// CSS `white-space: normal` does to text.
#[must_use]
pub fn collapse_whitespace(s: &str) -> String {
    s.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests;
