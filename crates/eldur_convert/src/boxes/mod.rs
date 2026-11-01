//! Step 4: the box tree (CSS Display 3 §2, CSS 2.2 §9.2).
//!
//! One [`LayoutBox`] per rendered element, plus anonymous block boxes
//! around inline runs that sit next to block-level siblings. A block
//! container holds either block-level boxes or one inline formatting
//! context ([`Content::Inline`]), never both. Whitespace is collapsed here
//! (see [`whitespace`]), so the emitter sees final text.
//!
//! Layout kinds this converter cannot map yet keep their own [`BoxKind`] so
//! their emitters can be added without touching the builder; until then the
//! emitter lays them out as block flow and says so.

mod counters;
mod table;
mod whitespace;

#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::collections::HashSet;

use eldur_css::{ComputedStyle, ContentItem, Display, Float, ListStyleType, Position, StyleMap};
use eldur_dom::{Document, NodeData, NodeId};

use crate::diag::Diagnostics;

/// How a box lays out its children (chosen from `display`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoxKind {
    /// Block container in normal flow (`block`, `flow-root`, `list-item`,
    /// and blockified inline-level boxes).
    Block,
    Flex,
    Grid,
    /// `table` and its internal boxes (`table-row`, `table-cell`, …).
    Table,
    /// A replaced element or form control (`img`, `video`, `input`, …):
    /// its DOM children are not rendered as boxes.
    Replaced,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayoutBox<'a> {
    /// The element; `None` for anonymous boxes and `::before`/`::after`.
    pub node: Option<NodeId>,
    pub style: Cow<'a, ComputedStyle>,
    pub kind: BoxKind,
    pub content: Content<'a>,
    /// The `outside` marker of a list item; an `inside` one is inline
    /// content.
    pub marker: Option<Marker<'a>>,
}

/// An `outside` list marker, drawn in the margin area of the list item.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Marker<'a> {
    pub kind: MarkerKind,
    /// `::marker` if it has rules, otherwise the list item's own style.
    pub style: &'a ComputedStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    Disc,
    Circle,
    Square,
    /// `1. `, `iv. `: the trailing space is part of the marker string.
    Text(String),
    Image(String),
}

