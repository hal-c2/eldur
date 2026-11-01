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
//! A sum meant to be whole can come out a hair over it in floating point,
//! and Qt then rounds the cell a whole pixel up (and places a last item
//! after the extra pixel). Each carry computed at run time is a micropixel
//! more than exact, and a static top margin whose sum would land a hair
//! over is a micropixel less, so such cells stay whole.
//!
//! What is known at conversion time stays a number: a column whose
//! heights are all whole needs nothing, and a fixed-height box only
//! shifts the next top margin. A height known only at run time (a `Text`
//! with a fractional line height, a box around one) gets
//! `property real eldurCarry`, which the next item's top margin reads.

use std::collections::HashMap;

use eldur_qml::{Member, Object, Value};

use super::Emitter;
use super::flow::is_layout;
use super::units::fmt_number;

/// What a carry computed at run time adds, so that the next cell, meant
/// to be whole, is not a hair over (module docs).
const NUDGE: f64 = 1e-6;

/// The comment an element's outermost object carries while its column is
/// built, so an id the carry gives it is named after the element as other
/// ids are, not after its QML type.
const HINT: &str = "\u{1}eldur-hint:";

/// The comment a carried column carries until its parent is built: the key
/// of its excess in `Emitter::excess`. Its id is not chosen yet (a wrapper
/// names it after the element).
const EXCESS: &str = "\u{1}eldur-excess:";

/// What [`HINT`] and [`EXCESS`] start with.
const CARRY_COMMENT: &str = "\u{1}eldur-";

pub(super) fn hint_comment(hint: &str) -> Member {
    Member::Comment(format!("{HINT}{hint}"))
}

pub(super) fn strip_hints(obj: &mut Object) {
    obj.members
        .retain(|m| !matches!(m, Member::Comment(c) if c.starts_with(CARRY_COMMENT)));
    for m in &mut obj.members {
        match m {
            Member::Child(c) => strip_hints(c),
            Member::Property { value, .. } => strip_value(value),
            _ => {}
        }
    }
}

fn strip_value(value: &mut Value) {
    match value {
        Value::Object(o) => strip_hints(o),
        Value::List(items) => items.iter_mut().for_each(strip_value),
        _ => {}
    }
}

/// A carried column's last rounding (`Emitter::carry`), by its id.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Excess {
    Static(f64),
    Expr(String),
    /// The last item's carry, declared as its `eldurCarry` once a parent
    /// reads it (`item` counts the column's children).
    Pending {
        item: usize,
        binding: String,
    },
}

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
#[derive(Debug, Clone, PartialEq)]
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
    /// A carried column's (or a box around one, `extra` being what Qt
    /// rounds the box's insets up by): whole, `excess` over its CSS height.
    /// A painted box is laid out at its CSS height all the same.
    Excess { key: usize, extra: f64 },
    /// Not its implicit height (it fills, or is clamped): the chain starts
    /// again after it.
    Unknown,
}

