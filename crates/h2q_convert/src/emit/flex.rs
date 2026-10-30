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

use h2q_css::{
    Align, BoxSizing, ComputedStyle, FlexDirection, FlexWrap, Length, LengthAuto, Size,
    TextDecorationLine,
};
use h2q_qml::{Member, Object, Value};

use super::Emitter;
use super::flow::{is_layout, is_viewport_relative, size_value};
use super::units::{fmt_number, length_expr, number, px_part, round};
use crate::boxes::{LayoutBox, describe, node_line};

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
        let gap = self.gap_px(gap_len.as_ref(), line);

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
        if slots.iter().all(|s| *s == 0) && !any_grow {
            slots = spacer_slots(pack(style.justify_content, reverse), n, reverse);
        } else if any_grow {
            // Growing items take all free space; auto margins get none.
            slots = vec![0; n + 1];
        }
        let spacers = slots.iter().any(|s| *s > 0);

        let mut layout = Object::new(if row { "RowLayout" } else { "ColumnLayout" });
        layout.set("spacing", number(if spacers { 0.0 } else { gap }));
        let spacer = || {
            Object::new("Item").prop(
                if row {
                    "Layout.fillWidth"
                } else {
                    "Layout.fillHeight"
                },
                true,
            )
        };
        for (i, child) in items.iter().enumerate() {
            for _ in 0..slots[i] {
                layout.push_child(spacer());
            }
            let mut obj = self.item(child, decoration).obj;
            let lead_gap = if spacers && i > 0 { gap } else { 0.0 };
            self.flex_item(&mut obj, child, style, axis, unequal_grow, lead_gap);
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
        }
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
        lead_gap: f32,
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
            && style.overflow_y == h2q_css::Overflow::Visible
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
            let cap = basis
                .as_ref()
                .map_or_else(|| "Math.ceil(implicitWidth)".to_owned(), value_text);
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
            (Cross::Stretch, None) => {
                obj.set(&format!("Layout.fill{cross_name}"), true);
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
                                format!("Math.min(Math.ceil(implicitWidth), {})", value_text(m))
                            }
                            None => "Math.ceil(implicitWidth)".to_owned(),
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
                lead_gap
            } else {
                0.0
            };
            let value = match (value, lead) {
                (None, 0.0) => continue,
                (None, lead) => number(lead),
                (Some(Value::Number(v)), lead) => Value::Number(v + round(lead)),
                (Some(v), 0.0) => v,
                (Some(v), lead) => {
                    Value::expr(format!("{} + {}", value_text(&v), fmt_number(round(lead))))
                }
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
        let column_gap = self.gap_px(style.column_gap.as_ref(), line);
        let row_gap = self.gap_px(style.row_gap.as_ref(), line);
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
            let objs: Vec<Object> = items.iter().map(|c| self.item(c, decoration).obj).collect();
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

        let mut flow = Object::new("Flow");
        flow.set("spacing", number(column_gap));
        if row_gap != column_gap {
            self.diags.info(
                "qml",
                line,
                format!(
                    "`Flow` has one spacing: the row gap of {} uses the column gap",
                    self.what(b)
                ),
            );
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
            let mut obj = self.item(child, decoration).obj;
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
            let width = match basis {
                Some(Value::Number(n)) if cs.flex_shrink > 0.0 => Value::expr(format!(
                    "Math.min({}, parent.width)",
                    super::units::fmt_number(n)
                )),
                Some(v) => v,
                None => Value::expr("Math.min(Math.ceil(implicitWidth), parent.width)"),
            };
            obj.set("width", width);
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

    /// A gap in px; `normal` is 0 in flex and grid layout.
    pub(super) fn gap_px(&mut self, gap: Option<&Length>, line: Option<u32>) -> f32 {
        let Some(gap) = gap else { return 0.0 };
        let (px, dropped) = px_part(gap);
        if dropped {
            self.diags.info(
                "css",
                line,
                format!(
                    "gap `{}` uses its px part only",
                    length_expr(gap, "parent.width")
                ),
            );
        }
        px.max(0.0)
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
