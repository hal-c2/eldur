//! CSS Values and Units 4: `<length>`, `<percentage>`, `<length-percentage>`,
//! `<angle>`, `<time>`, `<number>`, `<integer>`, and math functions
//! (`calc()`, `min()`, `max()`, `clamp()`).

use std::f32::consts::{E, PI};

use swc_css_ast::ComponentValue;

use crate::{CalcNode, ComputedStyle, Length, LengthAuto, LengthPercentage, LineHeight};

use super::{
    ComputeContext,
    cursor::{Cursor, MAX_NESTING_DEPTH, is_ws, split_comma_separated},
};

/// Environment for resolving relative length units (`em`, `rem`, `lh`, `vmin`, `%`).
#[derive(Clone, Copy)]
pub(crate) struct UnitContext {
    pub em_base: f32,
    pub rem_base: f32,
    pub lh_base: f32,
    pub rlh_base: f32,
    pub viewport_w: f32,
    pub viewport_h: f32,
    /// When `Some(base)`, percentages resolve immediately to `px` (`p / 100 * base`).
    /// When `None`, percentages stay in [`LengthPercentage::percent`].
    pub percent_base: Option<f32>,
    pub allow_percent: bool,
}

impl UnitContext {
    /// Context for normal element properties (`em` uses the element's own `font-size`).
    pub(crate) fn for_element(
        style: &ComputedStyle,
        ctx: &ComputeContext<'_>,
        allow_percent: bool,
    ) -> Self {
        Self {
            em_base: style.font_size,
            rem_base: ctx.root_font_size,
            lh_base: resolved_line_height_px(style),
            rlh_base: 1.2 * ctx.root_font_size,
            viewport_w: ctx.viewport.width,
            viewport_h: ctx.viewport.height,
            percent_base: None,
            allow_percent,
        }
    }

    /// Context for computing `font-size` itself (`em`, `lh`, and `%` resolve against the parent).
    pub(crate) fn for_font_size(ctx: &ComputeContext<'_>) -> Self {
        let parent_fs = ctx.parent.map_or(16.0, |p| p.font_size);
        let parent_lh = ctx.parent.map_or(1.2 * 16.0, resolved_line_height_px);
        Self {
            em_base: parent_fs,
            rem_base: ctx.root_font_size,
            lh_base: parent_lh,
            rlh_base: 1.2 * ctx.root_font_size,
            viewport_w: ctx.viewport.width,
            viewport_h: ctx.viewport.height,
            percent_base: Some(parent_fs),
            allow_percent: true,
        }
    }

    /// Context for computing `line-height` (`em` and `%` resolve against the element's `font-size`;
    /// `lh` resolves against the parent's `line-height`).
    pub(crate) fn for_line_height(style: &ComputedStyle, ctx: &ComputeContext<'_>) -> Self {
        let parent_lh = ctx.parent.map_or(1.2 * 16.0, resolved_line_height_px);
        Self {
            em_base: style.font_size,
            rem_base: ctx.root_font_size,
            lh_base: parent_lh,
            rlh_base: 1.2 * ctx.root_font_size,
            viewport_w: ctx.viewport.width,
            viewport_h: ctx.viewport.height,
            percent_base: Some(style.font_size),
            allow_percent: true,
        }
    }

    /// Context for computing `vertical-align` (`%` resolves against the element's `line-height`).
    pub(crate) fn for_vertical_align(style: &ComputedStyle, ctx: &ComputeContext<'_>) -> Self {
        let lh = resolved_line_height_px(style);
        Self {
            em_base: style.font_size,
            rem_base: ctx.root_font_size,
            lh_base: lh,
            rlh_base: 1.2 * ctx.root_font_size,
            viewport_w: ctx.viewport.width,
            viewport_h: ctx.viewport.height,
            percent_base: Some(lh),
            allow_percent: true,
        }
    }
}

pub(crate) fn resolved_line_height_px(style: &ComputedStyle) -> f32 {
    match style.line_height {
        LineHeight::Normal => 1.2 * style.font_size,
        LineHeight::Number(n) => n * style.font_size,
        LineHeight::Px(px) => px,
    }
}

