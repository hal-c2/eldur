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

mod table;
mod whitespace;

#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::collections::HashSet;

use h2q_css::{ComputedStyle, ContentItem, Display, Float, ListStyleType, Position, StyleMap};
use h2q_dom::{Document, NodeData, NodeId};

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
    build_keeping(doc, styles, None, diags)
}

/// [`build`], also keeping a hidden box for each `display: none` element
/// in `keep`: scripts that show it (`el.hidden = false`) need an object.
pub(crate) fn build_keeping<'a>(
    doc: &'a Document,
    styles: &'a StyleMap,
    keep: Option<&'a HashSet<NodeId>>,
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
        diags,
    };
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
    diags: &'d mut Diagnostics,
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
                content: Content::Empty,
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
                    white_space: h2q_css::WhiteSpace::Pre,
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

    /// Appends the boxes of `node`'s children (and its `::before` /
    /// `::after`) to `out`.
    fn children_of(&mut self, node: NodeId, out: &mut Vec<Child<'a>>) {
        let element_style = self.styles.computed(node);
        if let Some(before) = element_style.and_then(|s| s.before.as_deref()) {
            self.pseudo(node, before, out);
        }
        for child in self.doc.children(node) {
            match &self.doc.node(child).data {
                NodeData::Text(text) => {
                    let white_space = self
                        .styles
                        .computed(node)
                        .map(|s| s.style.white_space)
                        .unwrap_or_default();
                    out.push(Child::Inline(whitespace::Raw::Text {
                        text: text.clone(),
                        white_space,
                    }));
                }
                NodeData::Element(_) => self.element(child, out),
                _ => {}
            }
        }
        if let Some(after) = element_style.and_then(|s| s.after.as_deref()) {
            self.pseudo(node, after, out);
        }
    }

    fn element(&mut self, node: NodeId, out: &mut Vec<Child<'a>>) {
        let style = self.styles.computed(node).map(|s| &s.style);
        let hidden = style.is_none_or(|s| s.display == Display::None);
        if hidden && self.keeps_descendant(node) {
            if let Some(b) = self.hidden_box(node) {
                out.push(Child::Block(Box::new(b)));
            }
            return;
        }
        let Some(style) = style else {
            return;
        };
        match self.level(node, style) {
            Level::None => {}
            Level::Contents => self.children_of(node, out),
            Level::Block(kind) => {
                out.push(Child::Block(Box::new(self.block_box(node, style, kind))))
            }
            Level::Inline => {
                let mut children = Vec::new();
                self.children_of(node, &mut children);
                let children = children
                    .into_iter()
                    .filter_map(|c| match c {
                        Child::Inline(raw) => Some(raw),
                        // `level` blockifies inline elements with block
                        // descendants, so none reach here.
                        Child::Block(_) => None,
                    })
                    .collect();
                out.push(Child::Inline(whitespace::Raw::Span {
                    node: Some(node),
                    style,
                    children,
                }));
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
        let mut built = self.block_box(node, computed, kind);
        let mut style = computed.clone();
        style.display = Display::Block;
        style.visibility = h2q_css::Visibility::Collapse;
        built.style = Cow::Owned(style);
        Some(built)
    }

    fn pseudo(&mut self, node: NodeId, style: &'a ComputedStyle, out: &mut Vec<Child<'a>>) {
        if style.display == Display::None {
            return;
        }
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
                ContentItem::Counter(..) | ContentItem::Url(_) => {
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
                self.diags.info(
                    "html",
                    node_line(self.doc, node),
                    format!(
                        "inline {} is laid out as a block (no inline replaced elements yet)",
                        describe(self.doc, node)
                    ),
                );
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
            // Alone in its parent there is no line to sit in, so a block
            // that shrinks to fit looks the same and lays out more simply.
            if self.is_only_content(node) {
                return Level::Block(kind_of(style.display));
            }
            return Level::Atomic(kind_of(style.display));
        }
        if self.has_block_descendant(node) {
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

    /// Whether `node` has no sibling that generates anything but whitespace.
    fn is_only_content(&self, node: NodeId) -> bool {
        let Some(parent) = self.doc.parent(node) else {
            return false;
        };
        self.doc.children(parent).all(|sibling| {
            sibling == node
                || match &self.doc.node(sibling).data {
                    NodeData::Text(t) => t.chars().all(|c| c.is_ascii_whitespace()),
                    NodeData::Element(_) => self
                        .styles
                        .computed(sibling)
                        .is_none_or(|s| s.style.display == Display::None),
                    _ => true,
                }
        })
    }

    /// Whether an in-flow descendant of `node` (through inline and
    /// `contents` elements only) is block-level, which splits the inline
    /// box in CSS.
    fn has_block_descendant(&self, node: NodeId) -> bool {
        self.doc.element_children(node).any(|child| {
            let Some(style) = self.styles.computed(child).map(|s| &s.style) else {
                return false;
            };
            if style.display == Display::None {
                return false;
            }
            is_replaced(self.doc, child)
                || is_block_level(style.display)
                || style.float != Float::None
                || matches!(style.position, Position::Absolute | Position::Fixed)
                // An atomic inline's blocks are inside its own box.
                || (matches!(style.display, Display::Inline | Display::Contents)
                    && self.has_block_descendant(child))
        })
    }

    /// The marker of a list item, unless `list-style` says there is none.
    fn list_marker(&mut self, node: NodeId, style: &ComputedStyle) -> Option<MarkerKind> {
        if let Some(url) = &style.list_style_image {
            // No asset pipeline yet: only a self-contained image can load.
            if url.starts_with("data:") {
                return Some(MarkerKind::Image(url.clone()));
            }
            self.diags.warning(
                "css",
                node_line(self.doc, node),
                format!(
                    "`list-style-image: url({url})` on {} is not converted; using `list-style-type`",
                    describe(self.doc, node)
                ),
            );
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
            Child::Block(b) => {
                flush(&mut run, &mut blocks);
                blocks.push(*b);
            }
        }
    }
    flush(&mut run, &mut blocks);
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
            h2q_dom::collapse_whitespace(class)
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
