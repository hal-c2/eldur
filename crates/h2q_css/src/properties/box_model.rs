//! Box generation, positioning, sizing, margins, padding, borders, radii, and outlines.

use swc_css_ast::ComponentValue;

use crate::{
    BorderSide, BorderStyle, BoxSizing, Display, Float, Length, LengthAuto, LengthPercentage,
    Position, Rgba, Size,
};

use super::{
    cursor::Cursor,
    length::{
        UnitContext, eat_length, eat_number_or_calc, eat_px, length_to_lp, parse_integer_cvs,
        parse_length_auto_cvs, parse_length_cvs, parse_px_cvs,
    },
};

/// Parses CSS Display 3 single-keyword and multi-keyword (`block flex`,
/// `inline flow-root`, `block flow list-item`, …) `display` values.
pub(crate) fn parse_display(cvs: &[ComponentValue]) -> Result<Display, String> {
    let mut cur = Cursor::new(cvs);
    let mut words = Vec::new();
    while let Some(id) = cur.eat_ident() {
        words.push(id.to_ascii_lowercase());
    }
    cur.expect_eof()?;

    match words.as_slice() {
        [single] => match single.as_str() {
            "block" => Ok(Display::Block),
            "inline" => Ok(Display::Inline),
            "inline-block" => Ok(Display::InlineBlock),
            "flex" => Ok(Display::Flex),
            "inline-flex" => Ok(Display::InlineFlex),
            "grid" => Ok(Display::Grid),
            "inline-grid" => Ok(Display::InlineGrid),
            "flow-root" => Ok(Display::FlowRoot),
            "list-item" => Ok(Display::ListItem),
            "table" => Ok(Display::Table),
            "inline-table" => Ok(Display::InlineTable),
            "table-row-group" => Ok(Display::TableRowGroup),
            "table-header-group" => Ok(Display::TableHeaderGroup),
            "table-footer-group" => Ok(Display::TableFooterGroup),
            "table-row" => Ok(Display::TableRow),
            "table-cell" => Ok(Display::TableCell),
            "table-column-group" => Ok(Display::TableColumnGroup),
            "table-column" => Ok(Display::TableColumn),
            "table-caption" => Ok(Display::TableCaption),
            "contents" => Ok(Display::Contents),
            "none" => Ok(Display::None),
            _ => Err(format!("invalid display value '{single}'")),
        },
        multi if (2..=3).contains(&multi.len()) => parse_multi_keyword_display(multi),
        _ => Err("invalid display value".to_owned()),
    }
}

fn parse_multi_keyword_display(words: &[String]) -> Result<Display, String> {
    let mut outside: Option<&str> = None;
    let mut inside: Option<&str> = None;
    let mut list_item = false;

    for w in words {
        match w.as_str() {
            "block" | "inline" => {
                if outside.replace(w.as_str()).is_some() {
                    return Err("duplicate display-outside keyword".to_owned());
                }
            }
            "flow" | "flow-root" | "flex" | "grid" | "table" => {
                if inside.replace(w.as_str()).is_some() {
                    return Err("duplicate display-inside keyword".to_owned());
                }
            }
            "list-item" => {
                if list_item {
                    return Err("duplicate 'list-item' in display".to_owned());
                }
                list_item = true;
            }
            _ => return Err(format!("invalid keyword '{w}' in multi-keyword display")),
        }
    }

    if list_item {
        // CSS Display 3 §2.3: `<display-outside>? && [ flow | flow-root ]? && list-item`
        if matches!(inside, None | Some("flow" | "flow-root")) {
            return Ok(Display::ListItem);
        }
        return Err("list-item can only combine with flow or flow-root".to_owned());
    }

    match (outside, inside) {
        (Some("block"), Some("flow")) => Ok(Display::Block),
        (Some("block"), Some("flow-root")) => Ok(Display::FlowRoot),
        (Some("inline"), Some("flow")) => Ok(Display::Inline),
        (Some("inline"), Some("flow-root")) => Ok(Display::InlineBlock),
        (Some("block"), Some("flex")) => Ok(Display::Flex),
        (Some("inline"), Some("flex")) => Ok(Display::InlineFlex),
        (Some("block"), Some("grid")) => Ok(Display::Grid),
        (Some("inline"), Some("grid")) => Ok(Display::InlineGrid),
        (Some("block"), Some("table")) => Ok(Display::Table),
        (Some("inline"), Some("table")) => Ok(Display::InlineTable),
        _ => Err("invalid multi-keyword display combination".to_owned()),
    }
}

