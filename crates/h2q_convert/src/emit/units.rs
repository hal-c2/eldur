//! CSS lengths and colours as QML values.

use h2q_css::{CalcNode, Length, LengthPercentage, Rgba};
use h2q_qml::{Color, Value};

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
