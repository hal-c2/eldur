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
use crate::emit::shadow::{bands, outer_bands};
use crate::emit::transform::flat;

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
            let rings = if s.inset {
                bands(s.blur, alpha, 1.0, 24)
            } else {
                outer_bands(s.blur, alpha)
            };
            let bands: Vec<[f32; 2]> = rings.iter().map(|b| [b.depth, b.alpha]).collect();
            let opaque = Rgba { a: 255, ..s.color };
            json!({
                "x": s.offset_x, "y": s.offset_y, "spread": s.spread, "inset": s.inset,
                "c": qml_color(opaque), "bands": bands, "blur": s.blur, "a": alpha,
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
    let mut list: Vec<Json> = Vec::new();
    let mut depth: Option<[[f32; 4]; 4]> = None;
    let settle = |list: &mut Vec<Json>, depth: &mut Option<[[f32; 4]; 4]>| {
        if let Some(m) = depth.take() {
            list.push(json!({"m": flat::plane(&m)}));
        }
    };
    for f in fns {
        if let TransformFn::Unsupported(text) = f
            && let Some(m) = flat::matrix(text)
        {
            depth = Some(flat::multiply(&depth.unwrap_or(flat::IDENTITY), &m));
            continue;
        }
        settle(&mut list, &mut depth);
        list.push(match f {
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
        });
    }
    settle(&mut list, &mut depth);
    json!({"fns": list})
}

/// A `transform` in ems: the font is known only at run time, so a translation
/// carries what one em adds. Reading the value at 0 and at 1 px per em separates
/// that from its pixels, whatever the mix.
fn transform_in_ems(text: &str, viewport: &Viewport) -> Option<Json> {
    let at =
        |px: f32| match eldur_css::parse_paint_value("transform", &ems_as_px(text, px)?, viewport)?
        {
            PaintValue::Transform(fns) => Some(fns),
            _ => None,
        };
    let (zero, one) = (at(0.0)?, at(1.0)?);
    let size = (viewport.width, viewport.height);
    let mut value = transform_json(&zero, size);
    let mut slots = value["fns"].as_array_mut()?.iter_mut();
    let mut in_depth = false;
    for (z, o) in zero.iter().zip(&one) {
        if let TransformFn::Unsupported(text) = z
            && flat::matrix(text).is_some()
        {
            // Neighbouring 3-D functions are one entry, and a depth does not
            // show in the plane's picture, so its ems change nothing.
            if !in_depth {
                slots.next()?;
            }
            in_depth = true;
            continue;
        }
        in_depth = false;
        let slot = slots.next()?;
        match (z, o) {
            (TransformFn::Translate(zx, zy), TransformFn::Translate(ox, oy)) => {
                let x = lp_json(zx, size);
                let y = lp_json(zy, size);
                slot["t"] = json!([[x[0], x[1], ox.px - zx.px], [y[0], y[1], oy.px - zy.px]]);
            }
            _ if z == o => {}
            _ => return None,
        }
    }
    Some(value)
}

/// `text` with each em length written in pixels, or `None` if it has none.
fn ems_as_px(text: &str, px_per_em: f32) -> Option<String> {
    let mut out = String::new();
    let mut any = false;
    let mut rest = text;
    while let Some(at) = rest.find("em") {
        let before = &rest[..at];
        let digits = before
            .rfind(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
            .map_or(0, |i| i + 1);
        let after = rest[at + 2..].chars().next();
        match before[digits..].parse::<f32>() {
            Ok(n) if !after.is_some_and(char::is_alphanumeric) => {
                any = true;
                out.push_str(&before[..digits]);
                out.push_str(&format!("{}px", n * px_per_em));
            }
            _ => {
                out.push_str(before);
                out.push_str("em");
            }
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    any.then_some(out)
}

fn entry(
    property: &str,
    text: &str,
    viewport: &Viewport,
    files: &BTreeMap<String, String>,
) -> Option<Json> {
    if property == "filter" {
        return filter_json(text, viewport);
    }
    if property == "transform"
        && let Some(value) = transform_in_ems(text, viewport)
    {
        return Some(value);
    }
    if property == "box-shadow"
        && let Some(hundreds) = em_as_hundreds(text)
    {
        let PaintValue::Shadows(list) =
            eldur_css::parse_paint_value(property, &hundreds, viewport)?
        else {
            return None;
        };
        let mut value = shadows_json(&list);
        value["em"] = json!(true);
        return Some(value);
    }
    let size = (viewport.width, viewport.height);
    match eldur_css::parse_paint_value(property, text, viewport)? {
        PaintValue::Images(layers) => Some(images_json(&layers, text, size, files)),
        PaintValue::Shadows(list) => Some(shadows_json(&list)),
        PaintValue::Transform(fns) => Some(transform_json(&fns, size)),
    }
}

/// A shadow written all in `em` has the same shape at every font size, and a rule
/// is shared by boxes of many: it is kept at 100px to the `em` and scaled where it
/// lands. With `px` lengths too it would not scale, and is left to warn.
fn em_as_hundreds(text: &str) -> Option<String> {
    let mut any = false;
    let mut out = String::new();
    for token in text.split_whitespace() {
        if token.ends_with("px")
            && token
                .trim_end_matches("px")
                .parse::<f64>()
                .is_ok_and(|v| v != 0.0)
        {
            return None;
        }
        match token.strip_suffix("em").and_then(|n| n.parse::<f64>().ok()) {
            Some(n) => {
                any = true;
                out.push_str(&format!("{}px ", n * 100.0));
            }
            None => {
                out.push_str(token);
                out.push(' ');
            }
        }
    }
    any.then_some(out)
}

/// The members of an `EldurFilter`, as QML. `em` in a rule's filter is against
/// the default font size: the value is shared by whatever box it reaches.
fn filter_json(text: &str, viewport: &Viewport) -> Option<Json> {
    let filter = crate::filter::parse(text, 16.0, viewport)?;
    let mut source = String::new();
    let numbers = |values: &mut dyn Iterator<Item = &f64>| {
        values
            .map(|v| format!("{v}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if filter.recolours() {
        source.push_str(&format!(
            "colorMatrix: Qt.matrix4x4({}); colorOffset: Qt.vector4d({}); ",
            numbers(&mut filter.matrix.iter().flatten()),
            numbers(&mut filter.offset.iter()),
        ));
    }
    if filter.blur > 0.0 {
        source.push_str(&format!("blurDeviation: {}; ", filter.blur));
    }
    if let Some(d) = filter.drop {
        let c = d.color;
        source.push_str(&format!(
            "hasDrop: true; dropX: {}; dropY: {}; dropDeviation: {}; dropColor: Qt.rgba({}, {}, {}, {}); ",
            d.x,
            d.y,
            d.deviation,
            f32::from(c.r) / 255.0,
            f32::from(c.g) / 255.0,
            f32::from(c.b) / 255.0,
            f32::from(c.a) / 255.0,
        ));
    }
    Some(serde_json::json!({ "source": source }))
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

/// The rules' `transform` values with a third dimension, which a script's class
/// flattens with no word at run time: pages reach them there, so it is said here.
pub(crate) fn flattened_transforms(rules: &[RuntimeRule]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for rule in rules {
        for (name, value, _) in &rule.declarations {
            let depth = |word: &str| {
                let word = word.trim().to_ascii_lowercase();
                [
                    "rotatex(",
                    "rotatey(",
                    "rotate3d(",
                    "translatez(",
                    "scalez(",
                    "matrix3d(",
                ]
                .iter()
                .any(|f| word.contains(f))
            };
            if name == "transform" && depth(value) {
                out.insert(value.trim().to_owned());
            }
        }
    }
    out
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
                "background" | "background-image" | "box-shadow" | "transform" | "filter"
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

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> Json {
        entry("transform", text, &Viewport::default(), &BTreeMap::new()).expect(text)
    }

    #[test]
    fn a_translation_in_ems_carries_what_one_em_adds() {
        let v = value("translate(1em, .5em)");
        assert_eq!(v["fns"][0]["t"][0][0].as_f64(), Some(0.0));
        assert_eq!(v["fns"][0]["t"][0][2].as_f64(), Some(1.0));
        assert_eq!(v["fns"][0]["t"][1][2].as_f64(), Some(0.5));
    }

    #[test]
    fn neighbouring_depth_is_one_flat_matrix_and_keeps_what_follows_apart() {
        let v = value("rotateY(60deg) translateZ(1em) translate(1em, 2px)");
        let fns = v["fns"].as_array().expect("list");
        assert_eq!(fns.len(), 2, "{fns:?}");
        let a = fns[0]["m"][0].as_f64().expect("a");
        assert!((a - 0.5).abs() < 1e-3, "{a}");
        assert_eq!(fns[1]["t"][0][2].as_f64(), Some(1.0));
        assert_eq!(fns[1]["t"][1][0].as_f64(), Some(2.0));
    }

    #[test]
    fn perspective_stays_a_function_with_no_form() {
        let v = value("perspective(9px) scale(2)");
        assert_eq!(v["fns"][0]["u"].as_str(), Some("perspective(9px)"));
    }

    #[test]
    fn only_an_em_length_counts_as_one() {
        assert_eq!(ems_as_px("translate(1rem, 2px)", 3.0), None);
        assert_eq!(
            ems_as_px("translate(-1.5em, 2px)", 2.0).as_deref(),
            Some("translate(-3px, 2px)")
        );
    }
}
