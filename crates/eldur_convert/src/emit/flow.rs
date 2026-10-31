//! Normal-flow block layout → `ColumnLayout` (layout.md §1).
//!
//! Vertical margins are collapsed here, at conversion time (CSS 2.2
//! §8.3.1): the collapsed gap goes on the following item's
//! `Layout.topMargin`, margins that escape through a parent's top or
//! bottom edge are handed up to the parent's own margins, and empty boxes
//! that collapse through are dropped.

use eldur_css::{
    BoxSizing, Clear, ComputedStyle, Display, Float, Length, LengthAuto, Size, TextAlign,
    TextDecorationLine, Visibility,
};
use eldur_qml::{Member, Object, Value};

use super::Emitter;
use super::units::{fmt_number, length_expr, number, px_part, round};
use crate::boxes::{LayoutBox, describe, is_out_of_flow};

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
    /// Only out-of-flow boxes' slots, which take no room: the box's own
    /// margins still collapse through it.
    pub only_slots: bool,
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
        // The last item was a hidden box a script can show (`display:
        // none`, kept invisible): its id and the margins before it, which
        // reach past it while it is hidden.
        let mut hidden: Option<(String, MarginSet)> = None;
        // The item before the hidden one, which keeps its bottom margin
        // while the hidden one ends the column.
        let mut before_hidden: Option<usize> = None;
        let mut rest = children;
        while let Some(child) = rest.first() {
            if is_out_of_flow(&child.style) {
                rest = &rest[1..];
                // Its static position is past the margins so far, unless
                // they escape through the parent's top edge, which moves
                // the slot with them.
                let margin = if last.is_none() && top_open {
                    0.0
                } else {
                    pending.value()
                };
                if let Some(slot) = self.out_of_flow_box(child, margin) {
                    layout.push_child(slot);
                }
                continue;
            }
            let run = float_run(rest);
            let wrapped = rest.get(run..).map_or(0, |after| {
                let side = child.style.float;
                after
                    .iter()
                    .take_while(|b| float_run(std::slice::from_ref(b)) == 0 && !clears(b, side))
                    .count()
            });
            if run > 0 && wrapped > 0 {
                let floats = &rest[..run];
                let around = &rest[run..run + wrapped];
                rest = &rest[run + wrapped..];
                let (mut obj, bottom) = self.float_group(floats, around, decoration);
                // Margins do not collapse with a float's.
                if last.is_none() && top_open && pending == MarginSet::default() {
                    escaped_top = Some(pending);
                } else {
                    self.set_margin(&mut obj, "Layout.topMargin", pending.value());
                }
                layout.push_child(obj);
                last = Some(layout.members.len() - 1);
                pending = bottom;
                hidden = None;
                continue;
            }
            if run > 1 {
                let floats = &rest[..run];
                rest = &rest[run..];
                let mut obj = self.float_row(floats, decoration);
                if let Some(next) = rest
                    .first()
                    .filter(|b| float_run(std::slice::from_ref(b)) > 0)
                {
                    let line = next.node.and_then(|n| crate::boxes::node_line(self.doc, n));
                    self.diags.warning(
                        "css",
                        line,
                        format!(
                            "{} is placed below the floats before it, not beside them",
                            next.node
                                .map_or_else(|| "a box".to_owned(), |n| describe(self.doc, n))
                        ),
                    );
                }
                // Margins do not collapse with a float's.
                if last.is_none() && top_open && pending == MarginSet::default() {
                    escaped_top = Some(pending);
                } else {
                    self.set_margin(&mut obj, "Layout.topMargin", pending.value());
                }
                layout.push_child(obj);
                last = Some(layout.members.len() - 1);
                pending = MarginSet::default();
                hidden = None;
                continue;
            }
            rest = &rest[1..];
            let emitted = self.block(child, decoration);
            if emitted.collapse_through {
                pending.join(emitted.top);
                pending.join(emitted.bottom);
                continue;
            }
            let collapsed = child.style.visibility == Visibility::Collapse;
            let mut emitted = emitted;
            let mut obj = std::mem::replace(&mut emitted.obj, Object::new("Item"));
            self.flow_item(&mut obj, child);
            // Its line box separates its margins from its neighbours'.
            let strut = if hidden.is_none() {
                line_strut(
                    self.doc,
                    child,
                    &obj,
                    emitted.top.value(),
                    emitted.bottom.value(),
                )
            } else {
                None
            };
            if strut.is_some() {
                emitted.top = MarginSet::default();
                emitted.bottom = MarginSet::default();
            }
            let mut skipped = None;
            match hidden.take() {
                // Shown, it separates its neighbours' margins; hidden, the
                // margins before it collapse with this item's.
                Some((id, mut before)) if !collapsed => {
                    pending.join(emitted.top);
                    before.join(emitted.top);
                    self.set_hidden_margin(&mut obj, &id, pending, before);
                    before_hidden = None;
                }
                previous => {
                    let before = previous.map(|(_, b)| b);
                    if collapsed && obj.id.is_some() && before.is_none() {
                        skipped = Some(pending);
                        before_hidden = last;
                    }
                    pending.join(emitted.top);
                    if last.is_none() && top_open {
                        escaped_top = Some(pending);
                    } else {
                        self.set_margin(&mut obj, "Layout.topMargin", pending.value());
                    }
                }
            }
            if let Some((above, below)) = strut {
                let set = match obj.get("Layout.topMargin") {
                    Some(Value::Number(n)) => *n as f32,
                    _ => 0.0,
                };
                self.set_margin(&mut obj, "Layout.topMargin", set + above);
                self.set_margin(&mut obj, "Layout.bottomMargin", below);
            }
            if let (Some(before), Some(id)) = (skipped, &obj.id) {
                hidden = Some((id.clone(), before));
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
        let only_slots =
            last.is_none() && layout.members.iter().any(|m| matches!(m, Member::Child(_)));
        let Some(mut last) = last else {
            return Column {
                obj: only_slots.then_some(layout),
                escaped_top: pending,
                escaped_bottom: MarginSet::default(),
                only_slots,
            };
        };
        // A slot after the last item counts its static position from past
        // the item's margins, so those go below the slot.
        if let Some(tail) = layout
            .members
            .iter()
            .rposition(|m| matches!(m, Member::Child(_)))
        {
            last = tail;
        }
        let escaped_bottom = if bottom_open {
            // The page as loaded: a hidden last box passes on the margins
            // before it.
            hidden.map_or(pending, |(_, before)| before)
        } else {
            if let Some(Member::Child(obj)) = layout.members.get_mut(last) {
                self.set_margin(obj, "Layout.bottomMargin", pending.value());
            }
            if let Some((id, before)) = &hidden
                && let Some(i) = before_hidden
                && let Some(Member::Child(obj)) = layout.members.get_mut(i)
                && before.value() > 0.0
            {
                obj.set(
                    "Layout.bottomMargin",
                    Value::expr(format!(
                        "{id}.visible ? 0 : {}",
                        fmt_number(round(before.value()))
                    )),
                );
            }
            MarginSet::default()
        };
        self.carry(&mut layout);
        Column {
            obj: Some(layout),
            escaped_top: escaped_top.unwrap_or_default(),
            escaped_bottom,
            only_slots: false,
        }
    }

    /// Floats and the blocks after them that wrap around them: the floats'
    /// row over a column of the blocks, where each line of a paragraph
    /// beside the floats' margin box is shortened by it (CSS 2.2 §9.5).
    /// Blocks themselves are not narrowed: only their lines move. Returns
    /// the group and the margins escaping through its bottom.
    fn float_group(
        &mut self,
        floats: &[LayoutBox<'_>],
        around: &[LayoutBox<'_>],
        decoration: TextDecorationLine,
    ) -> (Object, MarginSet) {
        let left = floats[0].style.float == Float::Left;
        let mut row = self.float_row(floats, decoration);
        let row_id = row.id.clone().expect("a float row has an id");
        row.members
            .retain(|m| !matches!(m, Member::Property { name, .. } if name == "Layout.fillWidth"));
        for edge in ["left", "right", "top"] {
            row.set(
                &format!("anchors.{edge}"),
                Value::expr(format!("parent.{edge}")),
            );
        }
        let column = self.column(around, decoration, false, true);
        let mut group = Object::new("Item");
        group.set("Layout.fillWidth", true);
        let Some(mut col) = column.obj else {
            group.set(
                "implicitHeight",
                Value::expr(format!("{row_id}.implicitHeight")),
            );
            group.push_child(row);
            return (group, column.escaped_bottom);
        };
        let col_id = self.ids.claim("wrapped");
        col.id = Some(col_id.clone());
        for edge in ["left", "right", "top"] {
            col.set(
                &format!("anchors.{edge}"),
                Value::expr(format!("parent.{edge}")),
            );
        }
        let rect = format!("{row_id}.childrenRect");
        let cut = if left {
            format!("{rect}.x + {rect}.width - x")
        } else {
            format!("x + width - {rect}.x")
        };
        let mut texts = Vec::new();
        for text in col.children_mut().filter(|c| c.type_name == "Text") {
            let id = match &text.id {
                Some(id) => id.clone(),
                None => {
                    let id = self.ids.claim("wrapped text");
                    text.id = Some(id.clone());
                    id
                }
            };
            texts.push(id);
            // A fixed line height spaces lines evenly from the top. With
            // this handler Qt puts a fixed line's leading above its glyphs
            // (`line.y` is offset within the line), where without it the
            // leading is below, which the `Text`'s half-leading shift
            // assumes: the line goes back to the top.
            let mut body = vec![
                "const fixed = lineHeightMode === Text.FixedHeight;".to_owned(),
                "if (fixed) {".to_owned(),
                "    line.y = line.number * lineHeight;".to_owned(),
                "}".to_owned(),
                "const top = y + (fixed ? line.number * lineHeight : line.y);".to_owned(),
                "const bottom = top + (fixed ? lineHeight : line.height);".to_owned(),
                format!("if (top < {rect}.y + {rect}.height && bottom > {rect}.y) {{"),
                format!("    const cut = {cut};"),
                "    if (cut > 0) {".to_owned(),
                "        line.width -= cut;".to_owned(),
            ];
            if left {
                body.push("        line.x += cut;".to_owned());
            }
            body.extend(["    }".to_owned(), "}".to_owned()]);
            text.members.push(Member::Handler {
                name: "onLineLaidOut".to_owned(),
                params: vec!["line".to_owned()],
                body,
            });
            // Lines are laid out for the place the text had then. Later:
            // the handler above reads geometry whose change would relayout
            // the text under it.
            text.members.push(Member::Handler {
                name: "onYChanged".to_owned(),
                params: Vec::new(),
                body: vec!["Qt.callLater(forceLayout)".to_owned()],
            });
        }
        if !texts.is_empty() {
            row.members.push(Member::Handler {
                name: "onChildrenRectChanged".to_owned(),
                params: Vec::new(),
                body: texts
                    .iter()
                    .map(|t| format!("Qt.callLater({t}.forceLayout);"))
                    .collect(),
            });
        }
        // Not the wrapped text's: its unwrapped width only settles after the
        // first polish, and an ancestor its layout has not sized yet takes
        // it as its width for a frame, where an SVG rendered at that width
        // exceeds Qt's image allocation limit. A shrink-to-fit container
        // therefore sizes this group to its floats.
        group.set(
            "implicitWidth",
            Value::expr(format!("{row_id}.implicitWidth")),
        );
        group.set(
            "implicitHeight",
            Value::expr(format!(
                "Math.max({col_id}.implicitHeight, {row_id}.implicitHeight)"
            )),
        );
        // Floats are painted over the blocks' backgrounds (CSS 2.2
        // Appendix E); the lines beside them do not reach them.
        group.push_child(col);
        group.push_child(row);
        (group, column.escaped_bottom)
    }

    /// A run of floats to one side as a `Flow`: each takes its margin box
    /// along the line, and one that does not fit starts a new line below
    /// (which is where CSS puts it when the floats are as tall).
    fn float_row(&mut self, floats: &[LayoutBox<'_>], decoration: TextDecorationLine) -> Object {
        let id = self.ids.claim("floats");
        let mut row = Object::new("Flow").with_id(id.clone());
        row.set("Layout.fillWidth", true);
        if floats[0].style.float == Float::Right {
            row.set("layoutDirection", Value::expr("Qt.RightToLeft"));
        }
        let base = format!("{id}.width");
        for b in floats {
            self.in_float_run = true;
            let mut obj = self.block(b, decoration).obj;
            self.in_float_run = false;
            let style = &*b.style;
            let line = b.node.and_then(|n| crate::boxes::node_line(self.doc, n));
            let insets = self.insets(style, line);
            let (extra_w, extra_h) = match style.box_sizing {
                BoxSizing::ContentBox => (insets.horizontal(), insets.vertical()),
                BoxSizing::BorderBox => (0.0, 0.0),
            };
            // `auto` margins of a float are zero; percentages are of the
            // containing block's width.
            let margin = |m: &LengthAuto| match m {
                LengthAuto::Length(l) => match l.as_px() {
                    Some(px) => fmt_number(round(px)),
                    None => format!("({})", length_expr(l, &base)),
                },
                LengthAuto::Auto => "0".to_owned(),
            };
            let m = &style.margin;
            let (mt, mr, mb, ml) = (
                margin(&m.top),
                margin(&m.right),
                margin(&m.bottom),
                margin(&m.left),
            );
            let child_id = match &obj.id {
                Some(child_id) => child_id.clone(),
                None => {
                    let child_id = self.ids.claim(&format!("{} box", super::hint(self.doc, b)));
                    obj.id = Some(child_id.clone());
                    child_id
                }
            };
            match &style.width {
                Size::Length(w) => {
                    let w = length_expr(w, &base);
                    obj.set(
                        "width",
                        Value::expr(if extra_w == 0.0 {
                            w
                        } else {
                            format!("{w} + {}", fmt_number(round(extra_w)))
                        }),
                    );
                }
                // Shrink-to-fit, within the line.
                _ => {
                    obj.set(
                        "width",
                        Value::expr(format!("Math.min(implicitWidth, {base} - {ml} - {mr})")),
                    );
                }
            }
            if let Size::Length(h) = &style.height
                && let Some(px) = h.as_px()
            {
                obj.set("height", number(px + extra_h));
            }
            if ml != "0" {
                obj.set("x", Value::expr(ml.clone()));
            }
            if mt != "0" {
                obj.set("y", Value::expr(mt.clone()));
            }
            let mut cell = Object::new("Item");
            cell.set(
                "width",
                Value::expr(format!("{child_id}.width + {ml} + {mr}").replace(" + 0", "")),
            );
            cell.set(
                "height",
                Value::expr(format!("{child_id}.height + {mt} + {mb}").replace(" + 0", "")),
            );
            if self.options.source_comments
                && let Some(node) = b.node
            {
                row.members.push(Member::Comment(describe(self.doc, node)));
            }
            cell.push_child(obj);
            row.push_child(cell);
        }
        row
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
        // Floats stack as blocks here, so one that does not share its line
        // is placed right; `auto` margins of a float are zero.
        let alignment = match (style.float, auto_left, auto_right) {
            (Float::Right, ..) => Some("Qt.AlignRight"),
            (Float::Left, ..) => None,
            (_, true, true) => Some("Qt.AlignHCenter"),
            (_, true, false) => Some("Qt.AlignRight"),
            _ => None,
        };
        // An `inline-flex`/`inline-grid` laid out as a block keeps its
        // shrink-to-fit width.
        let shrink = b.kind == crate::boxes::BoxKind::Replaced
            || matches!(
                style.display,
                Display::InlineFlex
                    | Display::InlineGrid
                    | Display::InlineTable
                    | Display::InlineBlock
            );
        // An inline-level box's place in its line comes from the `text-align`
        // of the block it is in; its own (a button's is `center`) is for its
        // content.
        let alignment = alignment.or(match (style.display, self.line_align) {
            (Display::InlineBlock | Display::InlineTable, TextAlign::Center) => {
                Some("Qt.AlignHCenter")
            }
            (Display::InlineBlock | Display::InlineTable, TextAlign::Right | TextAlign::End) => {
                Some("Qt.AlignRight")
            }
            _ => None,
        });
        // A Qt Layout fills its cell by default, which would override a
        // width the box has of its own.
        if is_layout(&obj.type_name) && matches!(style.width, Size::Length(_)) {
            obj.set("Layout.fillWidth", false);
        }
        // A preferred width bound to the column's width feeds the column's
        // implicit width back into itself, which can widen the whole page,
        // and a control that does not fill cannot be narrower than its
        // implicit width: an item fills up to its percentage instead. A
        // Layout's own width does not depend on its column's.
        let fill_percent = super::image::is_percent_width(style) && !is_layout(&obj.type_name);
        match &style.width {
            Size::Length(w) if fill_percent => {
                obj.set("Layout.fillWidth", true);
                if *w != Length::Linear(eldur_css::LengthPercentage::percent(100.0)) {
                    obj.set("Layout.maximumWidth", size_value(w, extra_w));
                }
                if let Some(a) = alignment {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
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
                if (matches!(style.max_width, Size::Length(_)) || style.display == Display::Table)
                    && let Some(a) = alignment
                {
                    obj.set("Layout.alignment", Value::expr(a));
                }
            }
        }
        if let Size::Length(max) = &style.max_width {
            obj.set("Layout.maximumWidth", size_value(max, extra_w));
        } else if is_layout(&obj.type_name) && obj.get("Layout.maximumWidth").is_none() {
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
        // A filling item is no wider than its cell, but a percentage of the
        // content box overflows by the padding and border, as in CSS; it
        // overflows on the right, so an item placed right keeps its edge.
        if fill_percent
            && extra_w > 0.0
            && alignment.is_none()
            && let LengthAuto::Length(l) = &style.margin.right
            && let Some(right) = l.as_px()
        {
            obj.set("Layout.rightMargin", number(right.max(0.0) - extra_w));
        }
        match &style.height {
            Size::Length(h) if h.as_px().is_some() || is_viewport_relative(h) => {
                obj.set("Layout.preferredHeight", size_value(h, extra_h));
            }
            Size::Length(Length::Linear(l))
                if let Some(base) = self.percent_height_base
                    && l.vw == 0.0
                    && l.vh == 0.0 =>
            {
                obj.set(
                    "Layout.preferredHeight",
                    number(l.px + l.percent / 100.0 * base + extra_h),
                );
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

    /// `Layout.topMargin` of the item after a hidden box `id`: `shown`
    /// while that is visible, else `hidden`.
    fn set_hidden_margin(
        &mut self,
        obj: &mut Object,
        id: &str,
        shown: MarginSet,
        hidden: MarginSet,
    ) {
        let (shown, hidden) = (shown.value().max(0.0), hidden.value().max(0.0));
        if shown == hidden {
            self.set_margin(obj, "Layout.topMargin", shown);
        } else {
            obj.set(
                "Layout.topMargin",
                Value::expr(format!(
                    "{id}.visible ? {} : {}",
                    fmt_number(round(shown)),
                    fmt_number(round(hidden))
                )),
            );
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
/// Whether `b` is placed below floats to `side` rather than beside them.
fn clears(b: &LayoutBox<'_>, side: Float) -> bool {
    match b.style.clear {
        Clear::None => false,
        Clear::Both => true,
        Clear::Left => side == Float::Left,
        Clear::Right => side == Float::Right,
    }
}

/// How many boxes from the first float to the same side in a row; 0 when
/// the first is not a float.
fn float_run(boxes: &[LayoutBox<'_>]) -> usize {
    let side = |b: &LayoutBox<'_>| match b.style.float {
        Float::None => None,
        _ if b.style.display == Display::None => None,
        side => Some(side),
    };
    let Some(first) = boxes.first().and_then(side) else {
        return 0;
    };
    boxes.iter().take_while(|b| side(b) == Some(first)).count()
}

pub(crate) fn has_height(style: &ComputedStyle) -> bool {
    let definite = |s: &Size| matches!(s, Size::Length(l) if l.as_px() != Some(0.0));
    definite(&style.height) || definite(&style.min_height)
}

/// QML Layout types: their size limits default to their children's, not
/// to infinity as for other items.
pub(crate) fn is_layout(type_name: &str) -> bool {
    matches!(type_name, "ColumnLayout" | "RowLayout" | "GridLayout")
}

/// A length of px and viewport units only: definite at any depth (unlike a
/// percentage height, which needs a definite containing block).
pub(crate) fn is_viewport_relative(length: &Length) -> bool {
    matches!(length, Length::Linear(l) if l.percent == 0.0)
}

pub(crate) fn size_value(length: &Length, extra: f32) -> Value {
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
pub(crate) fn length_value_nonneg(l: &Length) -> Value {
    match l.as_px() {
        Some(px) => number(px.max(0.0)),
        None => Value::Expr(format!("Math.max(0, {})", length_expr(l, "parent.width"))),
    }
}

/// The space a block-laid-out inline replaced element `b` (border box
/// `obj`) keeps above and below it in its line box, margins included: its
/// bottom margin edge sits on the baseline, raised by `vertical-align`,
/// and the line is at least `line-height` tall (CSS 2.2 §10.8). `None`
/// for a block-level box, one whose baseline is its text's, or one whose
/// height or alignment is not known here.
fn line_strut(
    doc: &eldur_dom::Document,
    b: &LayoutBox<'_>,
    obj: &Object,
    mt: f32,
    mb: f32,
) -> Option<(f32, f32)> {
    use eldur_css::VerticalAlign;
    let style = &*b.style;
    if b.kind != crate::boxes::BoxKind::Replaced
        || crate::boxes::is_block_level(style.display)
        || style.float != Float::None
    {
        return None;
    }
    // A form control that shows text has that text's baseline.
    let element = doc.element(b.node?)?;
    let bottom_baseline = element.is_svg()
        || (element.is_html()
            && matches!(
                element.local_name(),
                "img" | "video" | "canvas" | "iframe" | "embed" | "object" | "progress" | "meter"
            ));
    if !bottom_baseline {
        return None;
    }
    let raise = match style.vertical_align {
        VerticalAlign::Baseline => 0.0,
        VerticalAlign::Length(v) => v,
        _ => return None,
    };
    let Some(Value::Number(h)) = obj
        .get("Layout.preferredHeight")
        .or_else(|| obj.get("implicitHeight"))
    else {
        return None;
    };
    let line = super::inline::line_height(style);
    let bottom_edge = super::inline::text_baseline(style) - raise;
    let top_edge = bottom_edge - (mt + *h as f32 + mb);
    let line_top = top_edge.min(0.0);
    let line_bottom = bottom_edge.max(line);
    Some((top_edge - line_top + mt, line_bottom - bottom_edge + mb))
}
