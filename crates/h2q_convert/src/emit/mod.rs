//! Step 5: the QML tree. Each box picks its QML form (`docs/spec/layout.md`).
//!
//! [`Emitter::block`] dispatches on [`BoxKind`]; each layout kind has its
//! own submodule.

mod background;
mod controls;
mod decoration;
mod flex;
mod flow;
mod grid;
mod ids;
mod image;
mod inline;
mod links;
mod lists;
mod shadow;
mod table;
mod text;
mod transform;
pub(crate) mod units;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashSet};

use h2q_css::{
    Align, BoxSizing, ComputedStyle, Direction, Display, FlexDirection, Float, LengthAuto,
    Overflow, Position, Size, StyleMap, TextDecorationLine, Visibility,
};
use h2q_dom::{Document, NodeId};
use h2q_js::TargetKind;
use h2q_qml::{Member, Object, Value};

use crate::boxes::{BoxKind, Content, LayoutBox, describe, display_name, node_line};
use crate::diag::Diagnostics;
use crate::{Options, RootKind};
use flow::{Emitted, MarginSet, has_height};
use ids::Ids;
use units::{ROOT_ID, color, number};

const FLICKABLE_ID: &str = "flickable";
const PAGE_ID: &str = "page";

pub(crate) use controls::Form;

/// A converted element scripts can address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub qml_id: String,
    pub kind: TargetKind,
}

pub(crate) struct Emitter<'a, 'd> {
    doc: &'a Document,
    styles: &'a StyleMap,
    options: &'a Options,
    diags: &'d mut Diagnostics,
    ids: Ids,
    /// Elements scripts refer to: they get ids and keep a `Text` to write to.
    wanted: &'a HashSet<NodeId>,
    pub targets: BTreeMap<NodeId, Target>,
    /// Forms with controls, by `<form>` node.
    pub forms: BTreeMap<NodeId, controls::Form>,
    control_ids: BTreeMap<NodeId, String>,
    radio_groups: controls::RadioGroups,
    tapped_labels: HashSet<NodeId>,
    /// The element whose background became the window's (CSS Backgrounds 3
    /// §2.11.2); it is not painted again.
    canvas_node: Option<NodeId>,
    root_node: Option<NodeId>,
    /// The page has a table, so the root needs the table solver.
    has_table: bool,
    /// The box being emitted is an overlay of its positioned parent, so its
    /// `position` is handled.
    positioned_child: bool,
    /// The content height in px of the block whose children are being
    /// laid out, when it is definite: their percentage heights resolve
    /// against it (CSS 2.2 §10.5).
    percent_height_base: Option<f32>,
    /// `position: fixed` boxes, placed over the window outside the
    /// `Flickable` so they do not scroll.
    fixed: Vec<(Option<String>, Object)>,
    /// The plain colour painted behind the box being emitted, when it is
    /// one (a gradient or image makes it unknown).
    backdrop: Option<h2q_css::Rgba>,
    /// The QML id of each element with an HTML id, for fragment links.
    fragments: BTreeMap<String, String>,
    /// A link on the page goes to a fragment (`href="#…"`).
    fragment_links: bool,
    /// The box being emitted is one of a run of floats that share a line.
    in_float_run: bool,
    /// QML modules the emitted objects need beyond the default imports.
    pub imports: std::collections::BTreeSet<&'static str>,
    /// The file each `<img>` shows.
    pub images: BTreeMap<NodeId, crate::images::ImageAsset>,
}

impl<'a, 'd> Emitter<'a, 'd> {
    pub(crate) fn new(
        doc: &'a Document,
        styles: &'a StyleMap,
        options: &'a Options,
        diags: &'d mut Diagnostics,
        wanted: &'a HashSet<NodeId>,
    ) -> Self {
        let mut ids = Ids::default();
        for id in [ROOT_ID, FLICKABLE_ID, PAGE_ID] {
            ids.reserve(id);
        }
        Self {
            doc,
            styles,
            options,
            diags,
            ids,
            wanted,
            targets: BTreeMap::new(),
            forms: BTreeMap::new(),
            control_ids: BTreeMap::new(),
            radio_groups: BTreeMap::new(),
            tapped_labels: HashSet::new(),
            canvas_node: None,
            root_node: None,
            has_table: false,
            positioned_child: false,
            percent_height_base: None,
            fixed: Vec::new(),
            backdrop: None,
            fragments: BTreeMap::new(),
            fragment_links: false,
            in_float_run: false,
            imports: std::collections::BTreeSet::new(),
            images: BTreeMap::new(),
        }
    }

