//! An inline formatting context with atomic inline-level boxes
//! (`inline-block`, `inline-flex`, …) → a `Flow`.
//!
//! One `Text` cannot hold a box, so the line is cut into pieces a `Flow`
//! can wrap: one `Text` per word (its spans kept as StyledText tags) and
//! one item per atomic box. The space after a word is a trailing no-break
//! space, which Qt measures, so gaps come from the page's own white space.
//! `Flow` aligns the items of a row by their tops; CSS aligns baselines, so
//! each item is moved down by its baseline's distance from the lowest one,
//! estimated from the fonts and insets at conversion time.

use h2q_css::{
    BoxSizing, ComputedStyle, LengthAuto, LineHeight, Overflow, Size, TextAlign,
    TextDecorationLine, VerticalAlign,
};
use h2q_qml::{Member, Object, Value};

use super::Emitter;
use super::text::CONTENT_AREA;
use super::units::{fmt_number, number, round};
use crate::boxes::{BoxKind, Content, Inline, LayoutBox, Span, node_line};

/// Ascent of the common fallback fonts as a fraction of the font size
/// (Liberation Sans: 0.905).
const ASCENT: f32 = 0.905;

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

/// A piece of the line a `Flow` places.
enum Piece<'b, 'a> {
    /// A word and the space after it, its spans kept.
    Word(Vec<Inline<'a>>),
    Atomic(&'b LayoutBox<'a>),
    Break,
}

/// Text of one word under one chain of spans.
type Segment<'b, 'a> = (Vec<&'b Span<'a>>, String);

fn pieces<'b, 'a>(
    items: &'b [Inline<'a>],
    chain: &mut Vec<&'b Span<'a>>,
    word: &mut Vec<Segment<'b, 'a>>,
    out: &mut Vec<Piece<'b, 'a>>,
) {
    for item in items {
        match item {
            Inline::Text(text) => {
                for c in text.chars() {
                    if c == ' ' {
                        push_char(word, chain, '\u{a0}');
                        end_word(word, out);
                    } else {
                        push_char(word, chain, c);
                    }
                }
            }
            Inline::LineBreak => {
                end_word(word, out);
                out.push(Piece::Break);
            }
            Inline::Span(span) => {
                chain.push(span);
                pieces(&span.children, chain, word, out);
                chain.pop();
            }
            Inline::Atomic(b) => {
                end_word(word, out);
                out.push(Piece::Atomic(b));
            }
        }
    }
}

fn push_char<'b, 'a>(word: &mut Vec<Segment<'b, 'a>>, chain: &[&'b Span<'a>], c: char) {
    let same = |other: &[&Span<'a>]| {
        other.len() == chain.len() && other.iter().zip(chain).all(|(a, b)| std::ptr::eq(*a, *b))
    };
    match word.last_mut() {
        Some((spans, text)) if same(spans) => text.push(c),
        _ => word.push((chain.to_vec(), c.to_string())),
    }
}

fn end_word<'a>(word: &mut Vec<Segment<'_, 'a>>, out: &mut Vec<Piece<'_, 'a>>) {
    if word.is_empty() {
        return;
    }
    let items = word
        .drain(..)
        .map(|(chain, text)| {
            chain.iter().rev().fold(Inline::Text(text), |inner, span| {
                Inline::Span(Span {
                    node: span.node,
                    style: span.style,
                    children: vec![inner],
                })
            })
        })
        .collect();
    out.push(Piece::Word(items));
}

/// A piece converted, before baselines are lined up.
enum Part {
    Word(Object),
    Break,
    Atomic {
        obj: Object,
        /// Base for the id of its margin box.
        hint: String,
        /// Horizontal and vertical margins.
        margins: [f32; 4],
        /// From the top margin edge; `None` sits at the top of the row.
        baseline: Option<f32>,
        /// A created inline element: its object is its template, so it is
        /// not wrapped.
        own: bool,
    },
}

/// The line height of `style` in px.
fn line_height(style: &ComputedStyle) -> f32 {
    match style.line_height {
        LineHeight::Normal => CONTENT_AREA * style.font_size,
        LineHeight::Number(n) => n * style.font_size,
        LineHeight::Px(px) => px,
    }
}

/// The baseline of a line of `style`'s text, from the top of the line box:
/// the glyphs are centred in it (half-leading).
fn text_baseline(style: &ComputedStyle) -> f32 {
    (line_height(style) - CONTENT_AREA * style.font_size) / 2.0 + ASCENT * style.font_size
}

impl Emitter<'_, '_> {
    /// The `Flow` of block container `b`'s inline content `items`.
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
        let mut word = Vec::new();
        pieces(items, &mut Vec::new(), &mut word, &mut list);
        end_word(&mut word, &mut list);

