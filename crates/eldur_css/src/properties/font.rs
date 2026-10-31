//! CSS Fonts 4 & CSS Inline 3: `font-family`, `font-size`, `font-weight`,
//! `font-style`, `font-variant-caps`, and `line-height`.

use swc_css_ast::ComponentValue;

use crate::{ComputedStyle, FontStyle, LineHeight};

use super::{
    ComputeContext,
    cursor::{Cursor, is_css_wide_keyword, split_comma_separated},
    length::{
        UnitContext, eat_angle_deg, is_math_fn, length_to_px, parse_length_cvs, parse_number_cvs,
    },
};

const GENERIC_FAMILIES: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
    "emoji",
    "math",
    "fangsong",
];

fn is_generic_family(ident: &str) -> bool {
    GENERIC_FAMILIES
        .iter()
        .any(|g| ident.eq_ignore_ascii_case(g))
}

/// Parses `font-family`: comma-separated quoted strings or unquoted `<custom-ident>+` names.
pub(crate) fn parse_font_family(cvs: &[ComponentValue]) -> Result<Vec<String>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut families = Vec::with_capacity(parts.len());

    for part in parts {
        let mut cur = Cursor::new(part);
        if let Some(quoted) = cur.eat_string() {
            cur.expect_eof()?;
            families.push(quoted.to_owned());
            continue;
        }

        let mut words = Vec::new();
        while !cur.is_eof() {
            let id = cur
                .eat_ident()
                .ok_or_else(|| "invalid token in unquoted font-family".to_owned())?;
            if is_css_wide_keyword(id) || id.eq_ignore_ascii_case("default") {
                return Err(format!(
                    "reserved keyword '{id}' is not allowed in font-family"
                ));
            }
            words.push(id);
        }

        if words.is_empty() {
            return Err("empty font-family item".to_owned());
        }

        if words.len() == 1 && is_generic_family(words[0]) {
            families.push(words[0].to_ascii_lowercase());
        } else {
            // CSS Fonts 4 §2.1: generic family keywords cannot be part of a multi-word unquoted name?
            // Actually `Sans Serif` (without hyphen) is an unquoted custom family name `"Sans Serif"`,
            // while a single generic keyword like `Sans-Serif` normalises to `"sans-serif"`.
            families.push(words.join(" "));
        }
    }

    if families.is_empty() {
        return Err("font-family cannot be empty".to_owned());
    }
    Ok(families)
}

/// Returns `Some(px)` if `ident` is an absolute `font-size` keyword (medium = 16px).
pub(crate) fn absolute_font_size_kw(ident: &str) -> Option<f32> {
    match ident.to_ascii_lowercase().as_str() {
        "xx-small" => Some(9.0),
        "x-small" => Some(10.0),
        "small" => Some(13.0),
        "medium" => Some(16.0),
        "large" => Some(18.0),
        "x-large" => Some(24.0),
        "xx-large" => Some(32.0),
        "xxx-large" => Some(48.0),
        _ => None,
    }
}

/// Parses `font-size` to computed `px`.
pub(crate) fn parse_font_size(
    cvs: &[ComponentValue],
    ctx: &ComputeContext<'_>,
) -> Result<f32, String> {
    let parent_fs = ctx.parent.map_or(16.0, |p| p.font_size);
    let mut cur = Cursor::new(cvs);

    if let Some(id) = cur.eat_ident() {
        cur.expect_eof()?;
        if let Some(px) = absolute_font_size_kw(id) {
            return Ok(px);
        }
        return match id.to_ascii_lowercase().as_str() {
            "smaller" => Ok(parent_fs / 1.2),
            "larger" => Ok(parent_fs * 1.2),
            "math" => Ok(parent_fs),
            _ => Err(format!("invalid font-size keyword '{id}'")),
        };
    }

    let uctx = UnitContext::for_font_size(ctx);
    let len = parse_length_cvs(cvs, &uctx, true)?;
    length_to_px(&len, &uctx, true)
}

