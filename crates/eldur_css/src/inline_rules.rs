//! The cascade's input as data the runtime can evaluate itself: an inline
//! element has no QML object for the static class variants to be written on,
//! and a created element's class is often only known when the script runs.

use crate::cascade::layer_ranks;
use crate::parse::{CustomDeclValue, DeclValue};
use crate::properties::{CssWide, Longhand};
use crate::{Origin, Stylesheet, Viewport, user_agent_stylesheet};

/// One selector of a rule, with the declarations the runtime can paint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineRule {
    pub selector: String,
    /// `(id, class, type)`.
    pub specificity: (u32, u32, u32),
    pub user_agent: bool,
    /// `(property, value, important)`.
    pub declarations: Vec<(String, String, bool)>,
}

/// One selector of a rule, with every declaration the runtime can apply to a
/// box or to inline text. `@media` is settled here, at the conversion viewport,
/// as it is for the page's own elements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRule {
    pub selector: String,
    /// `(id, class, type)`.
    pub specificity: (u32, u32, u32),
    pub user_agent: bool,
    /// Place among the page's cascade layers; `None` is unlayered, above them.
    pub layer: Option<u32>,
    /// `(property, value, important)`; a value may hold `var()`, a property may
    /// be a custom property or a shorthand waiting on `var()`.
    pub declarations: Vec<(String, String, bool)>,
}

fn property(longhand: Longhand) -> Option<&'static str> {
    Some(match longhand {
        Longhand::FontSize => "font-size",
        Longhand::FontFamily => "font-family",
        Longhand::FontWeight => "font-weight",
        Longhand::FontStyle => "font-style",
        Longhand::Color => "color",
        Longhand::Display => "display",
        Longhand::BackgroundColor => "background-color",
        Longhand::Visibility => "visibility",
        Longhand::TextDecorationLine => "text-decoration-line",
        _ => return None,
    })
}

fn box_property(longhand: Longhand) -> Option<&'static str> {
    use Longhand as L;
    Some(match longhand {
        L::LineHeight => "line-height",
        L::TextAlign => "text-align",
        L::Opacity => "opacity",
        L::Position => "position",
        L::Top => "top",
        L::Right => "right",
        L::Bottom => "bottom",
        L::Left => "left",
        L::ZIndex => "z-index",
        L::BoxSizing => "box-sizing",
        L::Width => "width",
        L::Height => "height",
        L::MinWidth => "min-width",
        L::MinHeight => "min-height",
        L::MaxWidth => "max-width",
        L::MaxHeight => "max-height",
        L::MarginTop => "margin-top",
        L::MarginRight => "margin-right",
        L::MarginBottom => "margin-bottom",
        L::MarginLeft => "margin-left",
        L::PaddingTop => "padding-top",
        L::PaddingRight => "padding-right",
        L::PaddingBottom => "padding-bottom",
        L::PaddingLeft => "padding-left",
        L::BorderTopWidth => "border-top-width",
        L::BorderRightWidth => "border-right-width",
        L::BorderBottomWidth => "border-bottom-width",
        L::BorderLeftWidth => "border-left-width",
        L::BorderTopStyle => "border-top-style",
        L::BorderRightStyle => "border-right-style",
        L::BorderBottomStyle => "border-bottom-style",
        L::BorderLeftStyle => "border-left-style",
        L::BorderTopColor => "border-top-color",
        L::BorderRightColor => "border-right-color",
        L::BorderBottomColor => "border-bottom-color",
        L::BorderLeftColor => "border-left-color",
        L::BorderTopLeftRadius => "border-top-left-radius",
        L::BorderTopRightRadius => "border-top-right-radius",
        L::BorderBottomRightRadius => "border-bottom-right-radius",
        L::BorderBottomLeftRadius => "border-bottom-left-radius",
        L::FlexDirection => "flex-direction",
        L::FlexWrap => "flex-wrap",
        L::JustifyContent => "justify-content",
        L::AlignItems => "align-items",
        L::AlignSelf => "align-self",
        L::RowGap => "row-gap",
        L::ColumnGap => "column-gap",
        L::FlexGrow => "flex-grow",
        L::FlexShrink => "flex-shrink",
        L::FlexBasis => "flex-basis",
        L::Order => "order",
        L::GridTemplateColumns => "grid-template-columns",
        L::OverflowX => "overflow-x",
        L::OverflowY => "overflow-y",
        // Not painted on a created box: an element they match keeps its
        // template when the converter could write it one.
        L::BackgroundImage => "background-image",
        L::BoxShadow => "box-shadow",
        L::Transform => "transform",
        L::Filter => "filter",
        L::TransitionProperty => "transition-property",
        L::AnimationName => "animation-name",
        _ => return None,
    })
}

