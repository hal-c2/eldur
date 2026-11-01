//! Gradient background layers (CSS Images 3 §3) → QtQuick.Shapes fills.
//!
//! A `Rectangle`'s own `gradient` only runs vertically or horizontally, and
//! CSS gradients have any angle, positioned stops and repeat. A `Shape`
//! whose `ShapePath` is a `PathRectangle` with the box's corner radii,
//! filled with a `LinearGradient`/`RadialGradient`, does all of that, is
//! clipped to the rounded corners like the CSS background, and renders
//! with the software backend too.
//!
//! The gradient line depends on the box size, so its end points are
//! bindings on the shape's `width`/`height`. Everything that is linear in
//! those is kept as a [`Lin`].

use eldur_css::{
    BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin, BackgroundRepeat,
    BackgroundSize, ColorStop, ComputedStyle, Image, Length, LengthAuto, LengthPercentage,
};
use eldur_qml::{Object, Value};

use super::Emitter;
use super::decoration::{Insets, border_width, is_visible, set_radii, sides};
use super::units::{color, fmt_number};

pub(crate) const ELDUR_BACKGROUND_LAYER_QML: &str = include_str!("EldurBackgroundLayer.qml");

/// `w * width + h * height + k`, a coordinate or length on a box whose size
/// is only known at run time.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Lin {
    pub w: f64,
    pub h: f64,
    pub k: f64,
}

impl Lin {
    fn k(k: f64) -> Self {
        Self { w: 0.0, h: 0.0, k }
    }

    fn add(self, o: Self) -> Self {
        Self {
            w: self.w + o.w,
            h: self.h + o.h,
            k: self.k + o.k,
        }
    }

    fn sub(self, o: Self) -> Self {
        self.add(o.scale(-1.0))
    }

    fn scale(self, f: f64) -> Self {
        Self {
            w: self.w * f,
            h: self.h * f,
            k: self.k * f,
        }
    }

    fn is_zero(self) -> bool {
        self.w.abs() < 1e-9 && self.h.abs() < 1e-9 && self.k.abs() < 1e-9
    }

    /// `t` with `self == other * t`, when there is one.
    fn ratio_to(self, other: Self) -> Option<f64> {
        let pairs = [(self.w, other.w), (self.h, other.h), (self.k, other.k)];
        let mut t = None;
        for (a, b) in pairs {
            if b.abs() < 1e-9 {
                if a.abs() > 1e-9 {
                    return None;
                }
                continue;
            }
            let r = a / b;
            match t {
                None => t = Some(r),
                Some(t) if (t - r).abs() > 1e-6 => return None,
                _ => {}
            }
        }
        t
    }

    /// As a QML expression over `{id}.width` and `{id}.height`.
    pub(crate) fn expr(self, id: &str) -> String {
        let mut out = String::new();
        let mut term = |coef: f64, base: Option<String>| {
            let coef = round4(coef);
            if coef == 0.0 {
                return;
            }
            let sign = if coef < 0.0 { "-" } else { "+" };
            let abs = coef.abs();
            let text = match &base {
                Some(b) if abs == 1.0 => b.clone(),
                Some(b) => format!("{b} * {}", fmt_number(abs)),
                None => fmt_number(abs),
            };
            if out.is_empty() {
                if coef < 0.0 {
                    out.push('-');
                }
            } else {
                out.push_str(&format!(" {sign} "));
            }
            out.push_str(&text);
        };
        term(self.w, Some(format!("{id}.width")));
        term(self.h, Some(format!("{id}.height")));
        term(self.k, None);
        if out.is_empty() { "0".to_owned() } else { out }
    }

    /// A number when the size does not matter, else an expression.
    fn value(self, id: &str) -> Value {
        if self.w.abs() < 1e-9 && self.h.abs() < 1e-9 {
            Value::Number(round4(self.k))
        } else {
            Value::expr(self.expr(id))
        }
    }
}

fn round4(v: f64) -> f64 {
    let r = (v * 10_000.0).round() / 10_000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// A stop position along the gradient line: px plus a fraction of the
/// line length (CSS Images 3 §3.5.1 positions are `<length-percentage>`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct StopPos {
    pub px: f64,
    pub frac: f64,
}

impl StopPos {
    fn lerp(self, o: Self, t: f64) -> Self {
        Self {
            px: self.px + (o.px - self.px) * t,
            frac: self.frac + (o.frac - self.frac) * t,
        }
    }

