//! Pixel-exact positions in block flow (layout.md §1.1).
//!
//! Qt Quick Layouts snap to the pixel grid: an item's implicit height is
//! rounded up, and so is each cell (top margin + height + bottom margin),
//! with no tolerance (measured: a 27 px item with a 1e-12 px bottom margin
//! takes 28). A column of 27.2 px paragraphs therefore drifts 0.8 px per
//! paragraph from where a browser puts them. Chromium places boxes at
//! their fractional offsets and snaps only when painting.
//!
//! The cells cannot be made fractional, so each item's top margin takes
//! back what the cell before it was rounded up by: item k's *carry* is
//! `ceil(top + H) - top - h`, where `H` is the height Qt lays it out at and
//! `h` its CSS height, and item k+1's top margin is `gap - carry`. Each
//! cell is then whole, positions are exact, and only the column's last
//! cell is rounded (once). The carry is a sum of two terms in Qt as in the
//! binding (bottom margins are folded into the next top margin), so the
//! rounding the binding predicts is the one Qt does.
//!
//! What is known at conversion time stays a number: a column whose
//! heights are all whole needs nothing, and a fixed-height box only
//! shifts the next top margin. A height known only at run time (a `Text`
//! with a fractional line height, a box around one) gets
//! `property real eldurCarry`, which the next item's top margin reads.

use eldur_qml::{Member, Object, Value};

use super::Emitter;
use super::flow::is_layout;
use super::units::fmt_number;

/// What the next item's top margin takes back.
#[derive(Debug, Clone, PartialEq)]
enum Carry {
    Static(f64),
    /// A self-contained expression (an id's `eldurCarry`, or a
    /// parenthesised conditional).
    Expr(String),
    /// Computed by the item itself, as its `eldurCarry`.
    Binding(String),
}

/// The height an item is laid out at, as far as it is known here.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Height {
    /// Known: Qt lays it out at `qt` (an implicit height rounded up, or a
    /// preferred height as is), CSS at `css`.
    Static { qt: f64, css: f64 },
    /// Whole at run time (a Layout's, a fixed whole line height's).
    Whole,
    /// A `Text`'s implicit height, which Qt rounds up. Its height paints
    /// nothing, so it is left rounded.
    Text,
    /// Any other implicit height: laid out at its exact value through
    /// `Layout.preferredHeight`, as a painted box's must be.
    Other,
    /// Not its implicit height (it fills, or is clamped): the chain starts
    /// again after it.
    Unknown,
}