#[cfg(test)]
impl LayoutBox<'_> {
    pub(crate) fn is_anonymous(&self) -> bool {
        self.node.is_none()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content<'a> {
    /// Block-level children only.
    Blocks(Vec<LayoutBox<'a>>),
    /// One inline formatting context, whitespace already collapsed; never
    /// empty.
    Inline(Vec<Inline<'a>>),
    /// No children (or only collapsed-away whitespace).
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Inline<'a> {
    Text(String),
    /// `<br>`, or a preserved newline.
    LineBreak,
    Span(Span<'a>),
    /// An atomic inline-level box (`inline-block`, `inline-flex`, …): a
    /// box of its own that sits in the line like one big glyph.
    Atomic(Box<LayoutBox<'a>>),
}

/// An inline box (`<a>`, `<strong>`, `::before`, …).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span<'a> {
    pub node: Option<NodeId>,
    pub style: &'a ComputedStyle,
    pub children: Vec<Inline<'a>>,
}

/// Builds the box tree of `doc`'s root element. `None` when the root
/// element generates no box.
#[cfg(test)]
pub(crate) fn build<'a>(
    doc: &'a Document,
    styles: &'a StyleMap,
    diags: &mut Diagnostics,
) -> Option<LayoutBox<'a>> {
    build_with(doc, styles, None, None, diags)
}

/// An element a script reaches as a node of its line or `<select>`
/// (`docs/spec/dom-api.md`, Inline nodes).
pub(crate) fn is_inline_node(doc: &Document, styles: &StyleMap, node: NodeId) -> bool {
    let tag = doc.element(node).map(eldur_dom::Element::local_name);
    matches!(tag, Some("option" | "optgroup" | "select"))
        || styles
            .computed(node)
            .is_some_and(|c| c.style.display == Display::Inline)
        || stays_in_line(doc, styles, node)
}

/// A hidden phrasing element among its line's text: showing it puts it in
/// that line, so a box of its own would split the line for nothing.
fn stays_in_line(doc: &Document, styles: &StyleMap, node: NodeId) -> bool {
    let hidden = styles
        .computed(node)
        .is_none_or(|c| c.style.display == Display::None);
    if !hidden || !is_phrasing(doc, node) || !only_phrasing_inside(doc, styles, node) {
        return false;
    }
    let Some(parent) = doc.parent(node) else {
        return false;
    };
    // A box on the line cuts its text in runs, which hold no hidden node.
    let boxed = doc.element_children(parent).any(|sibling| {
        styles
            .style(sibling)
            .is_some_and(|s| s.display != Display::None && s.display != Display::Inline)
            || is_atomic_tag(doc, sibling)
    });
    !boxed && has_line_content(doc, styles, node, parent)
}

/// A hidden element that would be a box on its parent's line when shown: it
/// waits there, so that showing it does not need a line made around it.
fn hidden_in_line(doc: &Document, styles: &StyleMap, node: NodeId) -> bool {
    (is_phrasing(doc, node) || is_atomic_tag(doc, node))
        && doc.parent(node).is_some_and(|parent| {
            has_line_content(doc, styles, node, parent) || holds_only_inline(doc, styles, parent)
        })
}

/// A parent all of whose elements are inline-level, hidden or not: shown, they
/// share a line whichever it is.
fn holds_only_inline(doc: &Document, styles: &StyleMap, parent: NodeId) -> bool {
    doc.element_children(parent).all(|child| {
        styles
            .computed(child)
            .is_none_or(|c| match c.style.display {
                Display::None => is_phrasing(doc, child) || is_atomic_tag(doc, child),
                Display::Inline | Display::InlineBlock | Display::InlineFlex => true,
                _ => false,
            })
    })
}

/// Whether `parent` has something rendered on a line beside `node`.
fn has_line_content(doc: &Document, styles: &StyleMap, node: NodeId, parent: NodeId) -> bool {
    doc.children(parent).any(|sibling| {
        sibling != node
            && match doc.text(sibling) {
                Some(text) => !text.trim().is_empty(),
                None => styles.style(sibling).is_some_and(|s| {
                    matches!(
                        s.display,
                        Display::Inline | Display::InlineBlock | Display::InlineFlex
                    )
                }),
            }
    })
}

fn is_phrasing(doc: &Document, node: NodeId) -> bool {
    const PHRASING: &[&str] = &[
        "a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "del", "dfn", "em", "font", "i",
        "ins", "kbd", "label", "mark", "q", "s", "samp", "small", "span", "strong", "sub", "sup",
        "time", "u", "var",
    ];
    doc.element(node)
        .is_some_and(|e| PHRASING.contains(&e.local_name()))
}

fn only_phrasing_inside(doc: &Document, styles: &StyleMap, node: NodeId) -> bool {
    doc.element_children(node).all(|child| {
        is_phrasing(doc, child)
            && styles
                .computed(child)
                .is_none_or(|c| matches!(c.style.display, Display::None | Display::Inline))
            && only_phrasing_inside(doc, styles, child)
    })
}

fn is_atomic_tag(doc: &Document, node: NodeId) -> bool {
    doc.element(node).is_some_and(|e| {
        matches!(
            e.local_name(),
            "button" | "img" | "input" | "select" | "textarea"
        )
    })
}

/// The `display: none` elements of the body a script could show or read as
/// elements. What never renders whatever a script does (a `<script>`, a
/// `<datalist>`) has no box to keep.
pub(crate) fn hidden_elements(doc: &Document, styles: &StyleMap) -> Vec<NodeId> {
    const NEVER: &[&str] = &[
        "area", "base", "basefont", "datalist", "link", "meta", "noembed", "noframes", "noscript",
        "param", "rp", "script", "source", "style", "title", "track",
    ];
    let Some(body) = doc.body() else {
        return Vec::new();
    };
    doc.descendants(body)
        .filter(|&n| {
            doc.element(n)
                .is_some_and(|e| e.is_html() && !NEVER.contains(&e.local_name()))
                && styles
                    .computed(n)
                    .is_some_and(|c| c.style.display == Display::None)
        })
        .collect()
}

/// [`build`], also keeping a hidden box for each `display: none` element
/// in `keep`: scripts that show it (`el.hidden = false`) need an object.
/// Each inline element in `own` is laid out as an atomic inline-level box
/// of its own: the elements a script creates in a line need an item to be
/// materialised into (dynamic-styles.md §6.8).
pub(crate) fn build_with<'a>(
    doc: &'a Document,
    styles: &'a StyleMap,
    keep: Option<&'a HashSet<NodeId>>,
    own: Option<&'a HashSet<NodeId>>,
    diags: &mut Diagnostics,
) -> Option<LayoutBox<'a>> {
    let root = doc.html()?;
    let style = styles.style(root)?;
    if style.display == Display::None {
        return None;
    }
    let mut builder = Builder {
        doc,
        styles,
        keep,
        own,
        diags,
        counters: counters::Counters::new(),
    };
    builder.counters.apply(style);
    // The root element always establishes a block formatting context; its
    // own display only matters for the kind of layout it uses.
    Some(builder.block_box(root, style, kind_of(style.display)))
}