    /// Whether `self` is before `o` whatever the line length is.
    fn surely_before(self, o: Self) -> bool {
        self.px <= o.px && self.frac <= o.frac && self != o
    }

    fn on(self, len: Lin) -> Lin {
        Lin::k(self.px).add(len.scale(self.frac))
    }
}

/// Stop positions with the missing ones filled in (CSS Images 3 §3.5.1):
/// the first defaults to 0%, the last to 100%, a position before an earlier
/// one moves up to it, and runs without positions are spread evenly.
/// `vw`/`vh` resolve against `viewport`. The clamp needs comparable
/// positions; a px and a % position are left as they are.
pub(crate) fn fixup_stops(stops: &[ColorStop], viewport: (f32, f32)) -> Vec<StopPos> {
    let resolve = |l: &LengthPercentage| StopPos {
        px: f64::from(l.px + l.vw * viewport.0 / 100.0 + l.vh * viewport.1 / 100.0),
        frac: f64::from(l.percent) / 100.0,
    };
    let n = stops.len();
    let mut pos: Vec<Option<StopPos>> = stops
        .iter()
        .map(|s| s.position.as_ref().map(resolve))
        .collect();
    if n == 0 {
        return Vec::new();
    }
    if pos[0].is_none() {
        pos[0] = Some(StopPos::default());
    }
    if pos[n - 1].is_none() {
        pos[n - 1] = Some(StopPos { px: 0.0, frac: 1.0 });
    }
    let mut max: Option<StopPos> = None;
    for p in pos.iter_mut().flatten() {
        if let Some(m) = max {
            let comparable = (p.frac == 0.0 && m.frac == 0.0) || (p.px == 0.0 && m.px == 0.0);
            if comparable && p.surely_before(m) {
                *p = m;
            }
        }
        if max.is_none_or(|m| m.surely_before(*p)) {
            max = Some(*p);
        }
    }
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if let Some(p) = pos[i] {
            out.push(p);
            i += 1;
            continue;
        }
        let before = out[out.len() - 1];
        let end = (i..n).find(|&j| pos[j].is_some()).unwrap_or(n - 1);
        let after = pos[end].unwrap_or_default();
        let gaps = (end - i + 1) as f64;
        for (k, _) in (i..end).enumerate() {
            out.push(before.lerp(after, (k + 1) as f64 / gaps));
        }
        i = end;
    }
    out
}

/// The gradient line of a `linear-gradient(angle_deg, …)` on a box (CSS
/// Images 3 §3.1.1): its start point and unit direction, and its length.
pub(crate) fn gradient_line(angle_deg: f32) -> ((Lin, Lin), (f64, f64), Lin) {
    let a = f64::from(angle_deg).to_radians();
    let (s, c) = (snap(a.sin()), snap(a.cos()));
    let len = Lin {
        w: s.abs(),
        h: c.abs(),
        k: 0.0,
    };
    let half = len.scale(0.5);
    let centre_x = Lin {
        w: 0.5,
        ..Lin::default()
    };
    let centre_y = Lin {
        h: 0.5,
        ..Lin::default()
    };
    let start = (centre_x.sub(half.scale(s)), centre_y.add(half.scale(c)));
    (start, (s, -c), len)
}

/// Removes float noise so that `90deg` gives exact 0s and 1s.
fn snap(v: f64) -> f64 {
    if v.abs() < 1e-9 {
        0.0
    } else if (v.abs() - 1.0).abs() < 1e-9 {
        v.signum()
    } else {
        v
    }
}

/// A length's px, with its viewport units, and its percentage.
fn linear_parts(l: Option<&LengthPercentage>, viewport: (f32, f32)) -> (f32, f32) {
    l.map_or((0.0, 0.0), |l| {
        (
            l.px + l.vw * viewport.0 / 100.0 + l.vh * viewport.1 / 100.0,
            l.percent,
        )
    })
}

/// Whether a gradient layer is the whole box: then it takes one `Shape` with
/// the box's corners and no texture.
fn fills_the_box(style: &ComputedStyle, layer: &BackgroundLayer) -> bool {
    layer.size == BackgroundSize::Auto
        && layer.position_x.is_zero()
        && layer.position_y.is_zero()
        && matches!(layer.repeat, BackgroundRepeat::Repeat | BackgroundRepeat::NoRepeat)
        && layer.clip == BackgroundClip::BorderBox
        && layer.attachment != BackgroundAttachment::Fixed
        // The gradient spans the padding box, so a border needs it placed.
        && (layer.origin == BackgroundOrigin::BorderBox
            || (layer.origin == BackgroundOrigin::PaddingBox
                && sides(style).iter().all(|s| !is_visible(s))))
}

