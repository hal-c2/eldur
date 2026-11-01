//! `@keyframes` on pages the runtime does not run (animations.md): QML
//! animations, so a page without scripts gains no clock of its own. Pages with
//! open elements have the runtime's engine instead.
//!
//! An animation cannot start part way through in QML, so a paused one is
//! worked out here and written as plain values; that is also what pins a time
//! in a test.

use eldur_css::{
    Animation, BoxSizing, ComputedStyle, Easing, Keyframe, Length, LengthAuto, Position, Rgba,
    Size, TransformFn,
};
use eldur_dom::NodeId;
use eldur_qml::{Member, Object, Value};

use super::Emitter;
use super::units::{color, fmt_number, number};

#[derive(Clone, Copy, PartialEq)]
enum Sample {
    Number(f32),
    Color(Rgba),
}

impl Sample {
    fn value(self) -> Value {
        match self {
            Self::Number(n) => number(n),
            Self::Color(c) => color(c),
        }
    }

    fn towards(self, to: Self, t: f32) -> Self {
        match (self, to) {
            (Self::Number(a), Self::Number(b)) => Self::Number(a + (b - a) * t),
            (Self::Color(a), Self::Color(b)) => Self::Color(mix(a, b, t)),
            _ => self,
        }
    }
}

/// Colours meet premultiplied (CSS Color 4 §12.3).
fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let alpha = f32::from(a.a) + (f32::from(b.a) - f32::from(a.a)) * t;
    let channel = |x: u8, y: u8| {
        if alpha <= 0.0 {
            return 0;
        }
        let from = f32::from(x) * f32::from(a.a);
        let to = f32::from(y) * f32::from(b.a);
        byte((from + (to - from) * t) / alpha)
    };
    Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: byte(alpha),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn byte(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// One animated property of one object, by id.
struct Track {
    target: String,
    property: String,
    base: Sample,
    /// Offset, value, and the timing to the next stop; 0% and 100% always there.
    stops: Vec<(f32, Sample, Easing)>,
}

impl Track {
    fn at(&self, progress: f32) -> Sample {
        for pair in self.stops.windows(2) {
            let (from, to) = (&pair[0], &pair[1]);
            if progress <= to.0 {
                let span = to.0 - from.0;
                let local = if span > 0.0 {
                    (progress - from.0) / span
                } else {
                    1.0
                };
                return from.1.towards(to.1, ease(from.2, local));
            }
        }
        self.stops.last().map_or(self.base, |s| s.1)
    }

    fn first(&self, reversed: bool) -> Sample {
        let stop = if reversed {
            self.stops.last()
        } else {
            self.stops.first()
        };
        stop.map_or(self.base, |s| s.1)
    }
}

/// CSS Easing 1: the output for `t`, with the bezier's x solved by halving.
fn ease(easing: Easing, t: f32) -> f32 {
    match easing {
        Easing::Linear => t,
        Easing::Steps(n, jump_start) => {
            let n = n.max(1) as f32;
            let done = if jump_start {
                (t * n).ceil()
            } else {
                (t * n).floor()
            };
            (done / n).clamp(0.0, 1.0)
        }
        Easing::CubicBezier(x1, y1, x2, y2) => {
            let along = |a: f32, b: f32, s: f32| {
                let r = 1.0 - s;
                3.0 * r * r * s * a + 3.0 * r * s * s * b + s * s * s
            };
            let (mut low, mut high) = (0.0_f32, 1.0_f32);
            for _ in 0..30 {
                let middle = f32::midpoint(low, high);
                if along(x1, x2, middle) < t {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            along(y1, y2, f32::midpoint(low, high))
        }
    }
}

/// `easing.*` for `easing` run forwards or, for a stretch played backwards, in
/// reverse. Stairs are a polyline with near-upright risers: Qt has no steps,
/// and a `Behavior` can only ask an animation for a curve.
pub(super) fn set_easing(anim: &mut Object, easing: Easing, reversed: bool) {
    let curve = match easing {
        Easing::Linear => return,
        Easing::CubicBezier(x1, y1, x2, y2) if reversed => {
            vec![1.0 - x2, 1.0 - y2, 1.0 - x1, 1.0 - y1, 1.0, 1.0]
        }
        Easing::CubicBezier(x1, y1, x2, y2) => vec![x1, y1, x2, y2, 1.0, 1.0],
        Easing::Steps(n, jump_start) => stairs(n.max(1), jump_start, reversed),
    };
    let curve: Vec<String> = curve
        .iter()
        .map(|v| fmt_number((f64::from(*v) * 10000.0).round() / 10000.0))
        .collect();
    anim.set("easing.type", Value::expr("Easing.BezierSpline"));
    anim.set(
        "easing.bezierCurve",
        Value::expr(format!("[{}]", curve.join(", "))),
    );
}

fn stairs(n: u32, jump_start: bool, reversed: bool) -> Vec<f32> {
    const RISER: f32 = 0.0005;
    let mut points = vec![(0.0_f32, 0.0_f32)];
    let n_f = n as f32;
    for k in 0..n {
        let x = k as f32 / n_f;
        if jump_start {
            points.push((x + RISER, (k + 1) as f32 / n_f));
            points.push((x + 1.0 / n_f, (k + 1) as f32 / n_f));
        } else {
            points.push((x + 1.0 / n_f - RISER, k as f32 / n_f));
            points.push((x + 1.0 / n_f, (k + 1) as f32 / n_f));
        }
    }
    if reversed {
        points = points
            .iter()
            .rev()
            .map(|p| (1.0 - p.0, 1.0 - p.1))
            .collect();
    }
    // Straight lines as cubic segments, the only form the curve takes.
    let mut curve = Vec::new();
    for pair in points.windows(2) {
        let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
        curve.extend([
            ax + (bx - ax) / 3.0,
            ay + (by - ay) / 3.0,
            ax + (bx - ax) * 2.0 / 3.0,
            ay + (by - ay) * 2.0 / 3.0,
            bx,
            by,
        ]);
    }
    curve
}

/// Declarations this emitter moves; the rest get a diagnostic.
const MOVED: [&str; 12] = [
    "opacity",
    "background",
    "background-color",
    "color",
    "width",
    "height",
    "transform",
    "left",
    "margin-left",
    "margin-right",
    "margin-top",
    "margin-bottom",
];

fn px(l: &Length) -> Option<f32> {
    l.as_px()
}

fn auto_px(l: &LengthAuto) -> Option<f32> {
    match l {
        LengthAuto::Length(l) => px(l),
        LengthAuto::Auto => None,
    }
}

fn width_of(s: &ComputedStyle) -> Option<f32> {
    let Size::Length(l) = &s.width else {
        return None;
    };
    let extra = if s.box_sizing == BoxSizing::BorderBox {
        0.0
    } else {
        px(&s.padding.left)? + px(&s.padding.right)? + s.border.left.width + s.border.right.width
    };
    Some(px(l)? + extra)
}

fn height_of(s: &ComputedStyle) -> Option<f32> {
    let Size::Length(l) = &s.height else {
        return None;
    };
    let extra = if s.box_sizing == BoxSizing::BorderBox {
        0.0
    } else {
        px(&s.padding.top)? + px(&s.padding.bottom)? + s.border.top.width + s.border.bottom.width
    };
    Some(px(l)? + extra)
}

/// What a declared transform comes to, one number per moving part.
#[derive(Clone, PartialEq)]
enum Part {
    Translate {
        x: f32,
        y: f32,
        percent_x: f32,
        percent_y: f32,
    },
    Scale(f32, f32),
    Rotate(f32),
}

impl Part {
    fn identity_like(&self) -> Self {
        match self {
            Self::Translate { .. } => Self::Translate {
                x: 0.0,
                y: 0.0,
                percent_x: 0.0,
                percent_y: 0.0,
            },
            Self::Scale(..) => Self::Scale(1.0, 1.0),
            Self::Rotate(_) => Self::Rotate(0.0),
        }
    }

    fn same_kind(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

fn parts(list: &[TransformFn]) -> Option<Vec<Part>> {
    list.iter()
        .map(|f| match f {
            TransformFn::Translate(x, y)
                if x.vw == 0.0 && x.vh == 0.0 && y.vw == 0.0 && y.vh == 0.0 =>
            {
                Some(Part::Translate {
                    x: x.px,
                    y: y.px,
                    percent_x: x.percent,
                    percent_y: y.percent,
                })
            }
            TransformFn::Scale(x, y) => Some(Part::Scale(*x, *y)),
            TransformFn::Rotate(a) => Some(Part::Rotate(*a)),
            _ => None,
        })
        .collect()
}

/// Whichever stops declare a property, with its timing, and 0% and 100% filled
/// from the box's own value (CSS Animations 1 §3.2).
fn stops_of(
    frames: &[Keyframe],
    declares: &dyn Fn(&Keyframe) -> bool,
    timing: Easing,
    mut value: impl FnMut(&Keyframe) -> Option<Sample>,
    base: Option<Sample>,
) -> Option<Vec<(f32, Sample, Easing)>> {
    let mut stops = Vec::new();
    for frame in frames.iter().filter(|f| declares(f)) {
        stops.push((frame.offset, value(frame)?, frame.easing.unwrap_or(timing)));
    }
    if stops.first().is_none_or(|s| s.0 > 0.0) {
        stops.insert(0, (0.0, base?, timing));
    }
    if stops.last().is_none_or(|s| s.0 < 1.0) {
        stops.push((1.0, base?, timing));
    }
    Some(stops)
}

fn find<'o>(obj: &'o mut Object, id: &str) -> Option<&'o mut Object> {
    if obj.id.as_deref() == Some(id) {
        return Some(obj);
    }
    for m in &mut obj.members {
        match m {
            Member::Child(c) => {
                if let Some(found) = find(c, id) {
                    return Some(found);
                }
            }
            Member::Property { value, .. } => {
                if let Some(found) = find_in_value(value, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_in_value<'o>(value: &'o mut Value, id: &str) -> Option<&'o mut Object> {
    match value {
        Value::Object(o) => find(o, id),
        Value::List(items) => items.iter_mut().find_map(|v| find_in_value(v, id)),
        _ => None,
    }
}

/// Every `Text` in `obj` drawn in `base`, the box's own colour: a child with
/// its own colour keeps it.
fn inked_texts<'o>(obj: &'o mut Object, base: &Value, out: &mut Vec<&'o mut Object>) {
    if obj.type_name == "Text" {
        if obj.get("color") == Some(base) {
            out.push(obj);
        }
        return;
    }
    for m in &mut obj.members {
        if let Member::Child(c) = m {
            inked_texts(c, base, out);
        }
    }
}

fn is_layout(obj: &Object) -> bool {
    matches!(
        obj.type_name.as_str(),
        "ColumnLayout" | "RowLayout" | "GridLayout"
    )
}

/// A number the binding `name` can add to or fall back from.
fn source(value: Option<&Value>, none: &str) -> String {
    match value {
        Some(Value::Number(n)) => fmt_number(*n),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Expr(e)) => format!("({e})"),
        _ => none.to_owned(),
    }
}

fn declare_real(obj: &mut Object, name: &str, value: f32) {
    obj.members.insert(
        0,
        Member::PropertyDecl {
            ty: "real".to_owned(),
            name: name.to_owned(),
            value: Some(number(value)),
            readonly: false,
            required: false,
        },
    );
}

const WIDTH: &str = "eldurMotionWidth";
const HEIGHT: &str = "eldurMotionHeight";

fn declares(obj: &Object, property: &str) -> bool {
    obj.members
        .iter()
        .any(|m| matches!(m, Member::PropertyDecl { name, .. } if name == property))
}

/// What moves the layout binds last: a box's sizes and margins are set by whoever
/// places it, after its own emission, and the carry rewrites its top margin.
pub(super) fn bind_layout_motion(obj: &mut Object) {
    if declares(obj, WIDTH) {
        bind_size(obj, WIDTH, "width", "Layout.preferredWidth");
        if obj.get("width").is_none() {
            // A fixed width in a column overflows it, not widens it (flow.rs).
            let fill = source(obj.get("Layout.fillWidth"), "false");
            obj.set(
                "Layout.fillWidth",
                Value::expr(format!("{WIDTH} >= 0 ? false : {fill}")),
            );
            let margin = source(obj.get("Layout.rightMargin"), "0");
            obj.set(
                "Layout.rightMargin",
                Value::expr(format!(
                    "{WIDTH} >= 0 && parent instanceof ColumnLayout ? -{WIDTH} : {margin}"
                )),
            );
        }
    }
    if declares(obj, HEIGHT) {
        bind_size(obj, HEIGHT, "height", "Layout.preferredHeight");
    }
    for (side, capital) in [
        ("left", "Left"),
        ("right", "Right"),
        ("top", "Top"),
        ("bottom", "Bottom"),
    ] {
        let property = format!("eldurMotionMargin{capital}");
        if declares(obj, &property) {
            let layout = format!("Layout.{side}Margin");
            let orig = source(obj.get(&layout), "0");
            obj.set(&layout, Value::expr(format!("{orig} + {property}")));
        }
    }
    for child in obj.children_mut() {
        bind_layout_motion(child);
    }
}

fn bind_size(obj: &mut Object, property: &str, plain: &str, preferred: &str) {
    let (name, none) = if obj.get(plain).is_some() {
        (plain, "0")
    } else {
        (preferred, "-1")
    };
    let orig = source(obj.get(name), none);
    obj.set(
        name,
        Value::expr(format!("{property} >= 0 ? {property} : {orig}")),
    );
}

/// `Object`s of an animation run, by what is needed of it.
struct Run {
    duration: f32,
    reversed: bool,
}

fn number_or_color(track: &Track, from: Sample, to: Sample, ms: f32) -> Object {
    let kind = match from {
        Sample::Number(_) => "NumberAnimation",
        Sample::Color(_) => "ColorAnimation",
    };
    let mut anim = Object::new(kind);
    anim.set("target", Value::expr(track.target.clone()));
    anim.set("property", Value::string(track.property.clone()));
    anim.set("from", from.value());
    anim.set("to", to.value());
    anim.set("duration", Value::Number(f64::from(ms.round())));
    anim
}

/// One iteration of `track`, as a run of segments.
fn iteration(track: &Track, run: &Run) -> Object {
    let mut seq = Object::new("SequentialAnimation");
    let pairs: Vec<_> = track.stops.windows(2).collect();
    let mut ordered = pairs;
    if run.reversed {
        ordered.reverse();
    }
    for pair in ordered {
        let (from, to) = (&pair[0], &pair[1]);
        let ms = (to.0 - from.0) * run.duration;
        if ms <= 0.0 {
            continue;
        }
        let mut anim = if run.reversed {
            number_or_color(track, to.1, from.1, ms)
        } else {
            number_or_color(track, from.1, to.1, ms)
        };
        set_easing(&mut anim, from.2, run.reversed);
        seq.push_child(anim);
    }
    seq
}

fn action(track: &Track, value: Sample) -> Object {
    let mut a = Object::new("PropertyAction");
    a.set("target", Value::expr(track.target.clone()));
    a.set("property", Value::string(track.property.clone()));
    a.set("value", value.value());
    a
}

impl Emitter<'_, '_> {
    /// Whether a static box animates something, and so has to exist even empty.
    pub(super) fn static_animation(&self, style: &ComputedStyle) -> bool {
        self.options.animations
            && !self.open_elements
            && style.animations.iter().any(|a| {
                a.duration_ms > 0.0
                    && self.styles.keyframes.get(&a.name).is_some_and(|frames| {
                        frames
                            .iter()
                            .any(|f| f.declarations.iter().any(|d| MOVED.contains(&d.0.as_str())))
                    })
            })
    }

    /// Puts `node`'s animations on `obj`. Returns whether one of them took over
    /// its `transform`, which then is not the style's to set.
    pub(super) fn keyframes(
        &mut self,
        obj: &mut Object,
        node: NodeId,
        style: &ComputedStyle,
        hint: &str,
        what: &str,
        line: Option<u32>,
    ) -> bool {
        if !self.static_animation(style) {
            return false;
        }
        let mut took_transform = false;
        for animation in &style.animations {
            let Some(frames) = self.styles.keyframes.get(&animation.name).cloned() else {
                continue;
            };
            let tracks = self.tracks(
                obj,
                node,
                style,
                animation,
                &frames,
                hint,
                what,
                line,
                &mut took_transform,
            );
            if !tracks.is_empty() {
                self.play(obj, &tracks, animation, what, line);
            }
        }
        took_transform
    }

    #[allow(clippy::too_many_arguments)]
    fn tracks(
        &mut self,
        obj: &mut Object,
        node: NodeId,
        style: &ComputedStyle,
        animation: &Animation,
        frames: &[Keyframe],
        hint: &str,
        what: &str,
        line: Option<u32>,
        took_transform: &mut bool,
    ) -> Vec<Track> {
        let mut styled: Vec<(Keyframe, ComputedStyle)> = Vec::new();
        for frame in frames {
            if let Some(s) = self.styles.keyframe_style(node, frame) {
                styled.push((frame.clone(), s));
            }
        }
        let by_offset = |f: &Keyframe| {
            styled
                .iter()
                .find(|(k, _)| {
                    k.offset.to_bits() == f.offset.to_bits() && k.declarations == f.declarations
                })
                .map(|(_, s)| s)
        };
        let names: Vec<String> = {
            let mut seen: Vec<String> = Vec::new();
            for f in frames {
                for (name, _) in &f.declarations {
                    if name != "animation-timing-function" && !seen.contains(name) {
                        seen.push(name.clone());
                    }
                }
            }
            seen
        };
        let timing = animation.easing;
        let mut tracks = Vec::new();
        for name in &names {
            let declares = |f: &Keyframe| f.declarations.iter().any(|d| &d.0 == name);
            let simple = |get: &dyn Fn(&ComputedStyle) -> Option<Sample>| {
                stops_of(
                    frames,
                    &declares,
                    timing,
                    |f| by_offset(f).and_then(get),
                    get(style),
                )
            };
            match name.as_str() {
                "opacity" => {
                    if let Some(stops) = simple(&|s| Some(Sample::Number(s.opacity))) {
                        let target = self.own_id(obj, hint);
                        tracks.push(Track {
                            target,
                            property: "opacity".to_owned(),
                            base: Sample::Number(style.opacity),
                            stops,
                        });
                    }
                }
                "background" | "background-color" => {
                    if let Some(stops) = simple(&|s| Some(Sample::Color(s.background_color))) {
                        if let Some(target) = self.fill_of(obj, style, hint) {
                            tracks.push(Track {
                                target,
                                property: "color".to_owned(),
                                base: Sample::Color(style.background_color),
                                stops,
                            });
                        } else {
                            self.diags.info(
                                "css",
                                line,
                                format!("the animated background of {what} stays still: its box has no fill of its own"),
                            );
                        }
                    }
                }
                "color" => {
                    if let Some(stops) = simple(&|s| Some(Sample::Color(s.color))) {
                        let base_value = color(style.color);
                        let mut texts = Vec::new();
                        inked_texts(obj, &base_value, &mut texts);
                        for text in texts {
                            let target = match &text.id {
                                Some(id) => id.clone(),
                                None => {
                                    let id = self.ids.claim(&format!("{hint} ink"));
                                    text.id = Some(id.clone());
                                    id
                                }
                            };
                            tracks.push(Track {
                                target,
                                property: "color".to_owned(),
                                base: Sample::Color(style.color),
                                stops: stops.clone(),
                            });
                        }
                    }
                }
                "width" | "height" => {
                    let wide = name == "width";
                    let get = move |s: &ComputedStyle| {
                        if wide { width_of(s) } else { height_of(s) }.map(Sample::Number)
                    };
                    let Some(stops) = stops_of(
                        frames,
                        &declares,
                        timing,
                        |f| by_offset(f).and_then(get),
                        // Auto: the box's own size is nothing to run to, so a stop
                        // left out cannot be filled.
                        get(style),
                    ) else {
                        self.diags.info(
                            "css",
                            line,
                            format!(
                                "{name} of {what} does not animate: a stop is not a pixel length"
                            ),
                        );
                        continue;
                    };
                    let target = self.own_id(obj, hint);
                    let property = self.size_track(obj, wide);
                    tracks.push(Track {
                        target,
                        property,
                        // Below zero: the box's own sizing.
                        base: Sample::Number(-1.0),
                        stops,
                    });
                }
                "left" => {
                    let moves = matches!(
                        style.position,
                        Position::Relative | Position::Absolute | Position::Fixed
                    );
                    if !moves {
                        continue;
                    }
                    let own = if style.position == Position::Relative {
                        Some(auto_px(&style.left).unwrap_or(0.0))
                    } else {
                        auto_px(&style.left)
                    };
                    let Some(own) = own else {
                        self.diags.info(
                            "css",
                            line,
                            format!(
                                "left of {what} does not animate: its own is not a pixel length"
                            ),
                        );
                        continue;
                    };
                    let get = |s: &ComputedStyle| auto_px(&s.left).map(|v| Sample::Number(v - own));
                    if let Some(stops) = stops_of(
                        frames,
                        &declares,
                        timing,
                        |f| by_offset(f).and_then(get),
                        Some(Sample::Number(0.0)),
                    ) {
                        let target = self.ids.claim(&format!("{hint} left"));
                        let mut t = Object::new("Translate");
                        t.id = Some(target.clone());
                        t.set("x", 0.0);
                        push_transform(obj, t);
                        tracks.push(Track {
                            target,
                            property: "x".to_owned(),
                            base: Sample::Number(0.0),
                            stops,
                        });
                    }
                }
                "margin-left" | "margin-right" | "margin-top" | "margin-bottom" => {
                    let side = name.trim_start_matches("margin-").to_owned();
                    let pick = |s: &ComputedStyle| match side.as_str() {
                        "left" => auto_px(&s.margin.left),
                        "right" => auto_px(&s.margin.right),
                        "top" => auto_px(&s.margin.top),
                        _ => auto_px(&s.margin.bottom),
                    };
                    let own = pick(style).unwrap_or(0.0);
                    let get = |s: &ComputedStyle| pick(s).map(|v| Sample::Number(v - own));
                    if let Some(stops) = stops_of(
                        frames,
                        &declares,
                        timing,
                        |f| by_offset(f).and_then(get),
                        Some(Sample::Number(0.0)),
                    ) {
                        let target = self.own_id(obj, hint);
                        let capital = format!("{}{}", side[..1].to_uppercase(), &side[1..]);
                        let property = format!("eldurMotionMargin{capital}");
                        declare_real(obj, &property, 0.0);
                        tracks.push(Track {
                            target,
                            property,
                            base: Sample::Number(0.0),
                            stops,
                        });
                    }
                }
                "transform" => {
                    let lists: Option<Vec<(Keyframe, Vec<Part>)>> = frames
                        .iter()
                        .filter(|f| declares(f))
                        .map(|f| {
                            let s = by_offset(f)?;
                            Some((f.clone(), parts(&s.transform)?))
                        })
                        .collect();
                    let base = parts(&style.transform);
                    match lists.zip(base).and_then(|(lists, base)| {
                        self.transform_tracks(obj, style, &lists, &base, timing, hint)
                    }) {
                        Some(mut more) => {
                            *took_transform = true;
                            tracks.append(&mut more);
                        }
                        None => self.diags.info(
                            "css",
                            line,
                            format!(
                                "the transform of {what} does not animate: its stops are not the same translate, scale and rotate functions"
                            ),
                        ),
                    }
                }
                other => self.diags.info(
                    "css",
                    line,
                    format!("`{other}` of {what} does not animate on a page without a runtime"),
                ),
            }
        }
        tracks
    }

    fn own_id(&mut self, obj: &mut Object, hint: &str) -> String {
        match &obj.id {
            Some(id) => id.clone(),
            None => {
                let id = self.ids.claim(&format!("{hint} box"));
                obj.id = Some(id.clone());
                id
            }
        }
    }

    /// The object painting the box's background; one is added under an item that
    /// has none. A Layout takes no such child: anything in it is laid out.
    fn fill_of(&mut self, obj: &mut Object, style: &ComputedStyle, hint: &str) -> Option<String> {
        if obj.type_name == "Rectangle" {
            return Some(self.own_id(obj, hint));
        }
        if is_layout(obj) {
            return None;
        }
        let id = self.ids.claim(&format!("{hint} fill"));
        let mut fill = Object::new("Rectangle");
        fill.id = Some(id.clone());
        fill.set("anchors.fill", Value::expr("parent"));
        fill.set("z", -1.0);
        fill.set("color", color(style.background_color));
        obj.push_child(fill);
        Some(id)
    }

    /// Size animates through a property of its own, for the box's own sizing to
    /// come back when the animation lets go of it (below zero).
    fn size_track(&mut self, obj: &mut Object, wide: bool) -> String {
        let property = if wide { WIDTH } else { HEIGHT };
        if !declares(obj, property) {
            declare_real(obj, property, -1.0);
        }
        property.to_owned()
    }

    fn transform_tracks(
        &mut self,
        obj: &mut Object,
        style: &ComputedStyle,
        lists: &[(Keyframe, Vec<Part>)],
        base: &[Part],
        timing: Easing,
        hint: &str,
    ) -> Option<Vec<Track>> {
        // `none` runs to the identity of whatever the other stops are.
        let shape = lists
            .iter()
            .map(|(_, l)| l.as_slice())
            .chain([base])
            .find(|l| !l.is_empty())?
            .to_vec();
        let fitted = |l: &[Part]| -> Option<Vec<Part>> {
            if l.is_empty() {
                return Some(shape.iter().map(Part::identity_like).collect());
            }
            (l.len() == shape.len() && l.iter().zip(&shape).all(|(a, b)| a.same_kind(b)))
                .then(|| l.to_vec())
        };
        let lists: Vec<(Keyframe, Vec<Part>)> = lists
            .iter()
            .map(|(f, l)| Some((f.clone(), fitted(l)?)))
            .collect::<Option<_>>()?;
        let base = fitted(base)?;
        let box_id = self.own_id(obj, hint);
        let (ox, oy) = style.transform_origin;
        let origin = |l: eldur_css::LengthPercentage, axis: &str| match Length::Linear(l).as_px() {
            Some(v) => number(v),
            None => Value::expr(super::units::length_expr(
                &Length::Linear(l),
                &format!("{box_id}.{axis}"),
            )),
        };
        let mut tracks = Vec::new();
        let mut objects = Vec::new();
        for (i, part) in base.iter().enumerate().rev() {
            let id = self.ids.claim(&format!("{hint} move"));
            let mut t;
            let mut channels: Vec<Channel> = Vec::new();
            match part {
                Part::Translate { .. } => {
                    t = Object::new("Translate");
                    channels.push(("eldurPx", |p| tr(p).0));
                    channels.push(("eldurPy", |p| tr(p).1));
                    channels.push(("eldurFx", |p| tr(p).2));
                    channels.push(("eldurFy", |p| tr(p).3));
                    for (name, read) in channels.iter().rev() {
                        declare_real(&mut t, name, read(part));
                    }
                    t.set(
                        "x",
                        Value::expr(format!("eldurPx + {box_id}.width * eldurFx / 100")),
                    );
                    t.set(
                        "y",
                        Value::expr(format!("eldurPy + {box_id}.height * eldurFy / 100")),
                    );
                }
                Part::Scale(x, y) => {
                    t = Object::new("Scale");
                    t.set("xScale", number(*x));
                    t.set("yScale", number(*y));
                    t.set("origin.x", origin(ox, "width"));
                    t.set("origin.y", origin(oy, "height"));
                    channels.push(("xScale", |p| sc(p).0));
                    channels.push(("yScale", |p| sc(p).1));
                }
                Part::Rotate(a) => {
                    t = Object::new("Rotation");
                    t.set("angle", number(*a));
                    t.set("origin.x", origin(ox, "width"));
                    t.set("origin.y", origin(oy, "height"));
                    channels.push(("angle", |p| ro(p)));
                }
            }
            t.id = Some(id.clone());
            for (name, read) in channels {
                let stops = stops_of(
                    &lists.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(),
                    &|_| true,
                    timing,
                    |f| {
                        lists
                            .iter()
                            .find(|(k, _)| k.offset.to_bits() == f.offset.to_bits())
                            .map(|(_, l)| Sample::Number(read(&l[i])))
                    },
                    Some(Sample::Number(read(&base[i]))),
                )?;
                // A channel that never moves is left to its value.
                if stops.iter().all(|s| s.1 == Sample::Number(read(&base[i]))) {
                    continue;
                }
                tracks.push(Track {
                    target: id.clone(),
                    property: name.to_owned(),
                    base: Sample::Number(read(&base[i])),
                    stops,
                });
            }
            objects.push(t);
        }
        for t in objects {
            push_transform(obj, t);
        }
        Some(tracks)
    }

    /// Paused, the values of the moment; running, an animation.
    fn play(
        &mut self,
        obj: &mut Object,
        tracks: &[Track],
        animation: &Animation,
        what: &str,
        line: Option<u32>,
    ) {
        if animation.paused {
            for track in tracks {
                let value = paused_at(track, animation);
                if let Some(owner) = find(obj, &track.target) {
                    set_or_declared(owner, &track.property, value.value());
                }
            }
            return;
        }
        if animation.delay_ms < 0.0 {
            self.diags.info(
                "css",
                line,
                format!(
                    "{what} starts its animation `{}` from the beginning: QML cannot start one part way through",
                    animation.name
                ),
            );
        }
        let count = if animation.iteration_count.is_infinite() {
            None
        } else {
            if animation.iteration_count.fract() != 0.0 {
                self.diags.info(
                    "css",
                    line,
                    format!(
                        "{what} plays `{}` whole iterations only: QML has no fraction of a loop",
                        animation.name
                    ),
                );
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some((animation.iteration_count.floor() as u32).max(1))
        };
        let mut all = Object::new("ParallelAnimation");
        for track in tracks {
            all.push_child(looped(track, animation, count));
        }
        let mut outer = Object::new("SequentialAnimation");
        outer.set("running", true);
        if animation.fill_backwards {
            for track in tracks {
                outer.push_child(action(track, track.first(animation.reverse)));
            }
        }
        if animation.delay_ms > 0.0 {
            let mut pause = Object::new("PauseAnimation");
            pause.set(
                "duration",
                Value::Number(f64::from(animation.delay_ms.round())),
            );
            outer.push_child(pause);
        }
        outer.push_child(all);
        if !animation.fill_forwards {
            for track in tracks {
                outer.push_child(action(track, track.base));
            }
        }
        obj.push_child(outer);
    }
}

/// A property of a transform object, and what it is at a stop.
type Channel = (&'static str, fn(&Part) -> f32);

fn tr(p: &Part) -> (f32, f32, f32, f32) {
    match p {
        Part::Translate {
            x,
            y,
            percent_x,
            percent_y,
        } => (*x, *y, *percent_x, *percent_y),
        _ => (0.0, 0.0, 0.0, 0.0),
    }
}

fn sc(p: &Part) -> (f32, f32) {
    match p {
        Part::Scale(x, y) => (*x, *y),
        _ => (1.0, 1.0),
    }
}

fn ro(p: &Part) -> f32 {
    match p {
        Part::Rotate(a) => *a,
        _ => 0.0,
    }
}

fn push_transform(obj: &mut Object, t: Object) {
    let mut values = match obj.get("transform") {
        Some(Value::List(old)) => old.clone(),
        Some(old) => vec![old.clone()],
        None => Vec::new(),
    };
    values.push(Value::Object(Box::new(t)));
    obj.set("transform", Value::List(values));
}

fn set_or_declared(owner: &mut Object, property: &str, value: Value) {
    for m in &mut owner.members {
        if let Member::PropertyDecl { name, value: v, .. } = m
            && name == property
        {
            *v = Some(value);
            return;
        }
    }
    owner.set(property, value);
}

/// CSS Animations 1 §4 at `-delay`, which is the only time a paused animation has.
fn paused_at(track: &Track, a: &Animation) -> Sample {
    let t = -a.delay_ms;
    let total = a.duration_ms * a.iteration_count;
    let forward_at = |iteration: f32, fraction: f32| {
        let flipped = a.reverse != (a.alternate && iteration % 2.0 >= 1.0);
        if flipped { 1.0 - fraction } else { fraction }
    };
    if t < 0.0 {
        if a.fill_backwards {
            return track.at(forward_at(0.0, 0.0));
        }
        return track.base;
    }
    if t >= total {
        if !a.fill_forwards {
            return track.base;
        }
        let fraction = a.iteration_count.fract();
        return if fraction > 0.0 {
            track.at(forward_at(a.iteration_count.floor(), fraction))
        } else {
            track.at(forward_at(a.iteration_count - 1.0, 1.0))
        };
    }
    let iteration = (t / a.duration_ms).floor();
    track.at(forward_at(iteration, t / a.duration_ms - iteration))
}

/// All of `track`'s iterations: an odd one out of an alternating run plays after
/// the pairs, since a loop can only repeat a whole.
fn looped(track: &Track, a: &Animation, count: Option<u32>) -> Object {
    let once = |reversed: bool| {
        iteration(
            track,
            &Run {
                duration: a.duration_ms,
                reversed,
            },
        )
    };
    let loops = |mut o: Object, n: Option<u32>| {
        o.set(
            "loops",
            n.map_or(Value::expr("Animation.Infinite"), |n| {
                Value::Number(f64::from(n))
            }),
        );
        o
    };
    if !a.alternate {
        return loops(once(a.reverse), count);
    }
    let mut pair = Object::new("SequentialAnimation");
    pair.push_child(once(a.reverse));
    pair.push_child(once(!a.reverse));
    let Some(n) = count else {
        return loops(pair, None);
    };
    let mut all = Object::new("SequentialAnimation");
    if n >= 2 {
        all.push_child(loops(pair, Some(n / 2)));
    }
    if n % 2 == 1 {
        all.push_child(once(a.reverse));
    }
    all
}
