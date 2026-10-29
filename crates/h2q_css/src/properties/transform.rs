//! CSS Transforms 1 & 2: `transform`, `transform-origin`, and individual
//! transform properties (`translate`, `rotate`, `scale`).

use swc_css_ast::ComponentValue;

use crate::{LengthPercentage, TransformFn};

use super::{
    background::parse_position_2d,
    cursor::{Cursor, is_ws, serialize_cv, serialize_cvs, split_comma_separated},
    length::{
        UnitContext, eat_angle_deg, eat_length, eat_number_or_calc, eat_number_or_percentage,
        eat_px, length_to_lp, parse_number_cvs, parse_px_cvs,
    },
};

/// Parses the `transform` property (`none | <transform-function>+`).
pub(crate) fn parse_transform_list(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Vec<TransformFn>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    while !cur.is_eof() {
        let cv = cur
            .peek()
            .ok_or_else(|| "expected transform function".to_owned())?;
        let (fname, fargs) = cur
            .eat_function()
            .ok_or_else(|| "expected transform function or 'none'".to_owned())?;
        out.push(parse_transform_fn(cv, fname, fargs, uctx)?);
    }

    if out.is_empty() {
        return Err("empty transform value".to_owned());
    }
    Ok(out)
}

fn parse_transform_fn(
    raw_cv: &ComponentValue,
    fname: &str,
    fargs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<TransformFn, String> {
    match fname.to_ascii_lowercase().as_str() {
        "translate" => {
            let parts = split_comma_separated(fargs)?;
            if !(1..=2).contains(&parts.len()) {
                return Err("translate() takes 1 or 2 arguments".to_owned());
            }
            let tx = parse_lp_arg(parts[0], uctx)?;
            let ty = if parts.len() == 2 {
                parse_lp_arg(parts[1], uctx)?
            } else {
                LengthPercentage::ZERO
            };
            Ok(TransformFn::Translate(tx, ty))
        }
        "translatex" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("translateX() takes 1 argument".to_owned());
            }
            let tx = parse_lp_arg(parts[0], uctx)?;
            Ok(TransformFn::Translate(tx, LengthPercentage::ZERO))
        }
        "translatey" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("translateY() takes 1 argument".to_owned());
            }
            let ty = parse_lp_arg(parts[0], uctx)?;
            Ok(TransformFn::Translate(LengthPercentage::ZERO, ty))
        }
        "translate3d" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 3 {
                return Err("translate3d() takes 3 arguments".to_owned());
            }
            let tx = parse_lp_arg(parts[0], uctx)?;
            let ty = parse_lp_arg(parts[1], uctx)?;
            // Z translation must be a `<length>` (no `%`), then flattened away.
            let _tz = parse_px_cvs(parts[2], uctx, false)?;
            Ok(TransformFn::Translate(tx, ty))
        }
        "scale" => {
            let parts = split_comma_separated(fargs)?;
            if !(1..=2).contains(&parts.len()) {
                return Err("scale() takes 1 or 2 arguments".to_owned());
            }
            let sx = parse_scale_arg(parts[0])?;
            let sy = if parts.len() == 2 {
                parse_scale_arg(parts[1])?
            } else {
                sx
            };
            Ok(TransformFn::Scale(sx, sy))
        }
        "scalex" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("scaleX() takes 1 argument".to_owned());
            }
            let sx = parse_scale_arg(parts[0])?;
            Ok(TransformFn::Scale(sx, 1.0))
        }
        "scaley" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("scaleY() takes 1 argument".to_owned());
            }
            let sy = parse_scale_arg(parts[0])?;
            Ok(TransformFn::Scale(1.0, sy))
        }
        "scale3d" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 3 {
                return Err("scale3d() takes 3 arguments".to_owned());
            }
            let sx = parse_scale_arg(parts[0])?;
            let sy = parse_scale_arg(parts[1])?;
            let _sz = parse_scale_arg(parts[2])?;
            Ok(TransformFn::Scale(sx, sy))
        }
        "rotate" | "rotatez" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err(format!("{fname}() takes 1 argument"));
            }
            let deg = parse_angle_arg(parts[0])?;
            Ok(TransformFn::Rotate(deg))
        }
        "skew" => {
            let parts = split_comma_separated(fargs)?;
            if !(1..=2).contains(&parts.len()) {
                return Err("skew() takes 1 or 2 arguments".to_owned());
            }
            let ax = parse_angle_arg(parts[0])?;
            let ay = if parts.len() == 2 {
                parse_angle_arg(parts[1])?
            } else {
                0.0
            };
            Ok(TransformFn::Skew(ax, ay))
        }
        "skewx" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("skewX() takes 1 argument".to_owned());
            }
            let ax = parse_angle_arg(parts[0])?;
            Ok(TransformFn::Skew(ax, 0.0))
        }
        "skewy" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("skewY() takes 1 argument".to_owned());
            }
            let ay = parse_angle_arg(parts[0])?;
            Ok(TransformFn::Skew(0.0, ay))
        }
        "matrix" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 6 {
                return Err("matrix() takes 6 arguments".to_owned());
            }
            let mut m = [0.0_f32; 6];
            for (dst, part) in m.iter_mut().zip(parts) {
                *dst = parse_number_cvs(part, false)?;
            }
            Ok(TransformFn::Matrix(m))
        }
        // Valid 3-D transform functions that cannot be represented in 2-D QML transforms
        // are validated for syntactic correctness and preserved as `Unsupported`.
        "translatez" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("translateZ() takes 1 argument".to_owned());
            }
            let _ = parse_px_cvs(parts[0], uctx, false)?;
            Ok(TransformFn::Unsupported(serialize_cv(raw_cv)))
        }
        "scalez" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("scaleZ() takes 1 argument".to_owned());
            }
            let _ = parse_scale_arg(parts[0])?;
            Ok(TransformFn::Unsupported(serialize_cv(raw_cv)))
        }
        "rotatex" | "rotatey" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err(format!("{fname}() takes 1 argument"));
            }
            let _ = parse_angle_arg(parts[0])?;
            Ok(TransformFn::Unsupported(serialize_cv(raw_cv)))
        }
        "rotate3d" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 4 {
                return Err("rotate3d() takes 4 arguments".to_owned());
            }
            for part in &parts[..3] {
                let _ = parse_number_cvs(part, false)?;
            }
            let _ = parse_angle_arg(parts[3])?;
            Ok(TransformFn::Unsupported(serialize_cv(raw_cv)))
        }
        "matrix3d" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 16 {
                return Err("matrix3d() takes 16 arguments".to_owned());
            }
            for part in parts {
                let _ = parse_number_cvs(part, false)?;
            }
            Ok(TransformFn::Unsupported(serialize_cv(raw_cv)))
        }
        "perspective" => {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 1 {
                return Err("perspective() takes 1 argument".to_owned());
            }
            let mut pcur = Cursor::new(parts[0]);
            if pcur.eat_ident_eq("none") {
                pcur.expect_eof()?;
            } else {
                let _ = parse_px_cvs(parts[0], uctx, true)?;
            }
            Ok(TransformFn::Unsupported(serialize_cv(raw_cv)))
        }
        _ => Err(format!("unknown transform function '{fname}()'")),
    }
}