/// What an element generates, from the point of view of its parent.
enum Level {
    None,
    /// `display: contents`: children are hoisted into the parent.
    Contents,
    Block(BoxKind),
    Inline,
    /// An atomic inline-level box of this kind.
    Atomic(BoxKind),
    LineBreak,
}

/// A child of a block container before inline runs are grouped.
enum Child<'a> {
    // Boxed: a box with an owned (anonymous) style is large.
    Block(Box<LayoutBox<'a>>),
    Inline(whitespace::Raw<'a>),
}

struct Builder<'a, 'd> {
    doc: &'a Document,
    styles: &'a StyleMap,
    keep: Option<&'a HashSet<NodeId>>,
    own: Option<&'a HashSet<NodeId>>,
    diags: &'d mut Diagnostics,
    counters: counters::Counters,
}

impl<'a> Builder<'a, '_> {
    fn block_box(
        &mut self,
        node: NodeId,
        style: &'a ComputedStyle,
        kind: BoxKind,
    ) -> LayoutBox<'a> {
        if kind == BoxKind::Replaced {
            return LayoutBox {
                node: Some(node),
                style: Cow::Borrowed(style),
                kind,
                content: self.button_content(node),
                marker: None,
            };
        }
        let mut children = Vec::new();
        let mut marker = None;
        if style.display == Display::ListItem
            && let Some(kind) = self.list_marker(node, style)
        {
            let marker_style = self.styles.computed(node).and_then(|s| s.marker.as_deref());
            if style.list_style_inside {
                // Chromium leaves about 1em after an inside bullet (measured:
                // text 22px from the edge at 16px): an em space, which white
                // space processing leaves alone.
                let text = match kind {
                    MarkerKind::Disc => "\u{2022}\u{2003}".to_owned(),
                    MarkerKind::Circle => "\u{25e6}\u{2003}".to_owned(),
                    MarkerKind::Square => "\u{25aa}\u{2003}".to_owned(),
                    MarkerKind::Text(t) => t,
                    MarkerKind::Image(_) => String::new(),
                };
                let raw = whitespace::Raw::Text {
                    text,
                    white_space: eldur_css::WhiteSpace::Pre,
                };
                children.push(Child::Inline(match marker_style {
                    Some(style) => whitespace::Raw::Span {
                        node: None,
                        style,
                        children: vec![raw],
                    },
                    None => raw,
                }));
            } else {
                marker = Some(Marker {
                    kind,
                    style: marker_style.unwrap_or(style),
                });
            }
        }
        self.children_of(node, &mut children);
        let content = if is_table_container(style.display) {
            table::internal_content(style.display, children, style)
        } else {
            group(table::wrap_orphans(children, style), style)
        };
        LayoutBox {
            node: Some(node),
            style: Cow::Borrowed(style),
            kind,
            content,
            marker,
        }
    }

    /// What a `<button>` draws beyond its text: inline content some of which has a
    /// box. Children a script can reach stay what they were: the text of the
    /// `Button`, as the runtime knows it.
    fn button_content(&mut self, node: NodeId) -> Content<'a> {
        let is_button = self
            .doc
            .element(node)
            .is_some_and(|e| e.is_html() && e.local_name() == "button");
        if !is_button
            || self
                .doc
                .element_children(node)
                .any(|child| self.keeps_descendant(child))
        {
            return Content::Empty;
        }
        let mut children = Vec::new();
        self.children_of(node, &mut children);
        if children.iter().any(|c| matches!(c, Child::Block(_))) {
            return Content::Empty;
        }
        let raw = children
            .into_iter()
            .filter_map(|c| match c {
                Child::Inline(raw) => Some(raw),
                Child::Block(_) => None,
            })
            .collect();
        match inline_content(raw) {
            Content::Inline(items)
                if crate::emit::has_atomic(&items)
                    || crate::emit::needs_lines_for_boxes(&items) =>
            {
                Content::Inline(items)
            }
            _ => Content::Empty,
        }
    }

    /// Appends the boxes of `node`'s children (and its `::before` /
    /// `::after`) to `out`.
    fn children_of(&mut self, node: NodeId, out: &mut Vec<Child<'a>>) {
        self.counters.push();
        let element_style = self.styles.computed(node);
        if let Some(before) = element_style.and_then(|s| s.before.as_deref()) {
            self.pseudo(node, before, out);
        }
        let mut first_letter = element_style
            .filter(|s| is_block_level(s.style.display) || s.style.display == Display::ListItem)
            .and_then(|s| s.first_letter.as_deref());
        if first_letter.is_some_and(|l| l.float != Float::None) {
            self.diags.warning(
                "css",
                node_line(self.doc, node),
                "a floated `::first-letter` is not converted; the letter stays plain".to_owned(),
            );
            first_letter = None;
        }
        for child in self.doc.children(node) {
            match &self.doc.node(child).data {
                NodeData::Text(text) => {
                    let white_space = self
                        .styles
                        .computed(node)
                        .map(|s| s.style.white_space)
                        .unwrap_or_default();
                    if let Some((letter_style, (letter, rest))) = first_letter
                        .filter(|_| !text.trim().is_empty())
                        .and_then(|l| Some((l, split_first_letter(text)?)))
                    {
                        first_letter = None;
                        out.push(Child::Inline(whitespace::Raw::Span {
                            node: None,
                            style: letter_style,
                            children: vec![whitespace::Raw::Text {
                                text: letter.into(),
                                white_space,
                            }],
                        }));
                        out.push(Child::Inline(whitespace::Raw::Text {
                            text: rest.into(),
                            white_space,
                        }));
                        continue;
                    }
                    out.push(Child::Inline(whitespace::Raw::Text {
                        text: text.clone(),
                        white_space,
                    }));
                }
                NodeData::Element(_) => {
                    // The letter of a nested element is that element's text.
                    if first_letter.take().is_some() {
                        self.diags.info(
                            "css",
                            node_line(self.doc, node),
                            "`::first-letter` reaches only text directly in its element; \
                             the letter stays plain"
                                .to_owned(),
                        );
                    }
                    self.element(child, out)
                }
                _ => {}
            }
        }
        if let Some(after) = element_style.and_then(|s| s.after.as_deref()) {
            self.pseudo(node, after, out);
        }
        self.counters.pop();
    }

    fn element(&mut self, node: NodeId, out: &mut Vec<Child<'a>>) {
        let style = self.styles.computed(node).map(|s| &s.style);
        let hidden = style.is_none_or(|s| s.display == Display::None);
        if hidden && self.keeps_descendant(node) && !stays_in_line(self.doc, self.styles, node) {
            if let Some(mut b) = self.hidden_box(node) {
                if hidden_in_line(self.doc, self.styles, node) {
                    let mut style = b.style.into_owned();
                    style.display = Display::InlineBlock;
                    b.style = Cow::Owned(style);
                    out.push(Child::Inline(whitespace::Raw::Atomic(Box::new(b))));
                } else {
                    out.push(Child::Block(Box::new(b)));
                }
            }
            return;
        }
        let Some(style) = style else {
            return;
        };
        self.counters.apply(style);
        match self.level(node, style) {
            Level::None => {}
            Level::Contents => self.children_of(node, out),
            Level::Block(kind) => {
                out.push(Child::Block(Box::new(self.block_box(node, style, kind))))
            }
            Level::Inline if self.own.is_some_and(|own| own.contains(&node)) => {
                let mut built = self.block_box(node, style, BoxKind::Block);
                // Vertical margins and padding of an inline box do not move
                // its line (CSS 2.2 §10.6.1); as an item they would.
                let mut owned = style.clone();
                owned.margin.top = eldur_css::LengthAuto::Length(eldur_css::Length::default());
                owned.margin.bottom = eldur_css::LengthAuto::Length(eldur_css::Length::default());
                owned.padding.top = eldur_css::Length::default();
                owned.padding.bottom = eldur_css::Length::default();
                built.style = Cow::Owned(owned);
                out.push(Child::Inline(whitespace::Raw::Atomic(Box::new(built))));
            }
            Level::Inline => {
                let mut children = Vec::new();
                self.children_of(node, &mut children);
                // `level` blockifies inline elements with block descendants,
                // so only out-of-flow boxes reach here. They do not split
                // the inline box; they leave for their containing block.
                let (inline, blocks): (Vec<_>, Vec<_>) = children
                    .into_iter()
                    .partition(|c| matches!(c, Child::Inline(_)));
                let children = inline
                    .into_iter()
                    .filter_map(|c| match c {
                        Child::Inline(raw) => Some(raw),
                        Child::Block(_) => None,
                    })
                    .collect();
                out.push(Child::Inline(whitespace::Raw::Span {
                    node: Some(node),
                    style,
                    children,
                }));
                out.extend(blocks);
            }
            Level::Atomic(kind) => {
                let atomic = self.block_box(node, style, kind);
                out.push(Child::Inline(whitespace::Raw::Atomic(Box::new(atomic))));
            }
            Level::LineBreak => out.push(Child::Inline(whitespace::Raw::LineBreak)),
        }
    }

    /// Whether a script can reach `node` or something inside it.
    fn keeps_descendant(&self, node: NodeId) -> bool {
        self.keep
            .is_some_and(|k| self.doc.descendants(node).any(|n| k.contains(&n)))
    }

    /// A `display: none` element a script can reach, with its real subtree
    /// and styles, invisible: Layouts skip invisible items, so it takes no
    /// space until a script shows it. What it would display as is unknown
    /// (`display` computed to `none`), so it is a block, or its control.
    fn hidden_box(&mut self, node: NodeId) -> Option<LayoutBox<'a>> {
        let computed = &self.styles.computed(node)?.style;
        self.diags.info(
            "css",
            node_line(self.doc, node),
            format!(
                "{} is `display: none` and used by a script: it is kept invisible, as a block when shown",
                describe(self.doc, node)
            ),
        );
        let kind = if is_replaced(self.doc, node) {
            BoxKind::Replaced
        } else {
            BoxKind::Block
        };
        // A `display: none` subtree counts nothing.
        let counters = self.counters.clone();
        let mut built = self.block_box(node, computed, kind);
        self.counters = counters;
        let mut style = computed.clone();
        style.display = Display::Block;
        style.visibility = eldur_css::Visibility::Collapse;
        built.style = Cow::Owned(style);
        Some(built)
    }

    fn pseudo(&mut self, node: NodeId, style: &'a ComputedStyle, out: &mut Vec<Child<'a>>) {
        if style.display == Display::None {
            return;
        }
        self.counters.apply(style);
        let text = self.generated_content(node, style);
        let children = if text.is_empty() {
            Vec::new()
        } else {
            vec![whitespace::Raw::Text {
                text,
                white_space: style.white_space,
            }]
        };
        if is_block_level(style.display) || style.float != Float::None {
            let content = group(children.into_iter().map(Child::Inline).collect(), style);
            out.push(Child::Block(Box::new(LayoutBox {
                node: None,
                style: Cow::Borrowed(style),
                kind: kind_of(style.display),
                content,
                marker: None,
            })));
        } else {
            out.push(Child::Inline(whitespace::Raw::Span {
                node: None,
                style,
                children,
            }));
        }
    }

    fn generated_content(&mut self, node: NodeId, style: &ComputedStyle) -> String {
        let mut text = String::new();
        let element = self.doc.element(node);
        for item in style.content.iter().flatten() {
            match item {
                ContentItem::String(s) => text.push_str(s),
                ContentItem::Attr(name) => {
                    text.push_str(element.and_then(|e| e.attr(name)).unwrap_or(""));
                }
                ContentItem::OpenQuote => text.push('\u{201c}'),
                ContentItem::CloseQuote => text.push('\u{201d}'),
                ContentItem::Counter(name, list_style) => {
                    text.push_str(&counters::format(self.counters.value(name), *list_style));
                }
                ContentItem::Counters(name, separator, list_style) => {
                    let values = self.counters.all(name);
                    let values = if values.is_empty() { vec![0] } else { values };
                    let parts: Vec<String> = values
                        .into_iter()
                        .map(|v| counters::format(v, *list_style))
                        .collect();
                    text.push_str(&parts.join(separator));
                }
                ContentItem::Url(_) => {
                    self.diags.warning(
                        "css",
                        node_line(self.doc, node),
                        format!(
                            "generated content `{item:?}` on {} is not supported; dropped",
                            describe(self.doc, node)
                        ),
                    );
                }
            }
        }
        text
    }

    fn level(&mut self, node: NodeId, style: &ComputedStyle) -> Level {
        let Some(element) = self.doc.element(node) else {
            return Level::None;
        };
        if style.display == Display::None {
            return Level::None;
        }
        if element.is_html() && element.local_name() == "br" {
            return Level::LineBreak;
        }
        if element.is_html() && element.local_name() == "wbr" {
            return Level::None;
        }
        if style.display == Display::Contents {
            return Level::Contents;
        }
        if is_replaced(self.doc, node) {
            if !is_block_level(style.display) {
                return Level::Atomic(BoxKind::Replaced);
            }
            return Level::Block(BoxKind::Replaced);
        }
        let out_of_flow = style.float != Float::None
            || matches!(style.position, Position::Absolute | Position::Fixed);
        if out_of_flow {
            // Floats and absolutely positioned boxes are blockified (CSS
            // Display 3 §2.7); their own emitters come with positioning.
            return Level::Block(kind_of(style.display));
        }
        if is_block_level(style.display) {
            return Level::Block(kind_of(style.display));
        }
        let atomic = matches!(
            style.display,
            Display::InlineBlock | Display::InlineFlex | Display::InlineGrid | Display::InlineTable
        );
        if atomic {
            return Level::Atomic(kind_of(style.display));
        }
        if self.has_block_descendant(node, is_containing_block(style)) {
            self.diags.info(
                "css",
                node_line(self.doc, node),
                format!(
                    "{} contains block-level boxes; laid out as a block",
                    describe(self.doc, node)
                ),
            );
            return Level::Block(kind_of(style.display));
        }
        Level::Inline
    }

    /// Whether an in-flow descendant of `node` (through inline and
    /// `contents` elements only) is block-level, which splits the inline
    /// box in CSS. Out-of-flow boxes do not split it, but one whose
    /// containing block is the inline box (`holds`) needs a box to sit in.
    fn has_block_descendant(&self, node: NodeId, holds: bool) -> bool {
        self.doc.element_children(node).any(|child| {
            let Some(style) = self.styles.computed(child).map(|s| &s.style) else {
                return false;
            };
            if style.display == Display::None {
                return false;
            }
            (is_out_of_flow(style) && holds)
                || (!is_out_of_flow(style)
                    && (is_block_level(style.display) || style.float != Float::None))
                // An atomic inline's blocks are inside its own box.
                || (!is_out_of_flow(style)
                    && matches!(style.display, Display::Inline | Display::Contents)
                    && self.has_block_descendant(child, holds || is_containing_block(style)))
        })
    }

    /// The marker of a list item, unless `list-style` says there is none.
    fn list_marker(&mut self, node: NodeId, style: &ComputedStyle) -> Option<MarkerKind> {
        if let Some(url) = &style.list_style_image {
            return Some(MarkerKind::Image(url.clone()));
        }
        let text = |s: String| Some(MarkerKind::Text(format!("{s}. ")));
        match style.list_style_type {
            ListStyleType::None => None,
            ListStyleType::Disc => Some(MarkerKind::Disc),
            ListStyleType::Circle => Some(MarkerKind::Circle),
            ListStyleType::Square => Some(MarkerKind::Square),
            ListStyleType::Decimal => text(self.ordinal(node).to_string()),
            ListStyleType::DecimalLeadingZero => {
                let n = self.ordinal(node);
                let sign = if n < 0 { "-" } else { "" };
                text(format!("{sign}{:02}", n.abs()))
            }
            ListStyleType::LowerAlpha => text(alpha(self.ordinal(node)).to_lowercase()),
            ListStyleType::UpperAlpha => text(alpha(self.ordinal(node))),
            ListStyleType::LowerRoman => text(roman(self.ordinal(node)).to_lowercase()),
            ListStyleType::UpperRoman => text(roman(self.ordinal(node))),
        }
    }

    /// The ordinal value of a list item (HTML §4.4.8): `start` and
    /// `reversed` on the `<ol>`, `value` on the item; only boxes that are
    /// list items count.
    fn ordinal(&self, node: NodeId) -> i64 {
        let attr_int = |n: NodeId, name: &str| {
            self.doc
                .element(n)
                .and_then(|e| e.attr(name))
                .and_then(|s| s.trim().parse::<i64>().ok())
        };
        let Some(parent) = self.doc.parent_element(node) else {
            return 1;
        };
        let items: Vec<NodeId> = self
            .doc
            .element_children(parent)
            .filter(|&c| {
                self.styles
                    .style(c)
                    .is_some_and(|s| s.display == Display::ListItem)
            })
            .collect();
        let parent_element = self.doc.element(parent);
        let is_ol = parent_element.is_some_and(|e| e.local_name() == "ol");
        let reversed = is_ol && parent_element.is_some_and(|e| e.attr("reversed").is_some());
        let start = is_ol.then(|| attr_int(parent, "start")).flatten();
        let step = if reversed { -1 } else { 1 };
        let mut n = start.unwrap_or(if reversed {
            i64::try_from(items.len()).unwrap_or(0)
        } else {
            1
        });
        for item in items {
            if let Some(value) = attr_int(item, "value") {
                n = value;
            }
            if item == node {
                return n;
            }
            n += step;
        }
        n
    }
}

