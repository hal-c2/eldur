//! `transform` and `transform-origin` (CSS Transforms 1) → the item's
//! `transform` list.
//!
//! CSS multiplies the functions left to right, so the last one acts on the
//! box first; QML applies its list from the front, so the list is reversed.
//! Each function turns about the origin on its own, which composes to the
//! same thing as turning the whole product about it. Percentages are of the
//! box's own size, so they refer to the item by id. Like CSS, a transform
//! moves only the painting, not the layout.

use h2q_css::{ComputedStyle, Length, LengthPercentage, TransformFn};
use h2q_qml::{Object, Value};

use super::Emitter;
use super::units::{fmt_number, length_expr, number};

/// A length along one axis of the box: a number, or an expression of the
/// item's size.
fn along(l: &Length, id: &str, axis: &str) -> Value {
    match l.as_px() {
        Some(px) => number(px),
        None => Value::expr(length_expr(l, &format!("{id}.{axis}"))),
    }
}

fn is_px(l: &LengthPercentage) -> bool {
    Length::Linear(*l).as_px().is_some()
}

impl Emitter<'_, '_> {
    /// Sets `obj`'s `transform` from `style`, after any it already has (a
    /// text's baseline shift is in its own coordinates).
    pub(super) fn transform(
        &mut self,
        obj: &mut Object,
        style: &ComputedStyle,
        hint: &str,
        what: &str,
        line: Option<u32>,
    ) {
        if style.transform.is_empty() {
            return;
        }
        let (ox, oy) = style.transform_origin;
        let needs_size = style.transform.iter().any(|f| match f {
            TransformFn::Translate(x, y) => !is_px(x) || !is_px(y),
            TransformFn::TranslateCalc(..) => true,
            TransformFn::Unsupported(_) => false,
            _ => !is_px(&ox) || !is_px(&oy),
        });
        let id = match (&obj.id, needs_size) {
            (Some(id), _) => id.clone(),
            (None, true) => {
                let id = self.ids.claim(&format!("{hint} box"));
                obj.id = Some(id.clone());
                id
            }
            (None, false) => String::new(),
        };
        let origin_x = along(&Length::Linear(ox), &id, "width");
        let origin_y = along(&Length::Linear(oy), &id, "height");
        let with_origin = |mut t: Object| {
            if origin_x != Value::Number(0.0) {
                t.set("origin.x", origin_x.clone());
            }
            if origin_y != Value::Number(0.0) {
                t.set("origin.y", origin_y.clone());
            }
            t
        };
        let mut list = Vec::new();
        for f in style.transform.iter().rev() {
            match f {
                TransformFn::Translate(x, y) => {
                    list.push(translate(
                        along(&Length::Linear(*x), &id, "width"),
                        along(&Length::Linear(*y), &id, "height"),
                    ));
                }
                TransformFn::TranslateCalc(x, y) => {
                    list.push(translate(along(x, &id, "width"), along(y, &id, "height")));
                }
                TransformFn::Scale(sx, sy) => {
                    let mut t = Object::new("Scale");
                    t.set("xScale", Value::Number(round_factor(*sx)));
                    t.set("yScale", Value::Number(round_factor(*sy)));
                    list.push(with_origin(t));
                }
                TransformFn::Rotate(deg) => {
                    let mut t = Object::new("Rotation");
                    t.set("angle", number(*deg));
                    list.push(with_origin(t));
                }
                TransformFn::Skew(ax, ay) => {
                    let (b, c) = (ay.to_radians().tan(), ax.to_radians().tan());
                    self.matrix(&mut list, [1.0, b, c, 1.0, 0.0, 0.0], &origin_x, &origin_y);
                }
                TransformFn::Matrix(m) => {
                    self.matrix(&mut list, *m, &origin_x, &origin_y);
                }
                TransformFn::Unsupported(text) => {
                    self.diags.warning(
                        "css",
                        line,
                        format!("`{text}` on {what} has no 2-D form and is left out"),
                    );
                }
            }
        }
        if list.is_empty() {
            return;
        }
        let mut values = match obj.get("transform") {
            Some(Value::List(old)) => old.clone(),
            Some(old) => vec![old.clone()],
            None => Vec::new(),
        };
        values.extend(list.into_iter().map(|t| Value::Object(Box::new(t))));
        let value = if values.len() == 1 {
            values.pop().expect("one transform")
        } else {
            Value::List(values)
        };
        obj.set("transform", value);
    }

    /// The 2-D matrix `[a b c d e f]` about the origin: moved there, applied
    /// and moved back (pushed in QML order).
    fn matrix(&mut self, list: &mut Vec<Object>, m: [f32; 6], ox: &Value, oy: &Value) {
        let at_origin = *ox == Value::Number(0.0) && *oy == Value::Number(0.0);
        let negate = |v: &Value| match v {
            Value::Number(n) => Value::Number(-n),
            Value::Expr(e) => Value::expr(format!("-({e})")),
            other => other.clone(),
        };
        if !at_origin {
            list.push(translate(negate(ox), negate(oy)));
        }
        let [a, b, c, d, e, f] = m.map(|v| fmt_number(round_factor(v)));
        let mut t = Object::new("Matrix4x4");
        t.set(
            "matrix",
            Value::expr(format!(
                "Qt.matrix4x4({a}, {c}, 0, {e}, {b}, {d}, 0, {f}, 0, 0, 1, 0, 0, 0, 0, 1)"
            )),
        );
        list.push(t);
        if !at_origin {
            list.push(translate(ox.clone(), oy.clone()));
        }
    }
}

fn translate(x: Value, y: Value) -> Object {
    let mut t = Object::new("Translate");
    if x != Value::Number(0.0) {
        t.set("x", x);
    }
    if y != Value::Number(0.0) {
        t.set("y", y);
    }
    t
}

/// Scale factors keep three decimals: a px rounding would lose them.
fn round_factor(v: f32) -> f64 {
    (f64::from(v) * 1000.0).round() / 1000.0
}
