//! Backgrounds, gradients, `<position>`, `box-shadow`, `opacity`, `overflow`,
//! `object-fit`, and `filter`.

use swc_css_ast::ComponentValue;

use crate::{
    BackgroundAttachment, BackgroundClip, BackgroundLayer, BackgroundOrigin, BackgroundRepeat,
    BackgroundSize, ColorStop, ComputedStyle, Image, LengthAuto, LengthPercentage, ObjectFit,
    Overflow, Rgba, Shadow,
};

use super::{
    color::{eat_color, parse_color_cvs},
    cursor::{Cursor, serialize_cvs, split_comma_separated},
    length::{
        UnitContext, eat_angle_deg, eat_length, eat_length_auto, eat_number_or_percentage, eat_px,
        length_to_lp,
    },
};

// ---------------------------------------------------------------------------
// 2-D `<position>` (1, 2, 3, and 4-value CSS Backgrounds 3 §3.6 syntax)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum PosTok {
    Left,
    Right,
    Top,
    Bottom,
    Center,
    Lp(LengthPercentage),
}

impl PosTok {
    fn is_horiz_kw(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    fn is_vert_kw(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

fn eat_pos_tok(cur: &mut Cursor<'_>, uctx: &UnitContext) -> Result<Option<PosTok>, String> {
    let saved = cur.save();
    if let Some(id) = cur.eat_ident() {
        let tok = match id.to_ascii_lowercase().as_str() {
            "left" => Some(PosTok::Left),
            "right" => Some(PosTok::Right),
            "top" => Some(PosTok::Top),
            "bottom" => Some(PosTok::Bottom),
            "center" => Some(PosTok::Center),
            _ => None,
        };
        if let Some(t) = tok {
            return Ok(Some(t));
        }
        cur.restore(saved);
    }
    if let Some(len) = eat_length(cur, uctx, false)? {
        return Ok(Some(PosTok::Lp(length_to_lp(len, uctx, false)?)));
    }
    Ok(None)
}

fn edge_offset(is_end: bool, lp: LengthPercentage) -> LengthPercentage {
    if is_end {
        LengthPercentage {
            px: -lp.px,
            percent: 100.0 - lp.percent,
            vw: -lp.vw,
            vh: -lp.vh,
        }
    } else {
        lp
    }
}

fn horiz_kw_lp(tok: PosTok) -> Option<LengthPercentage> {
    match tok {
        PosTok::Left => Some(LengthPercentage::percent(0.0)),
        PosTok::Center => Some(LengthPercentage::percent(50.0)),
        PosTok::Right => Some(LengthPercentage::percent(100.0)),
        _ => None,
    }
}

fn vert_kw_lp(tok: PosTok) -> Option<LengthPercentage> {
    match tok {
        PosTok::Top => Some(LengthPercentage::percent(0.0)),
        PosTok::Center => Some(LengthPercentage::percent(50.0)),
        PosTok::Bottom => Some(LengthPercentage::percent(100.0)),
        _ => None,
    }
}

fn resolve_pos_tokens(
    toks: &[PosTok],
    allow_3_4_value: bool,
) -> Result<(LengthPercentage, LengthPercentage), String> {
    match toks {
        [t1] => match *t1 {
            PosTok::Left => Ok((
                LengthPercentage::percent(0.0),
                LengthPercentage::percent(50.0),
            )),
            PosTok::Right => Ok((
                LengthPercentage::percent(100.0),
                LengthPercentage::percent(50.0),
            )),
            PosTok::Center => Ok((
                LengthPercentage::percent(50.0),
                LengthPercentage::percent(50.0),
            )),
            PosTok::Top => Ok((
                LengthPercentage::percent(50.0),
                LengthPercentage::percent(0.0),
            )),
            PosTok::Bottom => Ok((
                LengthPercentage::percent(50.0),
                LengthPercentage::percent(100.0),
            )),
            PosTok::Lp(x) => Ok((x, LengthPercentage::percent(50.0))),
        },
        [t1, t2] => {
            // Two keywords: order may be (H, V) or (V, H), provided axes do not collide.
            if !matches!(t1, PosTok::Lp(_)) && !matches!(t2, PosTok::Lp(_)) {
                if let (Some(x), Some(y)) = (horiz_kw_lp(*t1), vert_kw_lp(*t2)) {
                    return Ok((x, y));
                }
                if let (Some(y), Some(x)) = (vert_kw_lp(*t1), horiz_kw_lp(*t2)) {
                    return Ok((x, y));
                }
                return Err("conflicting axis keywords in 2-value <position>".to_owned());
            }
            // When a `<length-percentage>` is present in 2-value syntax, CSS requires (H, V).
            let x = match *t1 {
                PosTok::Lp(lp) => lp,
                other => horiz_kw_lp(other).ok_or_else(|| {
                    "first token of 2-value <position> must be horizontal".to_owned()
                })?,
            };
            let y = match *t2 {
                PosTok::Lp(lp) => lp,
                other => vert_kw_lp(other).ok_or_else(|| {
                    "second token of 2-value <position> must be vertical".to_owned()
                })?,
            };
            Ok((x, y))
        }
        [t1, t2, t3] if allow_3_4_value => match (*t1, *t2, *t3) {
            (h, PosTok::Lp(off), v) if h.is_horiz_kw() => {
                let y = vert_kw_lp(v)
                    .ok_or_else(|| "invalid vertical keyword in 3-value <position>".to_owned())?;
                Ok((edge_offset(matches!(h, PosTok::Right), off), y))
            }
            (v, PosTok::Lp(off), h) if v.is_vert_kw() => {
                let x = horiz_kw_lp(h)
                    .ok_or_else(|| "invalid horizontal keyword in 3-value <position>".to_owned())?;
                Ok((x, edge_offset(matches!(v, PosTok::Bottom), off)))
            }
            (h, v, PosTok::Lp(off)) if v.is_vert_kw() => {
                let x = horiz_kw_lp(h)
                    .ok_or_else(|| "invalid horizontal keyword in 3-value <position>".to_owned())?;
                Ok((x, edge_offset(matches!(v, PosTok::Bottom), off)))
            }
            (v, h, PosTok::Lp(off)) if h.is_horiz_kw() => {
                let y = vert_kw_lp(v)
                    .ok_or_else(|| "invalid vertical keyword in 3-value <position>".to_owned())?;
                Ok((edge_offset(matches!(h, PosTok::Right), off), y))
            }
            _ => Err("invalid 3-value <position>".to_owned()),
        },
        [t1, t2, t3, t4] if allow_3_4_value => match (*t1, *t2, *t3, *t4) {
            (h, PosTok::Lp(hx), v, PosTok::Lp(vy)) if h.is_horiz_kw() && v.is_vert_kw() => Ok((
                edge_offset(matches!(h, PosTok::Right), hx),
                edge_offset(matches!(v, PosTok::Bottom), vy),
            )),
            (v, PosTok::Lp(vy), h, PosTok::Lp(hx)) if v.is_vert_kw() && h.is_horiz_kw() => Ok((
                edge_offset(matches!(h, PosTok::Right), hx),
                edge_offset(matches!(v, PosTok::Bottom), vy),
            )),
            _ => Err("invalid 4-value <position>".to_owned()),
        },
        _ => Err("invalid <position>".to_owned()),
    }
}

/// Parses a complete slice as a 2-D `<position>` (`allow_3_4_value = true` for
/// `background-position` and gradients, `false` for `transform-origin`).
pub(crate) fn parse_position_2d(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    allow_3_4_value: bool,
) -> Result<(LengthPercentage, LengthPercentage), String> {
    let mut cur = Cursor::new(cvs);
    let mut toks = Vec::new();
    while !cur.is_eof() {
        let tok =
            eat_pos_tok(&mut cur, uctx)?.ok_or_else(|| "invalid token in <position>".to_owned())?;
        toks.push(tok);
        if toks.len() > 4 {
            return Err("too many tokens in <position>".to_owned());
        }
    }
    if toks.is_empty() {
        return Err("empty <position>".to_owned());
    }
    resolve_pos_tokens(&toks, allow_3_4_value)
}

/// Parses a single horizontal position value for `background-position-x`
/// (`left | center | right | <length-percentage> | [ left | right ] <length-percentage>`).
pub(crate) fn parse_bg_position_axis(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    is_x: bool,
) -> Result<LengthPercentage, String> {
    let mut cur = Cursor::new(cvs);
    let first = eat_pos_tok(&mut cur, uctx)?
        .ok_or_else(|| "expected background-position value".to_owned())?;

    if cur.is_eof() {
        return match (is_x, first) {
            (_, PosTok::Lp(lp)) => Ok(lp),
            (_, PosTok::Center) => Ok(LengthPercentage::percent(50.0)),
            (true, PosTok::Left) | (false, PosTok::Top) => Ok(LengthPercentage::percent(0.0)),
            (true, PosTok::Right) | (false, PosTok::Bottom) => Ok(LengthPercentage::percent(100.0)),
            _ => Err("wrong axis keyword for background-position-x/y".to_owned()),
        };
    }

    let second = eat_pos_tok(&mut cur, uctx)?
        .ok_or_else(|| "invalid offset in background-position-x/y".to_owned())?;
    cur.expect_eof()?;

    match (is_x, first, second) {
        (true, PosTok::Left, PosTok::Lp(lp)) => Ok(edge_offset(false, lp)),
        (true, PosTok::Right, PosTok::Lp(lp)) => Ok(edge_offset(true, lp)),
        (false, PosTok::Top, PosTok::Lp(lp)) => Ok(edge_offset(false, lp)),
        (false, PosTok::Bottom, PosTok::Lp(lp)) => Ok(edge_offset(true, lp)),
        _ => Err("invalid 2-value background-position-x/y".to_owned()),
    }
}

pub(crate) fn parse_bg_position_axis_list(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    is_x: bool,
) -> Result<Vec<LengthPercentage>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        out.push(parse_bg_position_axis(part, uctx, is_x)?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Images & gradients (`url()`, `linear-gradient`, `radial-gradient`, `conic-gradient`)
// ---------------------------------------------------------------------------

pub(crate) fn eat_bg_image_item(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<Option<Option<Image>>, String> {
    if cur.eat_ident_eq("none") {
        return Ok(Some(None));
    }
    Ok(eat_image(cur, uctx, current_color)?.map(Some))
}

pub(crate) fn eat_image(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<Option<Image>, String> {
    if let Some(url) = cur.eat_url()? {
        return Ok(Some(Image::Url(url)));
    }
    let saved = cur.save();
    let Some((fname, fargs)) = cur.eat_function() else {
        return Ok(None);
    };
    let lower = fname.to_ascii_lowercase();
    match lower.as_str() {
        "linear-gradient" | "repeating-linear-gradient" => {
            let repeating = lower.starts_with("repeating-");
            Ok(Some(parse_linear_gradient(
                fargs,
                uctx,
                current_color,
                repeating,
            )?))
        }
        "radial-gradient" | "repeating-radial-gradient" => {
            let repeating = lower.starts_with("repeating-");
            Ok(Some(parse_radial_gradient(
                fargs,
                uctx,
                current_color,
                repeating,
            )?))
        }
        "conic-gradient" | "repeating-conic-gradient" => {
            Ok(Some(parse_conic_gradient(fargs, uctx, current_color)?))
        }
        _ => {
            cur.restore(saved);
            Ok(None)
        }
    }
}

pub(crate) fn parse_background_image_list(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<Vec<Option<Image>>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        let item = eat_bg_image_item(&mut cur, uctx, current_color)?
            .ok_or_else(|| "expected 'none' or <image> in background-image".to_owned())?;
        cur.expect_eof()?;
        out.push(item);
    }
    Ok(out)
}

fn skip_optional_color_space(cur: &mut Cursor<'_>) -> Result<bool, String> {
    if !cur.eat_ident_eq("in") {
        return Ok(false);
    }
    let _space = cur
        .eat_ident()
        .ok_or_else(|| "expected color space after 'in'".to_owned())?;
    let saved = cur.save();
    if let Some(method) = cur.eat_ident() {
        if matches!(
            method.to_ascii_lowercase().as_str(),
            "shorter" | "longer" | "increasing" | "decreasing"
        ) {
            if !cur.eat_ident_eq("hue") {
                return Err("expected 'hue' after interpolation keyword".to_owned());
            }
        } else {
            cur.restore(saved);
        }
    }
    Ok(true)
}

fn parse_linear_gradient(
    args: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
    repeating: bool,
) -> Result<Image, String> {
    let parts = split_comma_separated(args)?;
    if parts.len() < 2 {
        return Err("linear-gradient() requires at least 2 color stops".to_owned());
    }

    let mut direction = Direction::Angle(180.0);
    let mut stop_start = 0;

    if let Some(parsed) = try_parse_linear_prelude(parts[0])? {
        direction = parsed;
        stop_start = 1;
    }

    let stops = parse_gradient_stops(&parts[stop_start..], uctx, current_color, false)?;
    Ok(match direction {
        Direction::Angle(angle_deg) => Image::LinearGradient {
            angle_deg,
            stops,
            repeating,
        },
        Direction::Corner { right, bottom } => Image::LinearGradientToCorner {
            right,
            bottom,
            stops,
            repeating,
        },
    })
}

/// A linear gradient's direction: an angle, or a corner whose angle
/// depends on the box.
enum Direction {
    Angle(f32),
    Corner { right: bool, bottom: bool },
}

fn try_parse_linear_prelude(cvs: &[ComponentValue]) -> Result<Option<Direction>, String> {
    let mut cur = Cursor::new(cvs);
    let mut angle: Option<Direction> = None;
    let mut saw_in = false;

    for _ in 0..2 {
        if cur.is_eof() {
            break;
        }
        if !saw_in && skip_optional_color_space(&mut cur)? {
            saw_in = true;
            continue;
        }
        if angle.is_none() {
            if let Some(deg) = eat_angle_deg(&mut cur, true)? {
                angle = Some(Direction::Angle(deg));
                continue;
            }
            if cur.eat_ident_eq("to") {
                angle = Some(parse_to_side_or_corner(&mut cur)?);
                continue;
            }
        }
        break;
    }

    if angle.is_none() && !saw_in {
        return Ok(None);
    }
    cur.expect_eof()?;
    Ok(Some(angle.unwrap_or(Direction::Angle(180.0))))
}

/// Converts `to <side>` into degrees (CSS convention: 0 = to top, 90 = to
/// right); a corner is kept as one.
fn parse_to_side_or_corner(cur: &mut Cursor<'_>) -> Result<Direction, String> {
    let mut horiz: Option<bool> = None; // true = right, false = left
    let mut vert: Option<bool> = None; // true = bottom, false = top

    for _ in 0..2 {
        let saved = cur.save();
        let Some(id) = cur.eat_ident() else {
            break;
        };
        match id.to_ascii_lowercase().as_str() {
            "left" if horiz.is_none() => horiz = Some(false),
            "right" if horiz.is_none() => horiz = Some(true),
            "top" if vert.is_none() => vert = Some(false),
            "bottom" if vert.is_none() => vert = Some(true),
            "in" => {
                cur.restore(saved);
                break;
            }
            _ => return Err(format!("invalid linear-gradient direction keyword '{id}'")),
        }
    }

    match (horiz, vert) {
        (Some(right), Some(bottom)) => Ok(Direction::Corner { right, bottom }),
        (None, Some(false)) => Ok(Direction::Angle(0.0)),
        (Some(true), None) => Ok(Direction::Angle(90.0)),
        (None, Some(true)) => Ok(Direction::Angle(180.0)),
        (Some(false), None) => Ok(Direction::Angle(270.0)),
        (None, None) => Err("expected side or corner after 'to'".to_owned()),
    }
}

fn parse_radial_gradient(
    args: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
    repeating: bool,
) -> Result<Image, String> {
    let parts = split_comma_separated(args)?;
    if parts.len() < 2 {
        return Err("radial-gradient() requires at least 2 color stops".to_owned());
    }

    let mut circle = false;
    let mut center_x = LengthPercentage::percent(50.0);
    let mut center_y = LengthPercentage::percent(50.0);
    let mut stop_start = 0;

    if let Some((is_circle, cx, cy)) = try_parse_radial_prelude(parts[0], uctx, current_color)? {
        circle = is_circle;
        center_x = cx;
        center_y = cy;
        stop_start = 1;
    }

    let stops = parse_gradient_stops(&parts[stop_start..], uctx, current_color, false)?;
    Ok(Image::RadialGradient {
        circle,
        center_x,
        center_y,
        stops,
        repeating,
    })
}

fn try_parse_radial_prelude(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<Option<(bool, LengthPercentage, LengthPercentage)>, String> {
    // If the first argument begins with a colour (e.g. `radial-gradient(red, blue)`
    // or `radial-gradient(50% red, blue)`), it is a colour stop, not a prelude.
    let mut probe = Cursor::new(cvs);
    if eat_color(&mut probe, current_color)?.is_some() {
        return Ok(None);
    }

    let mut cur = Cursor::new(cvs);
    let mut shape: Option<bool> = None; // Some(true) = circle, Some(false) = ellipse
    let mut size_count: Option<usize> = None; // 0 = extent kw, 1 = 1 length (circle), 2 = 2 lp (ellipse)
    let mut saw_in = false;
    let mut saw_any = false;

    loop {
        if cur.is_eof() {
            break;
        }
        let saved = cur.save();
        if cur.eat_ident_eq("at") {
            cur.restore(saved);
            break;
        }
        if !saw_in && skip_optional_color_space(&mut cur)? {
            saw_in = true;
            saw_any = true;
            continue;
        }
        if shape.is_none() {
            if cur.eat_ident_eq("circle") {
                shape = Some(true);
                saw_any = true;
                continue;
            }
            if cur.eat_ident_eq("ellipse") {
                shape = Some(false);
                saw_any = true;
                continue;
            }
        }
        if size_count.is_none() {
            let before_kw = cur.save();
            if let Some(id) = cur.eat_ident() {
                if matches!(
                    id.to_ascii_lowercase().as_str(),
                    "closest-side" | "farthest-side" | "closest-corner" | "farthest-corner"
                ) {
                    size_count = Some(0);
                    saw_any = true;
                    continue;
                }
                cur.restore(before_kw);
            }
            if let Some(first_len) = eat_length(&mut cur, uctx, true)? {
                let first_is_pct =
                    matches!(first_len, crate::Length::Linear(lp) if lp.percent != 0.0);
                if let Some(_second_len) = eat_length(&mut cur, uctx, true)? {
                    size_count = Some(2);
                } else {
                    if first_is_pct {
                        // A single percentage followed by a colour could be `<pct> <color>` stop!
                        if eat_color(&mut cur, current_color)?.is_some() {
                            return Ok(None);
                        }
                        return Err(
                            "single-radius circle radial-gradient() size cannot be a percentage"
                                .to_owned(),
                        );
                    }
                    size_count = Some(1);
                }
                saw_any = true;
                continue;
            }
        }
        return Err("invalid token in radial-gradient() prelude".to_owned());
    }

    if let (Some(true), Some(2)) = (shape, size_count) {
        return Err("'circle' radial-gradient() cannot take two radii".to_owned());
    }
    if let (Some(false), Some(1)) = (shape, size_count) {
        return Err("'ellipse' radial-gradient() requires two radii".to_owned());
    }

    let mut cx = LengthPercentage::percent(50.0);
    let mut cy = LengthPercentage::percent(50.0);
    if cur.eat_ident_eq("at") {
        let rest = cur.remaining();
        let (px, py) = parse_position_2d(rest, uctx, true)?;
        cx = px;
        cy = py;
        saw_any = true;
    } else {
        cur.expect_eof()?;
    }

    if !saw_any {
        return Ok(None);
    }

    let is_circle = shape.unwrap_or(size_count == Some(1));
    Ok(Some((is_circle, cx, cy)))
}

fn parse_conic_gradient(
    args: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<Image, String> {
    let parts = split_comma_separated(args)?;
    if parts.len() < 2 {
        return Err("conic-gradient() requires at least 2 color stops".to_owned());
    }

    let mut from_deg = 0.0_f32;
    let mut center_x = LengthPercentage::percent(50.0);
    let mut center_y = LengthPercentage::percent(50.0);
    let mut stop_start = 0;

    if let Some((deg, cx, cy)) = try_parse_conic_prelude(parts[0], uctx)? {
        from_deg = deg;
        center_x = cx;
        center_y = cy;
        stop_start = 1;
    }

    let stops = parse_gradient_stops(&parts[stop_start..], uctx, current_color, true)?;
    Ok(Image::ConicGradient {
        from_deg,
        center_x,
        center_y,
        stops,
    })
}

fn try_parse_conic_prelude(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Option<(f32, LengthPercentage, LengthPercentage)>, String> {
    let mut cur = Cursor::new(cvs);
    let mut from_deg: Option<f32> = None;
    let mut saw_in = false;
    let mut saw_any = false;

    loop {
        if cur.is_eof() {
            break;
        }
        let saved = cur.save();
        if cur.eat_ident_eq("at") {
            cur.restore(saved);
            break;
        }
        if !saw_in && skip_optional_color_space(&mut cur)? {
            saw_in = true;
            saw_any = true;
            continue;
        }
        if from_deg.is_none() && cur.eat_ident_eq("from") {
            let deg = eat_angle_deg(&mut cur, true)?
                .ok_or_else(|| "expected <angle> after 'from' in conic-gradient()".to_owned())?;
            from_deg = Some(deg);
            saw_any = true;
            continue;
        }
        if !saw_any {
            return Ok(None);
        }
        return Err("invalid token in conic-gradient() prelude".to_owned());
    }

    let mut cx = LengthPercentage::percent(50.0);
    let mut cy = LengthPercentage::percent(50.0);
    if cur.eat_ident_eq("at") {
        let rest = cur.remaining();
        let (px, py) = parse_position_2d(rest, uctx, true)?;
        cx = px;
        cy = py;
        saw_any = true;
    } else {
        cur.expect_eof()?;
    }

    if !saw_any {
        return Ok(None);
    }
    Ok(Some((from_deg.unwrap_or(0.0), cx, cy)))
}

fn eat_stop_position(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
    angular: bool,
) -> Result<Option<LengthPercentage>, String> {
    if angular {
        if let Some(pct) = cur.eat_percentage() {
            return Ok(Some(LengthPercentage::percent(pct)));
        }
        if let Some(deg) = eat_angle_deg(cur, true)? {
            return Ok(Some(LengthPercentage::percent(deg / 360.0 * 100.0)));
        }
        return Ok(None);
    }
    if let Some(len) = eat_length(cur, uctx, false)? {
        return Ok(Some(length_to_lp(len, uctx, false)?));
    }
    Ok(None)
}

fn parse_gradient_stops(
    parts: &[&[ComponentValue]],
    uctx: &UnitContext,
    current_color: Rgba,
    angular: bool,
) -> Result<Vec<ColorStop>, String> {
    let mut stops = Vec::with_capacity(parts.len());
    let mut stop_items = 0_usize;
    let mut last_was_hint = false;

    for part in parts {
        let mut cur = Cursor::new(part);

        // Try `<color> [<pos>]{0,2}` first.
        if let Some(color) = eat_color(&mut cur, current_color)? {
            let p1 = eat_stop_position(&mut cur, uctx, angular)?;
            let p2 = if p1.is_some() && !cur.is_eof() {
                Some(
                    eat_stop_position(&mut cur, uctx, angular)?
                        .ok_or_else(|| "invalid second color-stop position".to_owned())?,
                )
            } else {
                None
            };
            cur.expect_eof()?;

            match (p1, p2) {
                (None, _) => stops.push(ColorStop {
                    color,
                    position: None,
                }),
                (Some(a), None) => stops.push(ColorStop {
                    color,
                    position: Some(a),
                }),
                (Some(a), Some(b)) => {
                    stops.push(ColorStop {
                        color,
                        position: Some(a),
                    });
                    stops.push(ColorStop {
                        color,
                        position: Some(b),
                    });
                }
            }
            stop_items += 1;
            last_was_hint = false;
            continue;
        }

        // Otherwise `<pos> [<pos>]? <color>` or a bare `<pos>` transition hint.
        let p1 = eat_stop_position(&mut cur, uctx, angular)?
            .ok_or_else(|| "expected color stop in gradient".to_owned())?;
        if cur.is_eof() {
            // Transition hint: must lie strictly between two colour stops.
            if stop_items == 0 || last_was_hint {
                return Err("misplaced gradient transition hint".to_owned());
            }
            last_was_hint = true;
            continue;
        }

        let p2 = eat_stop_position(&mut cur, uctx, angular)?;
        let color = eat_color(&mut cur, current_color)?
            .ok_or_else(|| "expected color in gradient stop".to_owned())?;
        cur.expect_eof()?;

        stops.push(ColorStop {
            color,
            position: Some(p1),
        });
        if let Some(b) = p2 {
            stops.push(ColorStop {
                color,
                position: Some(b),
            });
        }
        stop_items += 1;
        last_was_hint = false;
    }

    if last_was_hint {
        return Err("gradient cannot end with a transition hint".to_owned());
    }
    if stop_items < 2 {
        return Err("gradient requires at least 2 color stops".to_owned());
    }
    Ok(stops)
}

// ---------------------------------------------------------------------------
// `background-size`, `background-repeat`, `background-attachment/origin/clip`,
// and `ComputedStyle::background_layers` synchronisation
// ---------------------------------------------------------------------------

pub(crate) fn parse_bg_size_item(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<BackgroundSize, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("cover") {
        cur.expect_eof()?;
        return Ok(BackgroundSize::Cover);
    }
    if cur.eat_ident_eq("contain") {
        cur.expect_eof()?;
        return Ok(BackgroundSize::Contain);
    }

    let first = eat_length_auto(&mut cur, uctx, true)?
        .ok_or_else(|| "invalid background-size".to_owned())?;
    if cur.is_eof() {
        return match first {
            LengthAuto::Auto => Ok(BackgroundSize::Auto),
            w => Ok(BackgroundSize::Explicit(w, LengthAuto::Auto)),
        };
    }
    let second = eat_length_auto(&mut cur, uctx, true)?
        .ok_or_else(|| "invalid second component in background-size".to_owned())?;
    cur.expect_eof()?;
    if first == LengthAuto::Auto && second == LengthAuto::Auto {
        Ok(BackgroundSize::Auto)
    } else {
        Ok(BackgroundSize::Explicit(first, second))
    }
}

pub(crate) fn parse_bg_size_list(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Vec<BackgroundSize>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        out.push(parse_bg_size_item(part, uctx)?);
    }
    Ok(out)
}

pub(crate) fn eat_bg_repeat_in_shorthand(cur: &mut Cursor<'_>) -> Option<BackgroundRepeat> {
    let saved = cur.save();
    let first = cur.eat_ident()?.to_ascii_lowercase();
    match first.as_str() {
        "repeat-x" => return Some(BackgroundRepeat::RepeatX),
        "repeat-y" => return Some(BackgroundRepeat::RepeatY),
        "repeat" | "space" | "round" | "no-repeat" => {
            let after_first = cur.save();
            if let Some(second) = cur.eat_ident() {
                let s2 = second.to_ascii_lowercase();
                if matches!(s2.as_str(), "repeat" | "space" | "round" | "no-repeat") {
                    return Some(combine_two_repeats(&first, &s2));
                }
                cur.restore(after_first);
            }
            return Some(single_repeat(&first));
        }
        _ => {}
    }
    cur.restore(saved);
    None
}

fn single_repeat(kw: &str) -> BackgroundRepeat {
    match kw {
        "repeat" => BackgroundRepeat::Repeat,
        "no-repeat" => BackgroundRepeat::NoRepeat,
        "space" => BackgroundRepeat::Space,
        "round" => BackgroundRepeat::Round,
        _ => BackgroundRepeat::Repeat,
    }
}

fn combine_two_repeats(rx: &str, ry: &str) -> BackgroundRepeat {
    match (rx, ry) {
        ("repeat", "repeat") => BackgroundRepeat::Repeat,
        ("repeat", "no-repeat") => BackgroundRepeat::RepeatX,
        ("no-repeat", "repeat") => BackgroundRepeat::RepeatY,
        ("no-repeat", "no-repeat") => BackgroundRepeat::NoRepeat,
        ("space", _) | (_, "space") => BackgroundRepeat::Space,
        ("round", _) | (_, "round") => BackgroundRepeat::Round,
        _ => BackgroundRepeat::Repeat,
    }
}

pub(crate) fn parse_bg_repeat_list(
    cvs: &[ComponentValue],
) -> Result<Vec<BackgroundRepeat>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        let rep = eat_bg_repeat_in_shorthand(&mut cur)
            .ok_or_else(|| "invalid background-repeat".to_owned())?;
        cur.expect_eof()?;
        out.push(rep);
    }
    Ok(out)
}

pub(crate) fn parse_bg_attachment_list(
    cvs: &[ComponentValue],
) -> Result<Vec<BackgroundAttachment>, String> {
    let mut out = Vec::new();
    for part in split_comma_separated(cvs)? {
        let mut cur = Cursor::new(part);
        let id = cur
            .eat_ident()
            .ok_or_else(|| "expected background-attachment keyword".to_owned())?;
        out.push(match id.to_ascii_lowercase().as_str() {
            "scroll" => BackgroundAttachment::Scroll,
            "fixed" => BackgroundAttachment::Fixed,
            "local" => BackgroundAttachment::Local,
            _ => return Err(format!("invalid background-attachment '{id}'")),
        });
        cur.expect_eof()?;
    }
    Ok(out)
}

pub(crate) fn parse_bg_origin_list(
    cvs: &[ComponentValue],
) -> Result<Vec<BackgroundOrigin>, String> {
    let mut out = Vec::new();
    for part in split_comma_separated(cvs)? {
        let mut cur = Cursor::new(part);
        let id = cur
            .eat_ident()
            .ok_or_else(|| "expected box keyword".to_owned())?;
        out.push(match id.to_ascii_lowercase().as_str() {
            "border-box" => BackgroundOrigin::BorderBox,
            "padding-box" => BackgroundOrigin::PaddingBox,
            "content-box" => BackgroundOrigin::ContentBox,
            _ => return Err(format!("invalid background box '{id}'")),
        });
        cur.expect_eof()?;
    }
    Ok(out)
}

pub(crate) fn parse_bg_clip_list(cvs: &[ComponentValue]) -> Result<Vec<BackgroundClip>, String> {
    let mut out = Vec::new();
    for part in split_comma_separated(cvs)? {
        let mut cur = Cursor::new(part);
        let id = cur
            .eat_ident()
            .ok_or_else(|| "expected box keyword".to_owned())?;
        out.push(match id.to_ascii_lowercase().as_str() {
            "border-box" => BackgroundClip::BorderBox,
            "padding-box" => BackgroundClip::PaddingBox,
            "content-box" => BackgroundClip::ContentBox,
            "text" => BackgroundClip::Text,
            _ => return Err(format!("invalid background box '{id}'")),
        });
        cur.expect_eof()?;
    }
    Ok(out)
}

/// Applies a parsed `background-image` list to `style.background_layers` (CSS Backgrounds 3 §2.1:
/// `background-image` determines the layer count).
pub(crate) fn apply_background_images(style: &mut ComputedStyle, images: Vec<Option<Image>>) {
    if images.len() == 1 && images[0].is_none() {
        style.background_layers.clear();
        return;
    }
    let old = std::mem::take(&mut style.background_layers);
    let mut layers = Vec::with_capacity(images.len());
    for (i, img) in images.into_iter().enumerate() {
        let mut layer = if old.is_empty() {
            BackgroundLayer::default()
        } else {
            let mut base = old[i % old.len()].clone();
            base.image = None;
            base
        };
        layer.image = img;
        layers.push(layer);
    }
    style.background_layers = layers;
}

/// Applies a parallel `background-*` list cyclically across `style.background_layers`.
/// When `background_layers` is currently empty (no `background-image` set), keeps
/// it empty if every item equals `default_val`, and otherwise materialises image-less
/// layers so standalone `compute` calls remain observable.
pub(crate) fn apply_background_prop<T: Clone + PartialEq>(
    style: &mut ComputedStyle,
    values: &[T],
    default_val: &T,
    mut setter: impl FnMut(&mut BackgroundLayer, T),
) {
    if values.is_empty() {
        return;
    }
    if style.background_layers.is_empty() {
        if values.iter().all(|v| v == default_val) {
            return;
        }
        style.background_layers = values
            .iter()
            .cloned()
            .map(|v| {
                let mut layer = BackgroundLayer::default();
                setter(&mut layer, v);
                layer
            })
            .collect();
        return;
    }
    for (i, layer) in style.background_layers.iter_mut().enumerate() {
        setter(layer, values[i % values.len()].clone());
    }
    if style
        .background_layers
        .iter()
        .all(|l| *l == BackgroundLayer::default())
    {
        style.background_layers.clear();
    }
}

pub(crate) fn copy_background_field<T: Clone + PartialEq>(
    dst: &mut ComputedStyle,
    src: &ComputedStyle,
    default_val: T,
    getter: impl Fn(&BackgroundLayer) -> T,
    setter: impl FnMut(&mut BackgroundLayer, T),
) {
    if src.background_layers.is_empty() {
        apply_background_prop(
            dst,
            std::slice::from_ref(&default_val),
            &default_val,
            setter,
        );
    } else {
        let vals: Vec<T> = src.background_layers.iter().map(getter).collect();
        apply_background_prop(dst, &vals, &default_val, setter);
    }
}

// ---------------------------------------------------------------------------
// `box-shadow` & `text-shadow`
// ---------------------------------------------------------------------------

pub(crate) fn parse_shadow_list(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
    allow_inset_and_spread: bool,
) -> Result<Vec<Shadow>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(Vec::new());
    }

    let parts = split_comma_separated(cvs)?;
    let mut shadows = Vec::with_capacity(parts.len());

    for part in parts {
        let mut sc = Cursor::new(part);
        let mut inset = false;
        let mut color: Option<Rgba> = None;
        let mut lengths: Option<(f32, f32, f32, f32)> = None;

        while !sc.is_eof() {
            if allow_inset_and_spread && sc.eat_ident_eq("inset") {
                if inset {
                    return Err("duplicate 'inset' in box-shadow".to_owned());
                }
                inset = true;
                continue;
            }
            if lengths.is_none()
                && let Some(ox) = eat_px(&mut sc, uctx, false)?
            {
                let oy = eat_px(&mut sc, uctx, false)?
                    .ok_or_else(|| "shadow requires at least 2 offset lengths".to_owned())?;
                // 3rd length (blur radius) must be non-negative per CSS Backgrounds 3 §7.1.
                let blur = eat_px(&mut sc, uctx, true)?.unwrap_or(0.0);
                let spread = if allow_inset_and_spread {
                    eat_px(&mut sc, uctx, false)?.unwrap_or(0.0)
                } else {
                    0.0
                };
                lengths = Some((ox, oy, blur, spread));
                continue;
            }
            if color.is_none()
                && let Some(c) = eat_color(&mut sc, current_color)?
            {
                color = Some(c);
                continue;
            }
            return Err("invalid token in shadow declaration".to_owned());
        }

        let (offset_x, offset_y, blur, spread) =
            lengths.ok_or_else(|| "shadow requires offset-x and offset-y lengths".to_owned())?;
        shadows.push(Shadow {
            offset_x,
            offset_y,
            blur,
            spread,
            color: color.unwrap_or(current_color),
            inset,
        });
    }

    Ok(shadows)
}

// ---------------------------------------------------------------------------
// `opacity`, `overflow`, `object-fit`, `filter`
// ---------------------------------------------------------------------------

pub(crate) fn parse_opacity(cvs: &[ComponentValue]) -> Result<f32, String> {
    let mut cur = Cursor::new(cvs);
    let v = eat_number_or_percentage(&mut cur, false)?
        .ok_or_else(|| "expected <number> or <percentage> for opacity".to_owned())?;
    cur.expect_eof()?;
    Ok(v.clamp(0.0, 1.0))
}

pub(crate) fn eat_overflow_kw(cur: &mut Cursor<'_>) -> Option<Overflow> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    let ov = match id.to_ascii_lowercase().as_str() {
        "visible" => Overflow::Visible,
        "hidden" => Overflow::Hidden,
        "clip" => Overflow::Clip,
        "scroll" => Overflow::Scroll,
        "auto" | "overlay" => Overflow::Auto,
        _ => {
            cur.restore(saved);
            return None;
        }
    };
    Some(ov)
}

pub(crate) fn parse_overflow(cvs: &[ComponentValue]) -> Result<Overflow, String> {
    let mut cur = Cursor::new(cvs);
    let ov = eat_overflow_kw(&mut cur).ok_or_else(|| "expected overflow keyword".to_owned())?;
    cur.expect_eof()?;
    Ok(ov)
}

pub(crate) fn parse_object_fit(cvs: &[ComponentValue]) -> Result<ObjectFit, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected object-fit keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "fill" => Ok(ObjectFit::Fill),
        "contain" => Ok(ObjectFit::Contain),
        "cover" => Ok(ObjectFit::Cover),
        "none" => Ok(ObjectFit::None),
        "scale-down" => Ok(ObjectFit::ScaleDown),
        _ => Err(format!("invalid object-fit '{id}'")),
    }
}

