//! Flex containers → `RowLayout` / `ColumnLayout` / `Flow` (layout.md §3).
//!
//! Qt Layouts differ from flexbox in three ways that shape this module
//! (all measured with Qt 6.11):
//!
//! - Extra space is handed to the cells even when no item fills, so items
//!   drift apart. Free space therefore always goes to explicit spacer
//!   items, including a trailing one for `flex-start` packing (layout.md
//!   §3.3 says no spacer; without it the items spread out).
//! - Items that do not fill never shrink below their preferred size.
//!   `flex-shrink` is emulated by filling with the preferred size as the
//!   maximum, which Qt shrinks equally when space runs out.
//! - Stretch factors are only reliable when equal, so equal `flex-grow`
//!   is plain filling and unequal factors are reported.
//!
//! Reverse directions are emitted in reverse order instead of using
//! `layoutDirection`, so margins and spacers stay physical.

use eldur_css::{
    Align, BoxSizing, ComputedStyle, Display, FlexDirection, FlexWrap, Length, LengthAuto, Size,
    TextDecorationLine,
};
use eldur_qml::{Member, Object, Value};

use super::flow::{is_layout, is_viewport_relative, size_value};
use super::units::{Gap, fmt_number, has_percentage, length_expr, number, percent_of};
use super::{Emitter, Given};
use crate::boxes::{BoxKind, LayoutBox, describe, node_line};

/// The stretch factor of growing items against spacers of factor 1: the
/// spacers' share of the free space while the items still grow is below
/// a pixel on any screen (measured: exact at 1200px).
const CAPPED_STRETCH: f64 = 1_000_000.0;

/// Where `justify-content` puts the free space, in visual order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pack {
    Start,
    End,
    Center,
    Between,
    Around,
    Evenly,
}

/// `justify-content` in visual order: reverse directions are emitted
/// reversed, so `flex-start` packs to the visual end. `start`/`end`/`left`/
/// `right` follow the writing mode, not the flex direction.
pub(crate) fn pack(justify: Align, reverse: bool) -> Pack {
    let (start, end) = if reverse {
        (Pack::End, Pack::Start)
    } else {
        (Pack::Start, Pack::End)
    };
    match justify {
        // `normal` and `stretch` behave as `flex-start` (Flexbox 1 §8.2).
        Align::Normal | Align::Stretch | Align::FlexStart | Align::Auto | Align::Baseline => start,
        Align::FlexEnd => end,
        Align::Start | Align::Left => Pack::Start,
        Align::End | Align::Right => Pack::End,
        Align::Center => Pack::Center,
        // With one item, `space-between` is `flex-start`.
        Align::SpaceBetween => Pack::Between,
        Align::SpaceAround => Pack::Around,
        Align::SpaceEvenly => Pack::Evenly,
    }
}

/// How many spacers go into each of the `n + 1` slots around `n` items.
/// Equal fill items share space equally, so `space-around`'s double
/// interior share is two spacers, not a stretch factor.
pub(crate) fn spacer_slots(pack: Pack, n: usize, reverse: bool) -> Vec<u32> {
    let mut slots = vec![0; n + 1];
    match pack {
        Pack::Start => slots[n] = 1,
        Pack::End => slots[0] = 1,
        Pack::Center => {
            slots[0] = 1;
            slots[n] = 1;
        }
        Pack::Between if n <= 1 => {
            // Falls back to `flex-start`, which is the visual end when
            // reversed.
            if reverse {
                slots[0] = 1;
            } else {
                slots[n] = 1;
            }
        }
        Pack::Between => slots[1..n].iter_mut().for_each(|s| *s = 1),
        Pack::Around => {
            slots.iter_mut().for_each(|s| *s = 2);
            slots[0] = 1;
            slots[n] = 1;
        }
        Pack::Evenly => slots.iter_mut().for_each(|s| *s = 1),
    }
    slots
}

/// Cross-axis placement of one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cross {
    Stretch,
    Start,
    End,
    Center,
    Baseline,
}

/// `align-self` (falling back to the container's `align-items`), then
/// cross-axis auto margins, which win (Flexbox 1 §8.1).
pub(crate) fn cross(
    align_items: Align,
    align_self: Align,
    margin_start: &LengthAuto,
    margin_end: &LengthAuto,
) -> Cross {
    match (margin_start, margin_end) {
        (LengthAuto::Auto, LengthAuto::Auto) => return Cross::Center,
        (LengthAuto::Auto, _) => return Cross::End,
        (_, LengthAuto::Auto) => return Cross::Start,
        _ => {}
    }
    let align = if align_self == Align::Auto {
        align_items
    } else {
        align_self
    };
    match align {
        Align::Normal | Align::Auto | Align::Stretch => Cross::Stretch,
        Align::End | Align::FlexEnd | Align::Right => Cross::End,
        Align::Center => Cross::Center,
        Align::Baseline => Cross::Baseline,
        _ => Cross::Start,
    }
}

