//! Normal-flow block layout → `ColumnLayout` (layout.md §1).
//!
//! Vertical margins are collapsed here, at conversion time (CSS 2.2
//! §8.3.1): the collapsed gap goes on the following item's
//! `Layout.topMargin`, margins that escape through a parent's top or
//! bottom edge are handed up to the parent's own margins, and empty boxes
//! that collapse through are dropped.

use h2q_css::{BoxSizing, ComputedStyle, Length, LengthAuto, Size, TextDecorationLine};
use h2q_qml::{Member, Object, Value};

use super::Emitter;
use super::units::{length_expr, number, px_part};
use crate::boxes::{LayoutBox, describe};

/// A set of adjoining margins: the largest positive and the most negative
/// one; the collapsed margin is their sum.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct MarginSet {
    pos: f32,
    neg: f32,
}

impl MarginSet {
    pub(crate) fn of(px: f32) -> Self {
        Self {
            pos: px.max(0.0),
            neg: px.min(0.0),
        }
    }

    pub(crate) fn join(&mut self, other: Self) {
        self.pos = self.pos.max(other.pos);
        self.neg = self.neg.min(other.neg);
    }

    pub(crate) fn value(self) -> f32 {
        self.pos + self.neg
    }
}

/// A converted block-level box, as its parent's flow sees it.
pub(crate) struct Emitted {
    pub obj: Object,
    /// Collapsible margin at the top edge (own top margin, joined with
    /// what escaped from the first child).
    pub top: MarginSet,
    pub bottom: MarginSet,
    /// Empty box whose top and bottom margins collapse together; it is
    /// not emitted.
    pub collapse_through: bool,
}

/// The children of a block container as a `ColumnLayout`.
pub(crate) struct Column {
    /// `None` when every child collapsed through.
    pub obj: Option<Object>,
    /// Margins that escaped through the top edge (everything, when there
    /// is no item).
    pub escaped_top: MarginSet,
    pub escaped_bottom: MarginSet,
}

