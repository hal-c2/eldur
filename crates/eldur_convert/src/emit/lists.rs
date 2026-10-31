//! Hanging list markers and absolutely positioned children: both are drawn
//! on a box's wrapper, outside the `ColumnLayout` that holds its in-flow
//! content.

use std::borrow::Cow;

use eldur_css::{
    Align, BorderSide, BoxSizing, ComputedStyle, LengthAuto, LineHeight, Position, Sides, Size,
    TextDecorationLine, WhiteSpace,
};
use eldur_dom::NodeId;
use eldur_qml::{Member, Object, Value};

use super::decoration::border_width;
use super::{Emitter, hint};
use super::text::CONTENT_AREA;
use super::units::{ROOT_ID, color, fmt_number, length_expr, number, round};
use crate::boxes::{
    self, BoxKind, Content, LayoutBox, Marker, MarkerKind, describe, is_out_of_flow, node_line,
};

/// Measured in Chromium (10px image, 16px font): an image's right edge is
/// 7px from the box, regardless of the image.
const IMAGE_MARKER_GAP: f32 = 7.0;

/// A bullet's side, in px. Chromium draws it about 0.3em across at any font
/// size (measured at 12, 16, 24 and 40px).
pub(crate) fn bullet_size(font_size: f32) -> f32 {
    (0.3 * font_size).round().max(1.0)
}

/// From the bullet's right edge to the box's border edge, fitted to the same
/// measurements (10, 11, 14 and 19px).
pub(crate) fn bullet_gap(font_size: f32) -> f32 {
    (0.3 * font_size + 6.5).round()
}

/// A bullet's top within a line of height `line_height`: nearly centred, and
/// lower with the font (+0.05em fits the measurements).
pub(crate) fn bullet_top(line_height: f32, size: f32, font_size: f32) -> f32 {
    ((line_height - size) / 2.0 + 0.05 * font_size).round()
}

fn line_height_px(style: &ComputedStyle) -> f32 {
    match style.line_height {
        LineHeight::Normal => CONTENT_AREA * style.font_size,
        LineHeight::Number(n) => n * style.font_size,
        LineHeight::Px(px) => px,
    }
}

impl Emitter<'_, '_> {
    /// The first line's distance from `b`'s top, and its height. The marker
    /// follows the first line, through the padding and borders on the way.
    fn first_line(&mut self, b: &LayoutBox<'_>) -> (f32, f32) {
        let mut offset = 0.0;
        let mut current = b;
        loop {
            let line = current.node.and_then(|n| node_line(self.doc, n));
            offset += self.insets(&current.style, line).top;
            let Content::Blocks(children) = &current.content else {
                return (offset, line_height_px(&current.style));
            };
            let in_flow = children
                .iter()
                .find(|c| !matches!(c.style.position, Position::Absolute | Position::Fixed));
            let Some(child) = in_flow.filter(|c| c.kind == BoxKind::Block) else {
                return (offset, line_height_px(&current.style));
            };
            // With nothing above it, a child's margin escaped past the list
            // item, so the marker's line is the item's top too.
            if offset > 0.0 {
                offset += self.vertical_margin(&child.style.margin.top, line);
            }
            current = child;
        }
    }

    /// An `outside` marker: a child of `b`'s wrapper that hangs left of its
    /// border box, where CSS puts it. It takes no room in any layout.
    pub(super) fn marker(&mut self, b: &LayoutBox<'_>, marker: &Marker<'_>) -> Object {
        let line = b.node.and_then(|n| node_line(self.doc, n));
        let (offset, line_height) = self.first_line(b);
        let style = marker.style;
        let bullet = |shape: &MarkerKind| {
            let size = bullet_size(style.font_size);
            let mut obj = Object::new("Rectangle");
            obj.set("x", number(-(bullet_gap(style.font_size) + size)));
            obj.set(
                "y",
                number(offset + bullet_top(line_height, size, style.font_size)),
            );
            obj.set("width", number(size));
            obj.set("height", number(size));
            match shape {
                MarkerKind::Circle => {
                    obj.set("color", Value::String("transparent".to_owned()));
                    obj.set("border.width", Value::Number(1.0));
                    obj.set("border.color", color(style.color));
                    obj.set("radius", number(size / 2.0));
                }
                MarkerKind::Square => {
                    obj.set("color", color(style.color));
                }
                _ => {
                    obj.set("color", color(style.color));
                    obj.set("radius", number(size / 2.0));
                }
            }
            obj
        };
        match &marker.kind {
            shape @ (MarkerKind::Disc | MarkerKind::Circle | MarkerKind::Square) => bullet(shape),
            MarkerKind::Text(text) => {
                let mut obj = Object::new("Text");
                obj.set("text", Value::String(text.clone()));
                obj.set("textFormat", Value::expr("Text.PlainText"));
                self.font(&mut obj, style, TextDecorationLine::default(), line);
                obj.set("wrapMode", Value::expr("Text.NoWrap"));
                obj.set("anchors.right", Value::expr("parent.left"));
                let shift = (line_height - line_height_px(style)) / 2.0;
                if offset + shift != 0.0 {
                    obj.set("y", number(offset + shift));
                }
                obj
            }
            MarkerKind::Image(url) => {
                let mut obj = Object::new("Image");
                obj.set("source", Value::String(url.clone()));
                obj.set("anchors.right", Value::expr("parent.left"));
                obj.set("anchors.rightMargin", number(IMAGE_MARKER_GAP));
                obj.set(
                    "y",
                    Value::expr(format!(
                        "{} + ({} - height) / 2",
                        fmt_number(round(offset)),
                        fmt_number(round(line_height))
                    )),
                );
                obj
            }
        }
    }

