//! CSS Color 4 & `color-mix()` value parsing.
//!
//! Uses `csscolorparser` for standard CSS Color 4 syntaxes (named colours,
//! 3/4/6/8-digit hex, `rgb`/`rgba`, `hsl`/`hsla`, `hwb`, `lab`, `lch`,
//! `oklab`, `oklch`, `color()`), with AST-level guards so non-CSS extensions
//! in `csscolorparser` (bare hex without `#`, `hsv()`) are rejected. Adds
//! `currentColor`, `transparent`, CSS Color 4 system colours (light scheme),
//! and `color-mix()`.

use swc_css_ast::{ComponentValue, Function, Token};

use crate::Rgba;

use super::cursor::{
    Cursor, MAX_NESTING_DEPTH, is_comma, is_slash, is_ws, serialize_cv, split_comma_separated,
};

/// Parses a complete component-value slice as a single CSS `<color>`.
pub(crate) fn parse_color_cvs(cvs: &[ComponentValue], current_color: Rgba) -> Result<Rgba, String> {
    parse_color_cvs_depth(cvs, current_color, 0)
}

fn parse_color_cvs_depth(
    cvs: &[ComponentValue],
    current_color: Rgba,
    depth: usize,
) -> Result<Rgba, String> {
    let mut cur = Cursor::new(cvs);
    let color = eat_color_depth(&mut cur, current_color, depth)?
        .ok_or_else(|| "expected a <color>".to_owned())?;
    cur.expect_eof()?;
    Ok(color)
}

/// Attempts to consume a single `<color>` component value at `cur`.
///
/// - Returns `Ok(None)` without advancing `cur` if the next token does not
///   begin a colour (for example `solid`, `1px`, `inset`, `none`).
/// - Returns `Err` if the next token *is* a colour token/function (`#...`,
///   `rgb(...)`, `color-mix(...)`, …) whose syntax or arguments are invalid.
pub(crate) fn eat_color(cur: &mut Cursor<'_>, current_color: Rgba) -> Result<Option<Rgba>, String> {
    eat_color_depth(cur, current_color, 0)
}

fn eat_color_depth(
    cur: &mut Cursor<'_>,
    current_color: Rgba,
    depth: usize,
) -> Result<Option<Rgba>, String> {
    if depth > MAX_NESTING_DEPTH {
        return Err("color nesting too deep".to_owned());
    }
    let saved = cur.save();
    let Some(cv) = cur.next_cv() else {
        return Ok(None);
    };

    match cv {
        ComponentValue::PreservedToken(ts) => match &ts.token {
            Token::Hash { value, .. } => {
                let hex = value.as_ref();
                if !matches!(hex.len(), 3 | 4 | 6 | 8)
                    || !hex.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(format!("invalid hex color #{hex}"));
                }
                let mut buf = String::with_capacity(1 + hex.len());
                buf.push('#');
                buf.push_str(hex);
                let parsed = csscolorparser::parse(&buf).map_err(|e| e.to_string())?;
                let [r, g, b, a] = parsed.to_rgba8();
                Ok(Some(Rgba::new(r, g, b, a)))
            }
            Token::Ident { value, .. } => {
                let ident = value.as_ref();
                if let Some(rgba) = resolve_color_keyword(ident, current_color) {
                    Ok(Some(rgba))
                } else {
                    cur.restore(saved);
                    Ok(None)
                }
            }
            _ => {
                cur.restore(saved);
                Ok(None)
            }
        },
        ComponentValue::Function(func) => {
            let name = func.name.as_str();
            if name.eq_ignore_ascii_case("color-mix") {
                let mixed = parse_color_mix(&func.value, current_color, depth + 1)?;
                Ok(Some(mixed))
            } else if is_css_color_function(name) {
                let rgba = parse_standard_color_function(cv, func)?;
                Ok(Some(rgba))
            } else {
                cur.restore(saved);
                Ok(None)
            }
        }
        _ => {
            cur.restore(saved);
            Ok(None)
        }
    }
}

/// Resolves an identifier colour (`currentColor`, `transparent`, CSS system
/// colours, or CSS Color 4 named colours).
pub(crate) fn resolve_color_keyword(ident: &str, current_color: Rgba) -> Option<Rgba> {
    if ident.eq_ignore_ascii_case("currentcolor") {
        return Some(current_color);
    }
    if ident.eq_ignore_ascii_case("transparent") {
        return Some(Rgba::TRANSPARENT);
    }
    if let Some(sys) = system_color(ident) {
        return Some(sys);
    }
    if let Some(&[r, g, b]) = csscolorparser::NAMED_COLORS.get(ident.into()) {
        return Some(Rgba::new(r, g, b, 255));
    }
    None
}

