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
use super::text::CONTENT_AREA;
use super::units::{fmt_number, number, round};
use crate::boxes::{BoxKind, Inline, LayoutBox, Span, node_line};

/// The layouter, written beside `Main.qml` when anything may use it.
pub(crate) const ELDUR_LINES_QML: &str = include_str!("EldurLines.qml");

/// Ascent of the common fallback fonts as a fraction of the font size
/// (Liberation Sans: 0.905).
pub(super) const ASCENT: f32 = 0.905;

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
            Inline::Span(span) if has_atomic(&span.children) => {
                chain.push(span);
                // With only boxes in it, no run would give the element a node.
                if span.node.is_some() && !has_words(&span.children) {
                    push_in_chain(run, Inline::Text(String::new()), chain);
                }
                pieces(&span.children, chain, run, out);
                chain.pop();
            }
            other => push_in_chain(run, other.clone(), chain),
        }
    }
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
        LineHeight::Normal => CONTENT_AREA * style.font_size,
        LineHeight::Number(n) => n * style.font_size,
        LineHeight::Px(px) => px,
    }
}

/// The baseline of a line of `style`'s text, from the top of the line box:
/// the glyphs are centred in it (half-leading).
pub(super) fn text_baseline(style: &ComputedStyle) -> f32 {
    (line_height(style) - CONTENT_AREA * style.font_size) / 2.0 + ASCENT * style.font_size
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

fn vertical_align(obj: &mut Object, align: VerticalAlign) {
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
        pieces(items, &mut Vec::new(), &mut run, &mut list);
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
        match style.text_align {
            TextAlign::Center => {
                flow.set("textAlign", Value::expr("Text.AlignHCenter"));
            }
            TextAlign::Right | TextAlign::End => {
                flow.set("textAlign", Value::expr("Text.AlignRight"));
            }
            _ => {}
        }
        if style.white_space == WhiteSpace::NoWrap || style.white_space == WhiteSpace::Pre {
            flow.set("noWrap", true);
        }
        if !matches!(style.white_space, WhiteSpace::Normal | WhiteSpace::NoWrap) {
            flow.set("preserveSpace", true);
        }

        let mut runs = false;
        for piece in list {
            match piece {
                Piece::Run(items) => {
                    let drawn = drop_doubled_spaces(&items, &mut false);
                    let mut text = self.text(style, decoration, &drawn, line);
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
                    let [left, right] = run_margins(&items);
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
                    vertical_align(&mut placed, a.style.vertical_align);
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
                obj.set(
                    "width",
                    Value::expr(format!("Math.min(implicitWidth, {limit})")),
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