    /// The root object of `Main.qml`: the window, its scrolling surface and
    /// the page.
    pub(crate) fn document(
        &mut self,
        tree: Option<&LayoutBox<'_>>,
        body: Option<&ComputedStyle>,
    ) -> Object {
        let viewport = &self.options.viewport;
        let canvas = match tree {
            Some(html) if !html.style.background_color.is_transparent() => {
                self.canvas_node = html.node;
                html.style.background_color
            }
            _ => match (body, self.doc.body()) {
                (Some(style), Some(node)) if !style.background_color.is_transparent() => {
                    self.canvas_node = Some(node);
                    style.background_color
                }
                _ => h2q_css::Rgba::WHITE,
            },
        };

        self.backdrop = Some(canvas);
        let mut root = match self.options.root {
            RootKind::Window => {
                let mut window = Object::new("ApplicationWindow").with_id(ROOT_ID);
                window.set("visible", true);
                window.set("color", color(canvas));
                if let Some(title) = self.doc.title() {
                    let title = h2q_dom::collapse_whitespace(&title);
                    if !title.is_empty() {
                        window.set("title", Value::String(title));
                    }
                }
                window
            }
            RootKind::Item => {
                let mut item = Object::new("Item").with_id(ROOT_ID);
                item.push_child(
                    Object::new("Rectangle")
                        .prop("anchors.fill", Value::expr("parent"))
                        .prop("color", color(canvas)),
                );
                item
            }
        };
        root.set("width", number(viewport.width));
        root.set("height", number(viewport.height));

        let mut flickable = Object::new("Flickable").with_id(FLICKABLE_ID);
        flickable.set("anchors.fill", Value::expr("parent"));
        flickable.set("contentWidth", Value::expr("width"));
        flickable.set("boundsBehavior", Value::expr("Flickable.StopAtBounds"));
        flickable.set(
            "ScrollBar.vertical",
            Value::Object(Box::new(Object::new("ScrollBar"))),
        );
        let mut content_height = "0".to_owned();
        if let Some(html) = tree {
            self.root_node = html.node;
            let line = html.node.and_then(|n| node_line(self.doc, n));
            let margin = |m: &LengthAuto| match m {
                LengthAuto::Length(l) => l.as_px().unwrap_or(0.0),
                LengthAuto::Auto => 0.0,
            };
            let (mt, mr, mb, ml) = (
                margin(&html.style.margin.top),
                margin(&html.style.margin.right),
                margin(&html.style.margin.bottom),
                margin(&html.style.margin.left),
            );
            let emitted = self.block(html, TextDecorationLine::default());
            let mut page = if emitted.collapse_through {
                Object::new("Item")
            } else {
                emitted.obj
            };
            if let Some(node) = html.node
                && let Some(target) = self.targets.get_mut(&node)
            {
                target.qml_id = PAGE_ID.to_owned();
            }
            page.id = Some(PAGE_ID.to_owned());
            let mut width = format!("{FLICKABLE_ID}.width");
            if ml + mr != 0.0 {
                width = format!("{width} - {}", units::fmt_number(units::round(ml + mr)));
            }
            page.set("width", Value::expr(width));
            if ml != 0.0 {
                page.set("x", number(ml));
            }
            if mt != 0.0 {
                page.set("y", number(mt));
            }
            content_height = format!("{PAGE_ID}.implicitHeight");
            if mt + mb != 0.0 {
                content_height = format!(
                    "{content_height} + {}",
                    units::fmt_number(units::round(mt + mb))
                );
            }
            if html.style.width != Size::Auto || html.style.max_width != Size::None {
                self.diags.info(
                    "css",
                    line,
                    "the root element always spans the window width".to_owned(),
                );
            }
            if self.options.source_comments
                && let Some(node) = html.node
            {
                flickable
                    .members
                    .push(Member::Comment(describe(self.doc, node)));
            }
            flickable.push_child(page);
        }
        flickable.set("contentHeight", Value::expr(content_height));
        root.push_child(flickable);
        for (comment, fixed) in std::mem::take(&mut self.fixed) {
            if let Some(comment) = comment {
                root.members.push(Member::Comment(comment));
            }
            root.push_child(fixed);
        }
        for group in self.radio_groups.values() {
            root.push_child(Object::new("ButtonGroup").with_id(group.clone()));
        }
        if self.has_table {
            Self::table_solver(&mut root);
        }
        if self.fragment_links {
            self.scroll_to_fragment(&mut root);
        }
        self.finish_resets(&mut root);
        self.report_inline_labels();
        root
    }