/// Resolves a `<dimension>` unit to a linear [`LengthPercentage`].
pub(crate) fn resolve_length_unit(
    val: f32,
    unit: &str,
    uctx: &UnitContext,
) -> Result<LengthPercentage, String> {
    if !val.is_finite() {
        return Err("non-finite length value".to_owned());
    }
    match unit.to_ascii_lowercase().as_str() {
        "px" => Ok(LengthPercentage::px(val)),
        "cm" => Ok(LengthPercentage::px(val * (96.0 / 2.54))),
        "mm" => Ok(LengthPercentage::px(val * (96.0 / 25.4))),
        "q" => Ok(LengthPercentage::px(val * (96.0 / 101.6))),
        "in" => Ok(LengthPercentage::px(val * 96.0)),
        "pt" => Ok(LengthPercentage::px(val * (96.0 / 72.0))),
        "pc" => Ok(LengthPercentage::px(val * 16.0)),
        "em" => Ok(LengthPercentage::px(val * uctx.em_base)),
        "rem" => Ok(LengthPercentage::px(val * uctx.rem_base)),
        "ex" | "ch" => Ok(LengthPercentage::px(val * 0.5 * uctx.em_base)),
        "cap" => Ok(LengthPercentage::px(val * 0.7 * uctx.em_base)),
        "ic" => Ok(LengthPercentage::px(val * uctx.em_base)),
        "lh" => Ok(LengthPercentage::px(val * uctx.lh_base)),
        "rlh" => Ok(LengthPercentage::px(val * uctx.rlh_base)),
        "rex" | "rch" => Ok(LengthPercentage::px(val * 0.5 * uctx.rem_base)),
        "rcap" => Ok(LengthPercentage::px(val * 0.7 * uctx.rem_base)),
        "ric" => Ok(LengthPercentage::px(val * uctx.rem_base)),
        "vw" | "svw" | "lvw" | "dvw" | "vi" | "svi" | "lvi" | "dvi" => {
            Ok(LengthPercentage::vw(val))
        }
        "vh" | "svh" | "lvh" | "dvh" | "vb" | "svb" | "lvb" | "dvb" => {
            Ok(LengthPercentage::vh(val))
        }
        "vmin" | "svmin" | "lvmin" | "dvmin" => {
            if uctx.viewport_w <= uctx.viewport_h {
                Ok(LengthPercentage::vw(val))
            } else {
                Ok(LengthPercentage::vh(val))
            }
        }
        "vmax" | "svmax" | "lvmax" | "dvmax" => {
            if uctx.viewport_w >= uctx.viewport_h {
                Ok(LengthPercentage::vw(val))
            } else {
                Ok(LengthPercentage::vh(val))
            }
        }
        _ => Err(format!("unknown length unit '{unit}'")),
    }
}

pub(crate) fn resolve_percentage(pct: f32, uctx: &UnitContext) -> Result<LengthPercentage, String> {
    if !uctx.allow_percent {
        return Err("percentage is not allowed here".to_owned());
    }
    if !pct.is_finite() {
        return Err("non-finite percentage".to_owned());
    }
    if let Some(base) = uctx.percent_base {
        Ok(LengthPercentage::px(pct / 100.0 * base))
    } else {
        Ok(LengthPercentage::percent(pct))
    }
}

pub(crate) fn is_math_fn(name: &str) -> bool {
    name.eq_ignore_ascii_case("calc")
        || name.eq_ignore_ascii_case("min")
        || name.eq_ignore_ascii_case("max")
        || name.eq_ignore_ascii_case("clamp")
}

/// Typed intermediate value produced when evaluating `calc()`/`min()`/`max()`/`clamp()`.
#[derive(Debug, Clone)]
enum CalcVal {
    Number(f32),
    Dim(CalcNode),
    AngleDeg(f32),
    TimeMs(f32),
}

fn eval_math_function(
    name: &str,
    args: &[ComponentValue],
    uctx: Option<&UnitContext>,
    depth: usize,
) -> Result<CalcVal, String> {
    if depth > MAX_NESTING_DEPTH {
        return Err("calc() nesting too deep".to_owned());
    }
    if name.eq_ignore_ascii_case("calc") {
        parse_calc_sum(args, uctx, depth + 1)
    } else if name.eq_ignore_ascii_case("min") || name.eq_ignore_ascii_case("max") {
        let is_min = name.eq_ignore_ascii_case("min");
        let parts = split_comma_separated(args)?;
        let mut vals = Vec::with_capacity(parts.len());
        for part in parts {
            vals.push(parse_calc_sum(part, uctx, depth + 1)?);
        }
        combine_min_max(vals, is_min)
    } else if name.eq_ignore_ascii_case("clamp") {
        let parts = split_comma_separated(args)?;
        if parts.len() != 3 {
            return Err("clamp() requires exactly 3 arguments".to_owned());
        }
        let lo = parse_calc_sum(parts[0], uctx, depth + 1)?;
        let val = parse_calc_sum(parts[1], uctx, depth + 1)?;
        let hi = parse_calc_sum(parts[2], uctx, depth + 1)?;
        combine_clamp(lo, val, hi)
    } else {
        Err(format!("unsupported math function {name}()"))
    }
}