/// Wraps inline runs in anonymous blocks when `children` mix block-level
/// and inline-level boxes (CSS 2.2 §9.2.1.1).
fn group<'a>(children: Vec<Child<'a>>, parent: &'a ComputedStyle) -> Content<'a> {
    if !children.iter().any(|c| matches!(c, Child::Block(_))) {
        let raw = children
            .into_iter()
            .filter_map(|c| match c {
                Child::Inline(r) => Some(r),
                Child::Block(_) => None,
            })
            .collect();
        return inline_content(raw);
    }
    let mut blocks = Vec::new();
    let mut run = Vec::new();
    // Out-of-flow boxes that were block-level sit below the line before
    // them, so they follow the run they interrupt without ending it.
    let mut below_run = Vec::new();
    let flush = |run: &mut Vec<whitespace::Raw<'a>>, blocks: &mut Vec<LayoutBox<'a>>| {
        if let Content::Inline(items) = inline_content(std::mem::take(run)) {
            blocks.push(LayoutBox {
                node: None,
                style: Cow::Owned(anonymous_style(parent)),
                kind: BoxKind::Block,
                content: Content::Inline(items),
                marker: None,
            });
        }
    };
    for child in children {
        match child {
            Child::Inline(raw) => run.push(raw),
            Child::Block(b) if is_out_of_flow(&b.style) => {
                // With no line to sit below it stays where the tree has it: scripts
                // walk the items for tree order, in Tab and in queries.
                let no_line = matches!(inline_content(run.clone()), Content::Empty);
                if b.style.blockified_inline || no_line {
                    blocks.push(*b);
                } else {
                    below_run.push(*b);
                }
            }
            Child::Block(b) => {
                flush(&mut run, &mut blocks);
                blocks.append(&mut below_run);
                blocks.push(*b);
            }
        }
    }
    flush(&mut run, &mut blocks);
    blocks.append(&mut below_run);
    Content::Blocks(blocks)
}