    /// A block-level box and its margins, as its parent's flow sees it.
    pub(super) fn block(&mut self, b: &LayoutBox<'_>, inherited: TextDecorationLine) -> Emitted {
        self.block_box(b, inherited, false)
    }

    /// A flex or grid item: it establishes an independent formatting
    /// context (CSS Flexbox 1 §4, CSS Grid 2 §6), so no child margin
    /// escapes it, and its own margins do not collapse.
    pub(super) fn item(&mut self, b: &LayoutBox<'_>, inherited: TextDecorationLine) -> Emitted {
        self.block_box(b, inherited, true)
    }

    fn block_box(
        &mut self,
        b: &LayoutBox<'_>,
        inherited: TextDecorationLine,
        independent: bool,
    ) -> Emitted {
        let style = &*b.style;
        let line = b.node.and_then(|n| node_line(self.doc, n));
        match b.kind {
            BoxKind::Block | BoxKind::Flex | BoxKind::Grid => {}
            BoxKind::Replaced => {
                if let Some(node) = b.node
                    && let Some(control) = controls::classify(self.doc, node)
                {
                    let e = self.control(b, node, control, line);
                    return self.transformed(e, b, line);
                }
                if let Some(node) = b.node
                    && self
                        .doc
                        .element(node)
                        .is_some_and(|e| e.is_html() && e.local_name() == "img")
                {
                    let e = self.image(b, node, line);
                    return self.transformed(e, b, line);
                }
                let e = self.replaced(b, line);
                return self.transformed(e, b, line);
            }
            BoxKind::Table => match style.display {
                Display::Table | Display::InlineTable => {
                    self.has_table = true;
                    self.positioned_child = false;
                    let e = self.table(b, inherited, line);
                    return self.transformed(e, b, line);
                }
                Display::TableCell | Display::TableCaption => {}
                _ => self.unsupported_layout(b, line),
            },
        }
        self.out_of_flow(b, line);
        if let Some(node) = b.node
            && let Some((legend, rest)) = fieldset_parts(self.doc, b)
        {
            let e = self.fieldset(b, node, legend, rest, inherited, line);
            return self.transformed(e, b, line);
        }
        let decoration = TextDecorationLine {
            underline: inherited.underline || style.text_decoration_line.underline,
            overline: inherited.overline || style.text_decoration_line.overline,
            line_through: inherited.line_through || style.text_decoration_line.line_through,
        };
        let paint_background = b.node.is_none() || b.node != self.canvas_node;
        let mut deco = self.decoration(style, paint_background, line);
        deco.hint = hint(self.doc, b);
        let wanted = b.node.is_some_and(|n| self.wanted.contains(&n));
        let mt = self.vertical_margin(&style.margin.top, line);
        let mb = self.vertical_margin(&style.margin.bottom, line);
        let bfc_root =
            independent || establishes_bfc(style) || (b.node.is_some() && b.node == self.root_node);
        let top_open = !bfc_root && deco.insets.top == 0.0;
        let bottom_open = !bfc_root && deco.insets.bottom == 0.0 && !has_height(style);

        let mut escaped_top = MarginSet::default();
        let mut escaped_bottom = MarginSet::default();
        let outer_backdrop = self.backdrop;
        self.backdrop = if deco.gradients {
            None
        } else if paint_background {
            outer_backdrop.map(|under| over(style.background_color, under))
        } else {
            outer_backdrop
        };
        let lists::Positioned {
            flow: in_flow,
            overlays,
            fixed,
        } = self.split_positioned(b);
        for f in fixed {
            // The viewport has no border, so the initial style stands in
            // for the containing block's.
            let obj = self.overlay(&ComputedStyle::default(), f);
            let comment = f
                .node
                .filter(|_| self.options.source_comments)
                .map(|n| describe(self.doc, n));
            self.fixed.push((comment, obj));
        }
        let mut inner = match &b.content {
            Content::Blocks(_) if b.kind == BoxKind::Flex => {
                Some((self.flex(b, &in_flow, decoration, line), "Content"))
            }
            Content::Blocks(_) if b.kind == BoxKind::Grid => {
                Some((self.grid(b, &in_flow, decoration, line), "Content"))
            }
            Content::Blocks(_) => {
                let base = match &style.height {
                    Size::Length(h) => h.as_px().map(|px| match style.box_sizing {
                        BoxSizing::ContentBox => px,
                        BoxSizing::BorderBox => (px - deco.insets.vertical()).max(0.0),
                    }),
                    _ => None,
                };
                let outer = std::mem::replace(&mut self.percent_height_base, base);
                let column = self.column(&in_flow, decoration, top_open, bottom_open);
                self.percent_height_base = outer;
                escaped_top = column.escaped_top;
                escaped_bottom = column.escaped_bottom;
                column.obj.map(|o| (o, "Content"))
            }
            Content::Inline(items) if inline::has_atomic(items) => {
                Some((self.inline_flow(b, decoration, items, line), "Content"))
            }
            Content::Inline(items) => {
                let mut text = self.text(style, decoration, items, line);
                if matches!(b.kind, BoxKind::Flex | BoxKind::Grid) {
                    place_anonymous_item(&mut text, style, b.kind);
                }
                Some((text, "Text"))
            }
            Content::Empty => None,
        };
        let inner_backdrop = std::mem::replace(&mut self.backdrop, outer_backdrop);
        if inner.is_none() && (wanted || b.marker.is_some()) {
            // Scripts write text into it; a marker still has its line.
            inner = Some((self.text(style, decoration, &[], line), "Text"));
        }

        if inner.is_none() && top_open && bottom_open && !wanted && overlays.is_empty() {
            let mut top = MarginSet::of(mt);
            // With no item, every child margin is in `escaped_top`.
            top.join(escaped_top);
            top.join(MarginSet::of(mb));
            return Emitted {
                obj: Object::new("Item"),
                top,
                bottom: MarginSet::default(),
                collapse_through: true,
            };
        }
        let mut top = MarginSet::of(mt);
        if top_open {
            top.join(escaped_top);
        }
        let mut bottom = MarginSet::of(mb);
        if bottom_open {
            bottom.join(escaped_bottom);
        }

        let html_id = b
            .node
            .and_then(|n| self.doc.element(n))
            .and_then(|e| e.id())
            .map(str::to_owned);
        let outer_id = match (&html_id, wanted) {
            (Some(id), _) => Some(self.claim_html_id(id)),
            (None, true) => Some(self.ids.claim(&hint(self.doc, b))),
            (None, false) => None,
        };
        // A layout whose children refer to it by id keeps that id, so an
        // HTML id goes on a wrapper.
        let inner_has_id = inner.as_ref().is_some_and(|(o, _)| o.id.is_some());
        let needs_wrapper = deco.needed()
            || inner.is_none()
            || (has_height(style) && inner.is_some())
            || (inner_has_id && outer_id.is_some())
            || b.marker.is_some()
            || !overlays.is_empty()
            || style.display == Display::TableCell;
        let (mut obj, kind) = if needs_wrapper {
            let inner = inner.map(|(mut obj, suffix)| {
                let id = match (&obj.id, &outer_id) {
                    (Some(own), _) => own.clone(),
                    (None, Some(outer)) if self.ids.try_exact(&format!("{outer}{suffix}")) => {
                        format!("{outer}{suffix}")
                    }
                    _ => self.ids.claim(&format!("{} {suffix}", hint(self.doc, b))),
                };
                obj.id = Some(id.clone());
                (obj, id)
            });
            // Flex and grid layouts distribute the box's height (a
            // `height`, `min-height` or a stretching parent) among their
            // items, so they fill it; block flow keeps its content height.
            let fill = matches!(b.kind, BoxKind::Flex | BoxKind::Grid);
            let mut extra = Vec::new();
            if let Some(marker) = &b.marker {
                extra.push(self.marker(b, marker));
            }
            self.backdrop = inner_backdrop;
            for overlay in overlays {
                extra.push(self.overlay(style, overlay));
            }
            self.backdrop = outer_backdrop;
            let wrapper = self.wrap_with(style, &deco, paint_background, inner, extra, fill, line);
            (wrapper, TargetKind::Box)
        } else {
            let (obj, suffix) = inner.unwrap_or((Object::new("Item"), "Content"));
            let kind = if suffix == "Text" {
                TargetKind::Text
            } else {
                TargetKind::Box
            };
            (obj, kind)
        };
        if let Some(id) = &outer_id {
            obj.id = Some(id.clone());
            if let Some(html_id) = html_id.filter(|_| wanted) {
                // The runtime's `getElementById` finds items by it.
                obj.set("objectName", Value::String(html_id));
            }
            if let Some(node) = b.node
                && wanted
            {
                self.targets.insert(
                    node,
                    Target {
                        qml_id: id.clone(),
                        kind,
                    },
                );
            }
        }
        self.label_tap(&mut obj, b.node);
        self.visibility(&mut obj, style);
        let e = Emitted {
            obj,
            top,
            bottom,
            collapse_through: false,
        };
        self.transformed(e, b, line)
    }