fn combine_min_max(vals: Vec<CalcVal>, is_min: bool) -> Result<CalcVal, String> {
    let first = vals.first().ok_or_else(|| "empty min()/max()".to_owned())?;
    match first {
        CalcVal::Number(_) => {
            let mut acc = if is_min {
                f32::INFINITY
            } else {
                f32::NEG_INFINITY
            };
            for v in vals {
                let CalcVal::Number(n) = v else {
                    return Err("mismatched types in min()/max()".to_owned());
                };
                acc = if is_min { acc.min(n) } else { acc.max(n) };
            }
            Ok(CalcVal::Number(acc))
        }
        CalcVal::AngleDeg(_) => {
            let mut acc = if is_min {
                f32::INFINITY
            } else {
                f32::NEG_INFINITY
            };
            for v in vals {
                let CalcVal::AngleDeg(n) = v else {
                    return Err("mismatched types in min()/max()".to_owned());
                };
                acc = if is_min { acc.min(n) } else { acc.max(n) };
            }
            Ok(CalcVal::AngleDeg(acc))
        }
        CalcVal::TimeMs(_) => {
            let mut acc = if is_min {
                f32::INFINITY
            } else {
                f32::NEG_INFINITY
            };
            for v in vals {
                let CalcVal::TimeMs(n) = v else {
                    return Err("mismatched types in min()/max()".to_owned());
                };
                acc = if is_min { acc.min(n) } else { acc.max(n) };
            }
            Ok(CalcVal::TimeMs(acc))
        }
        CalcVal::Dim(_) => {
            let mut nodes = Vec::with_capacity(vals.len());
            let mut all_px = true;
            let mut acc_px = if is_min {
                f32::INFINITY
            } else {
                f32::NEG_INFINITY
            };
            for v in vals {
                let CalcVal::Dim(node) = v else {
                    return Err("mismatched types in min()/max()".to_owned());
                };
                if let CalcNode::Leaf(lp) = &node
                    && let Some(px) = lp.as_px()
                {
                    acc_px = if is_min {
                        acc_px.min(px)
                    } else {
                        acc_px.max(px)
                    };
                } else {
                    all_px = false;
                }
                nodes.push(node);
            }
            if all_px {
                Ok(CalcVal::Dim(CalcNode::Leaf(LengthPercentage::px(acc_px))))
            } else if is_min {
                Ok(CalcVal::Dim(CalcNode::Min(nodes)))
            } else {
                Ok(CalcVal::Dim(CalcNode::Max(nodes)))
            }
        }
    }
}

fn combine_clamp(lo: CalcVal, val: CalcVal, hi: CalcVal) -> Result<CalcVal, String> {
    // CSS Values 4 §10.5 defines `clamp(MIN, VAL, MAX)` as `max(MIN, min(VAL, MAX))`.
    match (lo, val, hi) {
        (CalcVal::Number(a), CalcVal::Number(b), CalcVal::Number(c)) => {
            Ok(CalcVal::Number(a.max(b.min(c))))
        }
        (CalcVal::AngleDeg(a), CalcVal::AngleDeg(b), CalcVal::AngleDeg(c)) => {
            Ok(CalcVal::AngleDeg(a.max(b.min(c))))
        }
        (CalcVal::TimeMs(a), CalcVal::TimeMs(b), CalcVal::TimeMs(c)) => {
            Ok(CalcVal::TimeMs(a.max(b.min(c))))
        }
        (CalcVal::Dim(a), CalcVal::Dim(b), CalcVal::Dim(c)) => {
            if let (CalcNode::Leaf(la), CalcNode::Leaf(lb), CalcNode::Leaf(lc)) = (&a, &b, &c)
                && let (Some(pa), Some(pb), Some(pc)) = (la.as_px(), lb.as_px(), lc.as_px())
            {
                return Ok(CalcVal::Dim(CalcNode::Leaf(LengthPercentage::px(
                    pa.max(pb.min(pc)),
                ))));
            }
            Ok(CalcVal::Dim(CalcNode::Clamp(
                Box::new(a),
                Box::new(b),
                Box::new(c),
            )))
        }
        _ => Err("mismatched argument types in clamp()".to_owned()),
    }
}

fn parse_calc_sum(
    cvs: &[ComponentValue],
    uctx: Option<&UnitContext>,
    depth: usize,
) -> Result<CalcVal, String> {
    if depth > MAX_NESTING_DEPTH {
        return Err("calc() nesting too deep".to_owned());
    }
    let mut cur = Cursor::new(cvs);
    let mut acc = parse_calc_product(&mut cur, uctx, depth)?;

    loop {
        // CSS Values 4 §10.8 requires whitespace on both sides of `+` and `-`.
        let had_leading_ws = matches!(cur.peek_raw(), Some(cv) if is_ws(cv));
        let saved = cur.save();
        cur.skip_ws();
        let op = if cur.eat_delim('+') {
            Some(true)
        } else if cur.eat_delim('-') {
            Some(false)
        } else {
            None
        };

        let Some(is_add) = op else {
            cur.restore(saved);
            break;
        };

        let has_trailing_ws = matches!(cur.peek_raw(), Some(cv) if is_ws(cv));
        if !had_leading_ws || !has_trailing_ws {
            return Err("whitespace is required around '+' and '-' in calc()".to_owned());
        }

        let rhs = parse_calc_product(&mut cur, uctx, depth)?;
        acc = add_calc_vals(acc, rhs, is_add)?;
    }

    cur.expect_eof()?;
    Ok(acc)
}