/// Axis-dependent names.
#[derive(Clone, Copy)]
struct Axis {
    row: bool,
}

impl Axis {
    fn main(self) -> &'static str {
        if self.row { "Width" } else { "Height" }
    }
    fn cross(self) -> &'static str {
        if self.row { "Height" } else { "Width" }
    }
    /// The main size, the base of the gap's percentages.
    fn size(self) -> &'static str {
        if self.row { "width" } else { "height" }
    }
    fn lead_margin(self) -> &'static str {
        if self.row {
            "Layout.leftMargin"
        } else {
            "Layout.topMargin"
        }
    }
}

/// A flex item's size in one axis, if the CSS gives one QML can use.
fn definite_size(length: &Length, horizontal: bool, extra: f32) -> Option<Value> {
    // Percentage heights need a definite containing block, which is not
    // known at conversion time.
    (horizontal || length.as_px().is_some() || is_viewport_relative(length))
        .then(|| size_value(length, extra))
}

fn is_zero(length: &Length) -> bool {
    matches!(length, Length::Linear(l) if l.px == 0.0 && l.percent == 0.0 && l.vw == 0.0 && l.vh == 0.0)
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Number(n) => fmt_number(*n),
        Value::Expr(e) => e.clone(),
        other => format!("{other:?}"),
    }
}

