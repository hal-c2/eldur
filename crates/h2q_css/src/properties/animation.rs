//! CSS Easing Functions 1, CSS Transitions 1, and CSS Animations 1.

use swc_css_ast::ComponentValue;

use crate::{Animation, ComputedStyle, Easing, Transition};

use super::{
    cursor::{Cursor, split_comma_separated},
    length::{eat_number_or_calc, eat_time_ms, is_math_fn, parse_integer_cvs, parse_number_cvs},
};

const MAX_SYNC_LIST_LEN: usize = 64;

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

fn capped_lcm(a: usize, b: usize) -> usize {
    if a == 0 || b == 0 {
        return a.max(b);
    }
    let g = gcd(a, b);
    (a / g).saturating_mul(b).min(MAX_SYNC_LIST_LEN)
}

pub(crate) fn css_initial_transition() -> Transition {
    Transition {
        property: "all".to_owned(),
        duration_ms: 0.0,
        delay_ms: 0.0,
        easing: Easing::default(),
    }
}

pub(crate) fn css_initial_animation() -> Animation {
    Animation {
        name: String::new(),
        duration_ms: 0.0,
        delay_ms: 0.0,
        easing: Easing::default(),
        iteration_count: 1.0,
        alternate: false,
        reverse: false,
        fill_forwards: false,
        fill_backwards: false,
        paused: false,
    }
}

// ---------------------------------------------------------------------------
// `<easing-function>`
// ---------------------------------------------------------------------------

/// Consumes an `<easing-function>` from `cur`.
///
/// Returns `Ok(None)` without advancing `cur` if the next token is neither an
/// easing keyword nor `cubic-bezier(...)`/`steps(...)`, or `Err` if a
/// `cubic-bezier()`/`steps()` call has invalid arguments.
pub(crate) fn eat_easing(cur: &mut Cursor<'_>) -> Result<Option<Easing>, String> {
    let saved = cur.save();
    if let Some(id) = cur.eat_ident() {
        let matched = match id.to_ascii_lowercase().as_str() {
            "linear" => Some(Easing::Linear),
            "ease" => Some(Easing::CubicBezier(0.25, 0.1, 0.25, 1.0)),
            "ease-in" => Some(Easing::CubicBezier(0.42, 0.0, 1.0, 1.0)),
            "ease-out" => Some(Easing::CubicBezier(0.0, 0.0, 0.58, 1.0)),
            "ease-in-out" => Some(Easing::CubicBezier(0.42, 0.0, 0.58, 1.0)),
            "step-start" => Some(Easing::Steps(1, true)),
            "step-end" => Some(Easing::Steps(1, false)),
            _ => None,
        };
        if let Some(easing) = matched {
            return Ok(Some(easing));
        }
        cur.restore(saved);
    }

    if let Some((fname, fargs)) = cur.eat_function() {
        if fname.eq_ignore_ascii_case("cubic-bezier") {
            return Ok(Some(parse_cubic_bezier(fargs)?));
        }
        if fname.eq_ignore_ascii_case("steps") {
            return Ok(Some(parse_steps(fargs)?));
        }
        cur.restore(saved);
    }

    Ok(None)
}

pub(crate) fn parse_easing_cvs(cvs: &[ComponentValue]) -> Result<Easing, String> {
    let mut cur = Cursor::new(cvs);
    let easing = eat_easing(&mut cur)?.ok_or_else(|| "expected <easing-function>".to_owned())?;
    cur.expect_eof()?;
    Ok(easing)
}

fn parse_cubic_bezier(args: &[ComponentValue]) -> Result<Easing, String> {
    let parts = split_comma_separated(args)?;
    if parts.len() != 4 {
        return Err("cubic-bezier() requires 4 arguments".to_owned());
    }
    let x1 = parse_unit_interval_x(parts[0])?;
    let y1 = parse_number_cvs(parts[1], false)?;
    let x2 = parse_unit_interval_x(parts[2])?;
    let y2 = parse_number_cvs(parts[3], false)?;
    Ok(Easing::CubicBezier(x1, y1, x2, y2))
}

fn parse_unit_interval_x(cvs: &[ComponentValue]) -> Result<f32, String> {
    let mut cur = Cursor::new(cvs);
    let is_calc = cur
        .peek()
        .is_some_and(|cv| matches!(cv, ComponentValue::Function(f) if is_math_fn(f.name.as_str())));
    let x = parse_number_cvs(cvs, false)?;
    if !is_calc && !(0.0..=1.0).contains(&x) {
        return Err(format!(
            "cubic-bezier() X coordinate '{x}' must be in [0, 1]"
        ));
    }
    Ok(x.clamp(0.0, 1.0))
}