fn parse_lp_arg(cvs: &[ComponentValue], uctx: &UnitContext) -> Result<LengthPercentage, String> {
    let mut cur = Cursor::new(cvs);
    let len = eat_length(&mut cur, uctx, false)?
        .ok_or_else(|| "expected <length-percentage> in transform".to_owned())?;
    cur.expect_eof()?;
    length_to_lp(len, uctx, false)
}

fn parse_scale_arg(cvs: &[ComponentValue]) -> Result<f32, String> {
    let mut cur = Cursor::new(cvs);
    let v = eat_number_or_percentage(&mut cur, false)?
        .ok_or_else(|| "expected <number> or <percentage> in scale".to_owned())?;
    cur.expect_eof()?;
    Ok(v)
}

fn parse_angle_arg(cvs: &[ComponentValue]) -> Result<f32, String> {
    let mut cur = Cursor::new(cvs);
    let deg = eat_angle_deg(&mut cur, true)?
        .ok_or_else(|| "expected <angle> in transform".to_owned())?;
    cur.expect_eof()?;
    Ok(deg)
}

/// Parses the individual `translate` property (`none | <length-percentage> [ <length-percentage> <length>? ]?`).
pub(crate) fn parse_individual_translate(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Option<TransformFn>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(None);
    }

    let tx_len = eat_length(&mut cur, uctx, false)?
        .ok_or_else(|| "expected <length-percentage> for translate".to_owned())?;
    let tx = length_to_lp(tx_len, uctx, false)?;

    if cur.is_eof() {
        return Ok(Some(TransformFn::Translate(tx, LengthPercentage::ZERO)));
    }

    let ty_len = eat_length(&mut cur, uctx, false)?
        .ok_or_else(|| "expected second <length-percentage> for translate".to_owned())?;
    let ty = length_to_lp(ty_len, uctx, false)?;

    if !cur.is_eof() {
        let _tz = eat_px(&mut cur, uctx, false)?
            .ok_or_else(|| "expected <length> for 3rd translate component".to_owned())?;
    }
    cur.expect_eof()?;
    Ok(Some(TransformFn::Translate(tx, ty)))
}

