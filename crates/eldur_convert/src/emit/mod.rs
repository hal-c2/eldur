//! Step 5: the QML tree. Each box picks its QML form (`docs/spec/layout.md`).
//!
//! [`Emitter::block`] dispatches on [`BoxKind`]; each layout kind has its
//! own submodule.

pub(crate) mod background;
mod carry;
mod controls;
pub(crate) use controls::control_fonts_json;
mod decoration;
pub(crate) mod dynamic;
mod flex;
mod flow;
mod grid;
mod ids;
mod image;
mod inline;
pub(crate) use background::ELDUR_BACKGROUND_LAYER_QML;
pub(crate) use decoration::ELDUR_ROUND_CLIP_QML;
pub(crate) const ELDUR_FILTER_QML: &str = include_str!("EldurFilter.qml");
pub(crate) const ELDUR_FILTER_QSB: &[u8] = include_bytes!("eldur_filter.frag.qsb");
pub(crate) const ELDUR_TEXT_SHADOW_QML: &str = include_str!("EldurTextShadow.qml");
pub(crate) use inline::{ELDUR_INLINE_BOX_QML, ELDUR_LINES_QML, has_atomic, needs_lines_for_boxes};
mod inline_model;
mod links;
mod lists;
mod paint;
pub(crate) mod shadow;
mod table;
pub(crate) mod templates;
mod text;
mod transform;
pub(crate) mod units;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap, HashSet};

use eldur_css::{
    Align, BoxSizing, ComputedStyle, Direction, Display, FlexDirection, Float, LengthAuto,
    Overflow, Position, Size, StyleMap, TextDecorationLine, Visibility,
};
use eldur_dom::{Document, NodeId};
use eldur_js::TargetKind;
use eldur_qml::{Member, Object, Value};

use crate::boxes::{self, BoxKind, Content, LayoutBox, describe, display_name, node_line};
use crate::diag::Diagnostics;
use crate::{Options, RootKind};
use flow::{Emitted, MarginSet, has_height};
use ids::Ids;
use units::{ROOT_ID, color, fmt_number, number};

const FLICKABLE_ID: &str = "flickable";
const PAGE_ID: &str = "page";
/// The initial containing block (CSS 2.2 §10.1), when a box has it.
const ICB_ID: &str = "initialContainingBlock";

pub(crate) use controls::Form;

/// A converted element scripts can address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub qml_id: String,
    pub kind: TargetKind,
}