fn parse_steps(args: &[ComponentValue]) -> Result<Easing, String> {
    let parts = split_comma_separated(args)?;
    if !(1..=2).contains(&parts.len()) {
        return Err("steps() requires 1 or 2 arguments".to_owned());
    }
    let count = parse_integer_cvs(parts[0])?;
    let (jump_start, min_steps) = if parts.len() == 2 {
        let mut pcur = Cursor::new(parts[1]);
        let kw = pcur
            .eat_ident()
            .ok_or_else(|| "expected step-position keyword in steps()".to_owned())?;
        pcur.expect_eof()?;
        match kw.to_ascii_lowercase().as_str() {
            "jump-start" | "start" | "jump-both" => (true, 1),
            "jump-end" | "end" => (false, 1),
            "jump-none" => (false, 2),
            _ => return Err(format!("invalid step-position '{kw}' in steps()")),
        }
    } else {
        (false, 1)
    };

    if count < min_steps {
        return Err(format!("steps() count must be at least {min_steps}"));
    }
    Ok(Easing::Steps(count as u32, jump_start))
}

pub(crate) fn parse_easing_list(cvs: &[ComponentValue]) -> Result<Vec<Easing>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        out.push(parse_easing_cvs(part)?);
    }
    Ok(out)
}

pub(crate) fn parse_time_list(
    cvs: &[ComponentValue],
    non_negative: bool,
    allow_auto: bool,
) -> Result<Vec<f32>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        if allow_auto && cur.eat_ident_eq("auto") {
            cur.expect_eof()?;
            out.push(0.0);
            continue;
        }
        let ms =
            eat_time_ms(&mut cur, non_negative)?.ok_or_else(|| "expected <time>".to_owned())?;
        cur.expect_eof()?;
        out.push(ms);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

/// Parses `transition-property` (`none | <single-transition-property>#`).
/// Returns `Ok(None)` when the value is the single keyword `none`.
pub(crate) fn parse_transition_property_list(
    cvs: &[ComponentValue],
) -> Result<Option<Vec<String>>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(None);
    }

    let parts = split_comma_separated(cvs)?;
    let mut props = Vec::with_capacity(parts.len());
    for part in parts {
        let mut pcur = Cursor::new(part);
        let id = pcur
            .eat_custom_ident()
            .ok_or_else(|| "expected property name in transition-property".to_owned())?;
        pcur.expect_eof()?;
        // CSS Transitions 1 §2.1 forbids `none` inside a multi-item transition-property list.
        if id.eq_ignore_ascii_case("none") {
            return Err("'none' is not allowed inside a transition-property list".to_owned());
        }
        let normalised = if id.starts_with("--") {
            id.to_owned()
        } else {
            id.to_ascii_lowercase()
        };
        props.push(normalised);
    }
    Ok(Some(props))
}

pub(crate) fn apply_transition_properties(style: &mut ComputedStyle, props: Option<Vec<String>>) {
    let Some(props) = props else {
        style.transitions.clear();
        return;
    };
    let old = std::mem::take(&mut style.transitions);
    let mut next = Vec::with_capacity(props.len());
    for (i, prop) in props.into_iter().enumerate() {
        let mut base = if old.is_empty() {
            css_initial_transition()
        } else {
            old[i % old.len()].clone()
        };
        base.property = prop;
        next.push(base);
    }
    style.transitions = next;
}

pub(crate) fn apply_transition_field<T: Clone>(
    style: &mut ComputedStyle,
    values: &[T],
    mut setter: impl FnMut(&mut Transition, T),
) {
    if values.is_empty() {
        return;
    }
    if style.transitions.is_empty() {
        style.transitions = values
            .iter()
            .cloned()
            .map(|v| {
                let mut tr = css_initial_transition();
                setter(&mut tr, v);
                tr
            })
            .collect();
        return;
    }
    let target_len = capped_lcm(style.transitions.len(), values.len());
    if target_len > style.transitions.len() {
        let orig_len = style.transitions.len();
        for i in orig_len..target_len {
            let cloned = style.transitions[i % orig_len].clone();
            style.transitions.push(cloned);
        }
    }
    for (i, tr) in style.transitions.iter_mut().enumerate() {
        setter(tr, values[i % values.len()].clone());
    }
}

// ---------------------------------------------------------------------------
// Animations
// ---------------------------------------------------------------------------

/// Consumes a single `<keyframes-name>` (`<custom-ident>` excluding `none`, or `<string>`).
pub(crate) fn eat_keyframes_name(cur: &mut Cursor<'_>) -> Option<String> {
    if let Some(s) = cur.eat_string() {
        return Some(s.to_owned());
    }
    let saved = cur.save();
    if let Some(id) = cur.eat_custom_ident() {
        if !id.eq_ignore_ascii_case("none") {
            return Some(id.to_owned());
        }
        cur.restore(saved);
    }
    None
}