impl Emitter<'_, '_> {
    /// Distances from the border box in to the padding and content boxes.
    fn background_box_insets(
        &mut self,
        style: &ComputedStyle,
        line: Option<u32>,
    ) -> (Insets, Insets) {
        let content = self.insets(style, line);
        let padding = Insets {
            top: border_width(&style.border.top),
            right: border_width(&style.border.right),
            bottom: border_width(&style.border.bottom),
            left: border_width(&style.border.left),
        };
        (padding, content)
    }

    /// An `EldurBackgroundLayer` holding an image file or a gradient tile.
    fn background_layer(
        &mut self,
        style: &ComputedStyle,
        layer: &BackgroundLayer,
        hint: &str,
        source: Option<&str>,
        gradient: Option<Object>,
        line: Option<u32>,
    ) -> Object {
        self.imports.insert("QtQuick.Shapes");
        let (padding, content) = self.background_box_insets(style, line);
        let none = Insets {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        };
        let clip = match layer.clip {
            BackgroundClip::BorderBox => none,
            BackgroundClip::PaddingBox => padding,
            BackgroundClip::ContentBox => content,
            BackgroundClip::Text => {
                self.diags.warning(
                    "css",
                    line,
                    "`background-clip: text` is not converted; the layer fills the box behind the text"
                        .to_owned(),
                );
                none
            }
        };
        let origin = match layer.origin {
            BackgroundOrigin::BorderBox => none,
            BackgroundOrigin::PaddingBox => padding,
            BackgroundOrigin::ContentBox => content,
        };
        let id = self.ids.claim(&format!("{hint} background"));
        let mut item = Object::new("EldurBackgroundLayer").with_id(id);
        item.set("anchors.fill", Value::expr("parent"));
        for (name, v) in [
            ("anchors.topMargin", clip.top),
            ("anchors.rightMargin", clip.right),
            ("anchors.bottomMargin", clip.bottom),
            ("anchors.leftMargin", clip.left),
            ("areaTop", origin.top - clip.top),
            ("areaRight", origin.right - clip.right),
            ("areaBottom", origin.bottom - clip.bottom),
            ("areaLeft", origin.left - clip.left),
        ] {
            if v != 0.0 {
                item.set(name, Value::Number(f64::from(v)));
            }
        }
        match layer.attachment {
            BackgroundAttachment::Fixed => {
                item.set("fixedToWindow", true);
                self.diags.info(
                    "css",
                    line,
                    "`background-attachment: fixed` is placed against the window once; it does not follow scrolling"
                        .to_owned(),
                );
            }
            BackgroundAttachment::Local => {
                self.diags.info(
                    "css",
                    line,
                    "`background-attachment: local` is drawn as `scroll`".to_owned(),
                );
            }
            BackgroundAttachment::Scroll => {}
        }
        let viewport = self.viewport();
        match &layer.size {
            BackgroundSize::Auto => {}
            BackgroundSize::Cover => {
                item.set("sizing", Value::Number(1.0));
            }
            BackgroundSize::Contain => {
                item.set("sizing", Value::Number(2.0));
            }
            BackgroundSize::Explicit(w, h) => {
                item.set("sizing", Value::Number(3.0));
                for (l, px, percent) in [
                    (w, "sizeWidth", "sizeWidthPercent"),
                    (h, "sizeHeight", "sizeHeightPercent"),
                ] {
                    if let LengthAuto::Length(l) = l {
                        let (k, frac) = linear_parts(
                            match l {
                                Length::Linear(l) => Some(l),
                                _ => None,
                            },
                            viewport,
                        );
                        item.set(px, Value::Number(f64::from(k)));
                        if frac != 0.0 {
                            item.set(percent, Value::Number(f64::from(frac)));
                        }
                    }
                }
            }
        }
        for (l, px, percent) in [
            (&layer.position_x, "positionX", "positionXPercent"),
            (&layer.position_y, "positionY", "positionYPercent"),
        ] {
            let (k, frac) = linear_parts(Some(l), viewport);
            if k != 0.0 {
                item.set(px, Value::Number(f64::from(k)));
            }
            if frac != 0.0 {
                item.set(percent, Value::Number(f64::from(frac)));
            }
        }
        let (repeat_x, repeat_y) = match layer.repeat {
            BackgroundRepeat::Repeat | BackgroundRepeat::Space | BackgroundRepeat::Round => {
                (true, true)
            }
            BackgroundRepeat::RepeatX => (true, false),
            BackgroundRepeat::RepeatY => (false, true),
            BackgroundRepeat::NoRepeat => (false, false),
        };
        if !repeat_x {
            item.set("repeatX", false);
        }
        if !repeat_y {
            item.set("repeatY", false);
        }
        match layer.repeat {
            BackgroundRepeat::Space => {
                item.set("spaced", true);
            }
            BackgroundRepeat::Round => {
                item.set("rounded", true);
            }
            _ => {}
        }
        // The corners of the clip box are the outer ones less what it is in by.
        let radii = self.corner_radii(style, line);
        let less = |r: f32, a: f32, b: f32| (r - a.max(b)).max(0.0);
        set_radii(
            &mut item,
            [
                less(radii[0], clip.top, clip.left),
                less(radii[1], clip.top, clip.right),
                less(radii[2], clip.bottom, clip.right),
                less(radii[3], clip.bottom, clip.left),
            ],
        );
        if let Some(source) = source {
            item.set("source", Value::string(source));
        }
        if let Some(gradient) = gradient {
            item.set("isGradient", true);
            item.push_child(gradient);
        }
        item
    }

