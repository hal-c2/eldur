//! `transform` and `transform-origin` (CSS Transforms 1) → the item's
//! `transform` list.
//!
//! CSS multiplies the functions left to right, so the last one acts on the
//! box first; QML applies its list from the front, so the list is reversed.
//! Each function turns about the origin on its own, which composes to the
//! same thing as turning the whole product about it. Percentages are of the
//! box's own size, so they refer to the item by id. Like CSS, a transform
//! moves only the painting, not the layout.

use eldur_css::{ComputedStyle, Length, LengthAuto, LengthPercentage, Position, TransformFn};
use eldur_qml::{Object, Value};

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
        // Neighbouring 3-D functions multiply before z goes: a move along z
        // through a turn lands in the plane.
        let mut depth: Option<([[f32; 4]; 4], Vec<&str>)> = None;
        for f in style.transform.iter().rev() {
            if !matches!(f, TransformFn::Unsupported(_))
                && let Some(run) = depth.take()
            {
                self.flatten(&mut list, run, &origin_x, &origin_y, what, line);
            }
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
                TransformFn::Unsupported(text) if flat::matrix(text).is_some() => {
                    let m = flat::matrix(text).expect("just checked");
                    let (after, mut texts) = depth.take().unwrap_or((flat::IDENTITY, Vec::new()));
                    texts.insert(0, text);
                    depth = Some((flat::multiply(&m, &after), texts));
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
        if let Some(run) = depth.take() {
            self.flatten(&mut list, run, &origin_x, &origin_y, what, line);
        }
        if list.is_empty() {
            return;
        }
        let mut values = match obj.get("transform") {
            Some(Value::List(old)) => old.clone(),
            Some(old) => vec![old.clone()],
            None => Vec::new(),
        };
        // The runtime tells a page's move of a Text from the one that places its
        // baseline, which a rect leaves out.
        if values.is_empty() && obj.type_name == "Text" {
            list[0].set("objectName", "eldurCss");
        }
        values.extend(list.into_iter().map(|t| Value::Object(Box::new(t))));
        let value = if values.len() == 1 {
            values.pop().expect("one transform")
        } else {
            Value::List(values)
        };
        obj.set("transform", value);
    }

    /// `position: sticky`, last in the list: it moves the box as transformed.
    pub(super) fn sticky_transform(&mut self, obj: &mut Object, style: &ComputedStyle, hint: &str) {
        if style.position != Position::Sticky {
            return;
        }
        let id = match &obj.id {
            Some(id) => id.clone(),
            None => {
                let id = self.ids.claim(&format!("{hint} sticky"));
                obj.id = Some(id.clone());
                id
            }
        };
        let mut sticky = Object::new("EldurSticky");
        sticky.set("target", Value::expr(id));
        let parent = |along: &str| format!("target.parent ? target.parent.{along} : 0");
        for (name, inset, along) in [
            ("stickTop", &style.top, "height"),
            ("stickRight", &style.right, "width"),
            ("stickBottom", &style.bottom, "height"),
            ("stickLeft", &style.left, "width"),
        ] {
            if let LengthAuto::Length(l) = inset {
                sticky.set(
                    name,
                    Value::expr(length_expr(l, &format!("({})", parent(along)))),
                );
            }
        }
        for (name, margin) in [
            ("marginTop", &style.margin.top),
            ("marginRight", &style.margin.right),
            ("marginBottom", &style.margin.bottom),
            ("marginLeft", &style.margin.left),
        ] {
            if let LengthAuto::Length(l) = margin
                && let Some(px) = l.as_px()
                && px != 0.0
            {
                sticky.set(name, number(px));
            }
        }
        let mut values = match obj.get("transform") {
            Some(Value::List(old)) => old.clone(),
            Some(old) => vec![old.clone()],
            None => Vec::new(),
        };
        values.push(Value::Object(Box::new(sticky)));
        let value = if values.len() == 1 {
            values.pop().expect("one transform")
        } else {
            Value::List(values)
        };
        obj.set("transform", value);
    }

    /// A browser without `perspective` also draws a 3-D transform flat, so the
    /// plane's part of the matrix is its picture.
    fn flatten(
        &mut self,
        list: &mut Vec<Object>,
        (m, texts): ([[f32; 4]; 4], Vec<&str>),
        ox: &Value,
        oy: &Value,
        what: &str,
        line: Option<u32>,
    ) {
        self.diags.warning(
            "css",
            line,
            format!(
                "`{}` on {what} is flattened to the plane: it has no depth here, \
                 and what it overlaps stays in document order",
                texts.join(" ")
            ),
        );
        self.matrix(list, flat::plane(&m), ox, oy);
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

/// The 3-D functions a parsed `transform` keeps as text, as matrices (rows).
pub(crate) mod flat {
    pub(crate) const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    /// What a browser with no `perspective` draws of a 3-D matrix: `[a b c d e f]`.
    pub(crate) fn plane(m: &[[f32; 4]; 4]) -> [f32; 6] {
        [m[0][0], m[1][0], m[0][1], m[1][1], m[0][3], m[1][3]]
    }

    pub(crate) fn multiply(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
        let mut out = [[0.0; 4]; 4];
        for (r, row) in out.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..4).map(|k| a[r][k] * b[k][c]).sum();
            }
        }
        out
    }

    fn angle(text: &str) -> Option<f32> {
        let text = text.trim();
        for (unit, per_degree) in [
            ("grad", 0.9),
            ("deg", 1.0),
            ("rad", 180.0 / std::f32::consts::PI),
            ("turn", 360.0),
        ] {
            if let Some(n) = text.strip_suffix(unit) {
                return n
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|n| (n * per_degree).to_radians());
            }
        }
        (text == "0").then_some(0.0)
    }

    /// `None` for `perspective()`, which a flat picture cannot show, and for
    /// lengths other than pixels.
    pub(crate) fn matrix(text: &str) -> Option<[[f32; 4]; 4]> {
        let (name, args) = text.trim().split_once('(')?;
        let args = args.trim_end().strip_suffix(')')?;
        let mut m = IDENTITY;
        match name.trim().to_ascii_lowercase().as_str() {
            "translatez" => {
                let z = args.trim();
                m[2][3] = z.strip_suffix("px").unwrap_or(z).trim().parse().ok()?;
            }
            "scalez" => m[2][2] = args.trim().parse().ok()?,
            "rotatex" => {
                let (sin, cos) = angle(args)?.sin_cos();
                m[1][1] = cos;
                m[1][2] = -sin;
                m[2][1] = sin;
                m[2][2] = cos;
            }
            "rotatey" => {
                let (sin, cos) = angle(args)?.sin_cos();
                m[0][0] = cos;
                m[0][2] = sin;
                m[2][0] = -sin;
                m[2][2] = cos;
            }
            "rotate3d" => {
                let parts: Vec<&str> = args.split(',').collect();
                let [x, y, z, a] = parts[..] else {
                    return None;
                };
                let (x, y, z): (f32, f32, f32) = (
                    x.trim().parse().ok()?,
                    y.trim().parse().ok()?,
                    z.trim().parse().ok()?,
                );
                let length = (x * x + y * y + z * z).sqrt();
                if length == 0.0 {
                    return Some(m);
                }
                let (x, y, z) = (x / length, y / length, z / length);
                let (sin, cos) = angle(a)?.sin_cos();
                let t = 1.0 - cos;
                m[0] = [
                    t * x * x + cos,
                    t * x * y - sin * z,
                    t * x * z + sin * y,
                    0.0,
                ];
                m[1] = [
                    t * x * y + sin * z,
                    t * y * y + cos,
                    t * y * z - sin * x,
                    0.0,
                ];
                m[2] = [
                    t * x * z - sin * y,
                    t * y * z + sin * x,
                    t * z * z + cos,
                    0.0,
                ];
            }
            "matrix3d" => {
                let parts: Vec<f32> = args
                    .split(',')
                    .map(|n| n.trim().parse().ok())
                    .collect::<Option<_>>()?;
                if parts.len() != 16 {
                    return None;
                }
                // Written column by column; perspective terms would be lost.
                if parts[3] != 0.0 || parts[7] != 0.0 || parts[11] != 0.0 || parts[15] != 1.0 {
                    return None;
                }
                for c in 0..4 {
                    for (r, row) in m.iter_mut().enumerate() {
                        row[c] = parts[c * 4 + r];
                    }
                }
            }
            _ => return None,
        }
        Some(m)
    }
}
