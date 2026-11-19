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

    /// A `<template>`'s contents written out as markup, for what parses them
    /// again at run time.
    #[must_use]
    pub fn template_markup(&self, id: NodeId) -> Option<String> {
        let fragment = self.element(id)?.template_contents?;
        let mut out = String::new();
        for child in self.children(fragment) {
            self.write_markup(child, &mut out);
        }
        Some(out)
    }

    /// An `<svg>` element and its subtree as a standalone SVG document, for
    /// an image loader: XML syntax, the SVG namespace (and `xlink` when an
    /// attribute uses it) declared on the root, attribute names in their SVG
    /// case (`viewBox`), childless elements self-closed. `value` sees each
    /// attribute's local name and value, and each `<style>` element's text
    /// with the name `"#style"`, and returns what is written. Its third
    /// argument is the colour `currentColor` means there when the svg sets
    /// one itself, by a `color` attribute or a `color` in a `style`
    /// attribute on the element or an ancestor within the svg; `None` when
    /// the element inherits the colour of what holds the svg.
    ///
    /// What the subtree refers to elsewhere in the page, a sprite's
    /// `<symbol>` through `<use href="#id">` or a gradient through
    /// `url(#id)`, is copied into a `<defs>` at the end: in the page those
    /// ids resolve document-wide (SVG 2 §16.1.3), in a file of its own only
    /// within it, and without the copies the icon draws nothing.
    ///
    /// `used` is the width and height the svg is laid out at, when the
    /// layout knows them. A root without a length of its own is written
    /// with that one, as a file has no CSS box to fill.
    #[must_use]
    pub fn svg_markup(
        &self,
        id: NodeId,
        used: Option<(f32, f32)>,
        value: &dyn Fn(&str, &str, Option<&str>) -> String,
    ) -> Option<String> {
        let element = self.element(id).filter(|e| e.is_svg())?;
        let copies = self.svg_outside_references(id);
        let xlink = std::iter::once(id)
            .chain(copies.iter().copied())
            .flat_map(|n| self.descendants(n))
            .any(|n| {
                self.element(n)
                    .is_some_and(|e| e.attrs.iter().any(|a| a.name.ns == ns!(xlink)))
            });
        let mut out = String::new();
        let mut root = String::from(" xmlns=\"http://www.w3.org/2000/svg\"");
        if xlink {
            root.push_str(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
        }
        let used = used.map(|(w, h)| [fmt_length(w), fmt_length(h)]);
        let root_size = ["width", "height"].map(|name| element.attr(name).and_then(svg_length));
        let root_size = [0, 1].map(|axis| {
            root_size[axis]
                .clone()
                .or_else(|| used.as_ref().map(|u| u[axis].clone()))
        });
        let mut sizes = self.svg_symbol_sizes(id, element, &root_size, &copies);
        if let [Some(width), Some(height)] = &root_size {
            sizes.push((id, width.clone(), height.clone()));
        }
        let svg = SvgWriter { value, sizes: &sizes };
        svg.write(self, id, element, &root, None, &mut out);
        if !copies.is_empty() {
            let mut defs = String::from("<defs>");
            for copy in copies {
                if let Some(e) = self.element(copy) {
                    // A copy draws where it is used, which the file cannot
                    // follow; it takes the colour the svg starts with.
                    svg.write(self, copy, e, "", None, &mut defs);
                }
            }
            defs.push_str("</defs>");
            // The root was written whole; the copies go in as its last child.
            if let Some(open) = out.strip_suffix("/>") {
                out = format!("{open}>{defs}</svg>");
            } else {
                let close = out.len() - "</svg>".len();
                out.insert_str(close, &defs);
            }
        }
        Some(out)
    }

    /// The width and height each `<symbol>` without a length of its own is
    /// written with: what its uses all ask for when they agree, else the
    /// svg's viewport, which is what 100 % resolves to (SVG 2 §5.6.2). Some
    /// loaders draw a symbol at its own size whatever the `<use>` says, and
    /// at its viewBox's size when it has none; a browser draws the same
    /// picture either way, as a use's width and height win over the
    /// symbol's. A symbol whose viewport is not known in units is left alone.
    fn svg_symbol_sizes(
        &self,
        id: NodeId,
        root: &Element,
        root_size: &[Option<String>],
        copies: &[NodeId],
    ) -> Vec<(NodeId, String, String)> {
        let viewport = |axis: usize| {
            root.attr("viewBox")
                .and_then(|b| {
                    b.split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|p| !p.is_empty())
                        .nth(axis + 2)
                        .and_then(svg_length)
                })
                .or_else(|| root_size.get(axis).cloned().flatten())
        };
        let viewport = [viewport(0), viewport(1)];
        let symbols = std::iter::once(id)
            .chain(copies.iter().copied())
            .flat_map(|n| self.descendants(n))
            .filter(|&n| {
                self.element(n)
                    .is_some_and(|e| e.is_svg() && e.local_name() == "symbol")
            });
        let uses: Vec<&Element> = self
            .descendants(id)
            .filter_map(|n| self.element(n))
            .filter(|e| e.is_svg() && e.local_name() == "use")
            .collect();
        let mut sizes = Vec::new();
        for symbol in symbols {
            let Some(name) = self.element(symbol).and_then(Element::id) else {
                continue;
            };
            let target = |e: &&Element| {
                e.attrs.iter().any(|a| {
                    &*a.name.local == "href"
                        && (a.name.ns == ns!() || a.name.ns == ns!(xlink))
                        && a.value.trim().strip_prefix('#') == Some(name)
                })
            };
            let mut size = [None, None];
            for (axis, attr) in ["width", "height"].into_iter().enumerate() {
                let mut asked = uses
                    .iter()
                    .filter(|u| target(u))
                    .map(|u| u.attr(attr).and_then(svg_length));
                let first = asked.next().flatten();
                size[axis] = match first {
                    Some(length) if asked.all(|a| a.as_deref() == Some(&*length)) => Some(length),
                    _ => viewport[axis].clone(),
                };
            }
            if let [Some(width), Some(height)] = size {
                sizes.push((symbol, width, height));
            }
        }
        sizes
    }

    /// The svg elements outside `id`'s subtree that it refers to by
    /// fragment, and those they refer to in turn, in the order first met.
    /// An id names the first element in document order that has it, as
    /// `getElementById` finds it; ids that name nothing are left alone, as
    /// they draw nothing in the page either.
    fn svg_outside_references(&self, id: NodeId) -> Vec<NodeId> {
        let mut inside: Vec<NodeId> = self.descendants(id).collect();
        let mut wanted: Vec<String> = Vec::new();
        let mut found = Vec::new();
        let mut scanned = 0;
        let mut next = 0;
        loop {
            for &n in &inside[scanned..] {
                self.svg_references(n, &mut wanted);
            }
            scanned = inside.len();
            let Some(name) = wanted.get(next).cloned() else {
                break;
            };
            next += 1;
            let has = |n: &NodeId| self.element(*n).and_then(Element::id) == Some(name.as_str());
            if inside.iter().any(has) {
                continue;
            }
            let Some(target) = self.descendants(self.root()).find(has) else {
                continue;
            };
            // A target that holds the svg itself would copy it into itself.
            if !self.element(target).is_some_and(Element::is_svg)
                || self.ancestors(id).any(|a| a == target)
            {
                continue;
            }
            found.push(target);
            inside.extend(self.descendants(target));
        }
        found
    }

    /// Pushes onto `out` each fragment id `n` names that is not there yet:
    /// an `href` (plain or `xlink:`) of `#id`, and every `url(#id)` in an
    /// attribute or a `<style>` text.
    fn svg_references(&self, n: NodeId, out: &mut Vec<String>) {
        let mut add = |name: &str| {
            if !name.is_empty() && !out.iter().any(|w| w == name) {
                out.push(name.to_owned());
            }
        };
        match &self.node(n).data {
            NodeData::Element(e) if e.is_svg() => {
                for attr in &e.attrs {
                    if &*attr.name.local == "href"
                        && (attr.name.ns == ns!() || attr.name.ns == ns!(xlink))
                        && let Some(name) = attr.value.trim().strip_prefix('#')
                    {
                        add(name);
                    }
                    for name in url_fragments(&attr.value) {
                        add(name);
                    }
                }
            }
            NodeData::Text(t)
                if self
                    .parent(n)
                    .and_then(|p| self.element(p))
                    .is_some_and(|e| e.is_svg() && e.local_name() == "style") =>
            {
                for name in url_fragments(t) {
                    add(name);
                }
            }
            _ => {}
        }
    }

    fn write_markup(&self, id: NodeId, out: &mut String) {
        match &self.node(id).data {
            NodeData::Text(t) => {
                out.push_str(
                    &t.replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;"),
                );
            }
            NodeData::Element(e) => {
                out.push('<');
                out.push_str(e.local_name());
                for attr in &e.attrs {
                    let value = attr.value.replace('&', "&amp;").replace('"', "&quot;");
                    out.push_str(&format!(" {}=\"{value}\"", attr.name.local));
                }
                out.push('>');
                if matches!(
                    e.local_name(),
                    "area"
                        | "base"
                        | "br"
                        | "col"
                        | "embed"
                        | "hr"
                        | "img"
                        | "input"
                        | "link"
                        | "meta"
                        | "source"
                        | "track"
                        | "wbr"
                ) {
                    return;
                }
                for child in self.children(id) {
                    self.write_markup(child, out);
                }
                out.push_str("</");
                out.push_str(e.local_name());
                out.push('>');
            }
            _ => {}
        }
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

/// Writes svg elements as XML for [`Document::svg_markup`].
struct SvgWriter<'a> {
    value: &'a dyn Fn(&str, &str, Option<&str>) -> String,
    /// The symbols written with a width and height, and those lengths.
    sizes: &'a [(NodeId, String, String)],
}