pub(crate) fn parse_filter(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<Option<String>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(None);
    }

    let mut count = 0_usize;
    while !cur.is_eof() {
        if cur.eat_url()?.is_some() {
            count += 1;
            continue;
        }
        let (fname, fargs) = cur
            .eat_function()
            .ok_or_else(|| "expected filter function or 'none'".to_owned())?;
        validate_filter_fn(fname, fargs, uctx, current_color)?;
        count += 1;
    }
    if count == 0 {
        return Err("empty filter value".to_owned());
    }
    Ok(Some(serialize_cvs(cvs)))
}

fn validate_filter_fn(
    fname: &str,
    fargs: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<(), String> {
    let mut cur = Cursor::new(fargs);
    match fname.to_ascii_lowercase().as_str() {
        "blur" => {
            if !cur.is_eof() {
                let _ = eat_px(&mut cur, uctx, true)?
                    .ok_or_else(|| "blur() requires a non-negative <length>".to_owned())?;
                cur.expect_eof()?;
            }
            Ok(())
        }
        "brightness" | "contrast" | "grayscale" | "invert" | "opacity" | "saturate" | "sepia" => {
            if !cur.is_eof() {
                let _ = eat_number_or_percentage(&mut cur, true)?.ok_or_else(|| {
                    format!("{fname}() requires a non-negative <number> or <percentage>")
                })?;
                cur.expect_eof()?;
            }
            Ok(())
        }
        "hue-rotate" => {
            if !cur.is_eof() {
                let _ = eat_angle_deg(&mut cur, true)?
                    .ok_or_else(|| "hue-rotate() requires an <angle>".to_owned())?;
                cur.expect_eof()?;
            }
            Ok(())
        }
        "drop-shadow" => {
            let shadows = parse_shadow_list(fargs, uctx, current_color, false)?;
            if shadows.len() != 1 {
                return Err("drop-shadow() takes a single shadow".to_owned());
            }
            Ok(())
        }
        _ => Err(format!("unknown filter function '{fname}()'")),
    }
}

pub(crate) fn parse_background_color(
    cvs: &[ComponentValue],
    current_color: Rgba,
) -> Result<Rgba, String> {
    parse_color_cvs(cvs, current_color)
}