    /// `e` with `b`'s `transform`, which every kind of box takes.
    fn transformed(&mut self, mut e: Emitted, b: &LayoutBox<'_>, line: Option<u32>) -> Emitted {
        if !b.style.transform.is_empty() {
            let what = b
                .node
                .map_or_else(|| "a generated box".to_owned(), |n| describe(self.doc, n));
            self.transform(&mut e.obj, &b.style, &hint(self.doc, b), &what, line);
        }
        e
    }

    /// Replaced elements and form controls have no emitter yet: an empty
    /// item keeps the space they would take.
    fn replaced(&mut self, b: &LayoutBox<'_>, line: Option<u32>) -> Emitted {
        let style = &*b.style;
        let element = b.node.and_then(|n| self.doc.element(n));
        let what = b.node.map_or_else(String::new, |n| describe(self.doc, n));
        self.diags.warning(
            "html",
            line,
            format!("{what} is not converted yet; an empty placeholder keeps its space"),
        );
        let attr_px = |name: &str| {
            element
                .and_then(|e| e.attr(name))
                .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
        };
        let css_px = |s: &Size| match s {
            Size::Length(l) => l.as_px(),
            _ => None,
        };
        let mut obj = Object::new("Item");
        if let Some(w) = css_px(&style.width).or_else(|| attr_px("width")) {
            obj.set("implicitWidth", number(w));
        }
        if let Some(h) = css_px(&style.height).or_else(|| attr_px("height")) {
            obj.set("implicitHeight", number(h));
        }
        if let Some(id) = element.and_then(|e| e.id()) {
            obj.id = Some(self.claim_html_id(id));
            if b.node.is_some_and(|n| self.wanted.contains(&n)) {
                // Not a script target (it has none of the control's
                // properties), but the runtime's `getElementById` finds it.
                obj.set("objectName", Value::String(id.to_owned()));
            }
        }
        self.visibility(&mut obj, style);
        let mt = self.vertical_margin(&style.margin.top, line);
        let mb = self.vertical_margin(&style.margin.bottom, line);
        Emitted {
            obj,
            top: MarginSet::of(mt),
            bottom: MarginSet::of(mb),
            collapse_through: false,
        }
    }