    /// One `Shape` per gradient layer, bottom layer first, each filling the
    /// box. `hint` names the shapes' ids.
    pub(super) fn gradient_layers(
        &mut self,
        style: &ComputedStyle,
        hint: &str,
        line: Option<u32>,
    ) -> Vec<Object> {
        let mut shapes = Vec::new();
        for layer in style.background_layers.iter().rev() {
            let Some(image) = &layer.image else { continue };
            let fill = match image {
                Image::LinearGradient {
                    angle_deg,
                    stops,
                    repeating,
                } => Fill::Linear(*angle_deg, stops, *repeating),
                Image::LinearGradientToCorner {
                    right,
                    bottom,
                    stops,
                    repeating,
                } => Fill::Corner(*right, *bottom, stops, *repeating),
                Image::RadialGradient {
                    circle,
                    center_x,
                    center_y,
                    stops,
                    repeating,
                } => Fill::Radial(*circle, center_x, center_y, stops, *repeating),
                Image::ConicGradient {
                    from_deg,
                    center_x,
                    center_y,
                    stops,
                } => Fill::Conic(*from_deg, center_x, center_y, stops),
                Image::Url(url) => {
                    let Some(source) = self.background_images.get(url).cloned() else {
                        self.diags.warning(
                            "css",
                            line,
                            format!("background image `{url}` was not found; it paints nothing"),
                        );
                        continue;
                    };
                    shapes.push(self.background_layer(
                        style,
                        layer,
                        hint,
                        Some(&source),
                        None,
                        line,
                    ));
                    continue;
                }
            };
            let plain = fills_the_box(style, layer);
            self.imports.insert("QtQuick.Shapes");
            let id = self.ids.claim(&format!("{hint} gradient"));
            let mut path = Object::new("ShapePath");
            path.set("strokeWidth", Value::Number(-1.0));
            match fill {
                Fill::Linear(angle, stops, repeating) => {
                    let g = self.linear(&id, angle, stops, repeating);
                    path.set("fillGradient", Value::Object(Box::new(g)));
                }
                Fill::Corner(right, bottom, stops, repeating) => {
                    let g = self.linear_to_corner(&id, (right, bottom), stops, repeating);
                    path.set("fillGradient", Value::Object(Box::new(g)));
                }
                Fill::Conic(from, cx, cy, stops) => {
                    let g = self.conic(&id, from, cx, cy, stops);
                    path.set("fillGradient", Value::Object(Box::new(g)));
                }
                Fill::Radial(circle, cx, cy, stops, repeating) => {
                    let (g, transform) = self.radial(&id, circle, cx, cy, stops, repeating, line);
                    if let Some(t) = transform {
                        path.set("fillTransform", Value::expr(t));
                    }
                    path.set("fillGradient", Value::Object(Box::new(g)));
                }
            }
            let mut rect = Object::new("PathRectangle");
            rect.set("width", Value::expr(format!("{id}.width")));
            rect.set("height", Value::expr(format!("{id}.height")));
            if plain {
                self.radius(&mut rect, style, line);
            }
            path.push_child(rect);
            let mut shape = Object::new("Shape").with_id(id);
            shape.set("anchors.fill", Value::expr("parent"));
            shape.push_child(path);
            if plain {
                shapes.push(shape);
            } else {
                shapes.push(self.background_layer(style, layer, hint, None, Some(shape), line));
            }
        }
        shapes
    }