fn parse_calc_product(
    cur: &mut Cursor<'_>,
    uctx: Option<&UnitContext>,
    depth: usize,
) -> Result<CalcVal, String> {
    let mut acc = parse_calc_value(cur, uctx, depth)?;

    loop {
        let saved = cur.save();
        let op = if cur.eat_delim('*') {
            Some(true)
        } else if cur.eat_delim('/') {
            Some(false)
        } else {
            None
        };

        let Some(is_mul) = op else {
            cur.restore(saved);
            break;
        };

        let rhs = parse_calc_value(cur, uctx, depth)?;
        acc = mul_div_calc_vals(acc, rhs, is_mul)?;
    }

    Ok(acc)
}

fn parse_calc_value(
    cur: &mut Cursor<'_>,
    uctx: Option<&UnitContext>,
    depth: usize,
) -> Result<CalcVal, String> {
    if let Some(inner) = cur.eat_paren_block() {
        return parse_calc_sum(inner, uctx, depth + 1);
    }
    if let Some((fname, fargs)) = cur.eat_function() {
        return eval_math_function(fname, fargs, uctx, depth + 1);
    }
    if let Some(n) = cur.eat_number() {
        return Ok(CalcVal::Number(n));
    }
    if let Some(pct) = cur.eat_percentage() {
        let uctx = uctx.ok_or_else(|| "percentage is not allowed here".to_owned())?;
        let lp = resolve_percentage(pct, uctx)?;
        return Ok(CalcVal::Dim(CalcNode::Leaf(lp)));
    }
    if let Some((val, unit, _)) = cur.eat_dimension() {
        if let Some(deg) = unit_to_deg(val, unit) {
            return Ok(CalcVal::AngleDeg(deg));
        }
        if let Some(ms) = unit_to_ms(val, unit) {
            return Ok(CalcVal::TimeMs(ms));
        }
        let uctx = uctx.ok_or_else(|| format!("unexpected dimension '{val}{unit}'"))?;
        let lp = resolve_length_unit(val, unit, uctx)?;
        return Ok(CalcVal::Dim(CalcNode::Leaf(lp)));
    }
    if let Some(ident) = cur.eat_ident() {
        return match ident.to_ascii_lowercase().as_str() {
            "pi" => Ok(CalcVal::Number(PI)),
            "e" => Ok(CalcVal::Number(E)),
            _ => Err(format!("unknown calc() constant '{ident}'")),
        };
    }
    Err("expected value in calc() expression".to_owned())
}

fn add_calc_vals(lhs: CalcVal, rhs: CalcVal, is_add: bool) -> Result<CalcVal, String> {
    let sign = if is_add { 1.0_f32 } else { -1.0_f32 };
    match (lhs, rhs) {
        (CalcVal::Number(a), CalcVal::Number(b)) => {
            let res = a + sign * b;
            check_finite(res)?;
            Ok(CalcVal::Number(res))
        }
        (CalcVal::AngleDeg(a), CalcVal::AngleDeg(b)) => {
            let res = a + sign * b;
            check_finite(res)?;
            Ok(CalcVal::AngleDeg(res))
        }
        (CalcVal::TimeMs(a), CalcVal::TimeMs(b)) => {
            let res = a + sign * b;
            check_finite(res)?;
            Ok(CalcVal::TimeMs(res))
        }
        (CalcVal::Dim(a), CalcVal::Dim(b)) => {
            let b_signed = scale_calc_node(b, sign);
            Ok(CalcVal::Dim(add_calc_nodes(a, b_signed)))
        }
        _ => Err("cannot add or subtract incompatible types in calc()".to_owned()),
    }
}

