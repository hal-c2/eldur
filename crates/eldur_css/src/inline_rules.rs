//! The cascade's input for the properties inline text can show, as data the
//! runtime can evaluate itself: an inline element has no QML object for the
//! static class variants to be written on.

use crate::parse::DeclValue;
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
fn split_list(text: &str) -> Vec<&str> {
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