/// Parses `animation-name`: comma-separated `none | <keyframes-name>`.
pub(crate) fn parse_animation_name_list(
    cvs: &[ComponentValue],
) -> Result<Vec<Option<String>>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut names = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        if cur.eat_ident_eq("none") {
            cur.expect_eof()?;
            names.push(None);
            continue;
        }
        let name = eat_keyframes_name(&mut cur)
            .ok_or_else(|| "expected keyframes name or 'none' in animation-name".to_owned())?;
        cur.expect_eof()?;
        names.push(Some(name));
    }
    Ok(names)
}

pub(crate) fn eat_anim_iteration_count(cur: &mut Cursor<'_>) -> Result<Option<f32>, String> {
    if cur.eat_ident_eq("infinite") {
        return Ok(Some(f32::INFINITY));
    }
    eat_number_or_calc(cur, true)
}

pub(crate) fn parse_animation_iteration_list(cvs: &[ComponentValue]) -> Result<Vec<f32>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        let c = eat_anim_iteration_count(&mut cur)?
            .ok_or_else(|| "expected 'infinite' or non-negative <number>".to_owned())?;
        cur.expect_eof()?;
        out.push(c);
    }
    Ok(out)
}

/// Returns `(alternate, reverse)`.
pub(crate) fn eat_anim_direction(cur: &mut Cursor<'_>) -> Option<(bool, bool)> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    match id.to_ascii_lowercase().as_str() {
        "normal" => Some((false, false)),
        "reverse" => Some((false, true)),
        "alternate" => Some((true, false)),
        "alternate-reverse" => Some((true, true)),
        _ => {
            cur.restore(saved);
            None
        }
    }
}

pub(crate) fn parse_animation_direction_list(
    cvs: &[ComponentValue],
) -> Result<Vec<(bool, bool)>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        let dir =
            eat_anim_direction(&mut cur).ok_or_else(|| "invalid animation-direction".to_owned())?;
        cur.expect_eof()?;
        out.push(dir);
    }
    Ok(out)
}

/// Returns `(fill_forwards, fill_backwards)`.
pub(crate) fn eat_anim_fill_mode(cur: &mut Cursor<'_>) -> Option<(bool, bool)> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    match id.to_ascii_lowercase().as_str() {
        "none" => Some((false, false)),
        "forwards" => Some((true, false)),
        "backwards" => Some((false, true)),
        "both" => Some((true, true)),
        _ => {
            cur.restore(saved);
            None
        }
    }
}

pub(crate) fn parse_animation_fill_mode_list(
    cvs: &[ComponentValue],
) -> Result<Vec<(bool, bool)>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        let fm =
            eat_anim_fill_mode(&mut cur).ok_or_else(|| "invalid animation-fill-mode".to_owned())?;
        cur.expect_eof()?;
        out.push(fm);
    }
    Ok(out)
}

/// Returns `paused`.
pub(crate) fn eat_anim_play_state(cur: &mut Cursor<'_>) -> Option<bool> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    match id.to_ascii_lowercase().as_str() {
        "running" => Some(false),
        "paused" => Some(true),
        _ => {
            cur.restore(saved);
            None
        }
    }
}

pub(crate) fn parse_animation_play_state_list(cvs: &[ComponentValue]) -> Result<Vec<bool>, String> {
    let parts = split_comma_separated(cvs)?;
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let mut cur = Cursor::new(part);
        let paused = eat_anim_play_state(&mut cur)
            .ok_or_else(|| "invalid animation-play-state".to_owned())?;
        cur.expect_eof()?;
        out.push(paused);
    }
    Ok(out)
}

pub(crate) fn apply_animation_names(style: &mut ComputedStyle, names: Vec<Option<String>>) {
    let old = std::mem::take(&mut style.animations);
    let mut next = Vec::new();
    for (i, maybe_name) in names.into_iter().enumerate() {
        let Some(name) = maybe_name else {
            continue;
        };
        let mut base = if old.is_empty() {
            css_initial_animation()
        } else {
            old[i % old.len()].clone()
        };
        base.name = name;
        next.push(base);
    }
    style.animations = next;
}

pub(crate) fn apply_animation_field<T: Clone>(
    style: &mut ComputedStyle,
    values: &[T],
    mut setter: impl FnMut(&mut Animation, T),
) {
    if values.is_empty() {
        return;
    }
    if style.animations.is_empty() {
        style.animations = values
            .iter()
            .cloned()
            .map(|v| {
                let mut anim = css_initial_animation();
                setter(&mut anim, v);
                anim
            })
            .collect();
        return;
    }
    let target_len = capped_lcm(style.animations.len(), values.len());
    if target_len > style.animations.len() {
        let orig_len = style.animations.len();
        for i in orig_len..target_len {
            let cloned = style.animations[i % orig_len].clone();
            style.animations.push(cloned);
        }
    }
    for (i, anim) in style.animations.iter_mut().enumerate() {
        setter(anim, values[i % values.len()].clone());
    }
}