fn is_css_color_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("rgb")
        || name.eq_ignore_ascii_case("rgba")
        || name.eq_ignore_ascii_case("hsl")
        || name.eq_ignore_ascii_case("hsla")
        || name.eq_ignore_ascii_case("hwb")
        || name.eq_ignore_ascii_case("lab")
        || name.eq_ignore_ascii_case("lch")
        || name.eq_ignore_ascii_case("oklab")
        || name.eq_ignore_ascii_case("oklch")
        || name.eq_ignore_ascii_case("color")
}

fn is_legacy_comma_capable_color_fn(name: &str) -> bool {
    name.eq_ignore_ascii_case("rgb")
        || name.eq_ignore_ascii_case("rgba")
        || name.eq_ignore_ascii_case("hsl")
        || name.eq_ignore_ascii_case("hsla")
}

fn parse_standard_color_function(cv: &ComponentValue, func: &Function) -> Result<Rgba, String> {
    let fname = func.name.as_str();
    validate_color_fn_separators(fname, &func.value)?;

    // Replace CSS Color 4 `none` missing-component keywords with `0` before
    // handing the serialised function to `csscolorparser`.
    let text = serialize_color_fn_for_parser(cv, func);
    let parsed = csscolorparser::parse(&text).map_err(|e| format!("invalid {fname}(): {e}"))?;
    let [r, g, b, a] = parsed.to_rgba8();
    Ok(Rgba::new(r, g, b, a))
}

/// Enforces CSS Color 4 separator rules that `csscolorparser` relaxes:
/// - `hwb`, `lab`, `lch`, `oklab`, `oklch`, `color` never allow commas, and
///   require `/` before an alpha component.
/// - `rgb`/`rgba`/`hsl`/`hsla` use *either* commas (`c1, c2, c3[, a]`, with no
///   `/`, and for `rgb`/`rgba` channel types cannot mix `<number>` and `<percentage>`
///   in legacy comma syntax) *or* spaces (`c1 c2 c3 [/ a]`).
fn validate_color_fn_separators(fname: &str, args: &[ComponentValue]) -> Result<(), String> {
    let tokens: Vec<&ComponentValue> = args.iter().filter(|c| !is_ws(c)).collect();
    if tokens.is_empty() {
        return Err(format!("empty {fname}()"));
    }

    // Relative color syntax `rgb(from red r g b / 0.5)`: no commas allowed.
    let is_relative = matches!(
        tokens.first(),
        Some(ComponentValue::PreservedToken(ts))
            if matches!(&ts.token, Token::Ident { value, .. } if value.eq_ignore_ascii_case("from"))
    );

    let comma_count = tokens.iter().filter(|c| is_comma(c)).count();
    let slash_count = tokens.iter().filter(|c| is_slash(c)).count();

    if slash_count > 1 {
        return Err(format!("multiple '/' in {fname}()"));
    }

    if is_relative || !is_legacy_comma_capable_color_fn(fname) {
        if comma_count > 0 {
            return Err(format!("commas are not allowed in {fname}()"));
        }
        // Without `/`, space-separated syntax has 3 components (or 4 for `color(space c1 c2 c3)`).
        if !is_relative && slash_count == 0 {
            let expected = if fname.eq_ignore_ascii_case("color") {
                4
            } else {
                3
            };
            if tokens.len() != expected {
                return Err(format!("expected {expected} components in {fname}()"));
            }
        }
        return Ok(());
    }

    if comma_count > 0 {
        if slash_count > 0 {
            return Err(format!("cannot mix ',' and '/' in {fname}()"));
        }
        let parts = split_comma_separated(args)?;
        if !matches!(parts.len(), 3 | 4) {
            return Err(format!(
                "expected 3 or 4 comma-separated arguments in {fname}()"
            ));
        }
        if fname.eq_ignore_ascii_case("rgb") || fname.eq_ignore_ascii_case("rgba") {
            // CSS Color 4 §5.1: legacy comma-separated rgb()/rgba() requires all three
            // RGB channels to be numbers or all three to be percentages (no mixing).
            let mut saw_pct = false;
            let mut saw_num = false;
            for part in &parts[..3] {
                if part.len() == 1
                    && let ComponentValue::PreservedToken(ts) = &part[0]
                {
                    match ts.token {
                        Token::Percentage { .. } => saw_pct = true,
                        Token::Number { .. } => saw_num = true,
                        _ => {}
                    }
                }
            }
            if saw_pct && saw_num {
                return Err("legacy comma rgb() cannot mix numbers and percentages".to_owned());
            }
        }
    } else if slash_count == 0 && tokens.len() != 3 {
        return Err(format!(
            "space-separated {fname}() requires '/' before alpha"
        ));
    }

    Ok(())
}

