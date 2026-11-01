//! What the converter knows about fonts, from the table in `eldur_fonts.js`.
//! The running page decides which font a stack reaches and what its line is
//! (layout.md §1.3); here the same table gives the estimate the converter
//! lays out with, assuming each name's first choice, and never asks the machine.

use std::collections::HashMap;
use std::sync::OnceLock;

use eldur_css::{ComputedStyle, FontStyle};
use eldur_qml::Value;
use serde::Deserialize;

use super::units::fmt_number;

/// A CSS length in a literal, without single precision's noise.
fn literal(v: f32) -> f64 {
    (f64::from(v) * 10000.0).round() / 10000.0
}

/// A face's hhea ascent, descent and line gap, in em.
type Extents = [f32; 3];

#[derive(Deserialize)]
struct Face {
    regular: Extents,
    bold: Option<Extents>,
    italic: Option<Extents>,
    bolditalic: Option<Extents>,
}

#[derive(Deserialize)]
struct Table {
    families: HashMap<String, Vec<String>>,
    default: Vec<String>,
    faces: HashMap<String, Face>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let json = eldur_js::FONTS_JS
            .split("/*TABLE*/")
            .nth(1)
            .expect("eldur_fonts.js marks its table");
        serde_json::from_str(json).expect("eldur_fonts.js's table is JSON")
    })
}

/// The CSS stack as the page's JavaScript gets it, in a string literal.
pub(super) fn stack(style: &ComputedStyle) -> String {
    let joined = style.font_family.join(", ");
    serde_json::to_string(&joined).expect("a string serializes")
}

/// The binding that resolves `style`'s stack on the running machine.
pub(super) fn family_value(style: &ComputedStyle) -> Value {
    Value::expr(format!("EldurFonts.family({})", stack(style)))
}

/// The CSS size, written as the running page gets it.
fn size(style: &ComputedStyle) -> String {
    fmt_number(literal(style.font_size))
}

/// The whole-pixel size stands in for a whole CSS size; a fractional one is
/// passed along, since Chromium rounds a font's metrics at it.
fn size_argument(style: &ComputedStyle) -> String {
    if style.font_size.fract() == 0.0 {
        String::new()
    } else {
        format!(", {}", size(style))
    }
}

/// `line-height: normal`, for a binding on an item with this `font`.
pub(super) fn normal_line_value(style: &ComputedStyle) -> Value {
    Value::expr(format!("{NORMAL_LINE}font{})", size_argument(style)))
}

/// How a bound normal line starts: it is a whole number of pixels.
pub(super) const NORMAL_LINE: &str = "EldurFonts.normalLine(";

/// What moves a fixed line's glyphs to Chromium's baseline; `line` 0 is `normal`.
pub(super) fn shift_expr(style: &ComputedStyle, line: Option<f32>) -> String {
    format!(
        "EldurFonts.shift({}, {}, {}, {}, {})",
        stack(style),
        size(style),
        style.font_weight,
        style.font_style != FontStyle::Normal,
        fmt_number(literal(line.unwrap_or(0.0)))
    )
}

/// The fractional size for an item that takes a `font` and works out lines.
pub(super) fn css_size(style: &ComputedStyle) -> Option<f64> {
    (style.font_size.fract() != 0.0).then(|| literal(style.font_size))
}

/// The table's first choice for a name in the stack, or the name of a face it
/// has measured; other names are taken to be absent, as most are.
fn known(name: &str) -> Option<&'static str> {
    let table = table();
    let lower = name.to_lowercase();
    if let Some(candidates) = table.families.get(&lower) {
        return candidates.first().map(String::as_str);
    }
    table
        .faces
        .get_key_value(&lower)
        .map(|(key, _)| key.as_str())
}

/// The face the stack is expected to reach: what two runs are compared by, and
/// what estimates use.
pub(crate) fn expected_face(families: &[String]) -> String {
    families
        .iter()
        .find_map(|name| known(name))
        .unwrap_or_else(|| table().default[0].as_str())
        .to_lowercase()
}

/// The stack in RichText's CSS, which does keep a list: the table's choices in
/// order, then the names as written.
pub(super) fn css_families(families: &[String]) -> String {
    let table = table();
    let mut names: Vec<&str> = Vec::new();
    for name in families {
        match table.families.get(&name.to_lowercase()) {
            Some(candidates) => names.extend(candidates.iter().map(String::as_str)),
            None => names.push(name),
        }
    }
    if names.is_empty() {
        names.extend(table.default.iter().map(String::as_str));
    }
    let quoted: Vec<String> = names
        .iter()
        .map(|n| format!("'{}'", n.replace(['\'', '"'], "")))
        .collect();
    quoted.join(",")
}

/// Ascent, descent and gap of the expected face, in em.
fn fractions(style: &ComputedStyle) -> Extents {
    let face_name = expected_face(&style.font_family);
    let bold = style.font_weight >= 600;
    let italic = style.font_style != FontStyle::Normal;
    // A stack the table cannot place is laid out as Liberation Sans until it runs.
    let face = table()
        .faces
        .get(&face_name)
        .or_else(|| table().faces.get("liberation sans"))
        .expect("the table has Liberation Sans");
    match (bold, italic) {
        (true, true) => face.bolditalic,
        (true, false) => face.bold,
        (false, true) => face.italic,
        (false, false) => None,
    }
    .unwrap_or(face.regular)
}

/// Unrounded ascent and descent at `size`, as Qt's whole-pixel font has them.
pub(super) fn qt_extents(style: &ComputedStyle, size: f32) -> (f32, f32) {
    let [a, d, _] = fractions(style);
    (a * size, d * size)
}

/// Rounded ascent, descent and gap, by Chromium's rule, of the expected face.
pub(super) fn extents(style: &ComputedStyle) -> (f32, f32, f32) {
    let [a, d, g] = fractions(style);
    let s = style.font_size;
    ((a * s).round(), (d * s).round(), (g * s).round())
}

/// The estimate of `line-height: normal`.
pub(super) fn normal_line(style: &ComputedStyle) -> f32 {
    let (a, d, g) = extents(style);
    a + d + g
}