fn mul_div_calc_vals(lhs: CalcVal, rhs: CalcVal, is_mul: bool) -> Result<CalcVal, String> {
    if is_mul {
        match (lhs, rhs) {
            (CalcVal::Number(a), CalcVal::Number(b)) => {
                let res = a * b;
                check_finite(res)?;
                Ok(CalcVal::Number(res))
            }
            (CalcVal::Number(n), CalcVal::Dim(node)) | (CalcVal::Dim(node), CalcVal::Number(n)) => {
                check_finite(n)?;
                Ok(CalcVal::Dim(scale_calc_node(node, n)))
            }
            (CalcVal::Number(n), CalcVal::AngleDeg(a))
            | (CalcVal::AngleDeg(a), CalcVal::Number(n)) => {
                let res = a * n;
                check_finite(res)?;
                Ok(CalcVal::AngleDeg(res))
            }
            (CalcVal::Number(n), CalcVal::TimeMs(t)) | (CalcVal::TimeMs(t), CalcVal::Number(n)) => {
                let res = t * n;
                check_finite(res)?;
                Ok(CalcVal::TimeMs(res))
            }
            _ => Err("multiplication in calc() requires at least one unitless number".to_owned()),
        }
    } else {
        let CalcVal::Number(divisor) = rhs else {
            return Err("division in calc() requires a unitless number divisor".to_owned());
        };
        if divisor == 0.0 || !divisor.is_finite() {
            return Err("division by zero in calc()".to_owned());
        }
        let inv = 1.0 / divisor;
        check_finite(inv)?;
        match lhs {
            CalcVal::Number(a) => {
                let res = a / divisor;
                check_finite(res)?;
                Ok(CalcVal::Number(res))
            }
            CalcVal::Dim(node) => Ok(CalcVal::Dim(scale_calc_node(node, inv))),
            CalcVal::AngleDeg(a) => Ok(CalcVal::AngleDeg(a / divisor)),
            CalcVal::TimeMs(t) => Ok(CalcVal::TimeMs(t / divisor)),
        }
    }
}

fn check_finite(v: f32) -> Result<(), String> {
    if v.is_finite() {
        Ok(())
    } else {
        Err("non-finite result in calc()".to_owned())
    }
}

fn scale_calc_node(node: CalcNode, factor: f32) -> CalcNode {
    if factor == 0.0 {
        return CalcNode::Leaf(LengthPercentage::ZERO);
    }
    if (factor - 1.0).abs() < f32::EPSILON {
        return node;
    }
    match node {
        CalcNode::Leaf(lp) => CalcNode::Leaf(lp.scale(factor)),
        CalcNode::Product(inner, k) => CalcNode::Product(inner, k * factor),
        other => CalcNode::Product(Box::new(other), factor),
    }
}

fn add_calc_nodes(lhs: CalcNode, rhs: CalcNode) -> CalcNode {
    match (lhs, rhs) {
        (CalcNode::Leaf(a), CalcNode::Leaf(b)) => CalcNode::Leaf(a + b),
        (CalcNode::Sum(mut items), CalcNode::Leaf(b)) => {
            for item in &mut items {
                if let CalcNode::Leaf(existing) = item {
                    *existing = *existing + b;
                    return CalcNode::Sum(items);
                }
            }
            items.push(CalcNode::Leaf(b));
            CalcNode::Sum(items)
        }
        (CalcNode::Leaf(a), CalcNode::Sum(mut items)) => {
            for item in &mut items {
                if let CalcNode::Leaf(existing) = item {
                    *existing = a + *existing;
                    return CalcNode::Sum(items);
                }
            }
            items.insert(0, CalcNode::Leaf(a));
            CalcNode::Sum(items)
        }
        (CalcNode::Sum(mut a), CalcNode::Sum(b)) => {
            a.extend(b);
            CalcNode::Sum(a)
        }
        (a, b) => CalcNode::Sum(vec![a, b]),
    }
}

// ---------------------------------------------------------------------------
// Public(crate) value parsers built on Cursor + UnitContext + CalcVal
// ---------------------------------------------------------------------------