pub(crate) struct Emitter<'a, 'd> {
    /// What a percentage height of the current block is of, as a percentage of
    /// the viewport: `html`'s containing block is the viewport, and `height:
    /// 100%` passes that on as far as it goes.
    percent_of_vh: Option<f32>,
    /// A percentage height of the current block is of its layout's height: it
    /// is stretched to a definite height, which only the run time knows.
    percent_of_parent: bool,
    /// Those items add up to the parent's height or more, with a sibling.
    percent_fills: bool,
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
    /// The box being emitted is an overlay of its containing block, so its
    /// `position` is handled.
    positioned_child: bool,
    /// Where `position: absolute` boxes go. At the top it is the initial
    /// containing block.
    abs_block: lists::ContainingBlock,
    /// Where `position: fixed` boxes go, when a transformed ancestor takes
    /// them from the window.
    fixed_block: Option<lists::ContainingBlock>,
    /// An element between the box being emitted and its containing block
    /// can be hidden while the box, which is not its child in QML, cannot.
    hideable_ancestor: bool,
    /// Some overlay reads where its slot is, through the root's functions.
    static_places: bool,
    /// The highest `z-index` of an overlay inside the page, which cannot
    /// reach above the initial containing block's.
    nested_z: Option<i32>,
    /// The `z` the box being emitted needs for something inside it to paint
    /// over its later siblings: every QML item is a stacking context, where
    /// CSS lets a positioned box paint over the page outside its parent.
    above_flow: Option<i32>,
    /// The content height in px of the block whose children are being
    /// laid out, when it is definite: their percentage heights resolve
    /// against it (CSS 2.2 §10.5).
    percent_height_base: Option<f32>,
    /// Whether the column being filled has a width its items cannot change,
    /// so one of them may be told to overflow it.
    column_fills: bool,
    /// `position: fixed` boxes, placed over the window outside the
    /// `Flickable` so they do not scroll.
    fixed: Vec<(Option<String>, Object)>,
    /// The plain colour painted behind the box being emitted, when it is
    /// one (a gradient or image makes it unknown).
    backdrop: Option<eldur_css::Rgba>,
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
    pub background_images: BTreeMap<String, String>,
    /// Scripts reach elements at run time, so every element object says
    /// what it is (`eldurTag`, `eldurClasses`).
    pub runtime_dom: bool,
    /// The runtime resolves some element from the page's rules, and reads what
    /// each element's markup says about itself.
    pub open_elements: bool,
    /// A script writes inline styles or `hidden`, so any item can leave its layout.
    pub hides_items: bool,
    /// The page holds the rule table: the runtime computes styles of its own.
    pub cascade_rules: bool,
    /// The boxes a script may put block children in.
    gains: HashSet<NodeId>,
    /// See `scripts::live_tables`.
    live_tables: HashSet<NodeId>,
    /// A script inserts into something found at run time: any empty box.
    gains_any_empty: bool,
    /// `text-align` of the box whose lines are being laid out: what places
    /// an inline-level child, which has its own for its content.
    line_align: eldur_css::TextAlign,
    /// Events run one dispatch over the path from the target.
    pub by_path: bool,
    /// Scripts read attributes, so elements carry `eldurAttrs`.
    pub script_attrs: bool,
    /// Each element's outermost object carries a marker comment naming its
    /// node, so [`dynamic`] can tell whose properties differ.
    markers: bool,
    /// Who gives the next independent item its height ([`Given`]); its parent
    /// says just before emitting it.
    child_given: Given,
    /// The same for the box being emitted.
    own_given: Given,
    /// Set for the templates' probe document: a template's ids are its
    /// own, so it cannot read a sibling's carry (`carry`).
    no_carry: bool,
    /// Each carried column's last rounding, by the key in its comment
    /// (`carry`).
    excess: HashMap<usize, carry::Excess>,
    /// Elements some condition gives paint, padding or a shadow: they keep
    /// a painting wrapper in every combination, so the forms agree.
    sticky: HashSet<NodeId>,
    /// Elements whose border form differs between combinations: they draw
    /// strips on the sides visible in any of them, in every combination.
    strips: HashMap<NodeId, [bool; 4]>,
    /// Elements `display: none` in the combination being emitted, laid out
    /// as shown and hidden with `visible`.
    collapsed: HashSet<NodeId>,
    /// The control being emitted is sticky.
    force_paint: bool,
    /// The run being written has its elements' boxes painted by `EldurInlineBox`.
    boxes_drawn: bool,
    /// Created inline elements laid out as items of their own in their
    /// line (`boxes::build_with`).
    inline_items: HashSet<NodeId>,
    /// Inline elements and options a script reaches through `eldurInline`.
    pub inline_covered: std::collections::BTreeSet<NodeId>,
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
        for id in [ROOT_ID, FLICKABLE_ID, PAGE_ID, ICB_ID] {
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
            abs_block: lists::ContainingBlock::initial(),
            fixed_block: None,
            hideable_ancestor: false,
            static_places: false,
            nested_z: None,
            above_flow: None,
            percent_height_base: None,
            column_fills: true,
            fixed: Vec::new(),
            backdrop: None,
            fragments: BTreeMap::new(),
            fragment_links: false,
            in_float_run: false,
            imports: std::collections::BTreeSet::new(),
            images: BTreeMap::new(),
            background_images: BTreeMap::new(),
            runtime_dom: false,
            open_elements: false,
            hides_items: false,
            cascade_rules: false,
            gains: HashSet::new(),
            live_tables: HashSet::new(),
            gains_any_empty: false,
            line_align: eldur_css::TextAlign::Start,
            by_path: false,
            script_attrs: false,
            markers: false,
            percent_of_vh: Some(100.0),
            percent_of_parent: false,
            percent_fills: false,
            child_given: Given::Stretched,
            own_given: Given::Content,
            no_carry: false,
            excess: HashMap::new(),
            sticky: HashSet::new(),
            strips: HashMap::new(),
            collapsed: HashSet::new(),
            force_paint: false,
            boxes_drawn: false,
            inline_items: HashSet::new(),
            inline_covered: std::collections::BTreeSet::new(),
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
                _ => eldur_css::Rgba::WHITE,
            },
        };

        self.backdrop = Some(canvas);
        let mut root = match self.options.root {
            RootKind::Window => {
                let mut window = Object::new("ApplicationWindow").with_id(ROOT_ID);
                window.set("visible", true);
                window.set("color", color(canvas));
                if let Some(title) = self.doc.title() {
                    let title = eldur_dom::collapse_whitespace(&title);
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
            let initial = std::mem::take(&mut self.abs_block.overlays);
            let (below, above): (Vec<_>, Vec<_>) = initial
                .into_iter()
                .partition(|(_, o)| matches!(o.get("z"), Some(Value::Number(z)) if *z < 0.0));
            self.report_z_above_initial(&above);
            // Paints over the canvas but under everything in flow.
            if !below.is_empty() {
                let mut block = initial_block();
                block.set("z", Value::Number(-1.0));
                push_overlays(&mut block, below);
                flickable.push_child(block);
            }
            if self.options.source_comments
                && let Some(node) = html.node
            {
                flickable
                    .members
                    .push(Member::Comment(describe(self.doc, node)));
            }
            flickable.push_child(page);
            if !above.is_empty() {
                let mut block = initial_block().with_id(ICB_ID);
                push_overlays(&mut block, above);
                flickable.push_child(block);
                content_height = height_with_initial_block(&content_height);
            }
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
        if self.static_places {
            lists::static_place_functions(&mut root);
        }
        if self.fragment_links {
            self.scroll_to_fragment(&mut root);
        }
        self.finish_resets(&mut root);
        self.report_inline_labels();
        carry::strip_hints(&mut root);
        paint::snap_rounded_boxes(&mut root, false);
        paint::native_text(&mut root, false);
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
        let given = if independent {
            std::mem::replace(&mut self.child_given, Given::Stretched)
        } else {
            Given::Content
        };
        let outer_given = std::mem::replace(&mut self.own_given, given);
        let outer = std::mem::replace(&mut self.line_align, b.style.text_align);
        let e = self.block_box_in_lines(b, inherited, independent);
        self.line_align = outer;
        self.own_given = outer_given;
        e
    }

    fn block_box_in_lines(
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
                    let e = if self.table_to_fill(b) {
                        self.column_to_fill(b, inherited, line)
                    } else {
                        self.table(b, inherited, line)
                    };
                    return self.transformed(e, b, line);
                }
                Display::TableCell | Display::TableCaption => {}
                // A section with no rows yet stands as a box of its own for
                // a script to fill.
                _ if b.node.is_some_and(|n| self.gains.contains(&n)) => {}
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
        if b.node.is_some_and(|n| self.sticky.contains(&n)) {
            deco.paints = true;
        }
        deco.strips = b.node.and_then(|n| self.strips.get(&n).copied());
        let wanted = b.node.is_some_and(|n| self.wanted.contains(&n));
        let mt = self.vertical_margin(&style.margin.top, line);
        let mb = self.vertical_margin(&style.margin.bottom, line);
        let bfc_root =
            independent || establishes_bfc(style) || (b.node.is_some() && b.node == self.root_node);
        let top_open = !bfc_root && deco.insets.top == 0.0;
        let bottom_open = !bfc_root && deco.insets.bottom == 0.0 && !has_height(style);

        let mut escaped_top = MarginSet::default();
        let mut escaped_bottom = MarginSet::default();
        let mut only_slots = false;
        let outer_backdrop = self.backdrop;
        self.backdrop = if deco.gradients {
            None
        } else if paint_background {
            outer_backdrop.map(|under| over(style.background_color, under))
        } else {
            outer_backdrop
        };
        let outer_abs = boxes::is_containing_block(style).then(|| {
            std::mem::replace(
                &mut self.abs_block,
                lists::ContainingBlock::of(b.node, style),
            )
        });
        let outer_fixed = boxes::is_fixed_containing_block(style).then(|| {
            self.fixed_block
                .replace(lists::ContainingBlock::of(b.node, style))
        });
        let outer_above_flow = self.above_flow.take();
        let outer_fills = std::mem::replace(&mut self.column_fills, false);
        let outer_hideable = self.hideable_ancestor;
        self.hideable_ancestor = if outer_abs.is_some() {
            false
        } else {
            outer_hideable
                || style.visibility == eldur_css::Visibility::Collapse
                || b.node
                    .is_some_and(|n| self.collapsed.contains(&n) || self.wanted.contains(&n))
                || (self.markers && b.node.is_some())
        };
        let gains = b.node.is_some_and(|n| {
            self.gains.contains(&n) || (self.gains_any_empty && matches!(b.content, Content::Empty))
        });
        let (in_flow, leaving) = lists::split_out_of_flow(b);
        let mut corner_slots = Vec::new();
        for c in leaving {
            if let Some(mut slot) = self.out_of_flow_box(c, 0.0) {
                if deco.insets.left != 0.0 {
                    slot.set("x", number(deco.insets.left));
                }
                if deco.insets.top != 0.0 {
                    slot.set("y", number(deco.insets.top));
                }
                self.report_aligned_static_place(b, c, line);
                // Not among the container's items, so a script's DOM is
                // told where the box goes between them.
                if self.runtime_dom
                    && let Content::Blocks(children) = &b.content
                    && let Some(order) =
                        children.iter().position(|sibling| std::ptr::eq(sibling, c))
                {
                    slot.members.insert(
                        0,
                        eldur_qml::Member::PropertyDecl {
                            ty: "int".to_owned(),
                            name: "eldurOrder".to_owned(),
                            value: Some(Value::Number(order as f64)),
                            readonly: true,
                            required: false,
                        },
                    );
                }
                corner_slots.push(slot);
            }
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
                let vh = match &style.height {
                    Size::Length(eldur_css::Length::Linear(l))
                        if l.px == 0.0 && l.vw == 0.0 && l.vh == 0.0 =>
                    {
                        self.percent_of_vh.map(|v| v * l.percent / 100.0)
                    }
                    Size::Length(eldur_css::Length::Linear(l))
                        if l.px == 0.0 && l.vw == 0.0 && l.percent == 0.0 =>
                    {
                        Some(l.vh)
                    }
                    _ => None,
                };
                let outer_vh = std::mem::replace(&mut self.percent_of_vh, vh);
                let of_parent = self.own_given == Given::Definite
                    && matches!(style.height, Size::Auto)
                    && !deco.needed();
                let outer_of_parent = std::mem::replace(&mut self.percent_of_parent, of_parent);
                let percents: f32 = in_flow
                    .iter()
                    .map(|k| match &k.style.height {
                        Size::Length(eldur_css::Length::Linear(l)) => l.percent,
                        _ => 0.0,
                    })
                    .sum();
                let fills = of_parent && in_flow.len() > 1 && percents >= 100.0;
                let outer_fills_percent = std::mem::replace(&mut self.percent_fills, fills);
                self.column_fills = b.kind == BoxKind::Block
                    && style.display == eldur_css::Display::Block
                    && style.float == eldur_css::Float::None
                    && !matches!(
                        style.position,
                        eldur_css::Position::Absolute | eldur_css::Position::Fixed
                    )
                    && match &style.width {
                        Size::Auto => outer_fills,
                        Size::Length(l) => l.as_px().is_some(),
                        _ => false,
                    };
                let mut column = self.column(&in_flow, decoration, top_open, bottom_open);
                self.column_fills = false;
                self.percent_height_base = outer;
                self.percent_of_vh = outer_vh;
                self.percent_of_parent = outer_of_parent;
                self.percent_fills = outer_fills_percent;
                if of_parent
                    && !fills
                    && let Some(obj) = &mut column.obj
                {
                    // What is left of its height stays below its items.
                    obj.push_child(Object::new("Item").prop("Layout.fillHeight", true));
                }
                escaped_top = column.escaped_top;
                escaped_bottom = column.escaped_bottom;
                only_slots = column.only_slots;
                column.obj.map(|o| (o, "Content"))
            }
            Content::Inline(items)
                if inline::has_atomic(items)
                    || inline::needs_lines_for_leading(style, items)
                    || inline::needs_lines_for_boxes(items) =>
            {
                Some((self.inline_flow(b, decoration, items, line), "Content"))
            }
            Content::Inline(items) => {
                let mut text = self.text(style, decoration, items, line);
                if self.runtime_dom {
                    self.declare_inline_model(&mut text, b, decoration, items);
                }
                if matches!(b.kind, BoxKind::Flex | BoxKind::Grid) {
                    place_anonymous_item(&mut text, style, b.kind);
                }
                Some((text, "Text"))
            }
            Content::Empty if gains && b.kind == BoxKind::Flex => {
                Some((self.flex_to_fill(b, decoration, line), "Content"))
            }
            Content::Empty if gains && b.kind == BoxKind::Grid => {
                Some((self.grid_to_fill(b, decoration, line), "Content"))
            }
            Content::Empty => None,
        };
        self.column_fills = outer_fills;
        self.backdrop = outer_backdrop;
        self.hideable_ancestor = outer_hideable;
        let overlays = match outer_abs {
            Some(outer) => std::mem::replace(&mut self.abs_block, outer).overlays,
            None => Vec::new(),
        };
        let mut overlays = overlays;
        if let Some(outer) = outer_fixed
            && let Some(own) = std::mem::replace(&mut self.fixed_block, outer)
        {
            overlays.extend(own.overlays);
        }
        if inner.is_none() && (wanted || gains || b.marker.is_some()) {
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
        if only_slots && top_open && bottom_open {
            top.join(bottom);
            bottom = MarginSet::default();
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
        // Text and the children a script adds stack, in one box.
        let text_then_flow = gains && matches!(&inner, Some((_, "Text")));
        let needs_wrapper = deco.needed()
            || text_then_flow
            || inner.is_none()
            || (has_height(style) && inner.is_some())
            || (inner_has_id && outer_id.is_some())
            || b.marker.is_some()
            || !overlays.is_empty()
            || !corner_slots.is_empty()
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
            extra.append(&mut corner_slots);
            extra.extend(overlays.into_iter().map(|(_, overlay)| overlay));
            let flow_text = inner
                .as_ref()
                .filter(|_| text_then_flow)
                .map(|(_, id)| id.clone());
            let mut wrapper =
                self.wrap_with(style, &deco, paint_background, inner, extra, fill, line);
            if let Some(text) = flow_text {
                self.add_script_flow(&mut wrapper, &text, &deco, b);
            }
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
        self.stack_above_flow(&mut obj, b, outer_above_flow);
        let e = Emitted {
            obj,
            top,
            bottom,
            collapse_through: false,
        };
        self.transformed(e, b, line)
    }

    /// Makes the box the text and block children of a script's filling: the
    /// text comes first, the column below, and each takes no height while
    /// it is empty. In CSS an empty box is no taller than its insets.
    fn add_script_flow(
        &mut self,
        wrapper: &mut Object,
        text: &str,
        deco: &decoration::Decoration,
        b: &LayoutBox<'_>,
    ) {
        let flow_id = self.ids.claim(&format!("{} flow", hint(self.doc, b)));
        let insets = deco.insets;
        let text_height = format!("({text}.text === \"\" ? 0 : {text}.implicitHeight)");
        wrapper.set(
            "implicitWidth",
            Value::expr(format!(
                "Math.max({text}.implicitWidth, {flow_id}.implicitWidth) + {}",
                insets.horizontal()
            )),
        );
        wrapper.set(
            "implicitHeight",
            Value::expr(format!(
                "{text_height} + {flow_id}.implicitHeight + {}",
                insets.vertical()
            )),
        );
        let mut flow = Object::new("ColumnLayout");
        flow.id = Some(flow_id);
        flow.set("spacing", Value::Number(0.0));
        flow.set("x", Value::Number(f64::from(insets.left)));
        flow.set("y", Value::expr(format!("{text_height} + {}", insets.top)));
        flow.set(
            "width",
            Value::expr(format!("parent.width - {}", insets.horizontal())),
        );
        for member in &mut wrapper.members {
            if let eldur_qml::Member::Child(child) = member
                && child.id.as_deref() == Some(text)
            {
                child.set("visible", Value::expr("text !== \"\""));
            }
        }
        wrapper.push_child(flow);
    }

    /// Raises an in-flow box that is positioned, or holds one, over its later
    /// siblings. Of the same `z`, the later is on top, as in CSS.
    fn stack_above_flow(&mut self, obj: &mut Object, b: &LayoutBox<'_>, outer: Option<i32>) {
        let style = &*b.style;
        if boxes::is_out_of_flow(style) {
            // Its block has the overlay's own `z` to see to.
            self.above_flow = outer;
            return;
        }
        let holds = boxes::is_containing_block(style);
        let own = match style.z_index {
            // `z-index: 0` is still over what is not positioned.
            Some(z) if holds && z >= 0 => Some(z + 1),
            Some(z) if holds && z < 0 => {
                self.above_flow = outer;
                return;
            }
            _ => None,
        };
        let inside = if own.is_some() { None } else { self.above_flow };
        let level = own.or(inside).or(holds.then_some(1));
        let page = b
            .node
            .is_some_and(|n| Some(n) == self.doc.body() || Some(n) == self.root_node);
        if let Some(z) = level.filter(|_| !page) {
            obj.set("z", Value::Number(f64::from(z)));
        }
        self.above_flow = match (level, outer) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }

    /// `e` with `b`'s `transform`, which every kind of box takes.
    fn transformed(&mut self, mut e: Emitted, b: &LayoutBox<'_>, line: Option<u32>) -> Emitted {
        if !b.style.transform.is_empty() {
            let what = b
                .node
                .map_or_else(|| "a generated box".to_owned(), |n| describe(self.doc, n));
            self.transform(&mut e.obj, &b.style, &hint(self.doc, b), &what, line);
        }
        // Every element box ends here, with its outermost object.
        if self.runtime_dom
            && let Some(node) = b.node
        {
            // An empty block has no line box until a script fills it, but an
            // empty `Text` still sets one.
            if self.wanted.contains(&node)
                && e.obj.type_name == "Text"
                && matches!(e.obj.get("text"), Some(Value::String(t)) if t.is_empty())
            {
                e.obj.set(
                    "Layout.preferredHeight",
                    Value::expr("text === \"\" ? 0 : -1"),
                );
            }
            crate::metadata::tag(&mut e.obj, self.doc, node);
            crate::metadata::declare_whitespace(&mut e.obj, self.doc, node, &self.live_tables);
            // A hidden box's `style` attribute may be what hides it.
            if self.open_elements
                || self.cascade_rules
                || b.style.visibility == eldur_css::Visibility::Collapse
            {
                crate::metadata::declare_style(&mut e.obj, self.doc, node);
            }
            if self.script_attrs {
                crate::metadata::declare_attrs(&mut e.obj, self.doc, node);
            }
        }
        if !self.no_carry && e.obj.id.is_none() && b.node.is_some() {
            e.obj.members.push(carry::hint_comment(&hint(self.doc, b)));
        }
        if let Some(node) = b.node {
            if self.collapsed.contains(&node) {
                e.obj.set("visible", false);
            }
            if self.markers {
                e.obj.members.push(Member::Comment(dynamic::marker(node)));
            }
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
            Position::Absolute | Position::Fixed => self.diags.warning(
                "css",
                line,
                format!(
                    "`position: {}` on {} is ignored: a table part or a control's content stays in its table or control",
                    format!("{:?}", style.position).to_lowercase(),
                    what()
                ),
            ),
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
            // QML multiplies the opacity into each thing drawn, so overlaps
            // would show through; CSS fades the group as one picture.
            if paints_count(obj) > 1 {
                obj.set("layer.enabled", true);
            }
        }
        self.filter(obj, style);
    }

    /// `filter` as the effect of a layer.
    fn filter(&mut self, obj: &mut Object, style: &ComputedStyle) {
        let Some(text) = &style.filter else { return };
        let what = hint_text(text);
        let Some(filter) = crate::filter::parse(text, style.font_size, &self.options.viewport)
        else {
            self.diags.warning(
                "css",
                None,
                format!("`filter: {what}` is not drawn: a `url()` reference has no conversion"),
            );
            return;
        };
        if obj.get("layer.effect").is_some() {
            self.diags.warning(
                "css",
                None,
                format!("`filter: {what}` is not drawn: the box already clips to rounded corners"),
            );
            return;
        }
        let mut effect = Object::new("EldurFilter");
        if filter.recolours() {
            let m = filter.matrix;
            let cells: Vec<String> = m.iter().flatten().map(|v| fmt_number(*v)).collect();
            effect.set(
                "colorMatrix",
                Value::expr(format!("Qt.matrix4x4({})", cells.join(", "))),
            );
            let o: Vec<String> = filter.offset.iter().map(|v| fmt_number(*v)).collect();
            effect.set(
                "colorOffset",
                Value::expr(format!("Qt.vector4d({})", o.join(", "))),
            );
        }
        if filter.blur > 0.0 {
            effect.set("blurDeviation", Value::Number(filter.blur));
        }
        if let Some(d) = filter.drop {
            effect.set("hasDrop", true);
            effect.set("dropX", Value::Number(d.x));
            effect.set("dropY", Value::Number(d.y));
            effect.set("dropDeviation", Value::Number(d.deviation));
            effect.set("dropColor", units::color(d.color));
        }
        obj.set("layer.enabled", true);
        obj.set("layer.effect", Value::Object(Box::new(effect)));
    }
}

/// How many things this object and what it holds draw, as far as two: a group
/// of one needs no layer to fade.
fn paints_count(obj: &Object) -> usize {
    let own = matches!(
        obj.type_name.as_str(),
        "Rectangle" | "Text" | "Image" | "Shape" | "EldurLines" | "EldurBackgroundLayer"
    );
    let mut count = usize::from(own);
    for child in obj.children() {
        if count > 1 {
            break;
        }
        count += paints_count(child);
    }
    count
}

fn hint_text(text: &str) -> String {
    text.chars().take(60).collect()
}

/// An item the size of the initial containing block: the viewport's, at the
/// page's origin, so it scrolls with the page.
fn initial_block() -> Object {
    let mut block = Object::new("Item");
    block.set("width", Value::expr(format!("{FLICKABLE_ID}.width")));
    block.set("height", Value::expr(format!("{FLICKABLE_ID}.height")));
    block
}

/// What overflows the initial containing block can be scrolled to (CSS
/// Overflow 3 §2.2).
fn height_with_initial_block(content_height: &str) -> String {
    format!("Math.max({content_height}, {ICB_ID}.childrenRect.y + {ICB_ID}.childrenRect.height)")
}

/// Gives a page without the initial containing block's item one, for a
/// script's boxes to go to.
/// The block an out-of-flow box the runtime places answers to, by name.
pub(crate) fn declare_initial_block(root: &mut Object) {
    ensure_initial_block(root);
    root.members.insert(
        0,
        Member::PropertyDecl {
            ty: "var".to_owned(),
            name: "eldurInitialBlock".to_owned(),
            value: Some(Value::expr(ICB_ID)),
            readonly: true,
            required: false,
        },
    );
}

fn ensure_initial_block(root: &mut Object) {
    let Some(flickable) = root.members.iter_mut().find_map(|m| match m {
        Member::Child(c) if c.id.as_deref() == Some(FLICKABLE_ID) => Some(c),
        _ => None,
    }) else {
        return;
    };
    let exists = flickable
        .members
        .iter()
        .any(|m| matches!(m, Member::Child(c) if c.id.as_deref() == Some(ICB_ID)));
    if exists {
        return;
    }
    if let Some(Value::Expr(height)) = flickable.get("contentHeight") {
        let height = height_with_initial_block(height);
        flickable.set("contentHeight", Value::expr(height));
    }
    flickable.push_child(initial_block().with_id(ICB_ID));
}

fn push_overlays(parent: &mut Object, overlays: Vec<(Option<String>, Object)>) {
    for (comment, overlay) in overlays {
        if let Some(comment) = comment {
            parent.members.push(Member::Comment(comment));
        }
        parent.push_child(overlay);
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

/// Who gives a box its height, past what its own CSS says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Given {
    /// Nobody: it is as high as its content.
    Content,
    /// A parent stretches it, to a height its content can still raise (an
    /// auto-high flex row, say).
    Stretched,
    /// A parent stretches it to a height its content has no say in.
    Definite,
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
fn over(top: eldur_css::Rgba, under: eldur_css::Rgba) -> eldur_css::Rgba {
    let a = u32::from(top.a);
    let mix = |t: u8, u: u8| ((u32::from(t) * a + u32::from(u) * (255 - a)) / 255) as u8;
    eldur_css::Rgba::new(
        mix(top.r, under.r),
        mix(top.g, under.g),
        mix(top.b, under.b),
        255,
    )
}