impl Emitter<'_, '_> {
    /// `top_open`/`bottom_open`: whether the parent's own margins collapse
    /// with its first/last child's.
    pub(super) fn column(
        &mut self,
        children: &[LayoutBox<'_>],
        decoration: TextDecorationLine,
        top_open: bool,
        bottom_open: bool,
    ) -> Column {
        let mut layout = Object::new("ColumnLayout");
        layout.set("spacing", Value::Number(0.0));
        let mut pending = MarginSet::default();
        let mut escaped_top = None;
        let mut last: Option<usize> = None;
        for child in children {
            let emitted = self.block(child, decoration);
            if emitted.collapse_through {
                pending.join(emitted.top);
                pending.join(emitted.bottom);
                continue;
            }
            pending.join(emitted.top);
            let mut obj = emitted.obj;
            self.flow_item(&mut obj, child);
            if last.is_none() && top_open {
                escaped_top = Some(pending);
            } else {
                self.set_margin(&mut obj, "Layout.topMargin", pending.value());
            }
            if self.options.source_comments
                && let Some(node) = child.node
            {
                layout
                    .members
                    .push(Member::Comment(describe(self.doc, node)));
            }
            layout.push_child(obj);
            last = Some(layout.members.len() - 1);
            pending = emitted.bottom;
        }
        let Some(last) = last else {
            return Column {
                obj: None,
                escaped_top: pending,
                escaped_bottom: MarginSet::default(),
            };
        };
        let escaped_bottom = if bottom_open {
            pending
        } else {
            if let Some(Member::Child(obj)) = layout.members.get_mut(last) {
                self.set_margin(obj, "Layout.bottomMargin", pending.value());
            }
            MarginSet::default()
        };
        Column {
            obj: Some(layout),
            escaped_top: escaped_top.unwrap_or_default(),
            escaped_bottom,
        }
    }

    /// `Layout.*` sizing of a block-level box in a `ColumnLayout`.
    fn flow_item(&mut self, obj: &mut Object, b: &LayoutBox<'_>) {
        let style = &*b.style;
        let line = b.node.and_then(|n| crate::boxes::node_line(self.doc, n));
        let insets = self.insets(style, line);
        // Sizes are of the content box unless `border-box`; QML sizes are
        // of the whole item.
        let extra_w = match style.box_sizing {
            BoxSizing::ContentBox => insets.horizontal(),
            BoxSizing::BorderBox => 0.0,
        };
        let extra_h = match style.box_sizing {
            BoxSizing::ContentBox => insets.vertical(),
            BoxSizing::BorderBox => 0.0,
        };
        let auto_left = style.margin.left == LengthAuto::Auto;
        let auto_right = style.margin.right == LengthAuto::Auto;
        let alignment = match (auto_left, auto_right) {
            (true, true) => Some("Qt.AlignHCenter"),
            (true, false) => Some("Qt.AlignRight"),
            _ => None,
        };
        let shrink = b.kind == crate::boxes::BoxKind::Replaced;
        match &style.width {
            Size::Length(w) => {
                obj.set("Layout.preferredWidth", size_value(w, extra_w));
                if let Some(a) = alignment {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
            _ if shrink => {
                if let Some(a) = alignment {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
            _ => {
                obj.set("Layout.fillWidth", true);
                // A fill item with a maximum width is centred by the
                // alignment (measured), as auto margins do in CSS.
                if matches!(style.max_width, Size::Length(_))
                    && let Some(a) = alignment
                {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
        }
        if let Size::Length(max) = &style.max_width {
            obj.set("Layout.maximumWidth", size_value(max, extra_w));
        } else if obj.type_name == "ColumnLayout" {
            // A layout's maximum width defaults to its children's largest;
            // a block without `max-width` still spans its container.
            obj.set(
                "Layout.maximumWidth",
                Value::expr("Number.POSITIVE_INFINITY"),
            );
        }
        if let Size::Length(min) = &style.min_width
            && min.as_px() != Some(0.0)
        {
            obj.set("Layout.minimumWidth", size_value(min, extra_w));
        }
        for (name, margin) in [
            ("Layout.leftMargin", &style.margin.left),
            ("Layout.rightMargin", &style.margin.right),
        ] {
            if let LengthAuto::Length(l) = margin
                && l.as_px() != Some(0.0)
            {
                obj.set(name, length_value_nonneg(l));
            }
        }
        match &style.height {
            Size::Length(h) if h.as_px().is_some() => {
                obj.set("Layout.preferredHeight", size_value(h, extra_h));
            }
            Size::Length(_) => {
                self.diags.info(
                    "css",
                    line,
                    format!(
                        "percentage height on {} is treated as `auto`",
                        b.node
                            .map_or("an anonymous box".to_owned(), |n| describe(self.doc, n))
                    ),
                );
            }
            _ => {}
        }
        if let Size::Length(min) = &style.min_height
            && let Some(px) = min.as_px()
            && px != 0.0
        {
            obj.set("Layout.minimumHeight", number(px + extra_h));
        }
        if let Size::Length(max) = &style.max_height
            && let Some(px) = max.as_px()
        {
            obj.set("Layout.maximumHeight", number(px + extra_h));
        }
    }

    fn set_margin(&mut self, obj: &mut Object, name: &str, px: f32) {
        if px > 0.0 {
            obj.set(name, number(px));
        } else if px < 0.0 {
            self.diags.info(
                "qml",
                None,
                "negative vertical margins are approximated as 0".to_owned(),
            );
        }
    }

    /// A box's own vertical margin in px (CSS: `auto` is 0 in block flow;
    /// percentages are of the containing block's width, which is not
    /// known at conversion time).
    pub(super) fn vertical_margin(&mut self, margin: &LengthAuto, line: Option<u32>) -> f32 {
        match margin {
            LengthAuto::Auto => 0.0,
            LengthAuto::Length(l) => {
                let (px, dropped) = px_part(l);
                if dropped {
                    self.diags.info(
                        "css",
                        line,
                        "percentage vertical margin uses its px part only".to_owned(),
                    );
                }
                px
            }
        }
    }
}

/// Whether a box's `height` (or `min-height`) keeps its margins apart.
pub(crate) fn has_height(style: &ComputedStyle) -> bool {
    let definite = |s: &Size| matches!(s, Size::Length(l) if l.as_px() != Some(0.0));
    definite(&style.height) || definite(&style.min_height)
}

fn size_value(length: &Length, extra: f32) -> Value {
    match length.as_px() {
        Some(px) => number(px + extra),
        None if extra == 0.0 => Value::Expr(length_expr(length, "parent.width")),
        None => Value::Expr(format!(
            "{} + {}",
            length_expr(length, "parent.width"),
            super::units::fmt_number(super::units::round(extra))
        )),
    }
}

/// Layout margins are not negative in Qt; negative CSS margins are
/// approximated as 0.
fn length_value_nonneg(l: &Length) -> Value {
    match l.as_px() {
        Some(px) => number(px.max(0.0)),
        None => Value::Expr(format!("Math.max(0, {})", length_expr(l, "parent.width"))),
    }
}