impl Emitter<'_, '_> {
    /// Rewrites the top margins of a block-flow `ColumnLayout`'s items so
    /// that their positions are not rounded (module docs).
    pub(super) fn carry(&mut self, layout: &mut Object) {
        let indices: Vec<usize> = layout
            .members
            .iter()
            .enumerate()
            .filter_map(|(i, m)| matches!(m, Member::Child(_)).then_some(i))
            .collect();
        // A template's item cannot name its siblings, so it keeps its own
        // rounding; it is still a pixel at most, not a height hint rounded
        // up and then the cell.
        if self.no_carry {
            for &i in &indices {
                let obj = child_mut(layout, i);
                if matches!(height(obj), Height::Other | Height::Text) {
                    obj.set("Layout.preferredHeight", Value::expr("implicitHeight"));
                }
            }
            return;
        }
        let mut carry = Carry::Static(0.0);
        for (n, &i) in indices.iter().enumerate() {
            let next = indices.get(n + 1).copied();
            let next_hideable = next.is_some_and(|j| hideable(child(layout, j)));
            let obj = child_mut(layout, i);
            let mut top = with_carry(top_margin(obj), &carry);
            set_top(obj, &top);
            let Some(next) = next else {
                // The column's last cell is rounded once, not its height
                // first and then the cell.
                if matches!(height(obj), Height::Other | Height::Text) {
                    obj.set("Layout.preferredHeight", Value::expr("implicitHeight"));
                }
                break;
            };
            // The space a line box keeps below an inline replaced box
            // (flow.rs `line_strut`) goes on the next item, so that a
            // cell is two terms. A hidden next item would take it along.
            let mut bottom = None;
            if let Some(Value::Number(b)) = obj.get("Layout.bottomMargin").cloned()
                && b != 0.0
            {
                if next_hideable {
                    carry = Carry::Static(0.0);
                    continue;
                }
                remove(obj, "Layout.bottomMargin");
                bottom = Some(b);
            }
            let h = height(obj);
            let own = match (h, &top) {
                (Height::Unknown, _) => None,
                (Height::Static { qt, css }, Gap::Static(t)) => {
                    Some(Carry::Static((t + qt).ceil() - t - css))
                }
                (Height::Whole, Gap::Static(t)) => Some(Carry::Static(t.ceil() - t)),
                _ => {
                    let (qt, css) = match h {
                        Height::Static { qt, css } => (fmt_number(qt), fmt_number(css)),
                        Height::Text => ("Math.ceil(implicitHeight)".to_owned(), "implicitHeight".to_owned()),
                        Height::Other => {
                            obj.set("Layout.preferredHeight", Value::expr("implicitHeight"));
                            ("implicitHeight".to_owned(), "implicitHeight".to_owned())
                        }
                        _ => ("implicitHeight".to_owned(), "implicitHeight".to_owned()),
                    };
                    Some(Carry::Binding(format!(
                        "Math.ceil(Layout.topMargin + {qt}) - Layout.topMargin - {css}"
                    )))
                }
            };
            let own = match own {
                Some(own) if hideable(obj) => Some(self.hideable_carry(obj, own, &carry)),
                own => own,
            };
            carry = match own {
                None => Carry::Static(0.0),
                Some(Carry::Binding(e)) => {
                    let id = self.item_id(obj);
                    obj.members.push(Member::PropertyDecl {
                        ty: "real".to_owned(),
                        name: "eldurCarry".to_owned(),
                        value: Some(Value::expr(e)),
                        readonly: false,
                        required: false,
                    });
                    Carry::Expr(format!("{id}.eldurCarry"))
                }
                Some(c) => c,
            };
            if let Some(b) = bottom {
                let next = child_mut(layout, next);
                top = top_margin(next);
                let moved = match top {
                    Gap::Static(t) => Gap::Static(t + b),
                    Gap::Expr(e) => Gap::Expr(format!("{e} + {}", fmt_number(round(b)))),
                };
                set_top(next, &moved);
            }
        }
    }

    /// Qt skips a hidden item, margins and all: what it carries then is
    /// what it was handed.
    fn hideable_carry(&mut self, obj: &mut Object, own: Carry, handed: &Carry) -> Carry {
        match (&own, handed) {
            (Carry::Static(a), Carry::Static(b)) if (a - b).abs() < 1e-9 => own,
            (Carry::Static(a), Carry::Static(b)) => {
                let id = self.item_id(obj);
                Carry::Expr(format!(
                    "({id}.visible ? {} : {})",
                    fmt_carry(*a),
                    fmt_carry(*b)
                ))
            }
            _ => Carry::Binding(format!(
                "visible ? {} : {}",
                carry_text(&own),
                carry_text(handed)
            )),
        }
    }

    fn item_id(&mut self, obj: &mut Object) -> String {
        if let Some(id) = &obj.id {
            return id.clone();
        }
        let hint = match obj.get("objectName") {
            Some(Value::String(name)) => name.clone(),
            _ => obj.type_name.to_lowercase(),
        };
        let id = self.ids.claim(&hint);
        obj.id = Some(id.clone());
        id
    }
}

/// A top margin: a number, or an expression (a hidden neighbour's).
#[derive(Debug, Clone, PartialEq)]
enum Gap {
    Static(f64),
    Expr(String),
}

fn top_margin(obj: &Object) -> Gap {
    match obj.get("Layout.topMargin") {
        Some(Value::Number(n)) => Gap::Static(*n),
        Some(Value::Expr(e)) => Gap::Expr(e.clone()),
        _ => Gap::Static(0.0),
    }
}

/// `gap - carry`, rounded as margins are printed: the next carry is of
/// the printed value, so the rounding does not add up.
fn with_carry(gap: Gap, carry: &Carry) -> Gap {
    match (gap, carry) {
        (Gap::Static(g), Carry::Static(c)) => Gap::Static(round(g - c)),
        (Gap::Static(g), Carry::Expr(c)) if g == 0.0 => Gap::Expr(format!("-{c}")),
        (Gap::Static(g), Carry::Expr(c)) => Gap::Expr(format!("{} - {c}", fmt_number(g))),
        (Gap::Expr(g), Carry::Static(c)) if c.abs() < 1e-9 => Gap::Expr(g),
        (Gap::Expr(g), Carry::Static(c)) => Gap::Expr(format!("({g}) - {}", fmt_carry(*c))),
        (Gap::Expr(g), Carry::Expr(c)) => Gap::Expr(format!("({g}) - {c}")),
        (_, Carry::Binding(_)) => unreachable!("a binding is read through its id"),
    }
}

