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

#[derive(Debug, Clone)]
pub(crate) struct LayoutBox<'a> {
    /// The element; `None` for anonymous boxes and `::before`/`::after`.
    pub node: Option<NodeId>,
    pub style: Cow<'a, ComputedStyle>,
    pub kind: BoxKind,
    pub content: Content<'a>,
}

#[cfg(test)]
impl LayoutBox<'_> {
    pub(crate) fn is_anonymous(&self) -> bool {
        self.node.is_none()
    }
}

#[derive(Debug, Clone)]
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
            };
        }
        let mut children = Vec::new();
        if style.display == Display::ListItem {
            let marker = self.list_marker(node, style);
            if !marker.is_empty() {
                // An `outside` marker sits in the padding area in CSS; put
                // inline it shifts the first line but keeps the bullet.
                children.push(Child::Inline(whitespace::Raw::Text {
                    text: marker,
                    white_space: h2q_css::WhiteSpace::Pre,
                }));
            }
        }
        self.children_of(node, &mut children);
        LayoutBox {
            node: Some(node),
            style: Cow::Borrowed(style),
            kind,
            content: group(children, style),
        }
    }

    /// Appends the boxes of `node`'s children (and its `::before` /
    /// `::after`) to `out`.
    fn children_of(&mut self, node: NodeId, out: &mut Vec<Child<'a>>) {
        let element_style = self.styles.get(node);
        if let Some(before) = element_style.and_then(|s| s.before.as_deref()) {
            self.pseudo(node, before, out);
        }
        for child in self.doc.children(node) {
            match &self.doc.node(child).data {
                NodeData::Text(text) => {
                    let white_space = self
                        .styles
                        .style(node)
                        .map(|s| s.white_space)
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
        let style = self.styles.style(node);
        let hidden = style.is_none_or(|s| s.display == Display::None);
        if hidden && self.keep.is_some_and(|k| k.contains(&node)) {
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
            Level::LineBreak => out.push(Child::Inline(whitespace::Raw::LineBreak)),
        }
    }

    /// A `display: none` element as an invisible block of its text, styled
    /// like an anonymous child of its parent: its own styles were not
    /// computed (the cascade skips `display: none` subtrees).
    fn hidden_box(&mut self, node: NodeId) -> Option<LayoutBox<'a>> {
        let parent = self.doc.parent_element(node)?;
        let mut style = anonymous_style(self.styles.style(parent)?);
        style.visibility = h2q_css::Visibility::Collapse;
        self.diags.info(
            "css",
            node_line(self.doc, node),
            format!(
                "{} is `display: none` and used by a script: it is kept invisible, as its text in its parent's style",
                describe(self.doc, node)
            ),
        );
        let text = h2q_dom::collapse_whitespace(&self.doc.text_content(node));
        let content = if text.is_empty() {
            Content::Empty
        } else {
            Content::Inline(vec![Inline::Text(text)])
        };
        Some(LayoutBox {
            node: Some(node),
            style: Cow::Owned(style),
            kind: BoxKind::Block,
            content,
        })
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
        if atomic {
            self.diags.info(
                "css",
                node_line(self.doc, node),
                format!(
                    "`display: {}` on {} is laid out as inline text",
                    display_name(style.display),
                    describe(self.doc, node)
                ),
            );
        }
        Level::Inline
    }

    /// Whether an in-flow descendant of `node` (through inline and
    /// `contents` elements only) is block-level, which splits the inline
    /// box in CSS.
    fn has_block_descendant(&self, node: NodeId) -> bool {
        self.doc.element_children(node).any(|child| {
            let Some(style) = self.styles.style(child) else {
                return false;
            };
            if style.display == Display::None {
                return false;
            }
            is_replaced(self.doc, child)
                || is_block_level(style.display)
                || style.float != Float::None
                || matches!(style.position, Position::Absolute | Position::Fixed)
                || self.has_block_descendant(child)
        })
    }

    fn list_marker(&self, node: NodeId, style: &ComputedStyle) -> String {
        if style.list_style_image.is_some() {
            return String::new();
        }
        let ordinal = || {
            let parent = self.doc.parent_element(node);
            let start = parent
                .and_then(|p| self.doc.element(p))
                .filter(|p| p.local_name() == "ol")
                .and_then(|p| p.attr("start"))
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(1);
            let before = self
                .doc
                .prev_siblings(node)
                .filter(|&s| {
                    self.styles
                        .style(s)
                        .is_some_and(|st| st.display == Display::ListItem)
                })
                .count();
            start + i64::try_from(before).unwrap_or(0)
        };
        let marker = match style.list_style_type {
            ListStyleType::None => return String::new(),
            ListStyleType::Disc => "\u{2022}".to_owned(),
            ListStyleType::Circle => "\u{25e6}".to_owned(),
            ListStyleType::Square => "\u{25aa}".to_owned(),
            ListStyleType::Decimal => format!("{}.", ordinal()),
            ListStyleType::DecimalLeadingZero => format!("{:02}.", ordinal()),
            ListStyleType::LowerAlpha => format!("{}.", alpha(ordinal()).to_lowercase()),
            ListStyleType::UpperAlpha => format!("{}.", alpha(ordinal())),
            ListStyleType::LowerRoman => format!("{}.", roman(ordinal()).to_lowercase()),
            ListStyleType::UpperRoman => format!("{}.", roman(ordinal())),
        };
        format!("{marker} ")
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