    fn unsupported_layout(&mut self, b: &LayoutBox<'_>, line: Option<u32>) {
        let display = b.style.display;
        // One diagnostic per table, not per row and cell.
        let report = !matches!(
            display,
            Display::TableRowGroup
                | Display::TableHeaderGroup
                | Display::TableFooterGroup
                | Display::TableRow
                | Display::TableCell
                | Display::TableColumnGroup
                | Display::TableColumn
                | Display::TableCaption
        );
        if report {
            let what = b
                .node
                .map_or_else(|| "a generated box".to_owned(), |n| describe(self.doc, n));
            self.diags.warning(
                "css",
                line,
                format!(
                    "`display: {}` on {what} is laid out as block flow (not implemented yet)",
                    display_name(display)
                ),
            );
        }
    }

    fn out_of_flow(&mut self, b: &LayoutBox<'_>, line: Option<u32>) {
        let style = &*b.style;
        let what = || {
            b.node
                .map_or_else(|| "a generated box".to_owned(), |n| describe(self.doc, n))
        };
        if style.float != Float::None && !self.in_float_run {
            self.diags.warning(
                "css",
                line,
                format!(
                    "`float` on {} is laid out as a block at its side: floats do not share a line with other boxes yet",
                    what()
                ),
            );
        }
        let positioned = std::mem::take(&mut self.positioned_child);
        match style.position {
            Position::Static => {}
            Position::Relative if is_unmoved(style) => {}
            Position::Absolute | Position::Fixed if positioned => {}
            p => self.diags.warning(
                "css",
                line,
                format!(
                    "`position: {}` on {} is laid out in normal flow (not implemented yet)",
                    format!("{p:?}").to_lowercase(),
                    what()
                ),
            ),
        }
    }