fn inline_content(raw: Vec<whitespace::Raw<'_>>) -> Content<'_> {
    let items = whitespace::collapse(raw);
    if items.is_empty() {
        Content::Empty
    } else {
        Content::Inline(items)
    }
}

/// Whether a box leaves the flow for a containing block.
pub(crate) fn is_out_of_flow(style: &ComputedStyle) -> bool {
    matches!(style.position, Position::Absolute | Position::Fixed)
}

/// Whether a box is the containing block of its `position: fixed`
/// descendants (CSS Transforms 1 §3, Filter Effects 1 §5).
pub(crate) fn is_fixed_containing_block(style: &ComputedStyle) -> bool {
    !style.transform.is_empty() || style.filter.is_some()
}

/// Whether a box is the containing block of its `position: absolute`
/// descendants (CSS 2.2 §10.1).
pub(crate) fn is_containing_block(style: &ComputedStyle) -> bool {
    style.position != Position::Static || is_fixed_containing_block(style)
}

/// The style of an anonymous block box: inherited properties from its
/// parent, initial values for the rest (CSS 2.2 §9.2.1.1).
pub(crate) fn anonymous_style(parent: &ComputedStyle) -> ComputedStyle {
    ComputedStyle {
        display: Display::Block,
        color: parent.color,
        font_family: parent.font_family.clone(),
        font_size: parent.font_size,
        font_weight: parent.font_weight,
        font_style: parent.font_style,
        small_caps: parent.small_caps,
        line_height: parent.line_height,
        letter_spacing: parent.letter_spacing,
        word_spacing: parent.word_spacing,
        text_align: parent.text_align,
        text_indent: parent.text_indent.clone(),
        text_transform: parent.text_transform,
        white_space: parent.white_space,
        word_break: parent.word_break,
        overflow_wrap_anywhere: parent.overflow_wrap_anywhere,
        text_shadow: parent.text_shadow.clone(),
        direction: parent.direction,
        visibility: parent.visibility,
        cursor: parent.cursor,
        pointer_events: parent.pointer_events,
        list_style_type: parent.list_style_type,
        list_style_inside: parent.list_style_inside,
        list_style_image: parent.list_style_image.clone(),
        user_select_none: parent.user_select_none,
        border_collapse: parent.border_collapse,
        border_spacing: parent.border_spacing,
        caption_side: parent.caption_side,
        custom_properties: parent.custom_properties.clone(),
        ..ComputedStyle::default()
    }
}

/// Block-level outer display types, including table-internal boxes, which
/// this converter treats as blocks until tables have their own emitter.
pub(crate) fn is_block_level(display: Display) -> bool {
    !matches!(
        display,
        Display::Inline
            | Display::InlineBlock
            | Display::InlineFlex
            | Display::InlineGrid
            | Display::InlineTable
            | Display::Contents
            | Display::None
    )
}

/// Displays whose children are table-internal boxes, not block flow.
fn is_table_container(display: Display) -> bool {
    matches!(
        display,
        Display::Table
            | Display::InlineTable
            | Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
            | Display::TableRow
            | Display::TableColumnGroup
            | Display::TableColumn
    )
}

fn kind_of(display: Display) -> BoxKind {
    match display {
        Display::Flex | Display::InlineFlex => BoxKind::Flex,
        Display::Grid | Display::InlineGrid => BoxKind::Grid,
        Display::Table
        | Display::InlineTable
        | Display::TableRowGroup
        | Display::TableHeaderGroup
        | Display::TableFooterGroup
        | Display::TableRow
        | Display::TableCell
        | Display::TableColumnGroup
        | Display::TableColumn
        | Display::TableCaption => BoxKind::Table,
        _ => BoxKind::Block,
    }
}

/// Elements whose rendering is not their children (HTML §15.4, §15.5).
fn is_replaced(doc: &Document, node: NodeId) -> bool {
    let Some(element) = doc.element(node) else {
        return false;
    };
    if element.is_svg() {
        return true;
    }
    element.is_html()
        && matches!(
            element.local_name(),
            "img"
                | "video"
                | "audio"
                | "canvas"
                | "iframe"
                | "embed"
                | "object"
                | "input"
                | "button"
                | "textarea"
                | "select"
                | "meter"
                | "progress"
                | "picture"
                | "math"
        )
}

pub(crate) fn display_name(display: Display) -> &'static str {
    match display {
        Display::Block => "block",
        Display::Inline => "inline",
        Display::InlineBlock => "inline-block",
        Display::Flex => "flex",
        Display::InlineFlex => "inline-flex",
        Display::Grid => "grid",
        Display::InlineGrid => "inline-grid",
        Display::FlowRoot => "flow-root",
        Display::ListItem => "list-item",
        Display::Table => "table",
        Display::InlineTable => "inline-table",
        Display::TableRowGroup => "table-row-group",
        Display::TableHeaderGroup => "table-header-group",
        Display::TableFooterGroup => "table-footer-group",
        Display::TableRow => "table-row",
        Display::TableCell => "table-cell",
        Display::TableColumnGroup => "table-column-group",
        Display::TableColumn => "table-column",
        Display::TableCaption => "table-caption",
        Display::Contents => "contents",
        Display::None => "none",
    }
}