    fn linear(&self, id: &str, angle: f32, stops: &[ColorStop], repeating: bool) -> Object {
        let (start, dir, len) = gradient_line(angle);
        let positions = fixup_stops(stops, self.viewport());
        let mut g = Object::new("LinearGradient");
        let along: Vec<Lin> = positions.iter().map(|p| p.on(len)).collect();
        let first = along.first().copied().unwrap_or_default();
        let last = along.last().copied().unwrap_or_default();
        // The gradient runs from the first stop to the last; CSS pads the
        // colours beyond them, or repeats that stretch.
        let (first, span) = if last.sub(first).is_zero() {
            // All stops at one place: a hard edge there.
            (first, Lin::k(0.01))
        } else {
            (first, last.sub(first))
        };
        let point = |p: Lin| (start.0.add(p.scale(dir.0)), start.1.add(p.scale(dir.1)));
        let (x1, y1) = point(first);
        let (x2, y2) = point(first.add(span));
        g.set("x1", x1.value(id));
        g.set("y1", y1.value(id));
        g.set("x2", x2.value(id));
        g.set("y2", y2.value(id));
        if repeating {
            g.set("spread", Value::expr("ShapeGradient.RepeatSpread"));
        }
        let mut positions = Vec::with_capacity(stops.len());
        for p in &along {
            let offset = p.sub(first);
            positions.push(match offset.ratio_to(span) {
                Some(t) => Value::Number(round4(t.clamp(0.0, 1.0))),
                None if offset.is_zero() => Value::Number(0.0),
                None => Value::expr(format!(
                    "Math.max(0, Math.min(1, ({}) / ({})))",
                    offset.expr(id),
                    span.expr(id)
                )),
            });
        }
        separate(&mut positions);
        for (stop, position) in stops.iter().zip(positions) {
            g.push_child(gradient_stop(position, stop));
        }
        g
    }

    /// `linear-gradient(to <corner>, …)`: the gradient line is
    /// perpendicular to the diagonal between the other two corners, so its
    /// angle follows the box's aspect ratio (CSS Images 3 §3.1.1). With
    /// `w`, `h` the box and `d = hypot(w, h)`, its direction is
    /// `(±h, ±w) / d` and its length `2wh / d`.
    fn linear_to_corner(
        &self,
        id: &str,
        (right, bottom): (bool, bool),
        stops: &[ColorStop],
        repeating: bool,
    ) -> Object {
        let positions = fixup_stops(stops, self.viewport());
        let first = positions.first().copied().unwrap_or_default();
        let mut last = positions.last().copied().unwrap_or_default();
        if last == first {
            // All stops at one place: a hard edge there.
            last.px += 0.01;
        }
        let (w, h) = (format!("{id}.width"), format!("{id}.height"));
        let len = format!("(2 * {w} * {h} / Math.max(1, Math.hypot({w}, {h})))");
        // The point `p` along the line, as `x` or `y`: `size` is the box's
        // side on that axis, `other` the other side.
        let coord = |p: StopPos, sign: f64, size: &str, other: &str| {
            let mut out = format!("{size} / 2");
            let f = round4(2.0 * p.frac - 1.0) * sign;
            if f != 0.0 {
                out.push_str(&format!(
                    " {} {size} * {other} * {other} * {} / Math.max(1, {w} * {w} + {h} * {h})",
                    if f < 0.0 { "-" } else { "+" },
                    fmt_number(f.abs())
                ));
            }
            let k = round4(p.px) * sign;
            if k != 0.0 {
                out.push_str(&format!(
                    " {} {other} * {} / Math.max(1, Math.hypot({w}, {h}))",
                    if k < 0.0 { "-" } else { "+" },
                    fmt_number(k.abs())
                ));
            }
            Value::expr(out)
        };
        let sx = if right { 1.0 } else { -1.0 };
        let sy = if bottom { 1.0 } else { -1.0 };
        let mut g = Object::new("LinearGradient");
        g.set("x1", coord(first, sx, &w, &h));
        g.set("y1", coord(first, sy, &h, &w));
        g.set("x2", coord(last, sx, &w, &h));
        g.set("y2", coord(last, sy, &h, &w));
        if repeating {
            g.set("spread", Value::expr("ShapeGradient.RepeatSpread"));
        }
        // A stop's distance along the line, as `px + frac * len`.
        let along = |p: StopPos| Lin {
            w: p.frac,
            h: 0.0,
            k: p.px,
        };
        let span = along(last).sub(along(first));
        let mut values = Vec::with_capacity(stops.len());
        for p in &positions {
            let offset = along(*p).sub(along(first));
            values.push(match offset.ratio_to(span) {
                Some(t) => Value::Number(round4(t.clamp(0.0, 1.0))),
                None if offset.is_zero() => Value::Number(0.0),
                None => {
                    let on = |l: Lin| {
                        format!(
                            "{} + {} * {len}",
                            fmt_number(round4(l.k)),
                            fmt_number(round4(l.w))
                        )
                    };
                    Value::expr(format!(
                        "Math.max(0, Math.min(1, ({}) / ({})))",
                        on(offset),
                        on(span)
                    ))
                }
            });
        }
        separate(&mut values);
        for (stop, position) in stops.iter().zip(values) {
            g.push_child(gradient_stop(position, stop));
        }
        g
    }