    fn visibility(&mut self, obj: &mut Object, style: &ComputedStyle) {
        let mut opacity = style.opacity;
        match style.visibility {
            Visibility::Visible => {}
            // Keeps its space, as in CSS; Layouts skip invisible items.
            Visibility::Hidden => {
                opacity = 0.0;
                obj.set("enabled", false);
            }
            Visibility::Collapse => {
                obj.set("visible", false);
            }
        }
        if opacity < 1.0 {
            obj.set("opacity", number(opacity));
        }
    }
}

/// Whether a box establishes a block formatting context, which keeps its
/// children's margins inside (CSS 2.2 §9.4.1, CSS Display 3 §3).
fn establishes_bfc(style: &ComputedStyle) -> bool {
    style.overflow_x != Overflow::Visible
        || style.overflow_y != Overflow::Visible
        || style.float != Float::None
        || matches!(style.position, Position::Absolute | Position::Fixed)
        || !matches!(style.display, Display::Block | Display::ListItem)
}

fn is_unmoved(style: &ComputedStyle) -> bool {
    [&style.top, &style.right, &style.bottom, &style.left]
        .iter()
        .all(|v| match v {
            LengthAuto::Auto => true,
            LengthAuto::Length(l) => l.as_px() == Some(0.0),
        })
}

/// The first child `<legend>` of a `<fieldset>` and the boxes after it. A
/// legend later in the fieldset is not "the" legend, so it stays a block.
fn fieldset_parts<'b, 'l>(
    doc: &Document,
    b: &'b LayoutBox<'l>,
) -> Option<(&'b LayoutBox<'l>, &'b [LayoutBox<'l>])> {
    let is = |n: Option<NodeId>, name: &str| {
        n.and_then(|n| doc.element(n))
            .is_some_and(|e| e.is_html() && e.local_name() == name)
    };
    if !is(b.node, "fieldset") {
        return None;
    }
    let Content::Blocks(children) = &b.content else {
        return None;
    };
    let (first, rest) = children.split_first()?;
    is(first.node, "legend").then_some((first, rest))
}