impl SvgWriter<'_> {
    fn write(
        &self,
        doc: &Document,
        id: NodeId,
        element: &Element,
        declarations: &str,
        inherited: Option<&str>,
        out: &mut String,
    ) {
        let value = self.value;
        let size = self
            .sizes
            .iter()
            .find(|(symbol, ..)| *symbol == id)
            .map(|(_, w, h)| [("width", w), ("height", h)]);
        let own = svg_own_colour(element);
        let colour = own.as_deref().or(inherited);
        let escape = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        };
        out.push('<');
        out.push_str(element.local_name());
        out.push_str(declarations);
        for attr in &element.attrs {
            // The namespaces are declared once, on the root.
            if attr.name.ns == ns!(xmlns) || &*attr.name.local == "xmlns" {
                continue;
            }
            // HTML binds only the xlink, xml and xmlns prefixes; any other
            // (an editor's `inkscape:label`) is a name with a colon and no
            // namespace, which makes the file ill-formed to a namespace-aware
            // reader, and browsers ignore it anyway.
            if attr.name.ns == ns!() && attr.name.local.contains(':') {
                continue;
            }
            // A name HTML accepts but XML does not (`@click`, `x.y` from a
            // framework's template) would make the whole file unreadable;
            // the browser draws it as if it were not there.
            if !is_xml_name(&attr.name.local) {
                continue;
            }
            if size.is_some_and(|size| size.iter().any(|(n, _)| &*attr.name.local == *n))
                && attr.name.ns == ns!()
                && svg_length(&attr.value).is_none()
            {
                continue;
            }
            out.push(' ');
            if attr.name.ns == ns!(xlink) {
                out.push_str("xlink:");
            } else if attr.name.ns == ns!(xml) {
                out.push_str("xml:");
            }
            out.push_str(&attr.name.local);
            out.push_str("=\"");
            out.push_str(&escape(&value(&attr.name.local, &attr.value, colour)));
            out.push('"');
        }
        for (name, length) in size.into_iter().flatten() {
            if element.attr(name).and_then(svg_length).is_none() {
                out.push_str(&format!(" {name}=\"{length}\""));
            }
        }
        let children: Vec<NodeId> = doc.children(id).collect();
        if children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        let style = element.local_name() == "style";
        for child in children {
            match &doc.node(child).data {
                NodeData::Text(t) if style => out.push_str(&escape(&value("#style", t, colour))),
                NodeData::Text(t) => out.push_str(&escape(t)),
                NodeData::Element(e) => self.write(doc, child, e, "", colour, out),
                _ => {}
            }
        }
        out.push_str("</");
        out.push_str(element.local_name());
        out.push('>');
    }
}