    /// An out-of-flow box goes to its containing block, leaving behind the
    /// slot its static position and visibility are read from, when it needs
    /// them. `margin` is what separates the slot from that static position
    /// on the y axis.
    pub(super) fn out_of_flow_box(&mut self, b: &LayoutBox<'_>, margin: f32) -> Option<Object> {
        let style = &*b.style;
        let fixed = style.position == Position::Fixed;
        let auto = |s: &LengthAuto, e: &LengthAuto| {
            matches!(s, LengthAuto::Auto) && matches!(e, LengthAuto::Auto)
        };
        let placed_by_slot = auto(&style.left, &style.right) || auto(&style.top, &style.bottom);
        let hideable = self.hideable_ancestor;
        let slot = (placed_by_slot || hideable)
            .then(|| self.ids.claim(&format!("{} slot", hint(self.doc, b))));
        let block = if fixed {
            self.fixed_block.as_ref()
        } else {
            Some(&self.abs_block)
        };
        let (node, border) = block.map_or_else(
            || (None, ComputedStyle::default().border),
            |c| (c.node, c.border.clone()),
        );
        self.report_crossed_ancestors(b, node, fixed);
        self.report_place_in_line(b);
        let static_place = slot.as_ref().filter(|_| placed_by_slot).map(|slot| {
            self.static_places = true;
            StaticPlace {
                slot: slot.clone(),
                // Only a containing block in the page is an ancestor of the slot.
                from: if node.is_some() {
                    "parent"
                } else {
                    "flickable.contentItem"
                },
                margin,
            }
        });
        self.hideable_ancestor = false;
        let mut obj = self.overlay(&border, b, static_place.as_ref());
        self.hideable_ancestor = hideable;
        if let Some(slot) = slot.as_ref().filter(|_| hideable)
            && obj.get("visible").is_none()
        {
            obj.set("visible", Value::expr(format!("{slot}.visible")));
        }
        if let (Some(_), Some(z)) = (node, style.z_index) {
            self.nested_z = Some(self.nested_z.map_or(z, |n| n.max(z)));
        }
        let comment = b
            .node
            .filter(|_| self.options.source_comments)
            .map(|n| describe(self.doc, n));
        match (fixed, self.fixed_block.as_mut()) {
            (false, _) => self.abs_block.overlays.push((comment, obj)),
            (true, Some(block)) => block.overlays.push((comment, obj)),
            (true, None) => self.fixed.push((comment, obj)),
        }
        slot.map(|id| Object::new("Item").with_id(id))
    }

    /// A slot goes before the line, whose start it knows and whose text it
    /// does not: a static `x` is where the text before the box ends in CSS.
    fn report_place_in_line(&mut self, b: &LayoutBox<'_>) {
        let style = &*b.style;
        let Some(node) = b.node.filter(|_| {
            style.blockified_inline
                && matches!(style.left, LengthAuto::Auto)
                && matches!(style.right, LengthAuto::Auto)
        }) else {
            return;
        };
        let siblings: Vec<_> = self.doc.prev_siblings(node).collect();
        for sibling in siblings {
            let in_line = match self.doc.text(sibling) {
                Some(text) => !text.trim().is_empty(),
                None => self
                    .styles
                    .computed(sibling)
                    .is_some_and(|s| !is_out_of_flow(&s.style) && s.style.display != eldur_css::Display::None),
            };
            if in_line {
                self.diags.warning(
                    "css",
                    node_line(self.doc, node),
                    format!(
                        "{} has no `left` or `right`, so the text before it would place it; it sits at the start of its line",
                        describe(self.doc, node)
                    ),
                );
                return;
            }
        }
    }