        let mut parts = Vec::new();
        for piece in list {
            parts.push(match piece {
                Piece::Word(items) => {
                    let mut text = self.text(style, decoration, &items, line);
                    // A word is as wide as its glyphs; the Flow wraps.
                    text.members.retain(|m| {
                        !matches!(m, Member::Property { name, .. }
                            if name == "wrapMode" || name == "horizontalAlignment")
                    });
                    Part::Word(text)
                }
                Piece::Break => Part::Break,
                Piece::Atomic(a) => self.atomic(a, decoration, &flow_id),
            });
        }
        let runs = parts
            .iter()
            .any(|p| matches!(p, Part::Atomic { own: true, .. }));
        let word_baseline = text_baseline(style);
        let lowest = parts
            .iter()
            .filter_map(|p| match p {
                Part::Word(_) => Some(word_baseline),
                Part::Atomic {
                    margins, baseline, ..
                } => baseline.map(|b| margins[2] + b),
                Part::Break => None,
            })
            .fold(0.0_f32, f32::max);

        let mut flow = Object::new("Flow").with_id(flow_id.clone());
        let padding = match style.text_align {
            TextAlign::Center => Some("Math.floor((width - used) / 2)"),
            TextAlign::Right | TextAlign::End => Some("Math.floor(width - used)"),
            _ => None,
        };
        if let Some(padding) = padding {
            // Flow has no alignment: the free space of a row goes before
            // it. Measured on all items, so it is right for one row and
            // left-aligns a paragraph that wraps.
            flow.set(
                "leftPadding",
                Value::expr(format!(
                    "{{ const used = Array.from(children).reduce((sum, item) => sum + item.width, 0); return Math.max(0, {padding}); }}"
                )),
            );
        }
        for part in parts {
            match part {
                Part::Word(mut text) => {
                    let shift = lowest - word_baseline;
                    if round(shift) > 0.0 {
                        text.set("topPadding", number(shift));
                    }
                    flow.push_child(text);
                }
                Part::Break => {
                    // A row of its own as wide as the Flow ends the line.
                    flow.push_child(
                        Object::new("Item")
                            .prop("width", Value::expr(format!("{flow_id}.width")))
                            .prop("height", 0.0),
                    );
                }
                Part::Atomic { obj, own: true, .. } => {
                    flow.push_child(obj);
                }
                Part::Atomic {
                    obj,
                    hint,
                    margins,
                    baseline,
                    own: false,
                } => {
                    let shift = baseline.map_or(0.0, |b| lowest - margins[2] - b);
                    flow.push_child(self.place_atomic(obj, &hint, margins, shift));
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
                    name: "h2qTextRun".to_owned(),
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
            }
            obj.set("width", Value::expr(OWN_WIDTH));
            return Part::Atomic {
                obj,
                hint: super::hint(self.doc, a),
                margins: [0.0; 4],
                baseline: None,
                own: true,
            };
        }
        let px = |s: &Size| match s {
            Size::Length(l) => l.as_px(),
            _ => None,
        };
        match px(&style.width) {
            Some(w) => obj.set("width", number(w + extra_w)),
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
        let baseline = match style.vertical_align {
            VerticalAlign::Baseline | VerticalAlign::Middle => self.first_baseline(a),
            _ => None,
        };
        Part::Atomic {
            obj,
            hint: super::hint(self.doc, a),
            margins,
            baseline,
            own: false,
        }
    }

    /// The baseline of an atomic inline from its top border edge: that of
    /// its first line (CSS 2.2 §10.8.1). `None` when it has no line or
    /// clips, where CSS uses the bottom margin edge.
    fn first_baseline(&mut self, b: &LayoutBox<'_>) -> Option<f32> {
        let style = &*b.style;
        if style.overflow_x != Overflow::Visible || style.overflow_y != Overflow::Visible {
            return None;
        }
        let line = b.node.and_then(|n| node_line(self.doc, n));
        let top = self.insets(style, line).top;
        let first = match &b.content {
            Content::Inline(_) => return Some(top + text_baseline(style)),
            Content::Empty => return None,
            // Flex items all start at the top of the content box.
            Content::Blocks(children) if b.kind == BoxKind::Flex => children
                .iter()
                .find_map(|c| Some((c, self.first_baseline(c)?)))?,
            Content::Blocks(children) => {
                let child = children.first()?;
                (child, self.first_baseline(child)?)
            }
        };
        let (child, baseline) = first;
        let margin = match &child.style.margin.top {
            LengthAuto::Length(l) => l.as_px().unwrap_or(0.0).max(0.0),
            LengthAuto::Auto => 0.0,
        };
        Some(top + margin + baseline)
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