impl Emitter<'_, '_> {
    /// Rewrites the top margins of a block-flow `ColumnLayout`'s items so
    /// that their positions are not rounded (module docs).
    pub(super) fn carry(&mut self, layout: &mut Object) {
        // Qt adds spacing between cells as is; only whole spacing keeps
        // the cell edges whole.
        match layout.get("spacing").or(layout.get("rowSpacing")) {
            None => {}
            Some(Value::Number(s)) if s.fract() == 0.0 => {}
            Some(_) => return,
        }
        let in_column = layout.type_name == "ColumnLayout";
        // A spacer's cell is between two whole cell edges whatever it
        // takes, so the carry goes past it.
        let indices: Vec<usize> = layout
            .members
            .iter()
            .enumerate()
            .filter_map(|(i, m)| match m {
                Member::Child(c) if !is_spacer(c) => Some(i),
                _ => None,
            })
            .collect();
        // A template's item cannot name its siblings, so it keeps its own
        // rounding; it is still a pixel at most, not a height hint rounded
        // up and then the cell.
        if self.no_carry {
            for &i in &indices {
                let obj = child_mut(layout, i);
                if matches!(height(obj, &self.excess), Height::Other | Height::Text) {
                    obj.set("Layout.preferredHeight", Value::expr("implicitHeight"));
                }
            }
            return;
        }
        let mut carry = Carry::Static(0.0);
        let mut last_own = None;
        for (n, &i) in indices.iter().enumerate() {
            let next = indices.get(n + 1).copied();
            let next_hideable = next.is_some_and(|j| hideable(child(layout, j)));
            let obj = child_mut(layout, i);
            let mut top = with_carry(top_margin(obj), &carry);
            set_top(obj, &top);
            // The space a line box keeps below an inline replaced box
            // (flow.rs `line_strut`) goes on the next item, so that a
            // cell is two terms. A hidden next item would take it along.
            // The last cell keeps its own, and only a number is added up.
            let mut bottom = None;
            let mut last_bottom = 0.0;
            match obj.get("Layout.bottomMargin").cloned() {
                Some(Value::Number(b)) if b != 0.0 && next.is_none() => last_bottom = b,
                Some(Value::Number(b)) if b != 0.0 => {
                    if next_hideable {
                        carry = Carry::Static(0.0);
                        continue;
                    }
                    remove(obj, "Layout.bottomMargin");
                    bottom = Some(b);
                }
                Some(Value::Expr(_)) if next.is_none() => break,
                _ => {}
            }
            let mut h = height(obj, &self.excess);
            // A box paints its height, and Chromium's bottom edge is that of
            // its exact one: what rounds a cell must stay out of its height.
            let painted = obj.type_name == "Rectangle";
            if let (true, Height::Static { qt, css }) = (painted, &h)
                && qt != css
            {
                let css = *css;
                obj.set("Layout.preferredHeight", Value::Number(css));
                h = Height::Static { qt: css, css };
            }
            // The last cell is rounded once, not its height first and
            // then the cell.
            if next.is_none() && h == Height::Text {
                h = Height::Other;
            }
            // A cell meant to be whole can come out a hair over in floating
            // point (-0.8 + 28.8 is not 28 for every pair), and Qt rounds
            // that up a whole pixel, then misplaces a last item by it. A
            // micropixel less keeps it under.
            if let (Gap::Static(t), Height::Static { qt, .. }) = (&top, &h) {
                let cell = t + qt;
                if cell > cell.round() && cell - cell.round() < 1e-9 {
                    top = Gap::Static(((t - NUDGE) * 1e6).round() / 1e6);
                    set_top(obj, &top);
                }
            }
            let exact = match h {
                Height::Excess { key, extra } if painted => {
                    let e = self.read_excess(obj, key, 0.0);
                    obj.set(
                        "Layout.preferredHeight",
                        Value::expr(format!("implicitHeight - {}", carry_text(&e))),
                    );
                    // The cell is now whole less `over`; see the check above.
                    if let (Gap::Static(t), Carry::Static(e)) = (&top, &e) {
                        let cell = t + last_bottom - round(e + extra);
                        if cell > cell.round() && cell - cell.round() < 1e-9 {
                            top = Gap::Static(((t - NUDGE) * 1e6).round() / 1e6);
                            set_top(obj, &top);
                        }
                    }
                    Some((e, extra))
                }
                _ => None,
            };
            let own = match (h, &top) {
                (Height::Unknown, _) => None,
                (Height::Excess { .. }, top) if exact.is_some() => {
                    let (e, extra) = exact.expect("checked");
                    match (top, e) {
                        (Gap::Static(t), Carry::Static(e)) => {
                            let t = t + last_bottom;
                            let over = round(e + extra);
                            Some(Carry::Static((t - over).ceil() - t + over))
                        }
                        _ if last_bottom != 0.0 => None,
                        (_, e) => {
                            let over = if round(extra) == 0.0 {
                                carry_text(&e)
                            } else {
                                format!("{} + {}", carry_text(&e), fmt_carry(extra))
                            };
                            Some(Carry::Binding(format!(
                                "Math.ceil(Layout.topMargin - ({over})) - Layout.topMargin + {over} + {NUDGE}"
                            )))
                        }
                    }
                }
                (Height::Static { qt, css }, Gap::Static(t)) => Some(Carry::Static(
                    (t + qt + last_bottom).ceil() - t - css - last_bottom,
                )),
                (Height::Whole, Gap::Static(t)) => {
                    Some(Carry::Static((t + last_bottom).ceil() - t - last_bottom))
                }
                (Height::Excess { key, extra }, top) => {
                    let plain_row = in_column
                        && obj.type_name == "RowLayout"
                        && matches!(self.excess.get(&key), Some(Excess::Pending { binding, .. })
                            if binding.starts_with("(h =>"));
                    let e = self.read_excess(obj, key, extra);
                    match (top, e) {
                        (Gap::Static(t), Carry::Static(e)) => Some(Carry::Static(
                            (t + last_bottom).ceil() - t - last_bottom + e,
                        )),
                        _ if last_bottom != 0.0 => None,
                        (Gap::Static(t), e) if t.fract() == 0.0 => Some(e),
                        (Gap::Static(t), e) => Some(Carry::Expr(format!(
                            "({} + {})",
                            fmt_carry(t.ceil() - t),
                            carry_text(&e)
                        ))),
                        // A row of plain items with only the carry for a margin is a whole height less
                        // it, to the nearest pixel (measured); with a margin, rounded up.
                        (Gap::Expr(_), e) if plain_row => Some(Carry::Binding(format!(
                            "(Layout.topMargin < 0 ? Math.round(Layout.topMargin) : \
                                 Math.ceil(Layout.topMargin)) - Layout.topMargin + {} + {NUDGE}",
                            carry_text(&e)
                        ))),
                        (Gap::Expr(_), e) => Some(Carry::Binding(format!(
                            "Math.ceil(Layout.topMargin) - Layout.topMargin + {} + {NUDGE}",
                            carry_text(&e)
                        ))),
                    }
                }
                _ if last_bottom != 0.0 => None,
                (h, _) => {
                    let (qt, css) = match h {
                        Height::Static { qt, css } => (fmt_number(qt), fmt_number(css)),
                        Height::Text => (
                            "Math.ceil(implicitHeight)".to_owned(),
                            "implicitHeight".to_owned(),
                        ),
                        Height::Other => {
                            obj.set("Layout.preferredHeight", Value::expr("implicitHeight"));
                            ("implicitHeight".to_owned(), "implicitHeight".to_owned())
                        }
                        _ => ("implicitHeight".to_owned(), "implicitHeight".to_owned()),
                    };
                    Some(Carry::Binding(format!(
                        "Math.ceil(Layout.topMargin + {qt}) - Layout.topMargin - {css} + {NUDGE}"
                    )))
                }
            };
            let own = match own {
                Some(own) if hideable(obj) => Some(self.hideable_carry(obj, own, &carry)),
                own => own,
            };
            let Some(next) = next else {
                last_own = own.map(|c| (n, c));
                break;
            };
            carry = match own {
                None => Carry::Static(0.0),
                Some(Carry::Binding(e)) => Carry::Expr(self.declare_carry(obj, e)),
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
        // What the column's implicit height has over its CSS height, for
        // the item it is in (`Height::Excess`).
        let excess = match last_own {
            Some((_, Carry::Static(v))) if round(v) == 0.0 => return,
            Some((_, Carry::Static(v))) => Excess::Static(v),
            Some((_, Carry::Expr(e))) => Excess::Expr(e),
            Some((item, Carry::Binding(binding))) => Excess::Pending { item, binding },
            None => return,
        };
        let key = self.excess.len();
        self.excess.insert(key, excess);
        layout
            .members
            .push(Member::Comment(format!("{EXCESS}{key}")));
    }

    /// A row rounds its tallest item up once. What it adds is an excess, as
    /// a carried column's is, for the column the row is in to take back.
    /// Only plain items: a nested layout's implicit height is rounded already,
    /// and a margin or fixed height is not in an item's implicit height.
    pub(super) fn row_excess(&mut self, row: &mut Object) {
        if self.no_carry {
            return;
        }
        let kids: Vec<&Object> = row.children().collect();
        let plain = kids.iter().all(|k| {
            !is_layout(&k.type_name)
                && [
                    "Layout.topMargin",
                    "Layout.bottomMargin",
                    "Layout.margins",
                    "Layout.preferredHeight",
                    "Layout.minimumHeight",
                ]
                .iter()
                .all(|p| k.get(p).is_none())
        });
        if !plain || !kids.iter().any(|k| holds_fractional_line(k)) {
            return;
        }
        let item = kids.len() - 1;
        let key = self.excess.len();
        self.excess.insert(
            key,
            Excess::Pending {
                item,
                binding: format!(
                    "(h => Math.ceil(h) - h)(Math.max(0, ...Array.from(parent.children)\
                     .filter(k => k.visible).map(k => k.implicitHeight))) + {NUDGE}"
                ),
            },
        );
        row.members.push(Member::Comment(format!("{EXCESS}{key}")));
    }

    /// A carried column's excess, plus `extra`; the last item declares
    /// its `eldurCarry` when it is first read.
    fn read_excess(&mut self, obj: &mut Object, key: usize, extra: f64) -> Carry {
        let excess = match self.excess.get(&key).cloned() {
            Some(Excess::Pending { item, binding }) => {
                let layout = find_mut(obj, key).expect("the excess of a column inside");
                let last = layout
                    .members
                    .iter_mut()
                    .filter_map(|m| match m {
                        Member::Child(c) => Some(c),
                        _ => None,
                    })
                    .nth(item)
                    .expect("the column's last item");
                let e = self.declare_carry(last, binding);
                self.excess.insert(key, Excess::Expr(e.clone()));
                Excess::Expr(e)
            }
            Some(e) => e,
            None => Excess::Static(0.0),
        };
        match excess {
            Excess::Static(v) => Carry::Static(v + extra),
            Excess::Expr(e) if round(extra) == 0.0 => Carry::Expr(e),
            Excess::Expr(e) => Carry::Expr(format!("({e} + {})", fmt_carry(extra))),
            Excess::Pending { .. } => unreachable!("declared above"),
        }
    }

    /// Declares `eldurCarry` on the item; what reads it.
    fn declare_carry(&mut self, obj: &mut Object, binding: String) -> String {
        let id = self.item_id(obj);
        obj.members.push(Member::PropertyDecl {
            ty: "real".to_owned(),
            name: "eldurCarry".to_owned(),
            value: Some(Value::expr(binding)),
            readonly: false,
            required: false,
        });
        format!("{id}.eldurCarry")
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

    pub(super) fn item_id(&mut self, obj: &mut Object) -> String {
        if let Some(id) = &obj.id {
            return id.clone();
        }
        let hint = obj
            .members
            .iter()
            .find_map(|m| match m {
                Member::Comment(c) => c.strip_prefix(HINT).map(str::to_owned),
                _ => None,
            })
            .or_else(|| match obj.get("objectName") {
                Some(Value::String(name)) => Some(name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| obj.type_name.to_lowercase());
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
        (Gap::Static(0.0), Carry::Expr(c)) => Gap::Expr(format!("-{c}")),
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

fn holds_fractional_line(obj: &Object) -> bool {
    matches!(obj.get("lineHeight"), Some(Value::Number(l)) if l.fract() != 0.0)
        || obj.children().any(holds_fractional_line)
}

fn height(obj: &Object, excess: &HashMap<usize, Excess>) -> Height {
    if matches!(obj.get("Layout.fillHeight"), Some(Value::Bool(true)))
        || obj.get("Layout.minimumHeight").is_some()
        || obj
            .get("Layout.maximumHeight")
            .is_some_and(|m| !matches!(m, Value::Expr(e) if e == "Number.POSITIVE_INFINITY"))
    {
        return Height::Unknown;
    }
    match obj.get("Layout.preferredHeight") {
        Some(Value::Number(n)) => return Height::Static { qt: *n, css: *n },
        Some(_) => return Height::Unknown,
        None => {}
    }
    implicit_height(obj, excess)
}

fn implicit_height(obj: &Object, excess: &HashMap<usize, Excess>) -> Height {
    if is_layout(&obj.type_name) {
        return match excess_key(obj) {
            Some(key) if excess.contains_key(&key) => Height::Excess { key, extra: 0.0 },
            _ => Height::Whole,
        };
    }
    match obj.get("implicitHeight") {
        Some(Value::Number(n)) => {
            return Height::Static {
                qt: n.ceil(),
                css: *n,
            };
        }
        Some(Value::Expr(e)) => return expr_height(obj, e, excess),
        _ => {}
    }
    if obj.type_name == "Text" {
        // A fixed line height is the height of each line.
        let fixed =
            matches!(obj.get("lineHeightMode"), Some(Value::Expr(m)) if m == "Text.FixedHeight");
        return match obj.get("lineHeight") {
            Some(Value::Number(l)) if fixed && l.fract() == 0.0 => Height::Whole,
            _ => Height::Text,
        };
    }
    Height::Other
}

/// `inner.implicitHeight + 12`: whole when the inner item's is and the
/// insets are.
fn expr_height(obj: &Object, expr: &str, excess: &HashMap<usize, Excess>) -> Height {
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
    match implicit_height(inner, excess) {
        Height::Whole if insets.fract() == 0.0 => Height::Whole,
        Height::Excess { key, extra } => Height::Excess {
            key,
            extra: extra + insets.ceil() - insets,
        },
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

/// A flex container's `Item { Layout.fillHeight: true }`.
fn is_spacer(obj: &Object) -> bool {
    obj.type_name == "Item"
        && obj.members.iter().all(|m| {
            matches!(m, Member::Property { name, .. }
                if obj.id.is_none()
                    && (name == "Layout.fillHeight" || name == "Layout.verticalStretchFactor"))
        })
}

fn excess_key(obj: &Object) -> Option<usize> {
    obj.members.iter().find_map(|m| match m {
        Member::Comment(c) => c.strip_prefix(EXCESS).and_then(|k| k.parse().ok()),
        _ => None,
    })
}

/// The carried column with excess `key`: `obj` or one inside it.
fn find_mut(obj: &mut Object, key: usize) -> Option<&mut Object> {
    if excess_key(obj) == Some(key) {
        return Some(obj);
    }
    obj.members.iter_mut().find_map(|m| match m {
        Member::Child(c) => find_mut(c, key),
        _ => None,
    })
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
