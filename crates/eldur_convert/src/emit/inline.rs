//! An inline formatting context with atomic inline-level boxes (replaced
//! elements, controls, `inline-block`, `inline-flex`, …) → `EldurLines`.
//!
//! One `Text` cannot hold a box, so the content is cut at the boxes only: each
//! run of text stays one `Text`, shaped whole, its inline elements its nodes. The
//! one line layouter, `EldurLines.qml`, is written beside `Main.qml`: it knows the
//! real measures, follows the viewport, and also places what a script changes.

use eldur_css::{
    BoxSizing, ComputedStyle, Length, LengthAuto, LineHeight, Overflow, Size, TextAlign,
    TextDecorationLine, VerticalAlign, WhiteSpace,
};
use eldur_dom::NodeId;
use eldur_qml::{Member, Object, Value};

use super::Emitter;
use super::controls::{line_box, normal_line, text_scale};
use super::decoration::{border_width, is_visible};
use super::units::{fmt_number, number, round};
use crate::boxes::{BoxKind, Inline, LayoutBox, Span, node_line};

/// The layouter, written beside `Main.qml` when anything may use it.
pub(crate) const ELDUR_LINES_QML: &str = include_str!("EldurLines.qml");
pub(crate) const ELDUR_RUN_QML: &str = include_str!("EldurRun.qml");

/// Every direct `Text` of an `EldurLines` under `obj` becomes an `EldurRun`,
/// without the properties that type already has. Done last, since the emitters
/// recognise a run by being a `Text`.
pub(crate) fn name_runs(obj: &mut Object) {
    let lines = obj.type_name == "EldurLines";
    for child in obj.children_mut() {
        if lines && child.type_name == "Text" {
            child.type_name = "EldurRun".to_owned();
            // `EldurLines` resolves a run's stack, for the same cost as an enum.
            if let Some(Value::Expr(e)) = child.get("font.family")
                && let Some(literal) = e
                    .strip_prefix("EldurFonts.family(")
                    .and_then(|e| e.strip_suffix(')'))
                && let Ok(stack) = serde_json::from_str::<String>(literal)
            {
                child.set("font.family", Value::string(stack));
            }
            // What a `Text` left unset stays Qt's default, not the run's.
            for (name, common, unset) in [
                ("textFormat", "Text.PlainText", "Text.AutoText"),
                ("wrapMode", "Text.WordWrap", "Text.NoWrap"),
                ("renderType", "Text.NativeRendering", "Text.QtRendering"),
            ] {
                match child.get(name) {
                    None => {
                        child.set(name, Value::expr(unset));
                    }
                    Some(Value::Expr(e)) if e == common => {
                        child.members.retain(
                            |m| !matches!(m, Member::Property { name: n, .. } if n == name),
                        );
                    }
                    Some(_) => {}
                }
            }
        }
        name_runs(child);
    }
}
/// What an inline element paints of its own box, beside it when used.
pub(crate) const ELDUR_INLINE_BOX_QML: &str = include_str!("EldurInlineBox.qml");

/// The width of a created inline element or text run in a `Flow`: its
/// natural width, no wider than the line.
const OWN_WIDTH: &str = "parent ? Math.min(implicitWidth, parent.width) : implicitWidth";

/// Whether `items` hold an atomic inline-level box, at any depth.
pub(crate) fn has_atomic(items: &[Inline<'_>]) -> bool {
    items.iter().any(|item| match item {
        Inline::Atomic(_) => true,
        Inline::Span(span) => has_atomic(&span.children),
        Inline::Text(_) | Inline::LineBreak => false,
    })
}

/// Whether a span's box on the line differs from its parent's: its own font,
/// line height or alignment. One `Text` has one of each, so such a span is a run
/// of its own, and `EldurLines` makes the line box from all of them (layout.md §1.3).
/// A bold face of the same extents changes nothing and stays a tag.
fn own_box(parent: &ComputedStyle, span: &ComputedStyle) -> bool {
    span.vertical_align != VerticalAlign::Baseline
        || (span.font_size - parent.font_size).abs() > 1e-3
        || super::fonts::expected_face(&span.font_family)
            != super::fonts::expected_face(&parent.font_family)
        || super::fonts::extents(span) != super::fonts::extents(parent)
        || (line_height(span) - line_height(parent)).abs() > 1e-3
        || relative_offset(span) != (0.0, 0.0)
}

/// How far `position: relative` moves an inline element from where its line has it.
/// A `%` has nothing to resolve against here, and left and top win as in CSS 2.2 §9.4.3.
pub(super) fn relative_offset(style: &ComputedStyle) -> (f32, f32) {
    if style.position != eldur_css::Position::Relative {
        return (0.0, 0.0);
    }
    let axis = |near: &LengthAuto, far: &LengthAuto| match (near, far) {
        (LengthAuto::Length(l), _) => l.as_px().unwrap_or(0.0),
        (LengthAuto::Auto, LengthAuto::Length(l)) => -l.as_px().unwrap_or(0.0),
        (LengthAuto::Auto, LengthAuto::Auto) => 0.0,
    };
    (
        axis(&style.left, &style.right),
        axis(&style.top, &style.bottom),
    )
}

fn holds_own_box(parent: &ComputedStyle, items: &[Inline<'_>]) -> bool {
    items.iter().any(|item| match item {
        Inline::Span(span) => {
            own_box(parent, span.style) || holds_own_box(span.style, &span.children)
        }
        Inline::Text(_) | Inline::Atomic(_) | Inline::LineBreak => false,
    })
}

/// Whether a run must be cut at this span for the box it paints: on a line of
/// runs nothing is RichText, so any background or side space is drawn beside the text.
fn paints_on_line(style: &ComputedStyle) -> bool {
    paints_box(style)
        || !style.background_color.is_transparent()
        || super::text::inline_space(style, super::text::Side::Left) > 0.0
        || super::text::inline_space(style, super::text::Side::Right) > 0.0
}

/// Whether an inline element paints a box RichText cannot: RichText has a background
/// as tall as its line, with no corners or borders. Such an element is a run of its
/// own and `EldurInlineBox` draws behind it. A square background stays in RichText,
/// its vertical padding unpainted (see 45-inline-boxes `#p-tall`).
pub(crate) fn paints_box(style: &ComputedStyle) -> bool {
    let sides = [
        &style.border.top,
        &style.border.right,
        &style.border.bottom,
        &style.border.left,
    ];
    if sides.into_iter().any(is_visible) {
        return true;
    }
    let r = &style.border_radius;
    let rounded = [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left]
        .iter()
        .any(|c| !c.is_zero());
    !style.background_color.is_transparent() && rounded
}

/// The innermost element that paints a box and holds all of `items`.
fn sole_box<'a>(items: &[Inline<'a>]) -> Option<&'a ComputedStyle> {
    let [Inline::Span(span)] = items else {
        return None;
    };
    sole_box(&span.children).or(paints_box(span.style).then_some(span.style))
}