    /// `conic-gradient()` as a `ConicalGradient`. CSS starts at 12 o'clock
    /// and turns clockwise; Qt starts at 3 o'clock and turns
    /// anticlockwise, so the start angle is `90 - from` and the stops are
    /// mirrored, `1 - position`.
    fn conic(
        &self,
        id: &str,
        from: f32,
        cx: &LengthPercentage,
        cy: &LengthPercentage,
        stops: &[ColorStop],
    ) -> Object {
        let (x, y) = self.centre(cx, cy);
        let mut g = Object::new("ConicalGradient");
        g.set("centerX", x.value(id));
        g.set("centerY", y.value(id));
        let angle = (90.0 - f64::from(from)).rem_euclid(360.0);
        g.set("angle", Value::Number(round4(angle)));
        let positions = fixup_stops(stops, self.viewport());
        let mut values: Vec<Value> = positions
            .iter()
            .rev()
            .map(|p| Value::Number(round4(1.0 - p.frac.clamp(0.0, 1.0))))
            .collect();
        separate(&mut values);
        for (stop, position) in stops.iter().rev().zip(values) {
            g.push_child(gradient_stop(position, stop));
        }
        g
    }

    /// A gradient's centre on the box, from `at <position>`.
    fn centre(&self, cx: &LengthPercentage, cy: &LengthPercentage) -> (Lin, Lin) {
        let (vw, vh) = self.viewport();
        let axis = |l: &LengthPercentage, w: f64, h: f64| Lin {
            w: f64::from(l.percent) / 100.0 * w,
            h: f64::from(l.percent) / 100.0 * h,
            k: f64::from(l.px + l.vw * vw / 100.0 + l.vh * vh / 100.0),
        };
        (axis(cx, 1.0, 0.0), axis(cy, 0.0, 1.0))
    }