    /// What hoisting a box cannot carry with it, or a containing block no
    /// emitter gives a place to.
    fn report_crossed_ancestors(&mut self, b: &LayoutBox<'_>, block: Option<NodeId>, fixed: bool) {
        let Some(node) = b.node else {
            return;
        };
        let line = node_line(self.doc, node);
        let what = describe(self.doc, node);
        let mut current = self.doc.parent(node);
        while let Some(ancestor) = current {
            current = self.doc.parent(ancestor);
            let Some(style) = self.styles.computed(ancestor).map(|s| &s.style) else {
                continue;
            };
            let holds = if fixed {
                boxes::is_fixed_containing_block(style)
            } else {
                boxes::is_containing_block(style)
            };
            if holds {
                if block != Some(ancestor) {
                    self.diags.warning(
                        "css",
                        line,
                        format!(
                            "{} cannot hold out-of-flow boxes yet, so {what} is placed against the next containing block out (a table, row or fieldset)",
                            describe(self.doc, ancestor)
                        ),
                    );
                }
                return;
            }
            if style.opacity < 1.0 {
                self.diags.warning(
                    "css",
                    line,
                    format!(
                        "{what} is painted without the `opacity` of {}: it leaves that box for its containing block",
                        describe(self.doc, ancestor)
                    ),
                );
            }
        }
    }

    /// A slot is a corner of its flex or grid container, where a lone item
    /// that starts the container goes; alignment moves the item in CSS.
    pub(super) fn report_aligned_static_place(
        &mut self,
        parent: &LayoutBox<'_>,
        b: &LayoutBox<'_>,
        line: Option<u32>,
    ) {
        let start = |a: Align| {
            matches!(
                a,
                Align::Normal | Align::Start | Align::FlexStart | Align::Stretch | Align::Auto
            )
        };
        let aligned = !start(parent.style.justify_content)
            || !start(parent.style.align_items)
            || !start(b.style.align_self);
        if aligned {
            let what = b.node.map_or_else(|| "a generated box".to_owned(), |n| describe(self.doc, n));
            self.diags.warning(
                "css",
                line,
                format!(
                    "{what} has no offsets on an axis, so its flex or grid container's alignment would place it; it sits at the container's start"
                ),
            );
        }
    }

    /// Overlays inside the page stay in the page's stacking order, under
    /// the initial containing block's, whatever their `z-index`.
    pub(super) fn report_z_above_initial(&mut self, initial: &[(Option<String>, Object)]) {
        let Some(nested) = self.nested_z else {
            return;
        };
        let under = initial.iter().any(|(_, o)| match o.get("z") {
            Some(Value::Number(z)) => *z < f64::from(nested),
            _ => true,
        });
        if under {
            self.diags.warning(
                "css",
                None,
                format!(
                    "a box with `z-index: {nested}` inside a positioned element paints under the absolutely positioned boxes that have no positioned ancestor, whatever their `z-index`"
                ),
            );
        }
    }