fn wide(keyword: &CssWide) -> &'static str {
    match keyword {
        CssWide::Initial => "initial",
        CssWide::Inherit => "inherit",
        CssWide::Unset => "unset",
        CssWide::Revert => "revert",
        CssWide::RevertLayer => "revert-layer",
    }
}

/// `a, :is(b, c) > d` → its top-level selectors.
pub(crate) fn split_list(text: &str) -> Vec<&str> {
    let (mut depth, mut start) = (0_i32, 0);
    let mut parts = Vec::new();
    for (at, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

/// `:is()` counts its most specific argument, and compiles to one selector per
/// argument, so each top-level part is measured on its own.
fn part_specificity(part: &str) -> Option<(u32, u32, u32)> {
    crate::selectors::parse_selector_list_str(part)
        .selectors
        .iter()
        .map(|s| (s.specificity.0, s.specificity.1, s.specificity.2))
        .max()
}

pub(crate) fn runtime_rules(sheets: &[Stylesheet], viewport: &Viewport) -> Vec<RuntimeRule> {
    let ranks = layer_ranks(sheets, viewport);
    let ua = user_agent_stylesheet();
    let mut out = Vec::new();
    for sheet in std::iter::once(ua).chain(sheets.iter()) {
        let user_agent = sheet.origin == Origin::UserAgent;
        for rule in &sheet.inner.rules {
            let Some(text) = &rule.selector_text else {
                continue;
            };
            if !rule.media.iter().all(|m| m.matches(viewport)) {
                continue;
            }
            let mut declarations: Vec<(String, String, bool)> = Vec::new();
            for d in &rule.declarations {
                let entry = match &d.value {
                    DeclValue::Expanded(l, v) => property(*l)
                        .or_else(|| box_property(*l))
                        .map(|name| (name.to_owned(), v.clone())),
                    DeclValue::Wide(l, w) => property(*l)
                        .or_else(|| box_property(*l))
                        .map(|name| (name.to_owned(), wide(w).to_owned())),
                    // One entry per longhand it may set; the runtime expands it
                    // once the variables are known.
                    DeclValue::PendingVar {
                        prop_name,
                        raw_value,
                        ..
                    } => Some((prop_name.to_ascii_lowercase(), raw_value.clone())),
                    DeclValue::Custom { name, value } => Some((
                        name.clone(),
                        match value {
                            CustomDeclValue::Tokens(t) => t.clone(),
                            CustomDeclValue::Wide(w) => wide(w).to_owned(),
                        },
                    )),
                };
                if let Some((name, value)) = entry {
                    let decl = (name, value, d.important);
                    if declarations.last() != Some(&decl) {
                        declarations.push(decl);
                    }
                }
            }
            if declarations.is_empty() {
                continue;
            }
            let layer = if user_agent {
                None
            } else {
                rule.layer.as_deref().and_then(|path| ranks.rank(path))
            };
            for part in split_list(text) {
                let Some(specificity) = part_specificity(part) else {
                    continue;
                };
                out.push(RuntimeRule {
                    selector: part.to_owned(),
                    specificity,
                    user_agent,
                    layer,
                    declarations: declarations.clone(),
                });
            }
        }
    }
    out
}

pub(crate) fn inline_rules(sheets: &[Stylesheet], viewport: &Viewport) -> Vec<InlineRule> {
    let ua = user_agent_stylesheet();
    let mut out = Vec::new();
    for sheet in std::iter::once(ua).chain(sheets.iter()) {
        for rule in &sheet.inner.rules {
            let Some(text) = &rule.selector_text else {
                continue;
            };
            if !rule.media.iter().all(|m| m.matches(viewport)) {
                continue;
            }
            let declarations: Vec<_> = rule
                .declarations
                .iter()
                .filter_map(|d| {
                    let (longhand, value) = match &d.value {
                        DeclValue::Expanded(l, v) => (*l, v.clone()),
                        DeclValue::Wide(l, w) => (*l, wide(w).to_owned()),
                        _ => return None,
                    };
                    Some((property(longhand)?.to_owned(), value, d.important))
                })
                .collect();
            if declarations.is_empty() {
                continue;
            }
            let parts = split_list(text);
            let aligned = parts.len() == rule.selectors.len();
            let highest = rule.selectors.iter().map(|s| s.specificity).max();
            for (i, part) in parts.into_iter().enumerate() {
                // `:is()` compiles to several selectors, so the text may not
                // line up with them; the highest is what `:is()` counts anyway.
                let selector = aligned.then(|| &rule.selectors[i]);
                if selector.map_or(part.contains("::"), |s| s.pseudo_element.is_some()) {
                    continue;
                }
                let Some(spec) = selector.map(|s| s.specificity).or(highest) else {
                    continue;
                };
                out.push(InlineRule {
                    selector: part.to_owned(),
                    specificity: (spec.0, spec.1, spec.2),
                    user_agent: sheet.origin == Origin::UserAgent,
                    declarations: declarations.clone(),
                });
            }
        }
    }
    out
}