    /// A radial gradient sized `farthest-corner` (the only size the style
    /// keeps); an ellipse is a circle scaled by `fillTransform`.
    #[allow(clippy::too_many_arguments)]
    fn radial(
        &mut self,
        id: &str,
        circle: bool,
        cx: &LengthPercentage,
        cy: &LengthPercentage,
        stops: &[ColorStop],
        repeating: bool,
        line: Option<u32>,
    ) -> (Object, Option<String>) {
        let (vw, vh) = self.viewport();
        let (x, y) = self.centre(cx, cy);
        let full_w = Lin {
            w: 1.0,
            ..Lin::default()
        };
        let full_h = Lin {
            h: 1.0,
            ..Lin::default()
        };
        // Distance from the centre to the farther side, per axis.
        let reach = |c: Lin, full: Lin| {
            let other = full.sub(c);
            if c == full.scale(0.5) {
                c.expr(id)
            } else {
                format!("Math.max({}, {})", c.expr(id), other.expr(id))
            }
        };
        let (dx, dy) = (reach(x, full_w), reach(y, full_h));
        let (radius, transform) = if circle {
            (format!("Math.hypot({dx}, {dy})"), None)
        } else {
            // farthest-corner keeps the box's aspect ratio: radii are the
            // side distances times √2.
            (
                format!("({dx}) * Math.SQRT2"),
                Some(format!(
                    "PlanarTransform.fromScale(1, ({dy}) / Math.max(1, {dx}), {}, {})",
                    x.expr(id),
                    y.expr(id)
                )),
            )
        };
        let mut g = Object::new("RadialGradient");
        g.set("centerX", x.value(id));
        g.set("centerY", y.value(id));
        g.set("centerRadius", Value::expr(radius.clone()));
        g.set("focalX", x.value(id));
        g.set("focalY", y.value(id));
        let positions = fixup_stops(stops, (vw, vh));
        if repeating {
            // One period runs from the first stop's circle to the last's.
            let at = |p: StopPos| {
                format!(
                    "{} + {} * ({radius})",
                    fmt_number(round4(p.px)),
                    fmt_number(round4(p.frac))
                )
            };
            let first = positions.first().copied().unwrap_or_default();
            let last = positions.last().copied().unwrap_or_default();
            let (inner, outer) = (at(first), at(last));
            g.set(
                "centerRadius",
                Value::expr(format!("Math.max(0.01, {outer})")),
            );
            g.set("focalRadius", Value::expr(inner.clone()));
            g.set("spread", Value::expr("ShapeGradient.RepeatSpread"));
            let span = |p: StopPos| (p.px - first.px, p.frac - first.frac);
            let (span_px, span_frac) = span(last);
            let mut values: Vec<Value> = positions
                .iter()
                .map(|p| {
                    let (px, frac) = span(*p);
                    // Lengths alone or fractions alone have a ratio known
                    // now, and a number keeps hard stops apart.
                    if span_frac == 0.0 && frac == 0.0 && span_px > 0.0 {
                        Value::Number(round4(px / span_px))
                    } else if span_px == 0.0 && px == 0.0 && span_frac > 0.0 {
                        Value::Number(round4(frac / span_frac))
                    } else {
                        Value::expr(format!(
                            "Math.max(0, Math.min(1, ({} - ({inner})) / Math.max(0.01, ({outer}) - ({inner}))))",
                            at(*p)
                        ))
                    }
                })
                .collect();
            separate(&mut values);
            for (stop, position) in stops.iter().zip(values) {
                g.push_child(gradient_stop(position, stop));
            }
            return (g, transform);
        }
        let mut lossy = false;
        let mut values = Vec::with_capacity(stops.len());
        for p in &positions {
            values.push(if p.px == 0.0 {
                lossy |= !(0.0..=1.0).contains(&p.frac);
                Value::Number(round4(p.frac.clamp(0.0, 1.0)))
            } else {
                Value::expr(format!(
                    "Math.max(0, Math.min(1, {} / ({radius}) + {}))",
                    fmt_number(round4(p.px)),
                    fmt_number(round4(p.frac))
                ))
            });
        }
        separate(&mut values);
        for (stop, position) in stops.iter().zip(values) {
            g.push_child(gradient_stop(position, stop));
        }
        if lossy {
            self.diags.info(
                "css",
                line,
                "radial-gradient stops beyond the ending shape are clamped to it".to_owned(),
            );
        }
        (g, transform)
    }

    fn viewport(&self) -> (f32, f32) {
        let v = &self.options.viewport;
        (v.width, v.height)
    }
}