    /// An out-of-flow box as an overlay of its containing block, whose
    /// border is `parent_border`. CSS offsets are from the padding box.
    pub(super) fn overlay(
        &mut self,
        parent_border: &Sides<BorderSide>,
        b: &LayoutBox<'_>,
        static_place: Option<&StaticPlace>,
    ) -> Object {
        let decoration = TextDecorationLine::default();
        self.positioned_child = true;
        let emitted = self.item(b, decoration);
        self.positioned_child = false;
        let mut obj = emitted.obj;
        let style = &*b.style;
        if let Some(z) = style.z_index {
            obj.set("z", Value::Number(f64::from(z)));
        }
        let line = b.node.and_then(|n| node_line(self.doc, n));
        let insets = self.insets(style, line);
        let (extra_w, extra_h) = match style.box_sizing {
            BoxSizing::ContentBox => (insets.horizontal(), insets.vertical()),
            BoxSizing::BorderBox => (0.0, 0.0),
        };
        let (left_border, right_border) = (
            border_width(&parent_border.left),
            border_width(&parent_border.right),
        );
        let (top_border, bottom_border) = (
            border_width(&parent_border.top),
            border_width(&parent_border.bottom),
        );
        // Percentages are of the padding box; `parent` is the border box.
        let base = |size: &str, borders: f32| {
            if borders == 0.0 {
                format!("parent.{size}")
            } else {
                format!("(parent.{size} - {})", fmt_number(round(borders)))
            }
        };
        let width_base = base("width", left_border + right_border);
        let height_base = base("height", top_border + bottom_border);
        if let Size::Length(w) = &style.width {
            obj.set("width", overlay_size(w, extra_w, &width_base));
        }
        if let Size::Length(h) = &style.height {
            obj.set("height", overlay_size(h, extra_h, &height_base));
        }
        let both_sides =
            !matches!(style.left, LengthAuto::Auto) && !matches!(style.right, LengthAuto::Auto);
        if !matches!(style.width, Size::Length(_)) && !both_sides {
            // Shrink-to-fit: no wider than what the containing block has
            // left (CSS 2.2 §10.3.7).
            let mut width = "implicitWidth".to_owned();
            let mut limited = false;
            if let Size::Length(max) = &style.max_width
                && let Some(px) = max.as_px()
            {
                width = format!("Math.min({width}, {})", fmt_number(round(px + extra_w)));
                limited = true;
            } else if has_text(b) {
                let taken = if matches!(style.left, LengthAuto::Auto)
                    && !matches!(style.right, LengthAuto::Auto)
                {
                    "anchors.rightMargin".to_owned()
                } else {
                    let right = margin_px(&style.margin.right) + right_border;
                    if right == 0.0 {
                        "x".to_owned()
                    } else {
                        format!("x - {}", fmt_number(round(right)))
                    }
                };
                width = format!("Math.min({width}, parent.width - {taken})");
                limited = true;
            }
            if limited {
                obj.set("width", Value::expr(width));
            }
        }
        place(
            &mut obj,
            ("x", "left", "right", "width", &width_base),
            (&style.left, &style.right),
            (&style.margin.left, &style.margin.right),
            (left_border, right_border),
            matches!(style.width, Size::Length(_)),
            static_place.map(|s| (s, 0.0)),
        );
        place(
            &mut obj,
            ("y", "top", "bottom", "height", &height_base),
            (&style.top, &style.bottom),
            (&style.margin.top, &style.margin.bottom),
            (top_border, bottom_border),
            matches!(style.height, Size::Length(_)),
            static_place.map(|s| (s, s.margin)),
        );
        obj
    }
}

/// Where to read a box's static position from.
pub(super) struct StaticPlace {
    slot: String,
    /// The item the position is relative to.
    from: &'static str,
    margin: f32,
}

/// The box a `position: absolute` or `fixed` descendant is placed against.
pub(super) struct ContainingBlock {
    node: Option<NodeId>,
    border: Sides<BorderSide>,
    /// Each overlay and its source comment, in document order.
    pub overlays: Vec<(Option<String>, Object)>,
}

impl ContainingBlock {
    pub(super) fn initial() -> Self {
        Self {
            node: None,
            // The viewport has no border, so the initial style stands in.
            border: ComputedStyle::default().border,
            overlays: Vec::new(),
        }
    }

    pub(super) fn of(node: Option<NodeId>, style: &ComputedStyle) -> Self {
        Self {
            node,
            border: style.border.clone(),
            overlays: Vec::new(),
        }
    }
}

/// The in-flow children of a flex or grid container and the out-of-flow
/// ones, which its layouts must not see. A block container's stay in order:
/// [`Emitter::column`] leaves a slot where each one was.
pub(super) fn split_out_of_flow<'c, 'l>(
    b: &'c LayoutBox<'l>,
) -> (Cow<'c, [LayoutBox<'l>]>, Vec<&'c LayoutBox<'l>>) {
    let Content::Blocks(children) = &b.content else {
        return (Cow::Borrowed(&[]), Vec::new());
    };
    let is_container = matches!(b.kind, BoxKind::Flex | BoxKind::Grid);
    if !is_container || !children.iter().any(|c| is_out_of_flow(&c.style)) {
        return (Cow::Borrowed(children), Vec::new());
    }
    (
        Cow::Owned(
            children
                .iter()
                .filter(|c| !is_out_of_flow(&c.style))
                .cloned()
                .collect(),
        ),
        children
            .iter()
            .filter(|c| is_out_of_flow(&c.style))
            .collect(),
    )
}

/// How far a slot is from the item a box's static position is relative to.
/// Reads every `x` or `y` on the way, so the binding follows layout.
pub(super) fn static_place_functions(root: &mut Object) {
    for (name, axis) in [("staticX", "x"), ("staticY", "y")] {
        root.members.push(Member::Function {
            name: name.to_owned(),
            params: vec!["item".to_owned(), "block".to_owned()],
            body: vec![
                "let at = 0".to_owned(),
                format!("for (; item && item !== block; item = item.parent) at += item.{axis}"),
                "return at".to_owned(),
            ],
        });
    }
}

