//! Inset `box-shadow`s as `Shape`s, which the software renderer draws
//! (`RectangularShadow` and `MultiEffect` it does not).
//!
//! An inset shadow paints the padding box outside a "hole": the padding
//! box moved by the offset and shrunk by the spread (CSS Backgrounds 3
//! §7.1.1). Without blur that is one ring, the padding box less the hole's
//! part inside it. A blur fades the edge of the hole; measured in Chromium,
//! the shadow's alpha at depth `d` inside the hole's edge is
//! `1 - Φ(d / σ)` with `σ = blur / 2`. That is drawn as nested rings whose
//! alphas compose to the curve.

use h2q_css::ComputedStyle;
use h2q_qml::{Object, Value};

use super::Emitter;
use super::decoration::{border_width, set_radii};
use super::units::{color, fmt_number, number, round};

/// One ring: the hole's edge `depth` px inside the shadow's own hole edge,
/// painted at `alpha` (of the shadow colour's alpha 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Band {
    pub depth: f32,
    pub alpha: f32,
}

/// The rings for a blur radius, as fractions of an `alpha`: a point at
/// depth `d` is covered by every ring with `d < depth`.
pub(crate) fn inset_bands(blur: f32, alpha: f32) -> Vec<Band> {
    if blur <= 0.0 {
        return vec![Band { depth: 0.0, alpha }];
    }
    let sigma = blur / 2.0;
    // ±2σ holds 95% of the fade; bands of about 2px, at most 12 of them.
    let span = 4.0 * sigma;
    let n = ((span / 2.0).ceil() as usize).clamp(1, 12);
    let step = span / n as f32;
    let edge = |k: usize| -2.0 * sigma + k as f32 * step;
    let coverage = |d: f32| alpha * (1.0 - phi(d / sigma));
    // `left[k]`: what is left uncovered in band k (between edges k-1 and
    // k), at its middle. Rings k..=n cover band k, so ring k's alpha is
    // the ratio of what is left on either side of its edge.
    let left: Vec<f32> = (0..=n)
        .map(|k| 1.0 - coverage(edge(k) - step / 2.0))
        .chain(std::iter::once(1.0))
        .collect();
    (0..=n)
        .map(|k| Band {
            depth: edge(k),
            alpha: 1.0 - left[k] / left[k + 1],
        })
        .filter(|b| b.alpha * 255.0 >= 0.5)
        .collect()
}

/// The standard normal CDF (Abramowitz and Stegun 7.1.26, error < 1.5e-7).
fn phi(x: f32) -> f32 {
    let z = f64::from(x) / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * z.abs());
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-z * z).exp();
    let erf = if z < 0.0 { -erf } else { erf };
    (0.5 * (1.0 + erf)) as f32
}

/// Which edge bounds the hole's part inside the padding box on one side.
#[derive(Clone, Copy, PartialEq)]
enum Edge {
    Hole,
    Box,
    Both,
}

fn edge(inset: f32) -> Edge {
    if inset > 0.0 {
        Edge::Hole
    } else if inset < 0.0 {
        Edge::Box
    } else {
        Edge::Both
    }
}

impl Emitter<'_, '_> {
    /// One `Shape` over the padding box with a ring per band of every
    /// inset shadow, the first shadow in CSS on top.
    pub(super) fn inset_shadows(
        &mut self,
        style: &ComputedStyle,
        hint: &str,
        line: Option<u32>,
    ) -> Object {
        self.imports.insert("QtQuick.Shapes");
        let id = self.ids.claim(&format!("{hint} inset"));
        let b = &style.border;
        let borders = [
            border_width(&b.top),
            border_width(&b.right),
            border_width(&b.bottom),
            border_width(&b.left),
        ];
        // The padding box's corners: the border box's less the border.
        let outer = self.corner_radii(style, line);
        let [top, right, bottom, left] = borders;
        let padding = [
            (outer[0] - left.max(top)).max(0.0),
            (outer[1] - right.max(top)).max(0.0),
            (outer[2] - right.max(bottom)).max(0.0),
            (outer[3] - left.max(bottom)).max(0.0),
        ];
        let mut shape = Object::new("Shape").with_id(id.clone());
        shape.set("anchors.fill", Value::expr("parent"));
        for (name, v) in [
            ("anchors.topMargin", top),
            ("anchors.rightMargin", right),
            ("anchors.bottomMargin", bottom),
            ("anchors.leftMargin", left),
        ] {
            if v != 0.0 {
                shape.set(name, number(v));
            }
        }
        for s in style.box_shadow.iter().rev().filter(|s| s.inset) {
            let alpha = f32::from(s.color.a) / 255.0;
            for band in inset_bands(s.blur, alpha) {
                let spread = s.spread + band.depth;
                // How far the hole's edge is inside the padding box on each
                // side; negative when the hole reaches past it.
                let insets = [
                    s.offset_y + spread,
                    spread - s.offset_x,
                    spread - s.offset_y,
                    s.offset_x + spread,
                ];
                if insets.iter().all(|i| *i <= 0.0) {
                    // The hole covers the padding box: nothing to paint.
                    continue;
                }
                let [it, ir, ib, il] = insets.map(|i| round(i.max(0.0)));
                let mut c = s.color;
                c.a = (band.alpha * 255.0).round().clamp(0.0, 255.0) as u8;
                let mut path = Object::new("ShapePath");
                path.set("strokeWidth", Value::Number(-1.0));
                path.set("fillColor", color(c));
                path.set("fillRule", Value::expr("ShapePath.OddEvenFill"));
                let mut outer_rect = Object::new("PathRectangle");
                outer_rect.set("width", Value::expr(format!("{id}.width")));
                outer_rect.set("height", Value::expr(format!("{id}.height")));
                set_radii(&mut outer_rect, padding);
                path.push_child(outer_rect);
                let mut hole = Object::new("PathRectangle");
                if il != 0.0 {
                    hole.set("x", Value::Number(il));
                }
                if it != 0.0 {
                    hole.set("y", Value::Number(it));
                }
                hole.set("width", Value::expr(less(&format!("{id}.width"), il + ir)));
                hole.set(
                    "height",
                    Value::expr(less(&format!("{id}.height"), it + ib)),
                );
                let edges = insets.map(edge);
                // The hole's own corners are the padding box's less the
                // spread; where the box's edge bounds it, the box's corner.
                let corners = [(0, 3), (0, 1), (2, 1), (2, 3)];
                let radii = std::array::from_fn(|i| {
                    let (v, h) = corners[i];
                    let hole_r = (padding[i] - spread).max(0.0);
                    match (edges[v], edges[h]) {
                        (Edge::Hole | Edge::Both, Edge::Hole | Edge::Both) => hole_r,
                        (Edge::Box | Edge::Both, Edge::Box | Edge::Both) => padding[i],
                        // One side each: the box's curve, cut short by the
                        // hole's edge.
                        _ => (padding[i] - insets[v].max(insets[h])).max(0.0),
                    }
                });
                set_radii(&mut hole, radii);
                path.push_child(hole);
                shape.push_child(path);
            }
        }
        shape
    }
}

/// `Math.max(0, expr - n)`, or `expr` when `n` is 0.
fn less(expr: &str, n: f64) -> String {
    if n == 0.0 {
        expr.to_owned()
    } else {
        format!("Math.max(0, {expr} - {})", fmt_number(n))
    }
}