/// `<div class="card">`-style description of an element for diagnostics
/// and source comments.
pub(crate) fn describe(doc: &Document, node: NodeId) -> String {
    let Some(element) = doc.element(node) else {
        return "text".to_owned();
    };
    let mut out = format!("<{}", element.local_name());
    if let Some(id) = element.id() {
        out.push_str(&format!(" id=\"{id}\""));
    }
    if let Some(class) = element
        .attr("class")
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        out.push_str(&format!(
            " class=\"{}\"",
            eldur_dom::collapse_whitespace(class)
        ));
    }
    out.push('>');
    out
}

pub(crate) fn node_line(doc: &Document, node: NodeId) -> Option<u32> {
    doc.element(node).and_then(|e| u32::try_from(e.line).ok())
}

fn alpha(mut n: i64) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(char::from(b'A' + u8::try_from(n % 26).unwrap_or(0)));
        n /= 26;
    }
    out.iter().rev().collect()
}

fn roman(n: i64) -> String {
    if !(1..4000).contains(&n) {
        return n.to_string();
    }
    const TABLE: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut n = n;
    let mut out = String::new();
    for (value, digits) in TABLE {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    out
}

/// The first letter with the punctuation before it (and the space there, which
/// the white-space pass collapses), and what follows.
fn split_first_letter(text: &str) -> Option<(&str, &str)> {
    let end = text
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, c)| i + c.len_utf8())?;
    Some(text.split_at(end))
}