pub(crate) fn parse_position(cvs: &[ComponentValue]) -> Result<Position, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected position keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "static" => Ok(Position::Static),
        "relative" => Ok(Position::Relative),
        "absolute" => Ok(Position::Absolute),
        "fixed" => Ok(Position::Fixed),
        "sticky" | "-webkit-sticky" => Ok(Position::Sticky),
        _ => Err(format!("invalid position '{id}'")),
    }
}

pub(crate) fn parse_inset_side(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<LengthAuto, String> {
    parse_length_auto_cvs(cvs, uctx, false)
}

pub(crate) fn parse_z_index(cvs: &[ComponentValue]) -> Result<Option<i32>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("auto") {
        cur.expect_eof()?;
        return Ok(None);
    }
    Ok(Some(parse_integer_cvs(cvs)?))
}

pub(crate) fn parse_float(cvs: &[ComponentValue]) -> Result<Float, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected float keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "none" => Ok(Float::None),
        "left" | "inline-start" => Ok(Float::Left),
        "right" | "inline-end" => Ok(Float::Right),
        _ => Err(format!("invalid float '{id}'")),
    }
}

pub(crate) fn parse_box_sizing(cvs: &[ComponentValue]) -> Result<BoxSizing, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected box-sizing keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "content-box" => Ok(BoxSizing::ContentBox),
        "border-box" => Ok(BoxSizing::BorderBox),
        _ => Err(format!("invalid box-sizing '{id}'")),
    }
}

/// Parses `width`/`height`/`min-*` (`is_max = false`) or `max-*` (`is_max = true`),
/// and `flex-basis` (`allow_content = true`).
pub(crate) fn parse_size(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    is_max: bool,
    allow_content: bool,
) -> Result<Size, String> {
    let mut cur = Cursor::new(cvs);

    let saved = cur.save();
    if let Some(id) = cur.eat_ident() {
        let matched = match id.to_ascii_lowercase().as_str() {
            "auto" if !is_max => Some(Size::Auto),
            "none" if is_max => Some(Size::None),
            "content" if allow_content => Some(Size::MaxContent),
            "min-content" | "-webkit-min-content" | "-moz-min-content" => Some(Size::MinContent),
            "max-content" | "-webkit-max-content" | "-moz-max-content" => Some(Size::MaxContent),
            "fit-content" | "-webkit-fit-content" | "-moz-fit-content" => Some(Size::FitContent),
            _ => None,
        };
        if let Some(sz) = matched {
            cur.expect_eof()?;
            return Ok(sz);
        }
        cur.restore(saved);
    }

    if let Some((fname, fargs)) = cur.eat_function() {
        if fname.eq_ignore_ascii_case("fit-content") {
            let _ = parse_length_cvs(fargs, uctx, true)?;
            cur.expect_eof()?;
            return Ok(Size::FitContent);
        }
        cur.restore(saved);
    }

    let len = eat_length(&mut cur, uctx, true)?.ok_or_else(|| "invalid sizing value".to_owned())?;
    cur.expect_eof()?;
    Ok(Size::Length(len))
}

/// Parses `aspect-ratio`: `auto | <ratio> | auto <ratio> | <ratio> auto`.
pub(crate) fn parse_aspect_ratio(cvs: &[ComponentValue]) -> Result<Option<f32>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("auto") {
        if cur.is_eof() {
            return Ok(None);
        }
        let ratio = eat_ratio(&mut cur)?;
        cur.expect_eof()?;
        return Ok(Some(ratio));
    }

    let ratio = eat_ratio(&mut cur)?;
    let _ = cur.eat_ident_eq("auto");
    cur.expect_eof()?;
    Ok(Some(ratio))
}

fn eat_ratio(cur: &mut Cursor<'_>) -> Result<f32, String> {
    let w = eat_number_or_calc(cur, true)?.ok_or_else(|| "expected ratio number".to_owned())?;
    if w <= 0.0 || !w.is_finite() {
        return Err("aspect-ratio width must be positive".to_owned());
    }
    let h = if cur.eat_slash() {
        let h = eat_number_or_calc(cur, true)?
            .ok_or_else(|| "expected denominator after '/' in aspect-ratio".to_owned())?;
        if h <= 0.0 || !h.is_finite() {
            return Err("aspect-ratio denominator must be positive".to_owned());
        }
        h
    } else {
        1.0
    };
    let ratio = w / h;
    if !ratio.is_finite() || ratio <= 0.0 {
        return Err("invalid aspect-ratio".to_owned());
    }
    Ok(ratio)
}