/// Parses the individual `rotate` property (`none | <angle> | [ x | y | z | <number>{3} ] && <angle>`).
pub(crate) fn parse_individual_rotate(
    cvs: &[ComponentValue],
) -> Result<Option<TransformFn>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(None);
    }

    // 1. Single `<angle | zero>`
    let saved = cur.save();
    if let Ok(Some(deg)) = eat_angle_deg(&mut cur, true)
        && cur.is_eof()
    {
        return Ok(Some(TransformFn::Rotate(deg)));
    }
    cur.restore(saved);

    // 2. `[ x | y | z ] <angle | zero>`
    if let Some(axis) = eat_axis_kw(&mut cur) {
        let deg = eat_angle_deg(&mut cur, true)?
            .ok_or_else(|| "expected <angle> after axis keyword in rotate".to_owned())?;
        cur.expect_eof()?;
        return Ok(Some(rotate_for_axis_kw(axis, deg, cvs)));
    }
    cur.restore(saved);

    // 3. `<angle | zero> [ x | y | z ]`
    if let Ok(Some(deg)) = eat_angle_deg(&mut cur, true)
        && let Some(axis) = eat_axis_kw(&mut cur)
    {
        cur.expect_eof()?;
        return Ok(Some(rotate_for_axis_kw(axis, deg, cvs)));
    }
    cur.restore(saved);

    // 4. `<number>{3} <angle | zero>` (checked before `<angle | zero> <number>{3}` so a leading
    //    `0` in `0 0 1 45deg` is parsed as the X vector component rather than a unitless-0 angle).
    if let (Ok(Some(x)), Ok(Some(y)), Ok(Some(z))) = (
        eat_number_or_calc(&mut cur, false),
        eat_number_or_calc(&mut cur, false),
        eat_number_or_calc(&mut cur, false),
    ) && let Ok(Some(deg)) = eat_angle_deg(&mut cur, true)
        && cur.is_eof()
    {
        return Ok(Some(rotate_for_vector(x, y, z, deg, cvs)));
    }
    cur.restore(saved);

    // 5. `<angle | zero> <number>{3}`
    if let Ok(Some(deg)) = eat_angle_deg(&mut cur, true)
        && let (Ok(Some(x)), Ok(Some(y)), Ok(Some(z))) = (
            eat_number_or_calc(&mut cur, false),
            eat_number_or_calc(&mut cur, false),
            eat_number_or_calc(&mut cur, false),
        )
        && cur.is_eof()
    {
        return Ok(Some(rotate_for_vector(x, y, z, deg, cvs)));
    }

    Err("invalid rotate property value".to_owned())
}

fn eat_axis_kw(cur: &mut Cursor<'_>) -> Option<char> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    match id.to_ascii_lowercase().as_str() {
        "x" => Some('x'),
        "y" => Some('y'),
        "z" => Some('z'),
        _ => {
            cur.restore(saved);
            None
        }
    }
}

fn rotate_for_axis_kw(axis: char, deg: f32, cvs: &[ComponentValue]) -> TransformFn {
    if axis == 'z' || deg == 0.0 {
        TransformFn::Rotate(deg)
    } else {
        TransformFn::Unsupported(format!("rotate({})", serialize_cvs(cvs)))
    }
}

fn rotate_for_vector(x: f32, y: f32, z: f32, deg: f32, cvs: &[ComponentValue]) -> TransformFn {
    if x == 0.0 && y == 0.0 {
        if z > 0.0 {
            TransformFn::Rotate(deg)
        } else if z < 0.0 {
            TransformFn::Rotate(-deg)
        } else {
            TransformFn::Rotate(0.0)
        }
    } else if deg == 0.0 {
        TransformFn::Rotate(0.0)
    } else {
        TransformFn::Unsupported(format!("rotate({})", serialize_cvs(cvs)))
    }
}

/// Parses the individual `scale` property (`none | [ <number> | <percentage> ]{1,3}`).
pub(crate) fn parse_individual_scale(
    cvs: &[ComponentValue],
) -> Result<Option<TransformFn>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(None);
    }

    let sx = eat_number_or_percentage(&mut cur, false)?
        .ok_or_else(|| "expected <number> or <percentage> for scale".to_owned())?;
    if cur.is_eof() {
        return Ok(Some(TransformFn::Scale(sx, sx)));
    }

    let sy = eat_number_or_percentage(&mut cur, false)?
        .ok_or_else(|| "invalid second component in scale".to_owned())?;
    if !cur.is_eof() {
        let _sz = eat_number_or_percentage(&mut cur, false)?
            .ok_or_else(|| "invalid third component in scale".to_owned())?;
    }
    cur.expect_eof()?;
    Ok(Some(TransformFn::Scale(sx, sy)))
}

/// Parses `transform-origin` (1 or 2 2-D origin values, plus an optional 3rd `<length>` Z offset).
pub(crate) fn parse_transform_origin(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<(LengthPercentage, LengthPercentage), String> {
    let non_ws_indices: Vec<usize> = cvs
        .iter()
        .enumerate()
        .filter_map(|(i, cv)| (!is_ws(cv)).then_some(i))
        .collect();

    match non_ws_indices.as_slice() {
        [_] | [_, _] => parse_position_2d(cvs, uctx, false),
        [_, _, third_idx] => {
            let xy = parse_position_2d(&cvs[..*third_idx], uctx, false)?;
            let _z = parse_px_cvs(&cvs[*third_idx..], uctx, false)?;
            Ok(xy)
        }
        _ => Err("invalid transform-origin".to_owned()),
    }
}