fn holds_box(items: &[Inline<'_>]) -> bool {
    items.iter().any(|item| match item {
        // RichText widens a joiner for side space, and that joiner's fallback font
        // then decides the line's height.
        Inline::Span(span) => {
            paints_box(span.style)
                || super::text::inline_space(span.style, super::text::Side::Left) > 0.0
                || super::text::inline_space(span.style, super::text::Side::Right) > 0.0
                || holds_box(&span.children)
        }
        Inline::Text(_) | Inline::Atomic(_) | Inline::LineBreak => false,
    })
}

/// Whether `items` must be placed by `EldurLines` for a box an element paints.
pub(crate) fn needs_lines_for_boxes(items: &[Inline<'_>]) -> bool {
    holds_box(items)
}

fn holds_painted(items: &[Inline<'_>]) -> bool {
    items.iter().any(|item| match item {
        Inline::Span(span) => paints_on_line(span.style) || holds_painted(&span.children),
        Inline::Text(_) | Inline::Atomic(_) | Inline::LineBreak => false,
    })
}

/// Whether `items` need `EldurLines` for the runs' own metrics. A span that is
/// the whole line is already the `Text`'s own (`whole_span`).
pub(crate) fn needs_lines_for_runs(block: &ComputedStyle, items: &[Inline<'_>]) -> bool {
    match (super::text::whole_span(block, items), items) {
        (Some(_), [Inline::Span(span)]) => {
            relative_offset(span.style) != (0.0, 0.0) || holds_own_box(span.style, &span.children)
        }
        _ => holds_own_box(block, items),
    }
}

fn has_words(items: &[Inline<'_>]) -> bool {
    items.iter().any(|item| match item {
        Inline::Text(_) | Inline::LineBreak => true,
        Inline::Span(span) => has_words(&span.children),
        Inline::Atomic(_) => false,
    })
}

/// The horizontal margins a run's spans take on the line. One `Text` cannot put
/// space inside itself, so only those at the run's edges can be honoured.
fn run_margins(items: &[Inline<'_>]) -> [f32; 2] {
    let side = |m: &LengthAuto| match m {
        LengthAuto::Length(l) => l.as_px().unwrap_or(0.0),
        LengthAuto::Auto => 0.0,
    };
    let is_space = |i: &&Inline<'_>| matches!(i, Inline::Text(t) if t.trim().is_empty());
    let first = items.iter().find(|i| !is_space(i));
    let last = items.iter().rev().find(|i| !is_space(i));
    let left = match first {
        Some(Inline::Span(s)) => side(&s.style.margin.left),
        _ => 0.0,
    };
    let right = match last {
        Some(Inline::Span(s)) => side(&s.style.margin.right),
        _ => 0.0,
    };
    [left, right]
}

fn side_px(m: &LengthAuto) -> f32 {
    match m {
        LengthAuto::Length(l) => l.as_px().unwrap_or(0.0),
        LengthAuto::Auto => 0.0,
    }
}

/// The boxes a run paints, outermost first, and whether each opens and closes in
/// this run; `runs` is every run of the line, for an element an atomic cuts.
fn run_boxes<'b, 'a>(
    items: &'b [Inline<'a>],
    this: usize,
    runs: &[Vec<Option<NodeId>>],
) -> Vec<(&'b Span<'a>, bool, bool)> {
    let mut spans = Vec::new();
    each_span(items, &mut spans);
    spans
        .into_iter()
        .filter(|s| paints_on_line(s.style))
        .map(|s| {
            let mut with = runs
                .iter()
                .enumerate()
                .filter(|(_, r)| s.node.is_some() && r.contains(&s.node))
                .map(|(n, _)| n);
            let first = with.next().unwrap_or(this);
            let last = with.next_back().unwrap_or(first);
            (s, first == this, last == this)
        })
        .collect()
}

/// The percentage and pixels of a `width` that follows the containing block.
fn percent_width(style: &ComputedStyle) -> Option<(f32, f32)> {
    match style.width {
        Size::Length(Length::Linear(l)) if l.percent != 0.0 && l.vw == 0.0 && l.vh == 0.0 => {
            Some((l.percent, l.px))
        }
        _ => None,
    }
}

/// A piece of the line `EldurLines` places.
enum Piece<'b, 'a> {
    /// A run of text between boxes, its spans kept.
    Run(Vec<Inline<'a>>),
    /// A box, with the innermost element around it that has its own node.
    Atomic(&'b LayoutBox<'a>, Option<NodeId>),
}

fn pieces<'b, 'a>(
    parent: &ComputedStyle,
    items: &'b [Inline<'a>],
    chain: &mut Vec<&'b Span<'a>>,
    run: &mut Vec<Inline<'a>>,
    out: &mut Vec<Piece<'b, 'a>>,
) {
    for item in items {
        match item {
            Inline::Atomic(b) => {
                end_run(run, out);
                let owner = chain.iter().rev().find_map(|span| span.node);
                out.push(Piece::Atomic(b, owner));
            }
            Inline::Span(span)
                if has_atomic(&span.children)
                    || holds_own_box(span.style, &span.children)
                    || holds_painted(&span.children) =>
            {
                let own = own_box(parent, span.style) || paints_on_line(span.style);
                if own {
                    end_run(run, out);
                }
                chain.push(span);
                // With only boxes in it, no run would give the element a node.
                if span.node.is_some() && !has_words(&span.children) {
                    push_in_chain(run, Inline::Text(String::new()), chain);
                }
                pieces(span.style, &span.children, chain, run, out);
                chain.pop();
                if own {
                    end_run(run, out);
                }
            }
            Inline::Span(span) if own_box(parent, span.style) || paints_on_line(span.style) => {
                end_run(run, out);
                push_in_chain(run, item.clone(), chain);
                end_run(run, out);
            }
            other => push_in_chain(run, other.clone(), chain),
        }
    }
}

/// What a run's boxes are, outermost first: each span around its text that has
/// its own box on the line. Every leaf of a run has the same ones, as `pieces`
/// cuts where they change, so the first path to a leaf tells.
fn run_levels<'b, 'a>(
    parent: &'b ComputedStyle,
    items: &'b [Inline<'a>],
    out: &mut Vec<(&'b ComputedStyle, Option<NodeId>)>,
) {
    let Some(span) = items.iter().find_map(|item| match item {
        Inline::Span(span) => Some(span),
        Inline::Text(t) if !t.is_empty() => None,
        _ => None,
    }) else {
        return;
    };
    // Text beside the span is of the parent's box, so the span is the same box.
    let beside = items
        .iter()
        .any(|i| matches!(i, Inline::Text(t) if !t.is_empty()));
    if beside && !own_box(parent, span.style) {
        return;
    }
    let mut next = parent;
    if own_box(parent, span.style) {
        out.push((span.style, span.node));
        next = span.style;
    }
    run_levels(next, &span.children, out);
}

/// One run's boxes for `EldurLines`, as a script's changes give them to it too.
/// In JSON, not a JavaScript literal: Qt compiles each binding of a document at a
/// cost that grows with how many it has, and a page has many runs.
fn levels_json(levels: &[(&ComputedStyle, Option<NodeId>)]) -> String {
    let parts: Vec<String> = levels
        .iter()
        .map(|(style, node)| {
            let mut fields = vec![
                format!("\"size\":{}", fmt_number(f64::from(style.font_size))),
                format!("\"family\":{}", super::fonts::stack(style)),
            ];
            if style.font_weight != 400 {
                fields.push(format!("\"weight\":{}", style.font_weight));
            }
            if style.font_style != eldur_css::FontStyle::Normal {
                fields.push("\"italic\":true".to_owned());
            }
            if let Some(line) = super::text::line_height(style) {
                fields.push(format!("\"line\":{}", fmt_number(f64::from(line))));
            }
            let (rel_x, rel_y) = relative_offset(style);
            if rel_x != 0.0 {
                fields.push(format!("\"relX\":{}", fmt_number(f64::from(rel_x))));
            }
            if rel_y != 0.0 {
                fields.push(format!("\"relY\":{}", fmt_number(f64::from(rel_y))));
            }
            let align = match style.vertical_align {
                VerticalAlign::Baseline => None,
                VerticalAlign::Sub => Some("sub"),
                VerticalAlign::Super => Some("super"),
                VerticalAlign::Top => Some("top"),
                VerticalAlign::TextTop => Some("text-top"),
                VerticalAlign::Middle => Some("middle"),
                VerticalAlign::Bottom => Some("bottom"),
                VerticalAlign::TextBottom => Some("text-bottom"),
                VerticalAlign::Length(px) => {
                    fields.push(format!("\"shift\":{}", fmt_number(f64::from(px))));
                    None
                }
                VerticalAlign::Percent(p) => {
                    fields.push(format!("\"percent\":{}", fmt_number(f64::from(p))));
                    None
                }
            };
            if let Some(align) = align {
                fields.push(format!("\"align\":\"{align}\""));
            }
            // The runs a top or bottom box holds are one thing on the line.
            if matches!(
                style.vertical_align,
                VerticalAlign::Top | VerticalAlign::Bottom
            ) && let Some(node) = node
            {
                fields.push(format!("\"group\":{}", node.index()));
            }
            format!("{{{}}}", fields.join(","))
        })
        .collect();
    format!("[{}]", parts.join(","))
}

/// Puts `inner` in `run` inside `chain`, the spans cut at a box, in the piece
/// already open for them when there is one.
fn push_in_chain<'a>(run: &mut Vec<Inline<'a>>, inner: Inline<'a>, chain: &[&Span<'a>]) {
    let Some((span, rest)) = chain.split_first() else {
        run.push(inner);
        return;
    };
    if let Some(Inline::Span(open)) = run.last_mut()
        && open.node == span.node
        && open.node.is_some()
    {
        push_in_chain(&mut open.children, inner, rest);
        return;
    }
    let mut children = Vec::new();
    push_in_chain(&mut children, inner, rest);
    run.push(Inline::Span(Span {
        node: span.node,
        style: span.style,
        children,
    }));
}

/// `items` without the second of two spaces an empty span holds apart: what the
/// `Text` draws until a script fills the span.
fn drop_doubled_spaces<'a>(items: &[Inline<'a>], after_space: &mut bool) -> Vec<Inline<'a>> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Inline::Text(t) => {
                let t = if *after_space {
                    t.strip_prefix(' ').unwrap_or(t)
                } else {
                    t
                };
                if !t.is_empty() {
                    *after_space = t.ends_with(' ');
                    out.push(Inline::Text(t.to_owned()));
                }
            }
            Inline::Span(s) => out.push(Inline::Span(Span {
                node: s.node,
                style: s.style,
                children: drop_doubled_spaces(&s.children, after_space),
            })),
            other => {
                *after_space = false;
                out.push(other.clone());
            }
        }
    }
    out
}

