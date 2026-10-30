//! Hanging list markers and absolutely positioned children: both are drawn
//! on a box's wrapper, outside the `ColumnLayout` that holds its in-flow
//! content.

use std::borrow::Cow;

use h2q_css::{
    BoxSizing, ComputedStyle, LengthAuto, LineHeight, Position, Size, TextDecorationLine,
};
use h2q_qml::{Object, Value};

use super::Emitter;
use super::decoration::border_width;
use super::flow::size_value;
use super::text::CONTENT_AREA;
use super::units::{color, fmt_number, length_expr, number, round};
use crate::boxes::{BoxKind, Content, LayoutBox, Marker, MarkerKind, node_line};

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

    /// An absolutely positioned child of a positioned box, as an overlay of
    /// the box's wrapper. CSS offsets are from the padding box.
    pub(super) fn overlay(&mut self, parent: &ComputedStyle, b: &LayoutBox<'_>) -> Object {
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
        if let Size::Length(w) = &style.width {
            obj.set("width", size_value(w, extra_w));
        }
        if let Size::Length(h) = &style.height {
            obj.set("height", size_value(h, extra_h));
        }
        let both_sides =
            !matches!(style.left, LengthAuto::Auto) && !matches!(style.right, LengthAuto::Auto);
        if let (Size::Length(max), false, false) = (
            &style.max_width,
            matches!(style.width, Size::Length(_)),
            both_sides,
        ) && let Some(px) = max.as_px()
        {
            obj.set(
                "width",
                Value::expr(format!(
                    "Math.min(implicitWidth, {})",
                    fmt_number(round(px + extra_w))
                )),
            );
        }
        let parent_border = &parent.border;
        place(
            &mut obj,
            ("x", "left", "right", "parent.width"),
            (&style.left, &style.right),
            (
                margin_px(&style.margin.left) + border_width(&parent_border.left),
                margin_px(&style.margin.right) + border_width(&parent_border.right),
            ),
            matches!(style.width, Size::Length(_)),
        );
        place(
            &mut obj,
            ("y", "top", "bottom", "parent.height"),
            (&style.top, &style.bottom),
            (
                margin_px(&style.margin.top) + border_width(&parent_border.top),
                margin_px(&style.margin.bottom) + border_width(&parent_border.bottom),
            ),
            matches!(style.height, Size::Length(_)),
        );
        obj
    }

    /// The in-flow children of a box, its absolute ones when it is
    /// positioned, and its fixed ones, none of which are part of its flow.
    /// Absolute children without a positioned parent stay in flow (reported
    /// by `out_of_flow`); fixed ones always belong to the viewport.
    pub(super) fn split_positioned<'c, 'l>(&self, b: &'c LayoutBox<'l>) -> Positioned<'c, 'l> {
        let Content::Blocks(children) = &b.content else {
            return Positioned {
                flow: Cow::Borrowed(&[]),
                overlays: Vec::new(),
                fixed: Vec::new(),
            };
        };
        let positioned = b.style.position != Position::Static;
        let is_overlay = |c: &LayoutBox<'_>| positioned && c.style.position == Position::Absolute;
        let is_fixed = |c: &LayoutBox<'_>| c.style.position == Position::Fixed;
        if !children.iter().any(|c| is_overlay(c) || is_fixed(c)) {
            return Positioned {
                flow: Cow::Borrowed(children),
                overlays: Vec::new(),
                fixed: Vec::new(),
            };
        }
        Positioned {
            flow: Cow::Owned(
                children
                    .iter()
                    .filter(|c| !is_overlay(c) && !is_fixed(c))
                    .cloned()
                    .collect(),
            ),
            overlays: children.iter().filter(|c| is_overlay(c)).collect(),
            fixed: children.iter().filter(|c| is_fixed(c)).collect(),
        }
    }
}

/// A box's children sorted by how they are placed.
pub(super) struct Positioned<'c, 'l> {
    pub flow: Cow<'c, [LayoutBox<'l>]>,
    /// Absolute children of a positioned box: overlays of its wrapper.
    pub overlays: Vec<&'c LayoutBox<'l>>,
    /// Fixed children: overlays of the window, outside the `Flickable`.
    pub fixed: Vec<&'c LayoutBox<'l>>,
}

fn margin_px(margin: &LengthAuto) -> f32 {
    match margin {
        LengthAuto::Length(l) => l.as_px().unwrap_or(0.0),
        LengthAuto::Auto => 0.0,
    }
}

/// Offsets on one axis: `start` is `left`/`top`, `end` is `right`/`bottom`.
fn place(
    obj: &mut Object,
    (coordinate, start_name, end_name, base): (&str, &str, &str, &str),
    (start, end): (&LengthAuto, &LengthAuto),
    (start_extra, end_extra): (f32, f32),
    definite: bool,
) {
    let with_extra = |l: &h2q_css::Length, extra: f32| match l.as_px() {
        Some(px) => number(px + extra),
        None if extra == 0.0 => Value::Expr(length_expr(l, base)),
        None => Value::Expr(format!(
            "{} + {}",
            length_expr(l, base),
            fmt_number(round(extra))
        )),
    };
    match (start, end) {
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
        // Its static position: approximated as the padding box's corner.
        (LengthAuto::Auto, LengthAuto::Auto) => {
            if start_extra != 0.0 {
                obj.set(coordinate, number(start_extra));
            }
        }
    }
}