fn set_top(obj: &mut Object, top: &Gap) {
    match top {
        Gap::Static(t) if *t == 0.0 => remove(obj, "Layout.topMargin"),
        Gap::Static(t) => {
            obj.set("Layout.topMargin", Value::Number(*t));
        }
        Gap::Expr(e) => {
            obj.set("Layout.topMargin", Value::expr(e.clone()));
        }
    }
}

fn carry_text(c: &Carry) -> String {
    match c {
        Carry::Static(v) => fmt_carry(*v),
        Carry::Expr(e) => e.clone(),
        Carry::Binding(e) => format!("({e})"),
    }
}

fn fmt_carry(v: f64) -> String {
    fmt_number(round(v))
}

fn round(v: f64) -> f64 {
    let r = (v * 100.0).round() / 100.0;
    if r == 0.0 { 0.0 } else { r }
}

fn remove(obj: &mut Object, name: &str) {
    obj.members
        .retain(|m| !matches!(m, Member::Property { name: n, .. } if n == name));
}

fn child(layout: &Object, i: usize) -> &Object {
    match &layout.members[i] {
        Member::Child(o) => o,
        _ => unreachable!("an index of a child"),
    }
}

fn child_mut(layout: &mut Object, i: usize) -> &mut Object {
    match &mut layout.members[i] {
        Member::Child(o) => o,
        _ => unreachable!("an index of a child"),
    }
}

/// Whether the item can be hidden while the page runs: it has a
/// `visible` binding, or scripts can reach it.
fn hideable(obj: &Object) -> bool {
    obj.get("visible").is_some()
        || obj.get("objectName").is_some()
        || obj
            .members
            .iter()
            .any(|m| matches!(m, Member::PropertyDecl { name, .. } if name == "eldurTag"))
}

fn height(obj: &Object) -> Height {
    if matches!(obj.get("Layout.fillHeight"), Some(Value::Bool(true)))
        || obj.get("Layout.minimumHeight").is_some()
        || obj.get("Layout.maximumHeight").is_some()
    {
        return Height::Unknown;
    }
    match obj.get("Layout.preferredHeight") {
        Some(Value::Number(n)) => return Height::Static { qt: *n, css: *n },
        Some(_) => return Height::Unknown,
        None => {}
    }
    implicit_height(obj)
}

fn implicit_height(obj: &Object) -> Height {
    if is_layout(&obj.type_name) {
        return Height::Whole;
    }
    match obj.get("implicitHeight") {
        Some(Value::Number(n)) => return Height::Static { qt: n.ceil(), css: *n },
        Some(Value::Expr(e)) => return expr_height(obj, e),
        _ => {}
    }
    if obj.type_name == "Text" {
        // A fixed line height is the height of each line.
        let fixed = matches!(obj.get("lineHeightMode"), Some(Value::Expr(m)) if m == "Text.FixedHeight");
        return match obj.get("lineHeight") {
            Some(Value::Number(l)) if fixed && l.fract() == 0.0 => Height::Whole,
            _ => Height::Text,
        };
    }
    Height::Other
}

/// `inner.implicitHeight + 12`: whole when the inner item's is and the
/// insets are.
fn expr_height(obj: &Object, expr: &str) -> Height {
    let mut terms = expr.split(" + ");
    let Some(id) = terms.next().and_then(|t| t.strip_suffix(".implicitHeight")) else {
        return Height::Other;
    };
    let mut insets = 0.0;
    for t in terms {
        match t.parse::<f64>() {
            Ok(v) => insets += v,
            Err(_) => return Height::Other,
        }
    }
    let Some(inner) = find(obj, id) else {
        return Height::Other;
    };
    match implicit_height(inner) {
        Height::Whole if insets.fract() == 0.0 => Height::Whole,
        Height::Static { css, .. } => {
            let css = css + insets;
            Height::Static {
                qt: css.ceil(),
                css,
            }
        }
        _ => Height::Other,
    }
}

fn find<'o>(obj: &'o Object, id: &str) -> Option<&'o Object> {
    obj.children().find_map(|c| {
        if c.id.as_deref() == Some(id) {
            Some(c)
        } else {
            find(c, id)
        }
    })
}