/// Every span in `items`, depth first.
fn each_span<'b, 'a>(items: &'b [Inline<'a>], out: &mut Vec<&'b Span<'a>>) {
    for item in items {
        if let Inline::Span(span) = item {
            out.push(span);
            each_span(&span.children, out);
        }
    }
}

fn end_run<'a>(run: &mut Vec<Inline<'a>>, out: &mut Vec<Piece<'_, 'a>>) {
    if !run.is_empty() {
        out.push(Piece::Run(std::mem::take(run)));
    }
}

/// The line height of `style` in px.
pub(super) fn line_height(style: &ComputedStyle) -> f32 {
    match style.line_height {
        LineHeight::Normal => super::fonts::normal_line(style),
        LineHeight::Number(n) => n * style.font_size,
        LineHeight::Px(px) => px,
    }
}

/// The baseline of a line of `style`'s text, from the top of the line box:
/// the glyphs are centred in it (half-leading).
pub(super) fn text_baseline(style: &ComputedStyle) -> f32 {
    super::text::whole_baseline(style, line_height(style))
}

/// The first box of an atomic inline a line places.
struct Part {
    obj: Object,
    hint: String,
    margins: [f32; 4],
    own: bool,
}

/// How the line finds an atomic's baseline, when it is not the walk over its
/// last lines; `none` is its bottom margin edge (CSS 2.2 §10.8.1).
fn baseline_kind(a: &LayoutBox<'_>) -> Option<&'static str> {
    let style = &*a.style;
    if style.overflow_x != Overflow::Visible || style.overflow_y != Overflow::Visible {
        return Some("none");
    }
    (a.kind == BoxKind::Flex || a.kind == BoxKind::Grid).then_some("first")
}

// The line puts each of a run's lines on its baseline, and Qt spaces fixed-height
// lines unevenly once they are moved: a run keeps its font's natural height.
fn drop_own_leading(text: &mut Object) {
    text.members.retain(|m| {
        !matches!(m, Member::Property { name, .. }
            if name == "lineHeight" || name == "lineHeightMode" || name == "transform"
                || name == "bottomPadding")
    });
}

/// What a run's own `line-height` asks of its line, for `EldurLines`.
fn declare_own_line(text: &mut Object, pixels: f32) {
    text.members.insert(
        0,
        Member::PropertyDecl {
            ty: "real".to_owned(),
            name: "eldurOwnLine".to_owned(),
            value: Some(number(pixels)),
            readonly: true,
            required: false,
        },
    );
}

fn string_property(obj: &mut Object, name: &str, value: &str) {
    obj.members.insert(
        0,
        Member::PropertyDecl {
            ty: "string".to_owned(),
            name: name.to_owned(),
            value: Some(Value::String(value.to_owned())),
            readonly: false,
            required: false,
        },
    );
}

fn vertical_align(obj: &mut Object, style: &ComputedStyle) {
    let mut align = style.vertical_align;
    // An atomic box's line height, if it has none of its own, is its font's.
    if let VerticalAlign::Percent(p) = align {
        align = VerticalAlign::Length(p / 100.0 * line_height(style));
    }
    let name = match align {
        VerticalAlign::Baseline => return,
        VerticalAlign::Sub => "sub",
        VerticalAlign::Super => "super",
        VerticalAlign::Top => "top",
        VerticalAlign::TextTop => "text-top",
        VerticalAlign::Middle => "middle",
        VerticalAlign::Bottom => "bottom",
        VerticalAlign::TextBottom => "text-bottom",
        VerticalAlign::Length(px) => {
            obj.members.insert(
                0,
                Member::PropertyDecl {
                    ty: "real".to_owned(),
                    name: "eldurVShift".to_owned(),
                    value: Some(number(px)),
                    readonly: false,
                    required: false,
                },
            );
            "length"
        }
        VerticalAlign::Percent(_) => return,
    };
    string_property(obj, "eldurVAlign", name);
}

impl Emitter<'_, '_> {
    /// The `EldurLines` of block container `b`'s inline content `items`.
    pub(super) fn inline_flow(
        &mut self,
        b: &LayoutBox<'_>,
        decoration: TextDecorationLine,
        items: &[Inline<'_>],
        line: Option<u32>,
    ) -> Object {
        let style = &*b.style;
        let flow_id = self
            .ids
            .claim(&format!("{} flow", super::hint(self.doc, b)));
        let mut list = Vec::new();
        let mut run = Vec::new();
        pieces(style, items, &mut Vec::new(), &mut run, &mut list);
        end_run(&mut run, &mut list);

        let mut flow = Object::new("EldurLines").with_id(flow_id.clone());
        // The strut: the block's font and line height.
        let strut = self.text(style, decoration, &[], line);
        for member in &strut.members {
            if let Member::Property { name, value } = member
                && (name.starts_with("font.") || name == "font" || name == "color")
            {
                flow.set(name, value.clone());
            }
        }
        if let Some(lh) = super::text::line_height(style) {
            flow.set("lineHeight", number(lh));
        }
        if let Some(size) = super::fonts::css_size(style) {
            flow.set("cssSize", Value::Number(size));
        }
        match style.text_align {
            TextAlign::Center => {
                flow.set("textAlign", Value::expr("Text.AlignHCenter"));
            }
            TextAlign::Right | TextAlign::End => {
                flow.set("textAlign", Value::expr("Text.AlignRight"));
            }
            TextAlign::Justify => {
                flow.set("textAlign", Value::expr("Text.AlignJustify"));
            }
            _ => {}
        }
        if style.white_space == WhiteSpace::NoWrap || style.white_space == WhiteSpace::Pre {
            flow.set("noWrap", true);
        }
        if !matches!(style.white_space, WhiteSpace::Normal | WhiteSpace::NoWrap) {
            flow.set("preserveSpace", true);
        }

        // Boxes cut elements too, but those are told by `all_inline`; these are
        // the elements only runs cut.
        let mut seen = std::collections::HashSet::new();
        for piece in &list {
            if let Piece::Run(items) = piece {
                let mut spans = Vec::new();
                each_span(items, &mut spans);
                for node in spans.iter().filter_map(|s| s.node) {
                    if !seen.insert(node) {
                        self.cut_nodes.insert(node);
                    }
                }
            }
        }

        let run_nodes: Vec<Vec<Option<NodeId>>> = list
            .iter()
            .filter_map(|piece| match piece {
                Piece::Run(items) => {
                    let mut spans = Vec::new();
                    each_span(items, &mut spans);
                    Some(spans.iter().map(|s| s.node).collect())
                }
                Piece::Atomic(..) => None,
            })
            .collect();
        let mut run_index = 0;
        let mut runs = false;
        for piece in list {
            match piece {
                Piece::Run(items) => {
                    let drawn = drop_doubled_spaces(&items, &mut false);
                    // The run is drawn in the font of its innermost box: tags
                    // cannot give an exact size or a family, and this line has
                    // only `lineLaidOut` to place a Text with.
                    let mut levels = Vec::new();
                    run_levels(style, &items, &mut levels);
                    let mut own = sole_box(&drawn).unwrap_or(style).clone();
                    if let Some((leaf, _)) = levels.last() {
                        own.font_size = leaf.font_size;
                        own.font_family.clone_from(&leaf.font_family);
                        own.line_height = leaf.line_height;
                    }
                    self.boxes_drawn = true;
                    let mut text = self.text(&own, decoration, &drawn, line);
                    self.boxes_drawn = false;
                    // A run that is all one span hoists that span's font, which
                    // may not be the innermost.
                    if !levels.is_empty() {
                        let leaf_font = self.text(&own, decoration, &[], line);
                        for member in &leaf_font.members {
                            if let Member::Property { name, value } = member
                                && (name == "font.pixelSize" || name == "font.family")
                            {
                                text.set(name, value.clone());
                            }
                        }
                    }
                    drop_own_leading(&mut text);
                    if !levels.is_empty() {
                        text.members.insert(
                            0,
                            Member::PropertyDecl {
                                ty: "string".to_owned(),
                                name: "eldurRun".to_owned(),
                                value: Some(Value::string(levels_json(&levels))),
                                readonly: false,
                                required: false,
                            },
                        );
                    }
                    let mut spans = Vec::new();
                    each_span(&items, &mut spans);
                    // A label cut by its control still takes a press on its words.
                    for span in spans {
                        if !span.node.is_some_and(|n| self.has_tapped_label(n)) {
                            self.label_tap(&mut text, span.node);
                        }
                    }
                    // The line aligns; a justified Text spreads its full lines.
                    if style.text_align != TextAlign::Justify {
                        text.members.retain(|m| {
                            !matches!(m, Member::Property { name, .. }
                                if name == "horizontalAlignment")
                        });
                    }
                    let boxes = run_boxes(&items, run_index, &run_nodes);
                    run_index += 1;
                    let [left, right] = self.paint_run_boxes(&mut text, &items, &boxes, line);
                    for (name, m) in [("eldurMarginLeft", left), ("eldurMarginRight", right)] {
                        if m != 0.0 {
                            text.members.insert(
                                0,
                                Member::PropertyDecl {
                                    ty: "real".to_owned(),
                                    name: name.to_owned(),
                                    value: Some(number(m)),
                                    readonly: true,
                                    required: false,
                                },
                            );
                        }
                    }
                    if self.runtime_dom {
                        self.declare_run_model(&mut text, b, decoration, &items);
                    }
                    flow.push_child(text);
                }
                Piece::Atomic(a, owner) => {
                    let part = self.atomic(a, decoration, &flow_id);
                    runs |= part.own;
                    let mut obj = part.obj;
                    // A `Text` is placed as a run: the line measures it on its
                    // natural height.
                    if obj.type_name == "Text" && own_box(style, &a.style) {
                        drop_own_leading(&mut obj);
                        declare_own_line(&mut obj, line_height(&a.style));
                    }
                    let kind = baseline_kind(a);
                    if let Some(kind) = kind {
                        string_property(&mut obj, "eldurBaseline", kind);
                    }
                    if let Some(owner) = owner.filter(|_| self.runtime_dom) {
                        obj.members.insert(
                            0,
                            Member::PropertyDecl {
                                ty: "int".to_owned(),
                                name: "eldurInlineOwner".to_owned(),
                                value: Some(number(owner.index() as f32)),
                                readonly: true,
                                required: false,
                            },
                        );
                    }
                    let mut placed = if part.own {
                        obj
                    } else {
                        self.place_atomic(obj, &part.hint, part.margins, 0.0)
                    };
                    vertical_align(&mut placed, &a.style);
                    if self.options.source_comments
                        && let Some(node) = a.node
                    {
                        flow.members
                            .push(Member::Comment(super::describe(self.doc, node)));
                    }
                    flow.push_child(placed);
                }
            }
        }
        if runs {
            // The text nodes a script puts between created elements: the
            // runtime makes each from this, in the block's font and colour.
            let run_id = self.ids.claim(&format!("{flow_id} run"));
            let mut run = self.text(style, decoration, &[], line);
            drop_own_leading(&mut run);
            run.members.retain(|m| {
                !matches!(m, Member::Property { name, .. }
                    if name == "horizontalAlignment")
            });
            run.set("width", Value::expr(OWN_WIDTH));
            flow.members.insert(
                0,
                Member::PropertyDecl {
                    ty: "Component".to_owned(),
                    name: "eldurTextRun".to_owned(),
                    value: Some(Value::expr(run_id.clone())),
                    readonly: true,
                    required: false,
                },
            );
            flow.push_child(Object::new("Component").with_id(run_id).child(run));
        }
        flow
    }

    /// Puts the boxes of a run's elements behind its text, and returns what they
    /// and its other spans take on the line beside it.
    fn paint_run_boxes(
        &mut self,
        text: &mut Object,
        items: &[Inline<'_>],
        boxes: &[(&Span<'_>, bool, bool)],
        line: Option<u32>,
    ) -> [f32; 2] {
        let mut margins = run_margins(items);
        if boxes.is_empty() {
            return margins;
        }
        // The outermost span's margin, when it paints, is counted below with the rest.
        if let Some(Inline::Span(outer)) = items
            .iter()
            .find(|i| !matches!(i, Inline::Text(t) if t.trim().is_empty()))
            && paints_box(outer.style)
        {
            margins = [0.0, 0.0];
        }
        let id = match &text.id {
            Some(id) => id.clone(),
            None => {
                let id = self.ids.claim("inline run");
                text.id = Some(id.clone());
                id
            }
        };
        text.members.insert(
            0,
            Member::PropertyDecl {
                ty: "var".to_owned(),
                name: "eldurFragments".to_owned(),
                value: Some(Value::expr("[]")),
                readonly: false,
                required: false,
            },
        );
        // Inner boxes first: what each takes pushes every box around it outwards.
        let (mut reach_left, mut reach_right) = (0.0, 0.0);
        let mut painted = Vec::new();
        for (span, opens, closes) in boxes.iter().rev() {
            let st = span.style;
            let pad = |l: &Length| l.as_px().unwrap_or(0.0).max(0.0);
            let (opens, closes) = (*opens, *closes);
            if opens {
                reach_left += pad(&st.padding.left) + border_width(&st.border.left);
            }
            if closes {
                reach_right += pad(&st.padding.right) + border_width(&st.border.right);
            }
            let mut paint = Object::new("EldurInlineBox");
            paint.set("fragments", Value::expr(format!("{id}.eldurFragments")));
            let metrics = self.text(st, TextDecorationLine::default(), &[], line);
            for member in &metrics.members {
                if let Member::Property { name, value } = member
                    && name.starts_with("font.")
                {
                    paint.set(name, value.clone());
                }
            }
            if !st.background_color.is_transparent() {
                paint.set("fill", super::units::color(st.background_color));
            }
            let radii = self.corner_radii(st, line);
            if radii.iter().any(|r| *r != 0.0) {
                paint.set(
                    "radii",
                    list_expr(radii.iter().map(|r| fmt_number(round(*r)))),
                );
            }
            let sides = [
                &st.border.top,
                &st.border.right,
                &st.border.bottom,
                &st.border.left,
            ];
            if sides.iter().any(|s| is_visible(s)) {
                paint.set(
                    "borderWidths",
                    list_expr(sides.iter().map(|s| fmt_number(round(border_width(s))))),
                );
                paint.set(
                    "borderColors",
                    list_expr(sides.iter().map(|s| {
                        format!("\"{}\"", super::units::qml_color(s.color).to_qml_string())
                    })),
                );
            }
            if pad(&st.padding.top) != 0.0 {
                paint.set("paddingTop", number(pad(&st.padding.top)));
            }
            if pad(&st.padding.bottom) != 0.0 {
                paint.set("paddingBottom", number(pad(&st.padding.bottom)));
            }
            if reach_left != 0.0 {
                paint.set("reachLeft", number(reach_left));
            }
            if reach_right != 0.0 {
                paint.set("reachRight", number(reach_right));
            }
            if !opens {
                paint.set("opens", false);
            }
            if !closes {
                paint.set("closes", false);
            }
            painted.push(paint);
            if opens {
                reach_left += side_px(&st.margin.left);
            }
            if closes {
                reach_right += side_px(&st.margin.right);
            }
        }
        // The outermost is painted first.
        for paint in painted.into_iter().rev() {
            text.push_child(paint);
        }
        [margins[0] + reach_left, margins[1] + reach_right]
    }

    fn atomic(&mut self, a: &LayoutBox<'_>, decoration: TextDecorationLine, flow_id: &str) -> Part {
        let style = &*a.style;
        let line = a.node.and_then(|n| node_line(self.doc, n));
        let own = a.node.is_some_and(|n| self.inline_items.contains(&n));
        // Text decorations are not propagated into atomic inlines, but a
        // created inline element is one only in QML.
        let emitted = self.item(
            a,
            if own {
                decoration
            } else {
                TextDecorationLine::default()
            },
        );
        let mut obj = emitted.obj;
        let insets = self.insets(style, line);
        let (extra_w, extra_h) = match style.box_sizing {
            BoxSizing::ContentBox => (insets.horizontal(), insets.vertical()),
            BoxSizing::BorderBox => (0.0, 0.0),
        };
        let horizontal = |m: &LengthAuto| match m {
            LengthAuto::Length(l) => l.as_px().unwrap_or(0.0).max(0.0),
            LengthAuto::Auto => 0.0,
        };
        let margins = [
            horizontal(&style.margin.left),
            horizontal(&style.margin.right),
            emitted.top.value().max(0.0),
            emitted.bottom.value().max(0.0),
        ];
        if own {
            // Its template is instantiated apart from the Flow, so it cannot
            // name it, and it has no wrapper to hold margins: a Text keeps
            // them as padding, a painted box loses them (§6.8).
            if obj.type_name == "Text" {
                for (name, m) in [("leftPadding", margins[0]), ("rightPadding", margins[1])] {
                    if round(m) != 0.0 {
                        obj.set(name, number(m));
                    }
                }
            } else {
                // For the line it joins, as a layout it joins would read them.
                for (name, m) in [
                    ("Layout.leftMargin", margins[0]),
                    ("Layout.rightMargin", margins[1]),
                    ("Layout.topMargin", margins[2]),
                    ("Layout.bottomMargin", margins[3]),
                ] {
                    if round(m) != 0.0 {
                        obj.set(name, number(m));
                    }
                }
            }
            if let Size::Length(l) = &style.width
                && let Some(w) = l.as_px()
            {
                obj.set("implicitWidth", number(w + extra_w));
            }
            obj.set("width", Value::expr(OWN_WIDTH));
            return Part {
                obj,
                hint: super::hint(self.doc, a),
                margins: [0.0; 4],
                own: true,
            };
        }
        let px = |s: &Size| match s {
            Size::Length(l) => l.as_px(),
            _ => None,
        };
        match px(&style.width) {
            Some(w) => {
                obj.set("implicitWidth", number(w + extra_w));
                obj.set("width", number(w + extra_w))
            }
            None if percent_width(style).is_some() => {
                let (percent, px) = percent_width(style).unwrap_or_default();
                let share = format!(
                    "{flow_id}.width * {} + {}",
                    fmt_number(f64::from(percent) / 100.0),
                    fmt_number(round(px + extra_w))
                );
                obj.set("width", Value::expr(share))
            }
            None => {
                // Shrink to fit, but no wider than the line.
                let mut limit = format!("{flow_id}.width");
                if margins[0] + margins[1] > 0.0 {
                    limit = format!("{limit} - {}", fmt_number(round(margins[0] + margins[1])));
                }
                if let Some(max) = px(&style.max_width) {
                    limit = format!("{}, {limit}", fmt_number(round(max + extra_w)));
                }
                // A field keeps its width on a line too narrow; only content can shrink.
                let fixed = matches!(
                    obj.type_name.as_str(),
                    "TextField" | "TextArea" | "ComboBox" | "CheckBox" | "RadioButton" | "Slider"
                ) && px(&style.max_width).is_none();
                // A button's label wraps unless the white-space it inherits forbids it.
                let label_fixed = obj.type_name == "Button"
                    && matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre);
                if obj.type_name == "Button" && !label_fixed {
                    limit = self.wrapping_label(&mut obj, style, limit);
                }
                let fixed = fixed || label_fixed;
                obj.set(
                    "width",
                    Value::expr(if fixed {
                        "implicitWidth".to_owned()
                    } else {
                        format!("Math.min(implicitWidth, {limit})")
                    }),
                )
            }
        };
        if let Some(h) = px(&style.height) {
            obj.set("height", number(h + extra_h));
        }
        Part {
            obj,
            hint: super::hint(self.doc, a),
            margins,
            own: false,
        }
    }

    /// Lets a button's label wrap, and returns the width `limit` the button
    /// can narrow to: never below its widest word, the min-content of CSS
    /// shrink-to-fit.
    fn wrapping_label(&mut self, obj: &mut Object, style: &ComputedStyle, limit: String) -> String {
        let Some(id) = obj.id.clone() else {
            return limit;
        };
        let mut stretch = None;
        for m in &mut obj.members {
            if let Member::Property {
                name,
                value: Value::Object(label),
            } = m
                && name == "contentItem"
            {
                label.set("wrapMode", Value::expr("Text.WordWrap"));
                stretch = Some(());
            }
        }
        if stretch.is_none() {
            return limit;
        }
        if matches!(obj.get("implicitHeight"), Some(Value::Number(_)))
            && !matches!(style.height, Size::Length(_))
        {
            obj.set(
                "implicitHeight",
                Value::expr(format!(
                    "topPadding + bottomPadding + Math.max(1, contentItem.lineCount) * {}",
                    fmt_number(round(line_box(style).unwrap_or_else(|| normal_line(style))))
                )),
            );
        }
        let words = self.ids.claim(&format!("{id} words"));
        let mut measure = Object::new("FontMetrics");
        measure.id = Some(words.clone());
        measure.set("font", Value::expr(format!("{id}.font")));
        obj.push_child(measure);
        let scale = text_scale(style);
        format!(
            "Math.max({limit}, Math.ceil(Math.max(...{id}.text.split(/\\s+/).map(w => {words}.advanceWidth(w))) * {scale}) + leftPadding + rightPadding)"
        )
    }

    /// `obj` in its margin box, moved down by `shift`: a Flow places
    /// items by their outer size.
    fn place_atomic(
        &mut self,
        mut obj: Object,
        hint: &str,
        margins: [f32; 4],
        shift: f32,
    ) -> Object {
        let [left, right, top, bottom] = margins;
        let shift = if round(shift) == 0.0 { 0.0 } else { shift };
        if left == 0.0 && right == 0.0 && top == 0.0 && bottom == 0.0 && shift == 0.0 {
            return obj;
        }
        let id = match &obj.id {
            Some(id) => id.clone(),
            None => {
                let id = self.ids.claim(&format!("{hint} box"));
                obj.id = Some(id.clone());
                id
            }
        };
        let plus = |expr: String, n: f32| {
            if n == 0.0 {
                expr
            } else {
                format!("{expr} + {}", fmt_number(round(n)))
            }
        };
        if left != 0.0 {
            obj.set("x", number(left));
        }
        if top + shift != 0.0 {
            obj.set("y", number(top + shift));
        }
        let mut wrapper = Object::new("Item");
        wrapper.set(
            "implicitWidth",
            Value::expr(plus(format!("{id}.implicitWidth"), left + right)),
        );
        wrapper.set(
            "width",
            Value::expr(plus(format!("{id}.width"), left + right)),
        );
        wrapper.set(
            "height",
            Value::expr(plus(format!("{id}.height"), top + bottom + shift)),
        );
        wrapper.push_child(obj);
        wrapper
    }
}

fn list_expr(items: impl Iterator<Item = String>) -> Value {
    Value::expr(format!("[{}]", items.collect::<Vec<_>>().join(", ")))
}
