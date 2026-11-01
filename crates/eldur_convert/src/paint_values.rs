//! `root.eldurPaintValues`: what the page's paint declarations mean, for the
//! runtime to paint created boxes with and to answer `getComputedStyle` from.
//! CSS is parsed here; the runtime only looks a written value up. A value it
//! does not find, one a script composed, it says it did not paint.

use std::collections::{BTreeMap, BTreeSet};

use eldur_css::{
    Image, LengthPercentage, PaintValue, Rgba, RuntimeRule, Shadow, TransformFn, Viewport,
};
use eldur_qml::{Member, Value};
use serde_json::{Value as Json, json};

use crate::emit::background::fixup_stops;
use crate::emit::shadow::bands;

/// How many readings of one value's `var()`s are worth writing down.
const MAX_READINGS: usize = 16;

/// How the runtime finds a written value, whatever its spacing.
fn key(property: &str, text: &str) -> String {
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    format!("{property}:{squeezed}")
}

fn split_commas(text: &str) -> Vec<&str> {
    let (mut depth, mut start) = (0_i32, 0);
    let mut parts = Vec::new();
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

/// The value with each `var()` replaced, once per declared value of that
/// custom property: which of them applies is only known at run time.
fn readings(text: &str, vars: &BTreeMap<&str, BTreeSet<&str>>, depth: u32) -> Vec<String> {
    let Some(at) = text.find("var(") else {
        return vec![text.to_owned()];
    };
    if depth > 4 {
        return Vec::new();
    }
    let mut nesting = 0_i32;
    let mut end = None;
    for (i, c) in text[at + 3..].char_indices() {
        match c {
            '(' => nesting += 1,
            ')' => {
                nesting -= 1;
                if nesting == 0 {
                    end = Some(at + 3 + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else { return Vec::new() };
    let inner = &text[at + 4..end];
    let (name, fallback) = match inner.split_once(',') {
        Some((n, f)) => (n.trim(), Some(f.trim())),
        None => (inner.trim(), None),
    };
    let declared = vars.get(name).into_iter().flatten().copied();
    let mut out = Vec::new();
    for value in declared.chain(fallback) {
        let swapped = format!("{}{}{}", &text[..at], value, &text[end + 1..]);
        out.extend(readings(&swapped, vars, depth + 1));
        if out.len() > MAX_READINGS {
            out.truncate(MAX_READINGS);
            break;
        }
    }
    out
}

fn qml_color(c: Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.a, c.r, c.g, c.b)
}

/// As a browser reads a colour back: two decimals of alpha if they say it.
fn css_color(c: Rgba) -> String {
    if c.a == 255 {
        return format!("rgb({}, {}, {})", c.r, c.g, c.b);
    }
    let a = f64::from(c.a) / 255.0;
    let short = (a * 100.0).round() / 100.0;
    let alpha = if (short * 255.0).round() as u8 == c.a {
        short
    } else {
        (a * 1000.0).round() / 1000.0
    };
    format!("rgba({}, {}, {}, {alpha})", c.r, c.g, c.b)
}

fn css_length(l: &LengthPercentage) -> String {
    if l.percent != 0.0 && l.px == 0.0 {
        format!("{}%", l.percent)
    } else {
        format!("{}px", l.px)
    }
}

fn stops_json(stops: &[eldur_css::ColorStop], viewport: (f32, f32)) -> Json {
    let at = fixup_stops(stops, viewport);
    stops
        .iter()
        .zip(at)
        .map(|(s, p)| json!({"c": qml_color(s.color), "px": p.px, "frac": p.frac}))
        .collect()
}

fn css_stops(stops: &[eldur_css::ColorStop]) -> String {
    stops
        .iter()
        .map(|s| match &s.position {
            Some(p) => format!("{} {}", css_color(s.color), css_length(p)),
            None => css_color(s.color),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What precedes the stops as written; the colour of a first stop is not it.
fn written_head(written: &str) -> Option<String> {
    let open = written.find('(')?;
    let close = written.rfind(')')?;
    let first = *split_commas(&written[open + 1..close]).first()?;
    let lower = first.to_ascii_lowercase();
    let says_shape = lower.starts_with("to ")
        || lower.ends_with("deg")
        || lower.ends_with("turn")
        || lower.ends_with("rad")
        || lower.starts_with("circle")
        || lower.starts_with("ellipse")
        || lower.starts_with("closest")
        || lower.starts_with("farthest")
        || lower.starts_with("at ")
        || lower.starts_with("from ");
    says_shape.then(|| lower.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn image_json(
    image: Option<&Image>,
    written: &str,
    viewport: (f32, f32),
    files: &BTreeMap<String, String>,
) -> (Json, String) {
    let head = written_head(written);
    let with_head = |name: &str, stops: &str, head: Option<String>| match head {
        Some(h) => format!("{name}({h}, {stops})"),
        None => format!("{name}({stops})"),
    };
    let prefix = |repeating: bool, base: &str| {
        if repeating {
            format!("repeating-{base}")
        } else {
            base.to_owned()
        }
    };
    match image {
        None => (json!({"kind": "none"}), "none".to_owned()),
        Some(Image::LinearGradient {
            angle_deg,
            stops,
            repeating,
        }) => {
            // Going down is the default, and a browser leaves the default out.
            let head = head.filter(|h| h != "to bottom");
            let head = head.map(|h| {
                if h.starts_with("to ") {
                    h
                } else {
                    format!("{angle_deg}deg")
                }
            });
            (
                json!({"kind": "linear", "angle": angle_deg, "stops": stops_json(stops, viewport), "repeating": repeating}),
                with_head(
                    &prefix(*repeating, "linear-gradient"),
                    &css_stops(stops),
                    head,
                ),
            )
        }
        Some(Image::LinearGradientToCorner {
            right,
            bottom,
            stops,
            repeating,
        }) => {
            let side = format!(
                "to {} {}",
                if *right { "right" } else { "left" },
                if *bottom { "bottom" } else { "top" }
            );
            (
                json!({"kind": "corner", "right": right, "bottom": bottom, "stops": stops_json(stops, viewport), "repeating": repeating}),
                with_head(
                    &prefix(*repeating, "linear-gradient"),
                    &css_stops(stops),
                    Some(side),
                ),
            )
        }
        Some(Image::RadialGradient {
            circle,
            center_x,
            center_y,
            stops,
            repeating,
        }) => (
            json!({
                "kind": "radial", "circle": circle,
                "cx": [center_x.px, center_x.percent], "cy": [center_y.px, center_y.percent],
                "stops": stops_json(stops, viewport), "repeating": repeating,
            }),
            with_head(
                &prefix(*repeating, "radial-gradient"),
                &css_stops(stops),
                head,
            ),
        ),
        Some(Image::ConicGradient {
            from_deg,
            center_x,
            center_y,
            stops,
        }) => (
            json!({
                "kind": "conic",
                "from": from_deg,
                "cx": lp_json(center_x, viewport),
                "cy": lp_json(center_y, viewport),
                "stops": stops_json(stops, viewport),
            }),
            with_head("conic-gradient", &css_stops(stops), head),
        ),
        Some(Image::Url(url)) => (
            files.get(url).map_or_else(
                || json!({"kind": "unpainted", "what": format!("image `{url}` was not found and")}),
                |source| json!({"kind": "image", "source": source}),
            ),
            format!("url(\"{url}\")"),
        ),
    }
}

fn images_json(
    layers: &[Option<Image>],
    text: &str,
    viewport: (f32, f32),
    files: &BTreeMap<String, String>,
) -> Json {
    let written = split_commas(text);
    let mut list = Vec::new();
    let mut css = Vec::new();
    for (i, layer) in layers.iter().enumerate() {
        let (one, read) = image_json(
            layer.as_ref(),
            written.get(i).unwrap_or(&""),
            viewport,
            files,
        );
        list.push(one);
        css.push(read);
    }
    json!({"layers": list, "css": css.join(", ")})
}

fn shadows_json(shadows: &[Shadow]) -> Json {
    let list: Vec<Json> = shadows
        .iter()
        .map(|s| {
            // Alpha 1 here: each band's own is what is painted over what's below.
            let alpha = f32::from(s.color.a) / 255.0;
            let bands: Vec<[f32; 2]> = bands(s.blur, alpha, 1.0, 24)
                .iter()
                .map(|b| [b.depth, b.alpha])
                .collect();
            let opaque = Rgba { a: 255, ..s.color };
            json!({
                "x": s.offset_x, "y": s.offset_y, "spread": s.spread, "inset": s.inset,
                "c": qml_color(opaque), "bands": bands,
            })
        })
        .collect();
    let css: Vec<String> = shadows
        .iter()
        .map(|s| {
            format!(
                "{} {}px {}px {}px {}px{}",
                css_color(s.color),
                s.offset_x,
                s.offset_y,
                s.blur,
                s.spread,
                if s.inset { " inset" } else { "" }
            )
        })
        .collect();
    json!({"list": list, "css": css.join(", ")})
}

fn lp_json(l: &LengthPercentage, viewport: (f32, f32)) -> [f32; 2] {
    [
        l.px + l.vw * viewport.0 / 100.0 + l.vh * viewport.1 / 100.0,
        l.percent,
    ]
}

fn transform_json(fns: &[TransformFn], viewport: (f32, f32)) -> Json {
    let list: Vec<Json> = fns
        .iter()
        .map(|f| match f {
            TransformFn::Translate(x, y) => {
                json!({"t": [lp_json(x, viewport), lp_json(y, viewport)]})
            }
            TransformFn::TranslateCalc(..) => json!({"u": "translate() of min() or max()"}),
            TransformFn::Scale(x, y) => json!({"m": [x, 0.0, 0.0, y, 0.0, 0.0]}),
            TransformFn::Rotate(deg) => {
                let r = f64::from(*deg).to_radians();
                json!({"m": [r.cos(), r.sin(), -r.sin(), r.cos(), 0.0, 0.0]})
            }
            TransformFn::Skew(ax, ay) => {
                let (b, c) = (ay.to_radians().tan(), ax.to_radians().tan());
                json!({"m": [1.0, b, c, 1.0, 0.0, 0.0]})
            }
            TransformFn::Matrix(m) => json!({"m": m}),
            TransformFn::Unsupported(text) => json!({"u": text}),
        })
        .collect();
    json!({"fns": list})
}

fn entry(
    property: &str,
    text: &str,
    viewport: &Viewport,
    files: &BTreeMap<String, String>,
) -> Option<Json> {
    let size = (viewport.width, viewport.height);
    match eldur_css::parse_paint_value(property, text, viewport)? {
        PaintValue::Images(layers) => Some(images_json(&layers, text, size, files)),
        PaintValue::Shadows(list) => Some(shadows_json(&list)),
        PaintValue::Transform(fns) => Some(transform_json(&fns, size)),
    }
}

/// Every background `url()` the page's rules can give at run time: a script may
/// set a class no element has yet, so these are not all in the computed styles.
pub(crate) fn background_urls(rules: &[RuntimeRule], viewport: &Viewport) -> BTreeSet<String> {
    let mut vars: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for rule in rules {
        for (name, value, _) in &rule.declarations {
            if name.starts_with("--") {
                vars.entry(name).or_default().insert(value.trim());
            }
        }
    }
    let mut urls = BTreeSet::new();
    for rule in rules {
        for (name, value, _) in &rule.declarations {
            if !matches!(name.as_str(), "background" | "background-image") {
                continue;
            }
            for reading in readings(value, &vars, 0) {
                let image = if name == "background" {
                    eldur_css::background_parts(&reading).and_then(|(image, _)| image)
                } else {
                    Some(reading)
                };
                let Some(image) = image else { continue };
                if let Some(PaintValue::Images(layers)) =
                    eldur_css::parse_paint_value("background-image", &image, viewport)
                {
                    urls.extend(layers.into_iter().filter_map(|l| match l {
                        Some(Image::Url(url)) => Some(url),
                        _ => None,
                    }));
                }
            }
        }
    }
    urls
}

/// Every value the page's rules can give a paint property.
pub(crate) fn paint_values_decl(
    rules: &[RuntimeRule],
    viewport: &Viewport,
    files: &BTreeMap<String, String>,
) -> Member {
    let mut vars: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for rule in rules {
        for (name, value, _) in &rule.declarations {
            if name.starts_with("--") {
                vars.entry(name).or_default().insert(value.trim());
            }
        }
    }
    let mut table = serde_json::Map::new();
    let mut add = |property: &str, text: &str| {
        if let Some(found) = entry(property, text, viewport, files) {
            table.insert(key(property, text), found);
        }
    };
    for rule in rules {
        for (name, value, _) in &rule.declarations {
            if !matches!(
                name.as_str(),
                "background" | "background-image" | "box-shadow" | "transform"
            ) {
                continue;
            }
            for reading in readings(value, &vars, 0) {
                if name != "background" {
                    if reading.trim() != "none" {
                        add(name, &reading);
                    }
                    continue;
                }
                let Some((image, _)) = eldur_css::background_parts(&reading) else {
                    continue;
                };
                if let Some(image) = image.filter(|i| i.trim() != "none") {
                    add("background-image", &image);
                }
            }
        }
    }
    // A `background` that held `var()` is split at run time, by this.
    let mut split = serde_json::Map::new();
    for rule in rules {
        for (name, value, _) in &rule.declarations {
            if name != "background" {
                continue;
            }
            for reading in readings(value, &vars, 0) {
                if let Some((image, color)) = eldur_css::background_parts(&reading) {
                    split.insert(
                        key("background", &reading),
                        json!({"image": image, "color": color}),
                    );
                }
            }
        }
    }
    table.extend(split);
    Member::PropertyDecl {
        ty: "string".to_owned(),
        name: "eldurPaintValues".to_owned(),
        value: Some(Value::String(Json::Object(table).to_string())),
        readonly: false,
        required: false,
    }
}