/// Whether a box has text somewhere in flow that wraps at the width it gets.
fn has_text(b: &LayoutBox<'_>) -> bool {
    match &b.content {
        Content::Inline(_) => matches!(
            b.style.white_space,
            WhiteSpace::Normal | WhiteSpace::PreWrap | WhiteSpace::PreLine
        ),
        Content::Blocks(children) => children
            .iter()
            .any(|c| !is_out_of_flow(&c.style) && has_text(c)),
        Content::Empty => false,
    }
}

/// A `width` or `height` against the containing block's padding box.
fn overlay_size(l: &eldur_css::Length, extra: f32, base: &str) -> Value {
    match l.as_px() {
        Some(px) => number(px + extra),
        None if extra == 0.0 => Value::Expr(length_expr(l, base)),
        None => Value::Expr(format!(
            "{} + {}",
            length_expr(l, base),
            fmt_number(round(extra))
        )),
    }
}

fn margin_px(margin: &LengthAuto) -> f32 {
    match margin {
        LengthAuto::Length(l) => l.as_px().unwrap_or(0.0),
        LengthAuto::Auto => 0.0,
    }
}

/// Offsets on one axis: `start` is `left`/`top`, `end` is `right`/`bottom`.
/// `base` is the padding box's length on the axis.
#[allow(clippy::too_many_arguments)]
fn place(
    obj: &mut Object,
    (coordinate, start_name, end_name, size, base): (&str, &str, &str, &str, &str),
    (start, end): (&LengthAuto, &LengthAuto),
    (start_margin, end_margin): (&LengthAuto, &LengthAuto),
    (start_border, end_border): (f32, f32),
    definite: bool,
    static_place: Option<(&StaticPlace, f32)>,
) {
    let (start_extra, end_extra) = (
        margin_px(start_margin) + start_border,
        margin_px(end_margin) + end_border,
    );
    let with_extra = |l: &eldur_css::Length, extra: f32| match l.as_px() {
        Some(px) => number(px + extra),
        None if extra == 0.0 => Value::Expr(length_expr(l, base)),
        None => Value::Expr(format!(
            "{} + {}",
            length_expr(l, base),
            fmt_number(round(extra))
        )),
    };
    let is_auto = |m: &LengthAuto| matches!(m, LengthAuto::Auto);
    match (start, end) {
        // Both offsets, a size and `auto` margins: centred between them
        // (CSS 2.2 §10.3.7).
        (LengthAuto::Length(s), LengthAuto::Length(e))
            if definite && is_auto(start_margin) && is_auto(end_margin) =>
        {
            let offset = |l: &eldur_css::Length| match l.as_px() {
                Some(px) => fmt_number(round(px)),
                None => length_expr(l, base),
            };
            obj.set(
                coordinate,
                Value::expr(format!(
                    "{start_border} + ({base} - {size} + {} - {}) / 2",
                    offset(s),
                    offset(e)
                )),
            );
        }
        (LengthAuto::Length(s), LengthAuto::Length(e)) if !definite => {
            obj.set(
                &format!("anchors.{start_name}"),
                Value::expr(format!("parent.{start_name}")),
            );
            obj.set(
                &format!("anchors.{end_name}"),
                Value::expr(format!("parent.{end_name}")),
            );
            obj.set(
                &format!("anchors.{start_name}Margin"),
                with_extra(s, start_extra),
            );
            obj.set(
                &format!("anchors.{end_name}Margin"),
                with_extra(e, end_extra),
            );
        }
        (LengthAuto::Length(s), _) => {
            obj.set(coordinate, with_extra(s, start_extra));
        }
        (LengthAuto::Auto, LengthAuto::Length(e)) => {
            obj.set(
                &format!("anchors.{end_name}"),
                Value::expr(format!("parent.{end_name}")),
            );
            obj.set(
                &format!("anchors.{end_name}Margin"),
                with_extra(e, end_extra),
            );
        }
        // Its static position: where its slot is, as if it were in flow.
        (LengthAuto::Auto, LengthAuto::Auto) => {
            if let Some((place, flow_margin)) = static_place {
                let function = if coordinate == "x" { "staticX" } else { "staticY" };
                let read = format!("{ROOT_ID}.{function}({}, {})", place.slot, place.from);
                let margin = flow_margin + margin_px(start_margin);
                obj.set(
                    coordinate,
                    Value::expr(if margin == 0.0 {
                        read
                    } else {
                        format!("{read} + {}", fmt_number(round(margin)))
                    }),
                );
            }
        }
    }
}