fn serialize_color_fn_for_parser(cv: &ComponentValue, func: &Function) -> String {
    let has_none = func.value.iter().any(|item| {
        matches!(
            item,
            ComponentValue::PreservedToken(ts)
                if matches!(&ts.token, Token::Ident { value, .. } if value.eq_ignore_ascii_case("none"))
        )
    });
    if !has_none {
        return serialize_cv(cv);
    }
    let mut out = String::new();
    out.push_str(func.name.as_str());
    out.push('(');
    let mut first = true;
    for item in &func.value {
        if is_ws(item) {
            if !first {
                out.push(' ');
            }
            continue;
        }
        first = false;
        if let ComponentValue::PreservedToken(ts) = item
            && let Token::Ident { value, .. } = &ts.token
            && value.eq_ignore_ascii_case("none")
        {
            out.push('0');
        } else {
            out.push_str(&serialize_cv(item));
        }
    }
    out.push(')');
    out
}

/// Parses `color-mix(in <colorspace> [<hue-method>]?, <color> [<p>%]?, <color> [<p>%]?)`.
///
/// Interpolation is performed in premultiplied sRGB (an approximation for
/// perceptual spaces such as `oklab`/`oklch`, as noted in the specification).
fn parse_color_mix(
    args: &[ComponentValue],
    current_color: Rgba,
    depth: usize,
) -> Result<Rgba, String> {
    let parts = split_comma_separated(args)?;
    if parts.len() != 3 {
        return Err("color-mix() requires 'in <colorspace>' and two colors".to_owned());
    }

    // 1. `in <color-space> [<hue-interpolation-method>]?`
    let mut space_cur = Cursor::new(parts[0]);
    if !space_cur.eat_ident_eq("in") {
        return Err("color-mix() must start with 'in <colorspace>'".to_owned());
    }
    let space = space_cur
        .eat_ident()
        .ok_or_else(|| "expected color space name in color-mix()".to_owned())?;
    let is_polar = matches!(
        space.to_ascii_lowercase().as_str(),
        "hsl" | "hwb" | "lch" | "oklch"
    );
    let is_rect = matches!(
        space.to_ascii_lowercase().as_str(),
        "srgb"
            | "srgb-linear"
            | "display-p3"
            | "a98-rgb"
            | "prophoto-rgb"
            | "rec2020"
            | "lab"
            | "oklab"
            | "xyz"
            | "xyz-d50"
            | "xyz-d65"
    );
    if !is_polar && !is_rect {
        return Err(format!("unknown color space '{space}' in color-mix()"));
    }
    if !space_cur.is_eof() {
        if !is_polar {
            return Err("hue interpolation method is only valid for polar color spaces".to_owned());
        }
        let method = space_cur
            .eat_ident()
            .ok_or_else(|| "expected hue interpolation keyword".to_owned())?;
        if !matches!(
            method.to_ascii_lowercase().as_str(),
            "shorter" | "longer" | "increasing" | "decreasing"
        ) || !space_cur.eat_ident_eq("hue")
        {
            return Err("invalid hue interpolation method in color-mix()".to_owned());
        }
        space_cur.expect_eof()?;
    }

    // 2. Two `<color> [<percentage [0,100]>]?` (or `<percentage> <color>`) items.
    let (c1, p1) = parse_mix_component(parts[1], current_color, depth)?;
    let (c2, p2) = parse_mix_component(parts[2], current_color, depth)?;

    let (w1, w2, alpha_mult) = match (p1, p2) {
        (None, None) => (0.5_f32, 0.5_f32, 1.0_f32),
        (Some(p1), None) => {
            let f1 = p1 / 100.0;
            (f1, 1.0 - f1, 1.0)
        }
        (None, Some(p2)) => {
            let f2 = p2 / 100.0;
            (1.0 - f2, f2, 1.0)
        }
        (Some(p1), Some(p2)) => {
            let sum = p1 + p2;
            if sum <= 0.0 {
                return Err("color-mix() percentages cannot sum to 0%".to_owned());
            }
            let alpha_mult = if sum < 100.0 { sum / 100.0 } else { 1.0 };
            (p1 / sum, p2 / sum, alpha_mult)
        }
    };

    let a1 = f32::from(c1.a) / 255.0;
    let a2 = f32::from(c2.a) / 255.0;
    let r1 = f32::from(c1.r) / 255.0;
    let g1 = f32::from(c1.g) / 255.0;
    let b1 = f32::from(c1.b) / 255.0;
    let r2 = f32::from(c2.r) / 255.0;
    let g2 = f32::from(c2.g) / 255.0;
    let b2 = f32::from(c2.b) / 255.0;

    let pr = w1 * r1 * a1 + w2 * r2 * a2;
    let pg = w1 * g1 * a1 + w2 * g2 * a2;
    let pb = w1 * b1 * a1 + w2 * b2 * a2;
    let a_interp = w1 * a1 + w2 * a2;

    let (r, g, b) = if a_interp > 0.0 {
        (pr / a_interp, pg / a_interp, pb / a_interp)
    } else {
        (0.0, 0.0, 0.0)
    };
    let a_final = a_interp * alpha_mult;

    Ok(Rgba::new(
        float_to_u8(r),
        float_to_u8(g),
        float_to_u8(b),
        float_to_u8(a_final),
    ))
}