enum Fill<'s> {
    Linear(f32, &'s [ColorStop], bool),
    /// To a corner: right?, bottom?.
    Corner(bool, bool, &'s [ColorStop], bool),
    Conic(
        f32,
        &'s LengthPercentage,
        &'s LengthPercentage,
        &'s [ColorStop],
    ),
    Radial(
        bool,
        &'s LengthPercentage,
        &'s LengthPercentage,
        &'s [ColorStop],
        bool,
    ),
}

/// Qt keeps one stop per position, so a hard edge (two stops at one
/// position) would lose a colour: equal constant positions are moved a
/// hair apart, keeping them within 0…1.
pub(crate) fn separate(positions: &mut [Value]) {
    const EPS: f64 = 0.0001;
    let mut prev: Option<f64> = None;
    for v in positions.iter_mut() {
        if let Value::Number(n) = v {
            if let Some(p) = prev
                && *n <= p
            {
                *n = round4(p + EPS);
            }
            prev = Some(*n);
        }
    }
    // Past the end: walk back from 1.
    let mut next: Option<f64> = None;
    for v in positions.iter_mut().rev() {
        if let Value::Number(n) = v {
            let cap = next.map_or(1.0, |x| round4(x - EPS));
            if *n > cap {
                *n = cap;
            }
            next = Some(*n);
        }
    }
}

fn gradient_stop(position: Value, stop: &ColorStop) -> Object {
    let mut s = Object::new("GradientStop");
    s.set("position", position);
    s.set("color", color(stop.color));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use eldur_css::Rgba;

    fn stop(position: Option<LengthPercentage>) -> ColorStop {
        ColorStop {
            color: Rgba::WHITE,
            position,
        }
    }

    fn pct(v: f32) -> Option<LengthPercentage> {
        Some(LengthPercentage::percent(v))
    }

    fn px(v: f32) -> Option<LengthPercentage> {
        Some(LengthPercentage::px(v))
    }

    fn frac(f: f64) -> StopPos {
        StopPos { px: 0.0, frac: f }
    }

    #[test]
    fn missing_stop_positions_are_spread_evenly() {
        let got = fixup_stops(&[stop(None), stop(None), stop(None)], (0.0, 0.0));
        assert_eq!(got, [frac(0.0), frac(0.5), frac(1.0)]);
        let got = fixup_stops(&[stop(pct(20.0)), stop(None), stop(pct(80.0))], (0.0, 0.0));
        assert_eq!(got, [frac(0.2), frac(0.5), frac(0.8)]);
    }

    #[test]
    fn a_stop_before_an_earlier_one_moves_up_to_it() {
        let got = fixup_stops(&[stop(pct(60.0)), stop(pct(20.0))], (0.0, 0.0));
        assert_eq!(got, [frac(0.6), frac(0.6)]);
        let got = fixup_stops(&[stop(px(10.0)), stop(px(5.0)), stop(px(20.0))], (0.0, 0.0));
        assert_eq!(
            got[1],
            StopPos {
                px: 10.0,
                frac: 0.0
            }
        );
    }

    #[test]
    fn gradient_line_follows_the_css_angle() {
        // `to bottom`: from the top edge's middle, straight down, the
        // box's height long.
        let ((x, y), dir, len) = gradient_line(180.0);
        assert_eq!(
            (x, y),
            (
                Lin {
                    w: 0.5,
                    h: 0.0,
                    k: 0.0
                },
                Lin::default()
            )
        );
        assert_eq!(dir, (0.0, 1.0));
        assert_eq!(
            len,
            Lin {
                w: 0.0,
                h: 1.0,
                k: 0.0
            }
        );
        // 135deg on a 200×100 box: the line passes through the centre and
        // is |w sin a| + |h cos a| long, so the corners get the end colours.
        let ((x, y), (dx, dy), len) = gradient_line(135.0);
        let (w, h) = (200.0, 100.0);
        let at = |l: Lin| l.w * w + l.h * h + l.k;
        let l = at(len);
        assert!((l - 150.0 * std::f64::consts::SQRT_2).abs() < 1e-6);
        let (mx, my) = (at(x) + dx * l / 2.0, at(y) + dy * l / 2.0);
        assert!((mx - 100.0).abs() < 1e-6 && (my - 50.0).abs() < 1e-6);
    }

    #[test]
    fn hard_edges_keep_both_colours() {
        let mut v = [0.0, 0.2, 0.2, 0.8, 0.8, 1.0, 1.0].map(Value::Number);
        separate(&mut v);
        assert_eq!(
            v,
            [0.0, 0.2, 0.2001, 0.8, 0.8001, 0.9999, 1.0].map(Value::Number)
        );
    }

    #[test]
    fn lin_prints_as_a_binding() {
        let l = Lin {
            w: 0.25,
            h: -0.25,
            k: 0.0,
        };
        assert_eq!(l.expr("g"), "g.width * 0.25 - g.height * 0.25");
        assert_eq!(Lin::k(3.0).expr("g"), "3");
        assert_eq!(
            Lin {
                w: 0.0,
                h: -1.0,
                k: 2.0
            }
            .expr("g"),
            "-g.height + 2"
        );
    }
}