/// Attempts to consume a `<length>` or `<length-percentage>` (according to `uctx.allow_percent`).
///
/// - Literal negative tokens return `Err` when `non_negative` is `true`.
/// - Math functions (`calc()`, `min()`, `max()`, `clamp()`) clamp negative pure-`px`
///   results to `0.0` when `non_negative` is `true` (CSS Values 4 §10.9).
pub(crate) fn eat_length(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<Option<Length>, String> {
    let saved = cur.save();

    if let Some((fname, fargs)) = cur.eat_function() {
        if is_math_fn(fname) {
            let val = eval_math_function(fname, fargs, Some(uctx), 0)?;
            let CalcVal::Dim(node) = val else {
                return Err(format!("{fname}() did not produce a length"));
            };
            return Ok(Some(calc_node_into_length(node, non_negative)));
        }
        cur.restore(saved);
        return Ok(None);
    }

    if let Some((val, unit, _)) = cur.eat_dimension() {
        // If the dimension is an angle, time, or flex (`fr`), do not consume it as a length.
        if unit_to_deg(val, unit).is_some()
            || unit_to_ms(val, unit).is_some()
            || unit.eq_ignore_ascii_case("fr")
        {
            cur.restore(saved);
            return Ok(None);
        }
        let lp = resolve_length_unit(val, unit, uctx)?;
        if non_negative && val < 0.0 {
            return Err(format!("negative length '{val}{unit}' is not allowed"));
        }
        return Ok(Some(Length::Linear(lp)));
    }

    if let Some(pct) = cur.eat_percentage() {
        if !uctx.allow_percent {
            return Err("percentage is not allowed here".to_owned());
        }
        if non_negative && pct < 0.0 {
            return Err(format!("negative percentage '{pct}%' is not allowed"));
        }
        let lp = resolve_percentage(pct, uctx)?;
        return Ok(Some(Length::Linear(lp)));
    }

    if let Some(n) = cur.eat_number() {
        if n == 0.0 {
            return Ok(Some(Length::px(0.0)));
        }
        cur.restore(saved);
        return Ok(None);
    }

    Ok(None)
}

fn lp_is_non_negative(lp: &LengthPercentage) -> bool {
    lp.px >= 0.0 && lp.percent >= 0.0 && lp.vw >= 0.0 && lp.vh >= 0.0
}

/// Returns `true` when every leaf/factor in `node` is non-negative, so the
/// expression is guaranteed `>= 0` under a non-negative percentage/viewport basis
/// and does not need a redundant `max(0px, ...)` wrapper.
fn calc_node_is_non_negative(node: &CalcNode) -> bool {
    match node {
        CalcNode::Leaf(lp) => lp_is_non_negative(lp),
        CalcNode::Min(items) | CalcNode::Max(items) | CalcNode::Sum(items) => {
            items.iter().all(calc_node_is_non_negative)
        }
        CalcNode::Clamp(lo, val, hi) => {
            calc_node_is_non_negative(lo)
                && calc_node_is_non_negative(val)
                && calc_node_is_non_negative(hi)
        }
        CalcNode::Product(inner, k) => *k >= 0.0 && calc_node_is_non_negative(inner),
    }
}

fn calc_node_into_length(node: CalcNode, non_negative: bool) -> Length {
    if !non_negative {
        return match node {
            CalcNode::Leaf(lp) => Length::Linear(lp),
            other => Length::Calc(other),
        };
    }
    match node {
        // Percentages/vw/vh have a non-negative basis here, so a leaf with all
        // components <= 0 is known to be <= 0 at parse time and clamps to 0.
        CalcNode::Leaf(lp) => {
            if lp.px <= 0.0 && lp.percent <= 0.0 && lp.vw <= 0.0 && lp.vh <= 0.0 {
                Length::Linear(LengthPercentage::ZERO)
            } else if lp_is_non_negative(&lp) {
                Length::Linear(lp)
            } else {
                Length::Calc(CalcNode::Max(vec![
                    CalcNode::Leaf(LengthPercentage::ZERO),
                    CalcNode::Leaf(lp),
                ]))
            }
        }
        other => {
            if calc_node_is_non_negative(&other) {
                Length::Calc(other)
            } else {
                Length::Calc(CalcNode::Max(vec![
                    CalcNode::Leaf(LengthPercentage::ZERO),
                    other,
                ]))
            }
        }
    }
}

/// Parses an entire slice as a `<length>` or `<length-percentage>`.
pub(crate) fn parse_length_cvs(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<Length, String> {
    let mut cur = Cursor::new(cvs);
    let len = eat_length(&mut cur, uctx, non_negative)?
        .ok_or_else(|| "expected a <length>".to_owned())?;
    cur.expect_eof()?;
    Ok(len)
}

/// Parses an entire slice as `<length-percentage> | auto`.
pub(crate) fn parse_length_auto_cvs(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<LengthAuto, String> {
    let mut cur = Cursor::new(cvs);
    let res = eat_length_auto(&mut cur, uctx, non_negative)?
        .ok_or_else(|| "expected <length-percentage> or 'auto'".to_owned())?;
    cur.expect_eof()?;
    Ok(res)
}

pub(crate) fn eat_length_auto(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<Option<LengthAuto>, String> {
    if cur.eat_ident_eq("auto") {
        return Ok(Some(LengthAuto::Auto));
    }
    Ok(eat_length(cur, uctx, non_negative)?.map(LengthAuto::Length))
}

/// Evaluates a [`Length`] directly to `px` using `uctx` (resolving any `vw`/`vh`
/// against `uctx.viewport_*`). Fails if an unresolved percentage remains.
pub(crate) fn length_to_px(
    len: &Length,
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<f32, String> {
    let px = match len {
        Length::Linear(lp) => lp_to_px(*lp, uctx)?,
        Length::Calc(node) => calc_node_to_px(node, uctx)?,
    };
    Ok(if non_negative { px.max(0.0) } else { px })
}

pub(crate) fn lp_to_px(lp: LengthPercentage, uctx: &UnitContext) -> Result<f32, String> {
    if lp.percent != 0.0 {
        if let Some(base) = uctx.percent_base {
            return Ok(lp.px
                + lp.percent / 100.0 * base
                + lp.vw * uctx.viewport_w / 100.0
                + lp.vh * uctx.viewport_h / 100.0);
        }
        return Err("percentage cannot be resolved to px here".to_owned());
    }
    Ok(lp.px + lp.vw * uctx.viewport_w / 100.0 + lp.vh * uctx.viewport_h / 100.0)
}

fn calc_node_to_px(node: &CalcNode, uctx: &UnitContext) -> Result<f32, String> {
    match node {
        CalcNode::Leaf(lp) => lp_to_px(*lp, uctx),
        CalcNode::Min(items) => {
            let mut acc = f32::INFINITY;
            for item in items {
                acc = acc.min(calc_node_to_px(item, uctx)?);
            }
            Ok(acc)
        }
        CalcNode::Max(items) => {
            let mut acc = f32::NEG_INFINITY;
            for item in items {
                acc = acc.max(calc_node_to_px(item, uctx)?);
            }
            Ok(acc)
        }
        CalcNode::Clamp(lo, val, hi) => {
            let a = calc_node_to_px(lo, uctx)?;
            let b = calc_node_to_px(val, uctx)?;
            let c = calc_node_to_px(hi, uctx)?;
            Ok(a.max(b.min(c)))
        }
        CalcNode::Sum(items) => {
            let mut sum = 0.0;
            for item in items {
                sum += calc_node_to_px(item, uctx)?;
            }
            Ok(sum)
        }
        CalcNode::Product(inner, k) => Ok(calc_node_to_px(inner, uctx)? * *k),
    }
}

/// Converts a [`Length`] to a linear [`LengthPercentage`] (evaluating non-linear
/// viewport `min()`/`max()`/`clamp()` to `px` when no `%` is involved).
/// Simplifies a [`Length`] when its calc tree has no unresolved `%` (folding
/// viewport-only `min()`/`max()`/`clamp()` to `px`), and otherwise preserves
/// [`Length::Calc`] intact.
pub(crate) fn simplify_length(len: Length, uctx: &UnitContext, non_negative: bool) -> Length {
    match len {
        Length::Linear(mut lp) => {
            if non_negative && lp.as_px().is_some_and(|p| p < 0.0) {
                lp.px = 0.0;
            }
            Length::Linear(lp)
        }
        Length::Calc(node) => {
            if let Ok(px) = calc_node_to_px(&node, uctx) {
                Length::Linear(LengthPercentage::px(if non_negative {
                    px.max(0.0)
                } else {
                    px
                }))
            } else {
                Length::Calc(node)
            }
        }
    }
}

pub(crate) fn length_to_lp(
    len: Length,
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<LengthPercentage, String> {
    match len {
        Length::Linear(mut lp) => {
            if non_negative && lp.as_px().is_some_and(|p| p < 0.0) {
                lp.px = 0.0;
            }
            Ok(lp)
        }
        Length::Calc(node) => {
            let px = calc_node_to_px(&node, uctx)?;
            Ok(LengthPercentage::px(if non_negative {
                px.max(0.0)
            } else {
                px
            }))
        }
    }
}

/// Consumes a `<length>` (no `%`) and resolves it directly to `f32` px.
pub(crate) fn eat_px(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<Option<f32>, String> {
    let mut no_pct = *uctx;
    no_pct.allow_percent = false;
    no_pct.percent_base = None;
    let Some(len) = eat_length(cur, &no_pct, non_negative)? else {
        return Ok(None);
    };
    Ok(Some(length_to_px(&len, &no_pct, non_negative)?))
}

pub(crate) fn parse_px_cvs(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    non_negative: bool,
) -> Result<f32, String> {
    let mut cur = Cursor::new(cvs);
    let px =
        eat_px(&mut cur, uctx, non_negative)?.ok_or_else(|| "expected a <length>".to_owned())?;
    cur.expect_eof()?;
    Ok(px)
}

// ---------------------------------------------------------------------------
// Numbers, integers, angles, and times (including calc()/min()/max()/clamp())
// ---------------------------------------------------------------------------

/// Consumes a `<number>` (literal token or folded math function).
pub(crate) fn eat_number_or_calc(
    cur: &mut Cursor<'_>,
    non_negative: bool,
) -> Result<Option<f32>, String> {
    let saved = cur.save();
    if let Some((fname, fargs)) = cur.eat_function() {
        if is_math_fn(fname) {
            let CalcVal::Number(mut n) = eval_math_function(fname, fargs, None, 0)? else {
                return Err(format!("{fname}() did not produce a unitless number"));
            };
            if non_negative && n < 0.0 {
                n = 0.0;
            }
            return Ok(Some(n));
        }
        cur.restore(saved);
        return Ok(None);
    }
    if let Some(n) = cur.eat_number() {
        if non_negative && n < 0.0 {
            return Err(format!("negative number '{n}' is not allowed"));
        }
        return Ok(Some(n));
    }
    Ok(None)
}

pub(crate) fn parse_number_cvs(cvs: &[ComponentValue], non_negative: bool) -> Result<f32, String> {
    let mut cur = Cursor::new(cvs);
    let n = eat_number_or_calc(&mut cur, non_negative)?
        .ok_or_else(|| "expected a <number>".to_owned())?;
    cur.expect_eof()?;
    Ok(n)
}

/// Consumes a `<number-percentage>` (e.g. `opacity`, `scale()`, `filter`), returning
/// the normalised factor (`50%` -> `0.5`).
pub(crate) fn eat_number_or_percentage(
    cur: &mut Cursor<'_>,
    non_negative: bool,
) -> Result<Option<f32>, String> {
    if let Some(pct) = cur.eat_percentage() {
        if non_negative && pct < 0.0 {
            return Err(format!("negative percentage '{pct}%' is not allowed"));
        }
        return Ok(Some(pct / 100.0));
    }
    eat_number_or_calc(cur, non_negative)
}

/// Consumes an `<integer>` (literal integer token or rounded math function per CSS Values 4 §10.9).
pub(crate) fn eat_integer_or_calc(cur: &mut Cursor<'_>) -> Result<Option<i32>, String> {
    let saved = cur.save();
    if let Some((fname, fargs)) = cur.eat_function() {
        if is_math_fn(fname) {
            let CalcVal::Number(n) = eval_math_function(fname, fargs, None, 0)? else {
                return Err(format!("{fname}() did not produce a number"));
            };
            let rounded = (f64::from(n) + 0.5).floor();
            let clamped = rounded.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
            return Ok(Some(clamped));
        }
        cur.restore(saved);
        return Ok(None);
    }
    if let Some(i) = cur.eat_integer_token() {
        return Ok(Some(i));
    }
    Ok(None)
}

pub(crate) fn parse_integer_cvs(cvs: &[ComponentValue]) -> Result<i32, String> {
    let mut cur = Cursor::new(cvs);
    let i = eat_integer_or_calc(&mut cur)?.ok_or_else(|| "expected an <integer>".to_owned())?;
    cur.expect_eof()?;
    Ok(i)
}

pub(crate) fn unit_to_deg(val: f32, unit: &str) -> Option<f32> {
    match unit.to_ascii_lowercase().as_str() {
        "deg" => Some(val),
        "grad" => Some(val * 0.9),
        "rad" => Some(val.to_degrees()),
        "turn" => Some(val * 360.0),
        _ => None,
    }
}

/// Consumes an `<angle>` in degrees (`allow_unitless_zero` is `true` for transforms
/// and gradients, `false` where CSS requires an angle unit).
pub(crate) fn eat_angle_deg(
    cur: &mut Cursor<'_>,
    allow_unitless_zero: bool,
) -> Result<Option<f32>, String> {
    let saved = cur.save();
    if let Some((fname, fargs)) = cur.eat_function() {
        if is_math_fn(fname) {
            let CalcVal::AngleDeg(deg) = eval_math_function(fname, fargs, None, 0)? else {
                return Err(format!("{fname}() did not produce an <angle>"));
            };
            return Ok(Some(deg));
        }
        cur.restore(saved);
        return Ok(None);
    }
    if let Some((val, unit, _)) = cur.eat_dimension() {
        if let Some(deg) = unit_to_deg(val, unit) {
            return Ok(Some(deg));
        }
        cur.restore(saved);
        return Ok(None);
    }
    if allow_unitless_zero && let Some(n) = cur.eat_number() {
        if n == 0.0 {
            return Ok(Some(0.0));
        }
        cur.restore(saved);
    }
    Ok(None)
}

pub(crate) fn unit_to_ms(val: f32, unit: &str) -> Option<f32> {
    match unit.to_ascii_lowercase().as_str() {
        "ms" => Some(val),
        "s" => Some(val * 1000.0),
        _ => None,
    }
}

/// Consumes a `<time>` in milliseconds. Unitless `0` is rejected per CSS Values 4 §6.2.
pub(crate) fn eat_time_ms(cur: &mut Cursor<'_>, non_negative: bool) -> Result<Option<f32>, String> {
    let saved = cur.save();
    if let Some((fname, fargs)) = cur.eat_function() {
        if is_math_fn(fname) {
            let CalcVal::TimeMs(mut ms) = eval_math_function(fname, fargs, None, 0)? else {
                return Err(format!("{fname}() did not produce a <time>"));
            };
            if non_negative && ms < 0.0 {
                ms = 0.0;
            }
            return Ok(Some(ms));
        }
        cur.restore(saved);
        return Ok(None);
    }
    if let Some((val, unit, _)) = cur.eat_dimension() {
        if let Some(ms) = unit_to_ms(val, unit) {
            if non_negative && ms < 0.0 {
                return Err(format!("negative time '{val}{unit}' is not allowed"));
            }
            return Ok(Some(ms));
        }
        cur.restore(saved);
    }
    Ok(None)
}