fn parse_mix_component(
    cvs: &[ComponentValue],
    current_color: Rgba,
    depth: usize,
) -> Result<(Rgba, Option<f32>), String> {
    let mut cur = Cursor::new(cvs);
    if let Some(pct) = cur.eat_percentage() {
        validate_mix_pct(pct)?;
        let color = eat_color_depth(&mut cur, current_color, depth)?
            .ok_or_else(|| "expected color after percentage in color-mix()".to_owned())?;
        cur.expect_eof()?;
        Ok((color, Some(pct)))
    } else {
        let color = eat_color_depth(&mut cur, current_color, depth)?
            .ok_or_else(|| "expected color in color-mix()".to_owned())?;
        let pct = if !cur.is_eof() {
            let p = cur
                .eat_percentage()
                .ok_or_else(|| "expected percentage in color-mix()".to_owned())?;
            validate_mix_pct(p)?;
            Some(p)
        } else {
            None
        };
        cur.expect_eof()?;
        Ok((color, pct))
    }
}

fn validate_mix_pct(pct: f32) -> Result<(), String> {
    if !(0.0..=100.0).contains(&pct) {
        Err(format!(
            "color-mix() percentage {pct}% out of [0%, 100%] range"
        ))
    } else {
        Ok(())
    }
}

#[inline]
fn float_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// CSS Color 4 §6.2 system colours (light-scheme reference values), plus
/// deprecated CSS 2 aliases mapped to their CSS Color 4 equivalents.
fn system_color(ident: &str) -> Option<Rgba> {
    match ident.to_ascii_lowercase().as_str() {
        "canvas" | "field" | "window" | "infobackground" | "menu" | "appworkspace"
        | "background" | "scrollbar" => Some(Rgba::WHITE),
        "canvastext"
        | "fieldtext"
        | "buttontext"
        | "marktext"
        | "windowtext"
        | "infotext"
        | "menutext"
        | "captiontext"
        | "inactivecaptiontext" => Some(Rgba::BLACK),
        "linktext" => Some(Rgba::new(0, 0, 238, 255)),
        "visitedtext" => Some(Rgba::new(85, 26, 139, 255)),
        "activetext" => Some(Rgba::new(255, 0, 0, 255)),
        "buttonface" | "threedface" => Some(Rgba::new(240, 240, 240, 255)),
        "buttonborder" | "windowframe" | "activeborder" | "inactiveborder" | "threedshadow"
        | "threeddarkshadow" | "threedhighlight" | "threedlightshadow" => {
            Some(Rgba::new(118, 118, 118, 255))
        }
        "highlight" | "selecteditem" | "accentcolor" | "activecaption" => {
            Some(Rgba::new(0, 96, 223, 255))
        }
        "highlighttext" | "selecteditemtext" | "accentcolortext" => Some(Rgba::WHITE),
        "graytext" | "inactivecaption" => Some(Rgba::new(109, 109, 109, 255)),
        "mark" => Some(Rgba::new(255, 255, 0, 255)),
        _ => None,
    }
}

pub(crate) fn is_current_color_cvs(cvs: &[ComponentValue]) -> bool {
    let mut cur = Cursor::new(cvs);
    cur.eat_ident_eq("currentcolor") && cur.is_eof()
}
