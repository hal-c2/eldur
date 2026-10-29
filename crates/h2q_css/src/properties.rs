//! Property values: shorthand expansion and computing longhands into
//! [`ComputedStyle`].
//!
//! Interface between the cascade (`cascade.rs`, `selectors.rs`,
//! `media.rs`, `parse.rs`) and value parsing (this module and its
//! children). The cascade decides, per element and per longhand, which
//! declared value wins; this module turns that text into a computed value.
//!
//! Values travel as source text (CSS serialisation of the declaration's
//! component values, comments removed), so `var()` substitution is plain
//! text substitution before parsing.
//!
//! The minimal bodies below exist so the cascade can be tested before the
//! full value parser lands; the value-parsing worker replaces them.

use crate::{ComputedStyle, Display, Length, LengthAuto, Rgba, Viewport};

/// Every longhand property the converter computes. Shorthands never appear
/// here; [`expand`] turns them into longhands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Longhand {
    Display,
    Color,
    FontSize,
    MarginTop,
}

/// CSS-wide keywords, handled by the cascade, never passed to [`compute`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CssWide {
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl CssWide {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "inherit" => Some(Self::Inherit),
            "initial" => Some(Self::Initial),
            "unset" => Some(Self::Unset),
            "revert" => Some(Self::Revert),
            "revert-layer" => Some(Self::RevertLayer),
            _ => None,
        }
    }
}

/// What a declaration's property name is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyKind {
    Longhand(Longhand),
    Shorthand,
    /// `--name`.
    Custom,
    /// Valid CSS this converter does not model (`cursor` before support,
    /// `-webkit-*`, …): reported as `Severity::Unsupported` once per name.
    Unsupported,
    /// Not a CSS property at all: reported as `Severity::Invalid`.
    Unknown,
}

pub(crate) fn kind(name: &str) -> PropertyKind {
    if name.starts_with("--") {
        return PropertyKind::Custom;
    }
    match name {
        "display" => PropertyKind::Longhand(Longhand::Display),
        "color" => PropertyKind::Longhand(Longhand::Color),
        "font-size" => PropertyKind::Longhand(Longhand::FontSize),
        "margin-top" => PropertyKind::Longhand(Longhand::MarginTop),
        "margin" => PropertyKind::Shorthand,
        _ if name.starts_with('-') => PropertyKind::Unsupported,
        _ => PropertyKind::Unknown,
    }
}

/// Expands a declaration (`name` is a shorthand or a longhand; `value` has
/// no `var()` and is not a CSS-wide keyword) into longhand values.
/// `Err` = invalid value (the declaration is dropped, as a browser does).
pub(crate) fn expand(name: &str, value: &str) -> Result<Vec<(Longhand, String)>, String> {
    match kind(name) {
        PropertyKind::Longhand(l) => Ok(vec![(l, value.trim().to_owned())]),
        PropertyKind::Shorthand if name == "margin" => {
            let first = value.split_whitespace().next().ok_or("empty margin")?;
            Ok(vec![(Longhand::MarginTop, first.to_owned())])
        }
        _ => Err(format!("cannot expand {name}")),
    }
}

/// The longhands a shorthand sets (so a CSS-wide keyword or a pending
/// `var()` on the shorthand can be applied to each).
pub(crate) fn longhands_of(name: &str) -> Vec<Longhand> {
    match name {
        "margin" => vec![Longhand::MarginTop],
        _ => match kind(name) {
            PropertyKind::Longhand(l) => vec![l],
            _ => Vec::new(),
        },
    }
}

pub(crate) fn is_inherited(l: Longhand) -> bool {
    matches!(l, Longhand::Color | Longhand::FontSize)
}

/// Compute order: properties others depend on first (`font-size` before
/// anything with `em`, `color` before `currentColor` users). Lower first.
pub(crate) fn priority(l: Longhand) -> u8 {
    match l {
        Longhand::FontSize => 0,
        Longhand::Color => 1,
        _ => 2,
    }
}

/// What computing a value may depend on.
pub(crate) struct ComputeContext<'a> {
    /// The parent element's computed style (`None` for the root).
    pub parent: Option<&'a ComputedStyle>,
    /// The root element's computed font size (for `rem`).
    pub root_font_size: f32,
    pub viewport: &'a Viewport,
}

/// Parses `value` for `l` and stores its computed value in `style`.
/// `style` already holds every higher-priority longhand's computed value.
pub(crate) fn compute(
    style: &mut ComputedStyle,
    l: Longhand,
    value: &str,
    ctx: &ComputeContext<'_>,
) -> Result<(), String> {
    let v = value.trim().to_ascii_lowercase();
    match l {
        Longhand::Display => {
            style.display = match v.as_str() {
                "block" => Display::Block,
                "inline" => Display::Inline,
                "flex" => Display::Flex,
                "none" => Display::None,
                _ => return Err(format!("display: {v}")),
            };
        }
        Longhand::Color => {
            let c = csscolorparser::parse(&v).map_err(|e| e.to_string())?;
            let [r, g, b, a] = c.to_rgba8();
            style.color = Rgba::new(r, g, b, a);
        }
        Longhand::FontSize => {
            let parent = ctx.parent.map_or(16.0, |p| p.font_size);
            style.font_size = parse_px(&v, parent).ok_or(format!("font-size: {v}"))?;
        }
        Longhand::MarginTop => {
            style.margin.top = LengthAuto::Length(Length::px(
                parse_px(&v, style.font_size).ok_or(format!("margin-top: {v}"))?,
            ));
        }
    }
    Ok(())
}

/// Copies `l`'s computed value from `src` (for `inherit`, and for
/// inherited properties without a declaration).
pub(crate) fn copy(dst: &mut ComputedStyle, src: &ComputedStyle, l: Longhand) {
    match l {
        Longhand::Display => dst.display = src.display,
        Longhand::Color => dst.color = src.color,
        Longhand::FontSize => dst.font_size = src.font_size,
        Longhand::MarginTop => dst.margin.top = src.margin.top.clone(),
    }
}

fn parse_px(v: &str, em: f32) -> Option<f32> {
    if let Some(n) = v.strip_suffix("px") {
        n.parse().ok()
    } else if let Some(n) = v.strip_suffix("em") {
        n.parse::<f32>().ok().map(|n| n * em)
    } else if v == "0" {
        Some(0.0)
    } else {
        None
    }
}