pub(crate) fn parse_margin_side(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<LengthAuto, String> {
    parse_length_auto_cvs(cvs, uctx, false)
}

pub(crate) fn parse_padding_side(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Length, String> {
    parse_length_cvs(cvs, uctx, true)
}

pub(crate) fn eat_line_width(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
) -> Result<Option<f32>, String> {
    let saved = cur.save();
    if let Some(id) = cur.eat_ident() {
        match id.to_ascii_lowercase().as_str() {
            "thin" => return Ok(Some(1.0)),
            "medium" => return Ok(Some(3.0)),
            "thick" => return Ok(Some(5.0)),
            _ => cur.restore(saved),
        }
    }
    eat_px(cur, uctx, true)
}

pub(crate) fn parse_line_width(cvs: &[ComponentValue], uctx: &UnitContext) -> Result<f32, String> {
    let mut cur = Cursor::new(cvs);
    let w = eat_line_width(&mut cur, uctx)?.ok_or_else(|| "expected <line-width>".to_owned())?;
    cur.expect_eof()?;
    Ok(w)
}

pub(crate) fn eat_border_style(cur: &mut Cursor<'_>, allow_auto: bool) -> Option<BorderStyle> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    let st = match id.to_ascii_lowercase().as_str() {
        "none" => BorderStyle::None,
        "hidden" => BorderStyle::Hidden,
        "solid" => BorderStyle::Solid,
        "dashed" => BorderStyle::Dashed,
        "dotted" => BorderStyle::Dotted,
        "double" => BorderStyle::Double,
        "groove" => BorderStyle::Groove,
        "ridge" => BorderStyle::Ridge,
        "inset" => BorderStyle::Inset,
        "outset" => BorderStyle::Outset,
        "auto" if allow_auto => BorderStyle::Solid,
        _ => {
            cur.restore(saved);
            return None;
        }
    };
    Some(st)
}

pub(crate) fn parse_border_style(
    cvs: &[ComponentValue],
    allow_auto: bool,
) -> Result<BorderStyle, String> {
    let mut cur = Cursor::new(cvs);
    let st =
        eat_border_style(&mut cur, allow_auto).ok_or_else(|| "expected <line-style>".to_owned())?;
    cur.expect_eof()?;
    Ok(st)
}

/// Applies `border-*-style` / `outline-style` to `side`, enforcing the CSS
/// computed-value rule that `none`/`hidden` force `width = 0` and populating
/// CSS initial `medium` (3px) and `currentColor` when transitioning out of `none`.
pub(crate) fn apply_border_style(
    side: &mut BorderSide,
    new_style: BorderStyle,
    current_color: Rgba,
) {
    let was_unpainted = matches!(side.style, BorderStyle::None | BorderStyle::Hidden);
    side.style = new_style;
    if matches!(new_style, BorderStyle::None | BorderStyle::Hidden) {
        side.width = 0.0;
    } else if was_unpainted {
        side.width = side.explicit_width.unwrap_or(3.0);
        if let Some(c) = side.explicit_color {
            side.color = c;
        } else {
            side.color = current_color;
        }
    }
}

pub(crate) fn apply_border_color(side: &mut BorderSide, color: Rgba, is_current_color: bool) {
    side.color = color;
    side.explicit_color = if is_current_color { None } else { Some(color) };
}

/// Applies `border-*-width` / `outline-width` to `side` (`0` when `style` is `none`/`hidden`).
pub(crate) fn apply_border_width(side: &mut BorderSide, parsed_px: f32) {
    side.explicit_width = Some(parsed_px);
    side.width = if matches!(side.style, BorderStyle::None | BorderStyle::Hidden) {
        0.0
    } else {
        parsed_px
    };
}

/// Parses a single corner radius longhand (`border-top-left-radius`, …):
/// `1–2 <length-percentage [0,∞]>`, keeping the horizontal radius.
pub(crate) fn parse_corner_radius(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<LengthPercentage, String> {
    let mut cur = Cursor::new(cvs);
    let horiz = eat_length(&mut cur, uctx, true)?
        .ok_or_else(|| "expected corner radius <length-percentage>".to_owned())?;
    if !cur.is_eof() {
        let _vert = eat_length(&mut cur, uctx, true)?
            .ok_or_else(|| "invalid vertical corner radius".to_owned())?;
    }
    cur.expect_eof()?;
    length_to_lp(horiz, uctx, true)
}

pub(crate) fn parse_outline_offset(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<f32, String> {
    parse_px_cvs(cvs, uctx, false)
}