impl Emitter<'_, '_> {
    pub(super) fn flex(
        &mut self,
        b: &LayoutBox<'_>,
        children: &[LayoutBox<'_>],
        decoration: TextDecorationLine,
        line: Option<u32>,
    ) -> Object {
        let style = &*b.style;
        let row = matches!(
            style.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        );
        let reverse = matches!(
            style.flex_direction,
            FlexDirection::RowReverse | FlexDirection::ColumnReverse
        );
        let mut items: Vec<&LayoutBox<'_>> = children.iter().collect();
        // Stable: equal `order` keeps document order (Flexbox 1 §5.4).
        items.sort_by_key(|c| c.style.order);
        if style.flex_wrap != FlexWrap::NoWrap {
            if row {
                return self.flex_wrap(b, &items, decoration, line);
            }
            self.diags.info(
                "css",
                line,
                format!(
                    "`flex-wrap` in a column on {} is laid out without wrapping",
                    self.what(b)
                ),
            );
        }
        if reverse {
            items.reverse();
        }
        let axis = Axis { row };
        let gap_len = if row {
            &style.column_gap
        } else {
            &style.row_gap
        };
        let own_definite = self.gap_height_definite(style, false);
        // A row's stretched item takes the line's height as definite
        // (css-flexbox §9.4 step 11) once the row has a height of its own,
        // even a `min-height` or a percentage that leaves it auto: Chromium
        // resolves the item's `height: 50%` children against it. A gap
        // percentage resolves only against a height that does.
        let line_definite = super::flow::has_height(style) || self.own_given == Given::Definite;
        let own = self.content_px(b, row);
        let gap = Self::gap(gap_len.as_ref(), row || own_definite, own);

        let grows: Vec<f32> = items
            .iter()
            .map(|c| c.style.flex_grow)
            .filter(|g| *g > 0.0)
            .collect();
        let any_grow = !grows.is_empty();
        let unequal_grow = grows.windows(2).any(|w| w[0] != w[1]);
        if unequal_grow {
            self.diags.info(
                "qml",
                line,
                format!(
                    "unequal `flex-grow` on the items of {} is approximated with stretch factors",
                    self.what(b)
                ),
            );
        }

        // Main-axis auto margins take the free space before
        // `justify-content` (Flexbox 1 §8.1).
        let n = items.len();
        let mut slots = vec![0u32; n + 1];
        for (i, c) in items.iter().enumerate() {
            let m = &c.style.margin;
            let (start, end) = if row {
                (&m.left, &m.right)
            } else {
                (&m.top, &m.bottom)
            };
            slots[i] += u32::from(*start == LengthAuto::Auto);
            slots[i + 1] += u32::from(*end == LengthAuto::Auto);
        }
        // Equal growing items that stop at a maximum leave free space, which
        // Qt would hand to the cells. They outweigh the spacers instead, so
        // the spacers only get what the items cannot take.
        let capped = any_grow
            && !unequal_grow
            && items.iter().any(|c| {
                let max = if row {
                    &c.style.max_width
                } else {
                    &c.style.max_height
                };
                c.style.flex_grow > 0.0 && matches!(max, Size::Length(l) if !is_zero(l))
            });
        if slots.iter().all(|s| *s == 0) && (!any_grow || capped) {
            slots = spacer_slots(pack(style.justify_content, reverse), n, reverse);
        } else if any_grow && !capped {
            // Growing items take all free space; auto margins get none.
            slots = vec![0; n + 1];
        }
        let spacers = slots.iter().any(|s| *s > 0);

        let mut layout = Object::new(if row { "RowLayout" } else { "ColumnLayout" });
        layout.set(
            "spacing",
            if spacers {
                Value::Number(0.0)
            } else {
                gap.value(axis.size())
            },
        );
        let stretch = if row {
            "Layout.horizontalStretchFactor"
        } else {
            "Layout.verticalStretchFactor"
        };
        let spacer = || {
            let mut item = Object::new("Item").prop(
                if row {
                    "Layout.fillWidth"
                } else {
                    "Layout.fillHeight"
                },
                true,
            );
            if capped {
                item.set(stretch, 1.0);
            }
            item
        };
        let mut may_lead = self.hides_items;
        let mut lead_follows = false;
        let mut spacers_at = Vec::new();
        for (i, child) in items.iter().enumerate() {
            for _ in 0..slots[i] {
                layout.push_child(spacer());
                spacers_at.push(layout.members.len() - 1);
            }
            self.child_given = if row {
                let (m_start, m_end) = (&child.style.margin.top, &child.style.margin.bottom);
                let stretches = cross(style.align_items, child.style.align_self, m_start, m_end)
                    == Cross::Stretch;
                match (stretches, line_definite) {
                    (true, true) => Given::Definite,
                    (true, false) => Given::Stretched,
                    (false, _) => Given::Content,
                }
            } else if child.style.flex_grow > 0.0 && own_definite {
                Given::Definite
            } else {
                Given::Content
            };
            let mut obj = self.layout_item(child, decoration).obj;
            let lead_gap = if spacers && i > 0 {
                gap.clone()
            } else {
                Gap::default()
            };
            let parent_size = format!("parent.{}", axis.size());
            self.flex_item(&mut obj, child, style, axis, unequal_grow, &lead_gap);
            // The gap is between items in the layout: `display: none` is not one.
            if !lead_gap.is_zero() && may_lead {
                lead_follows = true;
                let name = axis.lead_margin();
                let (with, without) = match obj.get(name) {
                    Some(Value::Number(v)) => {
                        (fmt_number(*v), gap.plus_text(*v as f32, -1.0, &parent_size))
                    }
                    Some(v) => (
                        value_text(v),
                        format!(
                            "{} - ({})",
                            value_text(v),
                            gap.plus_text(0.0, 1.0, &parent_size)
                        ),
                    ),
                    None => (gap.plus_text(0.0, 1.0, &parent_size), "0".to_owned()),
                };
                obj.set(
                    name,
                    Value::expr(format!(
                        "parent && parent.eldurLeads ? (parent.eldurLeads(this) ? {with} : {without}) : {without}"
                    )),
                );
            }
            may_lead |= obj.get("visible").is_some();
            if capped && child.style.flex_grow > 0.0 {
                obj.set(stretch, CAPPED_STRETCH);
            }
            if self.options.source_comments
                && let Some(node) = child.node
            {
                layout
                    .members
                    .push(Member::Comment(describe(self.doc, node)));
            }
            layout.push_child(obj);
        }
        for _ in 0..slots[n] {
            layout.push_child(spacer());
            spacers_at.push(layout.members.len() - 1);
        }
        if lead_follows {
            for at in spacers_at {
                if let Member::Child(item) = &mut layout.members[at] {
                    item.members.push(Member::PropertyDecl {
                        ty: "bool".to_owned(),
                        name: "eldurSpacer".to_owned(),
                        value: Some(Value::Bool(true)),
                        readonly: true,
                        required: false,
                    });
                }
            }
            layout.members.push(Member::Function {
                name: "eldurLeads".to_owned(),
                params: vec!["item".to_owned()],
                body: [
                    "for (const kid of children) {",
                    "    if (kid === item) return false",
                    "    if (kid.visible && !kid.eldurSpacer) return true",
                    "}",
                    "return false",
                ]
                .map(str::to_owned)
                .to_vec(),
            });
        }
        if row {
            one_baseline_shift(&mut layout);
            self.row_excess(&mut layout);
        } else {
            self.carry(&mut layout);
        }
        layout
    }

    /// An empty flex container a script fills. Where the free space goes
    /// depends on how many items it comes to hold, so the runtime places the
    /// spacers, and the gap cannot be left for spacers to replace.
    pub(super) fn flex_to_fill(
        &mut self,
        b: &LayoutBox<'_>,
        decoration: TextDecorationLine,
        line: Option<u32>,
    ) -> Object {
        let mut layout = self.flex(b, &[], decoration, line);
        let style = &*b.style;
        let row = matches!(
            style.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        );
        if row && style.flex_wrap != FlexWrap::NoWrap {
            return layout;
        }
        let reverse = matches!(
            style.flex_direction,
            FlexDirection::RowReverse | FlexDirection::ColumnReverse
        );
        let gap_len = if row {
            &style.column_gap
        } else {
            &style.row_gap
        };
        let definite = row || self.gap_height_definite(style, false);
        let own = self.content_px(b, row);
        let gap = Self::gap(gap_len.as_ref(), definite, own);
        let size = if row { "width" } else { "height" };
        let mut decl = |ty: &str, name: &str, value: Value| {
            layout.members.push(Member::PropertyDecl {
                ty: ty.to_owned(),
                name: name.to_owned(),
                value: Some(value),
                readonly: false,
                required: false,
            });
        };
        // A column as tall as its items has no free space to hold.
        let auto_column = !row && !super::flow::has_height(style);
        let name = match pack(style.justify_content, reverse) {
            _ if auto_column => "none",
            Pack::Start => "start",
            Pack::End => "end",
            Pack::Center => "center",
            Pack::Between => "between",
            Pack::Around => "around",
            Pack::Evenly => "evenly",
        };
        decl("string", "eldurPack", Value::String(name.to_owned()));
        decl("bool", "eldurPackRow", Value::Bool(row));
        decl("real", "eldurPackGap", gap.value(size));
        let between = matches!(name, "between" | "around" | "evenly");
        layout.set(
            "spacing",
            if between {
                Value::Number(0.0)
            } else {
                gap.value(size)
            },
        );
        layout
    }

    /// `Layout.*` sizing and margins of one item of a single-line flex
    /// container. `lead_gap` is the gap before it, when spacers replace
    /// `spacing`.
    fn flex_item(
        &mut self,
        obj: &mut Object,
        b: &LayoutBox<'_>,
        container: &ComputedStyle,
        axis: Axis,
        factors: bool,
        lead_gap: &Gap,
    ) {
        let style = &*b.style;
        let line = b.node.and_then(|n| node_line(self.doc, n));
        let insets = self.insets(style, line);
        let (extra_w, extra_h) = match style.box_sizing {
            BoxSizing::ContentBox => (insets.horizontal(), insets.vertical()),
            BoxSizing::BorderBox => (0.0, 0.0),
        };
        let row = axis.row;
        let (main_extra, cross_extra) = if row {
            (extra_w, extra_h)
        } else {
            (extra_h, extra_w)
        };
        let (main_size, cross_size) = if row {
            (&style.width, &style.height)
        } else {
            (&style.height, &style.width)
        };
        let (min_main, max_main, min_cross, max_cross) = if row {
            (
                &style.min_width,
                &style.max_width,
                &style.min_height,
                &style.max_height,
            )
        } else {
            (
                &style.min_height,
                &style.max_height,
                &style.min_width,
                &style.max_width,
            )
        };
        let limit = |s: &Size, horizontal: bool, extra: f32| match s {
            Size::Length(l) if !is_zero(l) => definite_size(l, horizontal, extra),
            _ => None,
        };
        let main = axis.main();
        let cross_name = axis.cross();
        let layout_item = is_layout(&obj.type_name);

        // `flex-basis: auto` uses the main size (Flexbox 1 §7.2.3).
        // A zero basis in a column still keeps the item's content height:
        // the automatic minimum size of a flex item is its content size
        // unless it is a scroll container (Flexbox 1 §4.5). Rows keep the
        // zero basis, as their content minimum (min-content) is not known.
        let content_min = !row
            && style.overflow_y == eldur_css::Overflow::Visible
            && matches!(style.min_height, Size::Auto);
        let basis = match &style.flex_basis {
            Size::Length(l) if is_zero(l) && content_min => None,
            Size::Length(l) if is_zero(l) => Some(number(main_extra)),
            Size::Length(l) => definite_size(l, row, main_extra),
            Size::Auto => match main_size {
                Size::Length(l) => definite_size(l, row, main_extra),
                _ => None,
            },
            _ => None,
        };
        let max_main = limit(max_main, row, main_extra);
        let grow = style.flex_grow;
        if grow > 0.0 {
            obj.set(&format!("Layout.fill{main}"), true);
            if let Some(v) = &basis {
                obj.set(&format!("Layout.preferred{main}"), v.clone());
            }
            if factors {
                let name = if row {
                    "Layout.horizontalStretchFactor"
                } else {
                    "Layout.verticalStretchFactor"
                };
                obj.set(name, Value::Number(f64::from(grow.round().max(1.0))));
            }
            match max_main {
                Some(v) => {
                    obj.set(&format!("Layout.maximum{main}"), v);
                }
                None if layout_item => {
                    obj.set(
                        &format!("Layout.maximum{main}"),
                        Value::expr("Number.POSITIVE_INFINITY"),
                    );
                }
                None => {}
            }
        } else if style.flex_shrink > 0.0 && row {
            // Filling up to the preferred size is how Qt shrinks an item.
            obj.set(&format!("Layout.fill{main}"), true);
            // A Flow is as wide as it is given: with nothing given, every item
            // on a row of its own. Its row of controls says its own width.
            let natural = if holds_control_row(obj) {
                obj.set("Layout.preferredWidth", Value::expr(CONTROL_ROW_WIDTH));
                CONTROL_ROW_WIDTH.to_owned()
            } else {
                natural_width(obj).to_owned()
            };
            let cap = basis.as_ref().map_or(natural, value_text);
            if let Some(v) = &basis {
                obj.set(&format!("Layout.preferred{main}"), v.clone());
            }
            let cap = match max_main {
                Some(m) => format!("Math.min({cap}, {})", value_text(&m)),
                None => cap,
            };
            // `min-width` beats the natural size and `max-width`, but a Qt
            // layout lets the maximum win.
            let cap = match limit(min_main, row, main_extra) {
                Some(m) => format!("Math.max({cap}, {})", value_text(&m)),
                None => cap,
            };
            obj.set(&format!("Layout.maximum{main}"), Value::expr(cap));
        } else {
            // A Qt layout fills by default; `flex-grow: 0` takes no spare room.
            if layout_item {
                obj.set(&format!("Layout.fill{main}"), false);
            }
            if let Some(v) = &basis {
                obj.set(&format!("Layout.preferred{main}"), v.clone());
                if style.flex_shrink == 0.0 {
                    obj.set(&format!("Layout.minimum{main}"), v.clone());
                }
            }
            if let Some(v) = max_main {
                obj.set(&format!("Layout.maximum{main}"), v);
            }
        }
        if let Some(v) = limit(min_main, row, main_extra) {
            obj.set(&format!("Layout.minimum{main}"), v);
        }

        let (cm_start, cm_end) = if row {
            (&style.margin.top, &style.margin.bottom)
        } else {
            (&style.margin.left, &style.margin.right)
        };
        let placement = cross(container.align_items, style.align_self, cm_start, cm_end);
        let cross_definite = match cross_size {
            Size::Length(l) => definite_size(l, !row, cross_extra),
            _ => None,
        };
        let max_cross = limit(max_cross, !row, cross_extra);
        match (placement, cross_definite) {
            // Stretching block flow would spread its children apart; a CSS
            // block's stay at the top and only its unpainted end moves.
            (Cross::Stretch, None)
                if row
                    && obj.type_name == "ColumnLayout"
                    && b.kind == BoxKind::Block
                    && !super::flow::holds_percent_height(b) =>
            {
                obj.set("Layout.fillHeight", false);
                obj.set("Layout.alignment", Value::expr("Qt.AlignTop"));
            }
            (Cross::Stretch, None) => {
                obj.set(&format!("Layout.fill{cross_name}"), true);
                // Its percentage children can overflow it, not raise the row.
                if row && b.kind == BoxKind::Block && super::flow::holds_percent_height(b) {
                    obj.set("Layout.minimumHeight", Value::Number(0.0));
                }
                match max_cross {
                    Some(v) => {
                        obj.set(&format!("Layout.maximum{cross_name}"), v);
                    }
                    None if layout_item => {
                        obj.set(
                            &format!("Layout.maximum{cross_name}"),
                            Value::expr("Number.POSITIVE_INFINITY"),
                        );
                    }
                    None => {}
                }
                if !row && max_cross_set(style) {
                    obj.set("Layout.alignment", Value::expr("Qt.AlignLeft"));
                }
            }
            (placement, definite) => {
                // Layouts fill by default, which would defeat the
                // alignment (the column fit-content case fills on purpose).
                if layout_item && (row || definite.is_some()) {
                    obj.set(&format!("Layout.fill{cross_name}"), false);
                }
                match definite {
                    Some(v) => {
                        obj.set(&format!("Layout.preferred{cross_name}"), v);
                    }
                    // Fit-content in a column: filling up to the natural
                    // width wraps long text and keeps short text narrow
                    // (a non-filling wrapped Text overflows instead).
                    None if !row => {
                        obj.set("Layout.fillWidth", true);
                        let cap = match &max_cross {
                            Some(m) => {
                                format!("Math.min({}, {})", natural_width(obj), value_text(m))
                            }
                            None => natural_width(obj).to_owned(),
                        };
                        obj.set("Layout.maximumWidth", Value::expr(cap));
                    }
                    None => {
                        if let Some(v) = max_cross {
                            obj.set(&format!("Layout.maximum{cross_name}"), v);
                        }
                    }
                }
                let flag = match (placement, row) {
                    (Cross::Start | Cross::Stretch, true) => "Qt.AlignTop",
                    (Cross::End, true) => "Qt.AlignBottom",
                    (Cross::Center, true) => "Qt.AlignVCenter",
                    // Measured: RowLayout aligns text baselines.
                    (Cross::Baseline, true) => "Qt.AlignBaseline",
                    (Cross::Start | Cross::Stretch | Cross::Baseline, false) => "Qt.AlignLeft",
                    (Cross::End, false) => "Qt.AlignRight",
                    (Cross::Center, false) => "Qt.AlignHCenter",
                };
                obj.set("Layout.alignment", Value::expr(flag));
            }
        }
        if let Some(v) = limit(min_cross, !row, cross_extra) {
            obj.set(&format!("Layout.minimum{cross_name}"), v);
        }

        // Flex items' margins never collapse.
        let mt = self.vertical_margin(&style.margin.top, line);
        let mb = self.vertical_margin(&style.margin.bottom, line);
        let edges: [(&str, Option<Value>); 4] = [
            ("Layout.leftMargin", horizontal_margin(&style.margin.left)),
            ("Layout.rightMargin", horizontal_margin(&style.margin.right)),
            ("Layout.topMargin", Some(number(mt.max(0.0)))),
            ("Layout.bottomMargin", Some(number(mb.max(0.0)))),
        ];
        if mt < 0.0 || mb < 0.0 {
            self.diags.info(
                "qml",
                line,
                "negative margins on flex items are approximated as 0".to_owned(),
            );
        }
        for (name, value) in edges {
            let lead = if name == axis.lead_margin() {
                lead_gap.clone()
            } else {
                Gap::default()
            };
            let base = format!("parent.{}", axis.size());
            let value = match (value, lead.is_zero()) {
                (None, true) => continue,
                (None, false) => lead.value(&base),
                (Some(Value::Number(v)), _) => lead.plus(v as f32, 1.0, &base),
                (Some(v), true) => v,
                (Some(v), false) => Value::expr(format!(
                    "{} + {}",
                    value_text(&v),
                    lead.plus_text(0.0, 1.0, &base)
                )),
            };
            if value != Value::Number(0.0) {
                obj.set(name, value);
            }
        }
    }

    /// A wrapping row: a `Flow`, or a grid when every item grows from the
    /// same basis (layout.md §3.5).
    fn flex_wrap(
        &mut self,
        b: &LayoutBox<'_>,
        items: &[&LayoutBox<'_>],
        decoration: TextDecorationLine,
        line: Option<u32>,
    ) -> Object {
        let style = &*b.style;
        let (own_w, own_h) = (self.content_px(b, true), self.content_px(b, false));
        let column_gap = Self::gap(style.column_gap.as_ref(), true, own_w);
        let definite = self.gap_height_definite(style, false);
        let row_gap = Self::gap(style.row_gap.as_ref(), definite, own_h);
        if let Some(min_cell) = self.uniform_basis(items) {
            self.diags.info(
                "qml",
                line,
                format!(
                    "wrapping flex items of {} that grow from one basis are laid out as an \
                     auto-fit grid: a short last line keeps the column width",
                    self.what(b)
                ),
            );
            let objs: Vec<Object> = items
                .iter()
                .map(|c| self.layout_item(c, decoration).obj)
                .collect();
            let comments: Vec<Option<String>> = items
                .iter()
                .map(|c| c.node.map(|n| describe(self.doc, n)))
                .collect();
            return self.auto_fit_grid(
                b,
                min_cell,
                true,
                column_gap,
                row_gap,
                objs.into_iter().zip(comments).collect(),
            );
        }

        // `Flow` has one spacing for both axes, so when the gaps differ the
        // items go in an `EldurFlow`, a positioner with a spacing for each.
        // It is only a default `implicitWidth`, unlike `Flow`'s read-only
        // one, so an `inline-flex` box with a set width gets one too.
        let fixed_inline = style.display == Display::InlineFlex
            && matches!(&style.width, Size::Length(l) if l.as_px().is_some());
        // Compared in px: equal percentages still resolve against
        // different axes, the column gap the width and the row gap the
        // height, so only two fixed gaps of one length make a `Flow`.
        let own = row_gap.px().is_none() || row_gap.px() != column_gap.px() || fixed_inline;
        let mut flow = Object::new(if own { "EldurFlow" } else { "Flow" });
        flow.set("spacing", column_gap.value("width"));
        if own {
            flow.set("rowSpacing", row_gap.value("height"));
        }
        if style.flex_direction == FlexDirection::RowReverse {
            flow.set("layoutDirection", Value::expr("Qt.RightToLeft"));
        }
        if style.flex_wrap == FlexWrap::WrapReverse {
            self.diags.info(
                "qml",
                line,
                "`flex-wrap: wrap-reverse` is laid out as `wrap`".to_owned(),
            );
        }
        if !matches!(
            pack(style.justify_content, false),
            Pack::Start | Pack::Between
        ) || (style.justify_content == Align::SpaceBetween && items.len() > 1)
        {
            self.diags.info(
                "qml",
                line,
                format!(
                    "`justify-content` on the wrapping {} is not converted: lines are packed \
                     to the start",
                    self.what(b)
                ),
            );
        }
        let mut lossy_margins = false;
        let mut grows = false;
        let mut fixed_heights = Vec::new();
        for child in items {
            let mut obj = self.layout_item(child, decoration).obj;
            let cs = &*child.style;
            let cline = child.node.and_then(|n| node_line(self.doc, n));
            let insets = self.insets(cs, cline);
            let (extra_w, extra_h) = match cs.box_sizing {
                BoxSizing::ContentBox => (insets.horizontal(), insets.vertical()),
                BoxSizing::BorderBox => (0.0, 0.0),
            };
            let basis = match (&cs.flex_basis, &cs.width) {
                (Size::Length(l), _) | (Size::Auto, Size::Length(l)) => {
                    Some(size_value(l, extra_w))
                }
                _ => None,
            };
            // A shrinkable item alone on a line narrower than its basis
            // shrinks to the line (CSS Flexbox §9.7).
            let unclamped = match basis {
                Some(Value::Number(n)) if cs.flex_shrink > 0.0 => Some(fmt_number(n)),
                Some(v) => {
                    obj.set("width", v);
                    None
                }
                None if holds_control_row(&obj) => Some(CONTROL_ROW_WIDTH.to_owned()),
                None => Some(natural_width(&obj).to_owned()),
            };
            if let Some(unclamped) = unclamped {
                if own {
                    // The width before the line clamps it: an `EldurFlow`
                    // sums these for its max-content `implicitWidth`, which
                    // the clamped widths of a box capped by it would hold
                    // at nothing.
                    obj.members.push(Member::PropertyDecl {
                        ty: "real".to_owned(),
                        name: "eldurBasis".to_owned(),
                        value: Some(Value::expr(unclamped)),
                        readonly: false,
                        required: false,
                    });
                    obj.set("width", Value::expr("Math.min(eldurBasis, parent.width)"));
                } else {
                    obj.set(
                        "width",
                        Value::expr(format!("Math.min({unclamped}, parent.width)")),
                    );
                }
            }
            match &cs.height {
                Size::Length(l) if l.as_px().is_some() || is_viewport_relative(l) => {
                    fixed_heights.push(l.as_px());
                    obj.set("height", size_value(l, extra_h));
                }
                _ => fixed_heights.push(None),
            }
            let m = &cs.margin;
            lossy_margins |= [&m.top, &m.right, &m.bottom, &m.left]
                .iter()
                .any(|m| !matches!(m, LengthAuto::Length(l) if l.as_px() == Some(0.0)));
            grows |= cs.flex_grow > 0.0;
            if self.options.source_comments
                && let Some(node) = child.node
            {
                flow.members.push(Member::Comment(describe(self.doc, node)));
            }
            flow.push_child(obj);
        }
        if lossy_margins {
            self.diags.info(
                "qml",
                line,
                format!(
                    "margins of the wrapping items of {} are ignored",
                    self.what(b)
                ),
            );
        }
        if grows {
            self.diags.info(
                "qml",
                line,
                format!(
                    "`flex-grow` on the wrapping items of {} is not converted",
                    self.what(b)
                ),
            );
        }
        let stretch = matches!(style.align_items, Align::Normal | Align::Stretch);
        let same_height = fixed_heights.first().is_some_and(|h| h.is_some())
            && fixed_heights.windows(2).all(|w| w[0] == w[1]);
        if stretch && items.len() > 1 && !same_height {
            self.diags.info(
                "qml",
                line,
                format!(
                    "items of the wrapping {} are not stretched to their line's height",
                    self.what(b)
                ),
            );
        }
        flow
    }

    /// The border-box basis every item grows from, when all items grow
    /// equally from the same px basis.
    fn uniform_basis(&mut self, items: &[&LayoutBox<'_>]) -> Option<f32> {
        let first = items.first()?;
        let grow = first.style.flex_grow;
        let mut basis = None;
        for c in items {
            let cs = &*c.style;
            let px = match (&cs.flex_basis, &cs.width) {
                (Size::Length(l), _) | (Size::Auto, Size::Length(l)) => l.as_px()?,
                _ => return None,
            };
            if cs.flex_grow != grow || grow <= 0.0 || px <= 0.0 {
                return None;
            }
            let line = c.node.and_then(|n| node_line(self.doc, n));
            let extra = match cs.box_sizing {
                BoxSizing::ContentBox => self.insets(cs, line).horizontal(),
                BoxSizing::BorderBox => 0.0,
            };
            let px = px + extra;
            if basis.is_some_and(|b| b != px) {
                return None;
            }
            basis = Some(px);
        }
        basis
    }

    /// A gap. A percentage of a size that is not definite resolves
    /// against 0 (CSS Box Alignment 3 §8.1), as Chromium has it for a
    /// column flex container of auto height. One of a px size (`own`, the
    /// content box's) is a constant: bound to the layout's own size, it
    /// would change the layout's spacing while Qt arranges it, which Qt
    /// stops as a recursive rearrange.
    pub(super) fn gap(gap: Option<&Length>, definite: bool, own: Option<f32>) -> Gap {
        Gap::new(gap.map(|l| match own {
            _ if !has_percentage(l) => l.clone(),
            Some(px) => percent_of(l, px),
            None if definite => l.clone(),
            None => percent_of(l, 0.0),
        }))
    }

    /// Whether a percentage gap along the block axis has a height to
    /// resolve against. A percentage of an auto height is auto (CSS 2
    /// §10.5), so only a length, a percentage of a definite height, or a
    /// parent that stretches the box to a set height gives one. A grid
    /// resolves its gaps against its laid-out height, so a `min-height`
    /// gives it one too; a flex container's stays 0 then, as in Chromium.
    pub(super) fn gap_height_definite(&self, style: &ComputedStyle, min_counts: bool) -> bool {
        let height = match &style.height {
            // A zero height adds nothing to resolve, and keeps a grid that
            // the rest of the emitter treats as auto-high.
            Size::Length(l) if l.as_px() == Some(0.0) => false,
            Size::Length(l) if is_viewport_relative(l) => true,
            Size::Length(_) => self.percent_resolves(),
            _ => false,
        };
        let min = matches!(&style.min_height, Size::Length(m) if m.as_px() != Some(0.0));
        height || (min_counts && min) || self.own_given == Given::Definite
    }

    /// The px size of `b`'s content box along an axis, when its style
    /// gives one. A percentage gap is of the used size (CSS Box Alignment
    /// §8.1), so a px `max-*` caps the set size and a px `min-*` then
    /// raises it (CSS 2 §10.4); a cap in any other unit leaves the size
    /// to the laid-out item, which the gap's percentage then follows.
    pub(super) fn content_px(&mut self, b: &LayoutBox<'_>, horizontal: bool) -> Option<f32> {
        let style = &*b.style;
        let (size, min, max) = if horizontal {
            (&style.width, &style.min_width, &style.max_width)
        } else {
            (&style.height, &style.min_height, &style.max_height)
        };
        let Size::Length(l) = size else { return None };
        let mut px = l.as_px()?;
        if let Size::Length(m) = max {
            px = px.min(m.as_px()?);
        }
        if let Size::Length(m) = min {
            px = px.max(m.as_px()?);
        }
        Some(match style.box_sizing {
            BoxSizing::ContentBox => px,
            BoxSizing::BorderBox => {
                let line = b.node.and_then(|n| node_line(self.doc, n));
                let insets = self.insets(style, line);
                let inner = if horizontal {
                    insets.horizontal()
                } else {
                    insets.vertical()
                };
                (px - inner).max(0.0)
            }
        })
    }

    pub(super) fn what(&self, b: &LayoutBox<'_>) -> String {
        b.node
            .map_or_else(|| "a generated box".to_owned(), |n| describe(self.doc, n))
    }
}

fn max_cross_set(style: &ComputedStyle) -> bool {
    matches!(&style.max_width, Size::Length(_))
}

/// A horizontal margin as a `Layout` margin (never negative in Qt).
fn horizontal_margin(margin: &LengthAuto) -> Option<Value> {
    match margin {
        LengthAuto::Auto => None,
        LengthAuto::Length(l) => match l.as_px() {
            Some(px) => Some(number(px.max(0.0))),
            None => Some(Value::expr(format!(
                "Math.max(0, {})",
                length_expr(l, "parent.width")
            ))),
        },
    }
}

const CONTROL_ROW_WIDTH: &str =
    "Math.ceil(Array.from(children).reduce((sum, item) => sum + item.implicitWidth, 0))";

/// `EldurFlow.qml`, written beside `Main.qml` when a wrapping flex
/// container's row gap and column gap differ.
pub(crate) const ELDUR_FLOW_QML: &str = include_str!("EldurFlow.qml");

/// A wrapping Text given a hair less than its fractional implicit width
/// breaks its last word, so text boxes are capped at the whole pixel above.
/// A button's label doesn't wrap, and Chromium sizes buttons to the
/// fraction (a 45.72px "Add"), so a button keeps its exact width.
fn natural_width(obj: &Object) -> &'static str {
    if obj.type_name == "Button" {
        "implicitWidth"
    } else {
        "Math.ceil(implicitWidth)"
    }
}

/// Controls are items of a `Flow` only when they are all its line holds.
pub(super) fn holds_control_row(obj: &Object) -> bool {
    matches!(obj.type_name.as_str(), "Flow" | "EldurFlow")
        && obj.members.iter().any(|m| {
            matches!(m, Member::Child(c) if matches!(
                c.type_name.as_str(),
                "Button" | "TextField" | "TextArea" | "ComboBox" | "CheckBox" | "RadioButton"
            ))
        })
}

/// RowLayout has already lined up Qt's baselines, so each Text moving by its
/// own shift would part them. The row's baseline is its largest font's, in
/// Chromium as in Qt, so that one's shift moves them all.
fn one_baseline_shift(layout: &mut Object) {
    let on_baseline = |o: &Object| {
        matches!(o.get("Layout.alignment"), Some(Value::Expr(e)) if e == "Qt.AlignBaseline")
            && o.get("transform").is_some()
    };
    let size = |o: &Object| match o.get("font.pixelSize") {
        Some(Value::Number(n)) => *n,
        _ => 0.0,
    };
    let Some(shift) = layout
        .children()
        .filter(|o| on_baseline(o))
        .max_by(|a, b| size(a).total_cmp(&size(b)))
        .and_then(|o| o.get("transform").cloned())
    else {
        return;
    };
    for item in layout.children_mut().filter(|o| on_baseline(o)) {
        item.set("transform", shift.clone());
    }
}