/// `px` as an svg length: at most two decimals, no trailing zeros.
fn fmt_length(px: f32) -> String {
    let s = format!("{:.2}", px.max(0.0));
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A length in user units, as written (`48`, `48px`, `1.5`); `None` for a
/// percentage, `auto` or anything else that needs a viewport to resolve.
fn svg_length(value: &str) -> Option<String> {
    let v = value.trim();
    let v = v.strip_suffix("px").unwrap_or(v);
    v.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|_| v.to_owned())
}

/// The colour an svg element sets for itself and what it holds: the last
/// `color` in its `style` attribute, else its `color` attribute (a
/// presentation attribute, below any style; SVG 2 §6.6). `currentColor` and
/// `inherit` there set nothing new.
fn svg_own_colour(element: &Element) -> Option<String> {
    let usable = |v: &str| {
        let v = v.trim();
        let v = v.strip_suffix("!important").map_or(v, str::trim_end);
        (!v.is_empty()
            && !v.eq_ignore_ascii_case("currentcolor")
            && !v.eq_ignore_ascii_case("inherit"))
        .then(|| v.to_owned())
    };
    let plain = |name: &str| {
        element
            .attrs
            .iter()
            .find(|a| a.name.ns == ns!() && &*a.name.local == name)
            .map(|a| &*a.value)
    };
    plain("style")
        .and_then(|style| {
            style
                .split(';')
                .filter_map(|d| d.split_once(':'))
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("color"))
                .last()
                .and_then(|(_, v)| usable(v))
        })
        .or_else(|| plain("color").and_then(usable))
}

/// Whether `name` is an XML 1.0 Name without a colon (XML §2.3), the
/// characters past ASCII taken as name characters.
fn is_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    let start = |c: char| c.is_ascii_alphabetic() || c == '_' || !c.is_ascii();
    chars.next().is_some_and(start)
        && chars.all(|c| start(c) || c.is_ascii_digit() || c == '-' || c == '.')
}

/// The ids in each `url(#id)` of `text`, quoted or not (CSS Values 4 §4.5).
fn url_fragments(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices("url(").filter_map(move |(at, _)| {
        let rest = text[at + 4..].trim_start();
        let rest = rest.strip_prefix(['"', '\'']).unwrap_or(rest);
        let rest = rest.strip_prefix('#')?;
        let end = rest
            .find(|c: char| c == ')' || c == '"' || c == '\'' || c.is_whitespace())
            .unwrap_or(rest.len());
        Some(&rest[..end])
    })
}

#[cfg(test)]
mod tests;
