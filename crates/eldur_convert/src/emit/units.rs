//! CSS lengths and colours as QML values.

use eldur_css::{CalcNode, Length, LengthPercentage, Rgba};
use eldur_qml::{Color, Value};

/// Id of the window (or root item); `vw`/`vh` bind to its size.
pub(crate) const ROOT_ID: &str = "root";

/// Rounds to 1/100 px: finer differences are invisible and only make the
/// output noisy (`17.6000004`).
pub(crate) fn round(px: f32) -> f64 {
    (f64::from(px) * 100.0).round() / 100.0
}

pub(crate) fn number(px: f32) -> Value {
    Value::Number(round(px))
}

pub(crate) fn color(c: Rgba) -> Value {
    Value::Color(qml_color(c))
}

pub(crate) fn qml_color(c: Rgba) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a)
}

/// A length as a QML expression; percentages are of `percent_base`
/// (e.g. `parent.width`).
pub(crate) fn length_expr(length: &Length, percent_base: &str) -> String {
    match length {
        Length::Linear(l) => linear_expr(l, percent_base),
        Length::Calc(node) => calc_expr(node, percent_base),
    }
}

fn linear_expr(l: &LengthPercentage, percent_base: &str) -> String {
    let mut terms = Vec::new();
    let mut term = |factor: f32, base: &str| {
        if factor != 0.0 {
            // Percent and viewport units are hundredths of the base.
            let factor = round(factor) / 100.0;
            terms.push(if factor == 1.0 {
                base.to_owned()
            } else {
                format!("{base} * {}", fmt_number(factor))
            });
        }
    };
    term(l.percent, percent_base);
    term(l.vw, &format!("{ROOT_ID}.width"));
    term(l.vh, &format!("{ROOT_ID}.height"));
    if l.px != 0.0 || terms.is_empty() {
        terms.push(fmt_number(round(l.px)));
    }
    terms.join(" + ").replace("+ -", "- ")
}

fn calc_expr(node: &CalcNode, percent_base: &str) -> String {
    let list = |nodes: &[CalcNode]| {
        nodes
            .iter()
            .map(|n| calc_expr(n, percent_base))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match node {
        CalcNode::Leaf(l) => linear_expr(l, percent_base),
        CalcNode::Min(nodes) => format!("Math.min({})", list(nodes)),
        CalcNode::Max(nodes) => format!("Math.max({})", list(nodes)),
        CalcNode::Clamp(min, value, max) => format!(
            "Math.max({}, Math.min({}, {}))",
            calc_expr(min, percent_base),
            calc_expr(value, percent_base),
            calc_expr(max, percent_base)
        ),
        CalcNode::Sum(nodes) => format!(
            "({})",
            nodes
                .iter()
                .map(|n| calc_expr(n, percent_base))
                .collect::<Vec<_>>()
                .join(" + ")
        ),
        CalcNode::Product(node, factor) => format!(
            "({}) * {}",
            calc_expr(node, percent_base),
            fmt_number(f64::from(*factor))
        ),
    }
}

/// Shortest round-tripping form, integers without `.0`.
pub(crate) fn fmt_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// The px part of a length, and whether anything else was dropped.
pub(crate) fn px_part(length: &Length) -> (f32, bool) {
    match length {
        Length::Linear(l) => (l.px, l.as_px().is_none()),
        Length::Calc(_) => (0.0, true),
    }
}

/// Whether `length` has a percentage term anywhere, even inside `calc()`.
pub(crate) fn has_percentage(length: &Length) -> bool {
    fn node(n: &CalcNode) -> bool {
        match n {
            CalcNode::Leaf(l) => l.percent != 0.0,
            CalcNode::Min(nodes) | CalcNode::Max(nodes) | CalcNode::Sum(nodes) => {
                nodes.iter().any(node)
            }
            CalcNode::Clamp(a, b, c) => node(a) || node(b) || node(c),
            CalcNode::Product(n, _) => node(n),
        }
    }
    match length {
        Length::Linear(l) => l.percent != 0.0,
        Length::Calc(n) => node(n),
    }
}

/// `length` with its percentages resolved against `base` px.
pub(crate) fn percent_of(length: &Length, base: f32) -> Length {
    let leaf = |l: &LengthPercentage| LengthPercentage {
        px: l.px + l.percent * base / 100.0,
        percent: 0.0,
        ..*l
    };
    fn node(n: &CalcNode, leaf: &dyn Fn(&LengthPercentage) -> LengthPercentage) -> CalcNode {
        let all = |nodes: &[CalcNode]| nodes.iter().map(|n| node(n, leaf)).collect();
        match n {
            CalcNode::Leaf(l) => CalcNode::Leaf(leaf(l)),
            CalcNode::Min(nodes) => CalcNode::Min(all(nodes)),
            CalcNode::Max(nodes) => CalcNode::Max(all(nodes)),
            CalcNode::Sum(nodes) => CalcNode::Sum(all(nodes)),
            CalcNode::Clamp(a, b, c) => CalcNode::Clamp(
                Box::new(node(a, leaf)),
                Box::new(node(b, leaf)),
                Box::new(node(c, leaf)),
            ),
            CalcNode::Product(n, k) => CalcNode::Product(Box::new(node(n, leaf)), *k),
        }
    }
    match length {
        Length::Linear(l) => Length::Linear(leaf(l)),
        Length::Calc(n) => Length::Calc(node(n, &leaf)),
    }
}

/// A `column-gap` or `row-gap` (`normal` is 0 in flex and grid layout):
/// px, or a length whose percentages are of the container's content box
/// (CSS Box Alignment 3 §8.1), bound to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Gap(Option<Length>);

impl Gap {
    pub(crate) fn new(length: Option<Length>) -> Self {
        Self(length.filter(|l| l.as_px() != Some(0.0)))
    }

    /// The gap in px, when it is a fixed length.
    pub(crate) fn px(&self) -> Option<f32> {
        match &self.0 {
            None => Some(0.0),
            Some(l) => l.as_px().map(|px| px.max(0.0)),
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.px() == Some(0.0)
    }

    /// The gap as an expression, its percentages of `base`. A gap is never
    /// negative, and only `calc()` can come out so.
    fn expr(&self, base: &str) -> String {
        match &self.0 {
            None => "0".to_owned(),
            Some(l @ Length::Calc(_)) => format!("Math.max(0, {})", length_expr(l, base)),
            Some(l) => length_expr(l, base),
        }
    }

    /// The gap, percentages of `base`.
    pub(crate) fn value(&self, base: &str) -> Value {
        self.plus(0.0, 1.0, base)
    }

    /// `px + k·gap`.
    pub(crate) fn plus(&self, px: f32, k: f32, base: &str) -> Value {
        match self.px() {
            Some(gap) => number(px + k * gap),
            None => Value::expr(self.plus_text(px, k, base)),
        }
    }

    /// [`Self::plus`] as expression text.
    pub(crate) fn plus_text(&self, px: f32, k: f32, base: &str) -> String {
        if let Some(gap) = self.px() {
            return fmt_number(round(px + k * gap));
        }
        let k = round(k);
        let gap = self.expr(base);
        let scaled = match k {
            0.0 => return fmt_number(round(px)),
            1.0 => gap,
            // A sum of terms, unlike a call.
            k if matches!(self.0, Some(Length::Linear(_))) && gap.contains(' ') => {
                format!("({gap}) * {}", fmt_number(k))
            }
            k => format!("{gap} * {}", fmt_number(k)),
        };
        match round(px) {
            0.0 => scaled,
            p => format!("{} + {scaled}", fmt_number(p)).replace("+ -", "- "),
        }
    }
}
