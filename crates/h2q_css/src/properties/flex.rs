//! Flexbox and CSS Box Alignment 3 property parsers.

use swc_css_ast::ComponentValue;

use crate::{Align, FlexDirection, FlexWrap, Length, Size};

use super::{
    box_model::parse_size,
    cursor::Cursor,
    length::{UnitContext, parse_integer_cvs, parse_length_cvs, parse_number_cvs},
};

pub(crate) fn eat_flex_direction(cur: &mut Cursor<'_>) -> Option<FlexDirection> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    match id.to_ascii_lowercase().as_str() {
        "row" => Some(FlexDirection::Row),
        "row-reverse" => Some(FlexDirection::RowReverse),
        "column" => Some(FlexDirection::Column),
        "column-reverse" => Some(FlexDirection::ColumnReverse),
        _ => {
            cur.restore(saved);
            None
        }
    }
}

pub(crate) fn parse_flex_direction(cvs: &[ComponentValue]) -> Result<FlexDirection, String> {
    let mut cur = Cursor::new(cvs);
    let dir =
        eat_flex_direction(&mut cur).ok_or_else(|| "expected flex-direction keyword".to_owned())?;
    cur.expect_eof()?;
    Ok(dir)
}

pub(crate) fn eat_flex_wrap(cur: &mut Cursor<'_>) -> Option<FlexWrap> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    match id.to_ascii_lowercase().as_str() {
        "nowrap" => Some(FlexWrap::NoWrap),
        "wrap" => Some(FlexWrap::Wrap),
        "wrap-reverse" => Some(FlexWrap::WrapReverse),
        _ => {
            cur.restore(saved);
            None
        }
    }
}

pub(crate) fn parse_flex_wrap(cvs: &[ComponentValue]) -> Result<FlexWrap, String> {
    let mut cur = Cursor::new(cvs);
    let wrap = eat_flex_wrap(&mut cur).ok_or_else(|| "expected flex-wrap keyword".to_owned())?;
    cur.expect_eof()?;
    Ok(wrap)
}

/// Which CSS Box Alignment 3 property is being parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlignTarget {
    JustifyContent,
    AlignContent,
    AlignItems,
    AlignSelf,
    JustifyItems,
    JustifySelf,
}

impl AlignTarget {
    fn allows_auto(self) -> bool {
        matches!(self, Self::AlignSelf | Self::JustifySelf)
    }

    fn allows_distribution(self) -> bool {
        matches!(self, Self::JustifyContent | Self::AlignContent)
    }

    fn allows_baseline(self) -> bool {
        !matches!(self, Self::JustifyContent)
    }

    fn allows_left_right(self) -> bool {
        matches!(
            self,
            Self::JustifyContent | Self::JustifyItems | Self::JustifySelf
        )
    }

    fn allows_self_start_end(self) -> bool {
        matches!(
            self,
            Self::AlignItems | Self::AlignSelf | Self::JustifyItems | Self::JustifySelf
        )
    }
}

/// Consumes one alignment value valid for `target` (which may span 2 keywords,
/// such as `first baseline`, `safe center`, or `legacy left`).
pub(crate) fn eat_align(cur: &mut Cursor<'_>, target: AlignTarget) -> Option<Align> {
    let saved = cur.save();
    let first = cur.eat_ident()?.to_ascii_lowercase();

    match first.as_str() {
        "normal" => return Some(Align::Normal),
        "auto" if target.allows_auto() => return Some(Align::Auto),
        "stretch" => return Some(Align::Stretch),
        "space-between" if target.allows_distribution() => return Some(Align::SpaceBetween),
        "space-around" if target.allows_distribution() => return Some(Align::SpaceAround),
        "space-evenly" if target.allows_distribution() => return Some(Align::SpaceEvenly),
        "baseline" if target.allows_baseline() => return Some(Align::Baseline),
        "first" | "last" if target.allows_baseline() => {
            if cur.eat_ident_eq("baseline") {
                return Some(Align::Baseline);
            }
            cur.restore(saved);
            return None;
        }
        "legacy" if target == AlignTarget::JustifyItems => {
            let after_legacy = cur.save();
            if let Some(next) = cur.eat_ident() {
                match next.to_ascii_lowercase().as_str() {
                    "left" => return Some(Align::Left),
                    "right" => return Some(Align::Right),
                    "center" => return Some(Align::Center),
                    _ => cur.restore(after_legacy),
                }
            }
            return Some(Align::Normal);
        }
        "safe" | "unsafe" => {
            if let Some(pos) = cur.eat_ident()
                && let Some(align) = match_position_keyword(&pos.to_ascii_lowercase(), target)
            {
                return Some(align);
            }
            cur.restore(saved);
            return None;
        }
        other => {
            if let Some(align) = match_position_keyword(other, target) {
                // `left legacy` / `right legacy` / `center legacy` on `justify-items`.
                if target == AlignTarget::JustifyItems
                    && matches!(align, Align::Left | Align::Right | Align::Center)
                {
                    let _ = cur.eat_ident_eq("legacy");
                }
                return Some(align);
            }
        }
    }

    cur.restore(saved);
    None
}

fn match_position_keyword(kw: &str, target: AlignTarget) -> Option<Align> {
    match kw {
        "start" => Some(Align::Start),
        "end" => Some(Align::End),
        "self-start" if target.allows_self_start_end() => Some(Align::Start),
        "self-end" if target.allows_self_start_end() => Some(Align::End),
        "flex-start" => Some(Align::FlexStart),
        "flex-end" => Some(Align::FlexEnd),
        "center" => Some(Align::Center),
        "left" if target.allows_left_right() => Some(Align::Left),
        "right" if target.allows_left_right() => Some(Align::Right),
        _ => None,
    }
}

pub(crate) fn parse_align(cvs: &[ComponentValue], target: AlignTarget) -> Result<Align, String> {
    let mut cur = Cursor::new(cvs);
    let align = eat_align(&mut cur, target).ok_or_else(|| "invalid alignment value".to_owned())?;
    cur.expect_eof()?;
    Ok(align)
}

pub(crate) fn parse_gap_side(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Option<Length>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("normal") {
        cur.expect_eof()?;
        return Ok(None);
    }
    Ok(Some(parse_length_cvs(cvs, uctx, true)?))
}

pub(crate) fn parse_flex_factor(cvs: &[ComponentValue]) -> Result<f32, String> {
    parse_number_cvs(cvs, true)
}

pub(crate) fn parse_flex_basis(cvs: &[ComponentValue], uctx: &UnitContext) -> Result<Size, String> {
    parse_size(cvs, uctx, false, true)
}

pub(crate) fn parse_order(cvs: &[ComponentValue]) -> Result<i32, String> {
    parse_integer_cvs(cvs)
}