/// Parses `font-weight` (`normal`, `bold`, `bolder`, `lighter`, or `<number [1,1000]>`).
pub(crate) fn parse_font_weight(
    cvs: &[ComponentValue],
    ctx: &ComputeContext<'_>,
) -> Result<u16, String> {
    let parent_w = ctx.parent.map_or(400, |p| p.font_weight);
    let mut cur = Cursor::new(cvs);

    let saved = cur.save();
    if let Some(id) = cur.eat_ident() {
        cur.expect_eof()?;
        return match id.to_ascii_lowercase().as_str() {
            "normal" => Ok(400),
            "bold" => Ok(700),
            "bolder" => Ok(relative_bolder(parent_w)),
            "lighter" => Ok(relative_lighter(parent_w)),
            _ => Err(format!("invalid font-weight keyword '{id}'")),
        };
    }
    cur.restore(saved);

    let is_calc = cur
        .peek()
        .is_some_and(|cv| matches!(cv, ComponentValue::Function(f) if is_math_fn(f.name.as_str())));

    let num = parse_number_cvs(cvs, false)?;
    if !is_calc && !(1.0..=1000.0).contains(&num) {
        return Err(format!("font-weight '{num}' out of range [1, 1000]"));
    }
    Ok(num.round().clamp(1.0, 1000.0) as u16)
}

fn relative_bolder(inherited: u16) -> u16 {
    if inherited < 350 {
        400
    } else if inherited <= 550 {
        700
    } else {
        inherited.max(900)
    }
}

fn relative_lighter(inherited: u16) -> u16 {
    if inherited < 100 {
        inherited
    } else if inherited <= 550 {
        100
    } else if inherited <= 750 {
        400
    } else {
        700
    }
}

/// Parses `font-style` (`normal | italic | oblique <angle [-90deg, 90deg]>?`).
pub(crate) fn parse_font_style(cvs: &[ComponentValue]) -> Result<FontStyle, String> {
    let mut cur = Cursor::new(cvs);
    let style =
        eat_font_style(&mut cur)?.ok_or_else(|| "expected font-style keyword".to_owned())?;
    cur.expect_eof()?;
    Ok(style)
}

/// Consumes a `font-style` value (`normal`, `italic`, or `oblique [<angle>]`) from `cur`.
pub(crate) fn eat_font_style(cur: &mut Cursor<'_>) -> Result<Option<FontStyle>, String> {
    let saved = cur.save();
    let Some(id) = cur.eat_ident() else {
        return Ok(None);
    };
    match id.to_ascii_lowercase().as_str() {
        "normal" => Ok(Some(FontStyle::Normal)),
        "italic" => Ok(Some(FontStyle::Italic)),
        "oblique" => {
            let is_calc = cur.peek().is_some_and(
                |cv| matches!(cv, ComponentValue::Function(f) if is_math_fn(f.name.as_str())),
            );
            if let Some(deg) = eat_angle_deg(cur, false)?
                && !is_calc
                && !(-90.0..=90.0).contains(&deg)
            {
                return Err(format!(
                    "oblique angle '{deg}deg' out of range [-90deg, 90deg]"
                ));
            }
            Ok(Some(FontStyle::Oblique))
        }
        _ => {
            cur.restore(saved);
            Ok(None)
        }
    }
}

/// Parses `font-variant-caps`.
pub(crate) fn parse_font_variant_caps(cvs: &[ComponentValue]) -> Result<bool, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected font-variant-caps keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "small-caps" | "all-small-caps" => Ok(true),
        "normal" | "petite-caps" | "all-petite-caps" | "unicase" | "titling-caps" => Ok(false),
        _ => Err(format!("invalid font-variant-caps '{id}'")),
    }
}

/// Returns `true` if `ident` is a CSS Fonts 3/4 `font-stretch` keyword (valid in `font` shorthand).
pub(crate) fn is_font_stretch_kw(ident: &str) -> bool {
    matches!(
        ident.to_ascii_lowercase().as_str(),
        "normal"
            | "ultra-condensed"
            | "extra-condensed"
            | "condensed"
            | "semi-condensed"
            | "semi-expanded"
            | "expanded"
            | "extra-expanded"
            | "ultra-expanded"
    )
}

/// Parses `line-height` (`normal | <number [0,∞]> | <length-percentage [0,∞]>`).
pub(crate) fn parse_line_height(
    cvs: &[ComponentValue],
    style: &ComputedStyle,
    ctx: &ComputeContext<'_>,
) -> Result<LineHeight, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("normal") {
        cur.expect_eof()?;
        return Ok(LineHeight::Normal);
    }

    if let Ok(num) = parse_number_cvs(cvs, true) {
        return Ok(LineHeight::Number(num));
    }

    let uctx = UnitContext::for_line_height(style, ctx);
    let len = parse_length_cvs(cvs, &uctx, true)?;
    let px = length_to_px(&len, &uctx, true)?;
    Ok(LineHeight::Px(px))
}