/// A readable base for a generated id: the first class, else the tag.
fn hint(doc: &Document, b: &LayoutBox<'_>) -> String {
    let element = b.node.and_then(|n| doc.element(n));
    element
        .and_then(|e| e.classes().next().map(str::to_owned))
        .or_else(|| element.map(|e| e.local_name().to_owned()))
        .unwrap_or_else(|| "block".to_owned())
}

/// Where along one axis a lone item sits in its flex or grid container.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    Start,
    Center,
    End,
    /// The item is as long as the container.
    Stretch,
}

/// Text directly in a flex or grid container is one anonymous item
/// (CSS Flexbox 1 §4, CSS Grid 1 §6.1) that the container's alignment
/// places, and the Text fills the container, so the item's place becomes
/// the Text's alignment. A line shorter than the container is the whole
/// item, so `text-align` does not move it; wrapped lines make the item as
/// wide as the container and `text-align` places them.
fn place_anonymous_item(text: &mut Object, style: &ComputedStyle, kind: BoxKind) {
    let content = |a: Align| match a {
        Align::Center | Align::SpaceAround | Align::SpaceEvenly => Place::Center,
        Align::End | Align::FlexEnd | Align::Right => Place::End,
        _ => Place::Start,
    };
    let item = |a: Align, stretch: Place| match a {
        Align::Center => Place::Center,
        Align::End | Align::FlexEnd | Align::Right => Place::End,
        Align::Normal | Align::Stretch | Align::Auto => stretch,
        _ => Place::Start,
    };
    let (horizontal, vertical) = if kind == BoxKind::Grid {
        // A lone auto track fills the container under `normal` content
        // alignment, and the item then fills the track.
        let track = |a: Align| match a {
            Align::Normal | Align::Stretch => Place::Stretch,
            other => content(other),
        };
        let h = match item(style.justify_items, Place::Stretch) {
            Place::Stretch => track(style.justify_content),
            p => p,
        };
        let v = match item(style.align_items, Place::Stretch) {
            Place::Stretch => track(style.align_content),
            p => p,
        };
        (h, v)
    } else {
        let main = content(style.justify_content);
        let cross = item(style.align_items, Place::Stretch);
        let flip = |p: Place| match p {
            Place::Start => Place::End,
            Place::End => Place::Start,
            p => p,
        };
        match style.flex_direction {
            FlexDirection::Row => (main, cross),
            FlexDirection::RowReverse => (flip(main), cross),
            FlexDirection::Column => (cross, main),
            FlexDirection::ColumnReverse => (cross, flip(main)),
        }
    };
    let rtl = style.direction == Direction::Rtl;
    let single = match horizontal {
        Place::Stretch => None,
        Place::Center => Some("Text.AlignHCenter"),
        Place::Start if rtl => Some("Text.AlignRight"),
        Place::End if !rtl => Some("Text.AlignRight"),
        Place::Start | Place::End => Some("Text.AlignLeft"),
    };
    // When both agree, `text-align` has already set it.
    let wrapped = text::text_align(style).unwrap_or("Text.AlignLeft");
    if let Some(single) = single.filter(|s| *s != wrapped) {
        text.set(
            "horizontalAlignment",
            Value::expr(format!("lineCount > 1 ? {wrapped} : {single}")),
        );
    }
    let flag = match vertical {
        Place::Center => Some("Text.AlignVCenter"),
        Place::End => Some("Text.AlignBottom"),
        Place::Start | Place::Stretch => None,
    };
    if let Some(flag) = flag {
        text.set("verticalAlignment", Value::expr(flag));
    }
}

/// `top` composited over the opaque `under` (source-over, 8-bit as
/// Chromium paints it).
fn over(top: h2q_css::Rgba, under: h2q_css::Rgba) -> h2q_css::Rgba {
    let a = u32::from(top.a);
    let mix = |t: u8, u: u8| ((u32::from(t) * a + u32::from(u) * (255 - a)) / 255) as u8;
    h2q_css::Rgba::new(
        mix(top.r, under.r),
        mix(top.g, under.g),
        mix(top.b, under.b),
        255,
    )
}
