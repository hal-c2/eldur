//! The cascade's input as data the runtime can evaluate itself: an inline
//! element has no QML object for the static class variants to be written on,
//! and a created element's class is often only known when the script runs.

use crate::cascade::layer_ranks;
use crate::parse::{CustomDeclValue, DeclValue};
use crate::properties::{CssWide, Longhand};
use crate::{
    ContentItem, CounterChange, ListStyleType, Origin, Stylesheet, Viewport, user_agent_stylesheet,
};

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

/// What a `::before` or `::after` selector generates, for an element whose class
/// a script sets: the cascade cannot know at conversion time that it will match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PseudoRule {
    /// The selector of the element, without the pseudo-element.
    pub selector: String,
    pub after: bool,
    /// `(id, class, type)`, the pseudo-element included.
    pub specificity: (u32, u32, u32),
    /// The text, with `attr(x)` as [`PseudoPart::Attr`]; empty for `content: none`.
    pub content: Vec<PseudoPart>,
    /// Whether the rule declares `content`; one that does not only draws
    /// the text another rule gives.
    pub sets_content: bool,
    /// `(property, value, important)` the generated text is painted with, as
    /// in [`InlineRule::declarations`].
    pub declarations: Vec<(String, String, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoPart {
    Text(String),
    Attr(String),
    /// `counter(name, style)`.
    Counter(String, ListStyleType),
    /// `counters(name, separator, style)`.
    Counters(String, String, ListStyleType),
}

/// What one selector does to the counters, for generated text that numbers
/// the elements a script adds, moves or removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterRule {
    pub selector: String,
    /// `(id, class, type)`.
    pub specificity: (u32, u32, u32),
    /// `(property, changes, important)`, the property one of `counter-reset`,
    /// `counter-increment` and `counter-set`; `none` is no changes.
    pub changes: Vec<(String, Vec<CounterChange>, bool)>,
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
        L::TextDecorationStyle => "text-decoration-style",
        L::TextDecorationColor => "text-decoration-color",
        L::TextAlign => "text-align",
        L::Opacity => "opacity",
        L::Position => "position",
        L::Top => "top",
        L::Right => "right",
        L::Bottom => "bottom",
        L::Left => "left",
        L::ZIndex => "z-index",
        L::BoxSizing => "box-sizing",
        L::BorderCollapse => "border-collapse",
        L::BorderSpacing => "border-spacing",
        L::TableLayout => "table-layout",
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
        // A created box beside floats is laid out by the runtime.
        L::Float => "float",
        L::Clear => "clear",
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
        L::AnimationDuration => "animation-duration",
        L::AnimationTimingFunction => "animation-timing-function",
        L::AnimationDelay => "animation-delay",
        L::AnimationIterationCount => "animation-iteration-count",
        L::AnimationDirection => "animation-direction",
        L::AnimationFillMode => "animation-fill-mode",
        L::AnimationPlayState => "animation-play-state",
        L::TransformOrigin => "transform-origin",
        L::TransitionDuration => "transition-duration",
        L::TransitionTimingFunction => "transition-timing-function",
        L::TransitionDelay => "transition-delay",
        // How a created box places its background layers.
        L::BackgroundSize => "background-size",
        L::BackgroundPositionX => "background-position-x",
        L::BackgroundPositionY => "background-position-y",
        L::BackgroundRepeat => "background-repeat",
        L::BackgroundOrigin => "background-origin",
        L::BackgroundClip => "background-clip",
        L::BackgroundAttachment => "background-attachment",
        L::ObjectFit => "object-fit",
        L::TextShadow => "text-shadow",
        L::OutlineWidth => "outline-width",
        L::OutlineStyle => "outline-style",
        L::OutlineColor => "outline-color",
        L::OutlineOffset => "outline-offset",
        L::ObjectPosition => "object-position",
        _ => return None,
    })
}

/// One value of a paint property, parsed for the runtime, which has no CSS parser.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintValue {
    /// Layers, top first; `None` for a `none` layer.
    Images(Vec<Option<crate::Image>>),
    Shadows(Vec<crate::Shadow>),
    Transform(Vec<crate::TransformFn>),
}

pub(crate) fn parse_paint_value(
    property: &str,
    text: &str,
    viewport: &Viewport,
) -> Option<PaintValue> {
    // A font-relative length depends on the element, and the runtime looks this
    // up by the text alone: it says the value was not painted instead.
    let relative = text.to_ascii_lowercase();
    if relative.contains("em") && relative.chars().any(|c| c.is_ascii_digit()) {
        let bytes = relative.as_bytes();
        let unit = relative.match_indices("em").any(|(at, _)| {
            let before = at
                .checked_sub(1)
                .map(|b| {
                    if bytes[b] == b'r' {
                        b.saturating_sub(1)
                    } else {
                        b
                    }
                })
                .map(|b| bytes[b]);
            let after = bytes.get(at + 2).copied();
            before.is_some_and(|b| b.is_ascii_digit() || b == b'.')
                && !after.is_some_and(|a| a.is_ascii_alphanumeric())
        });
        if unit {
            return None;
        }
    }
    if relative.contains("currentcolor") {
        return None;
    }
    let longhand = match property {
        "background-image" => Longhand::BackgroundImage,
        "box-shadow" => Longhand::BoxShadow,
        "transform" => Longhand::Transform,
        _ => return None,
    };
    let mut style = crate::ComputedStyle::default();
    let context = crate::properties::ComputeContext {
        parent: None,
        root_font_size: 16.0,
        viewport,
    };
    crate::properties::compute(&mut style, longhand, text, &context).ok()?;
    Some(match longhand {
        Longhand::BackgroundImage => PaintValue::Images(
            style
                .background_layers
                .into_iter()
                .map(|l| l.image)
                .collect(),
        ),
        Longhand::BoxShadow => PaintValue::Shadows(style.box_shadow),
        _ => PaintValue::Transform(style.transform),
    })
}

/// The longhands of a whole `background` value: `(image, colour)`.
pub(crate) fn background_parts(value: &str) -> Option<(Option<String>, Option<String>)> {
    let pairs = crate::properties::expand("background", value).ok()?;
    let find = |wanted: Longhand| {
        pairs
            .iter()
            .find(|(l, _)| *l == wanted)
            .map(|(_, v)| v.clone())
    };
    Some((
        find(Longhand::BackgroundImage),
        find(Longhand::BackgroundColor),
    ))
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

/// The declarations of a rule the runtime paints inline text with.
fn paint_declarations(declarations: &[crate::parse::ParsedDecl]) -> Vec<(String, String, bool)> {
    declarations
        .iter()
        .filter_map(|d| {
            let (longhand, value) = match &d.value {
                DeclValue::Expanded(l, v) => (*l, v.clone()),
                DeclValue::Wide(l, w) => (*l, wide(w).to_owned()),
                _ => return None,
            };
            Some((property(longhand)?.to_owned(), value, d.important))
        })
        .collect()
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
            let declarations = paint_declarations(&rule.declarations);
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

/// The page's `content` declarations on `::before` and `::after`. Only inline
/// generated text can follow a class at run time: a rule that also lays the
/// pseudo-element out as a box, or whose text is not text or a counter, is not
/// listed.
pub(crate) fn pseudo_rules(sheets: &[Stylesheet], viewport: &Viewport) -> Vec<PseudoRule> {
    let mut out = Vec::new();
    for sheet in sheets {
        for rule in &sheet.inner.rules {
            let Some(text) = &rule.selector_text else {
                continue;
            };
            if !rule.media.iter().all(|m| m.matches(viewport)) {
                continue;
            }
            let boxed = rule.declarations.iter().any(|d| {
                matches!(&d.value, DeclValue::Expanded(l, _)
                    if matches!(l, Longhand::Display | Longhand::Position | Longhand::Float))
            });
            let content = rule.declarations.iter().rev().find_map(|d| match &d.value {
                DeclValue::Expanded(Longhand::Content, v) => Some(pseudo_content(v)),
                _ => None,
            });
            // Text the runtime cannot make stays as the converter wrote it.
            let Some(content) = content.map_or(Some(None), |c| c.map(Some)) else {
                continue;
            };
            // A rule may say only how the text another rule gives is drawn.
            let declarations: Vec<_> = paint_declarations(&rule.declarations)
                .into_iter()
                .filter(|(p, ..)| p != "display")
                .collect();
            if boxed || (content.is_none() && declarations.is_empty()) {
                continue;
            }
            let sets_content = content.is_some();
            let content = content.unwrap_or_default();
            for part in split_list(text) {
                let part = part.trim();
                let (element, after) = if let Some(e) = strip_pseudo(part, "after") {
                    (e, true)
                } else if let Some(e) = strip_pseudo(part, "before") {
                    (e, false)
                } else {
                    continue;
                };
                let Some(specificity) = part_specificity(part) else {
                    continue;
                };
                let selector = if element.is_empty() { "*" } else { element };
                out.push(PseudoRule {
                    selector: selector.to_owned(),
                    after,
                    specificity,
                    content: content.clone(),
                    sets_content,
                    declarations: declarations.clone(),
                });
            }
        }
    }
    out
}

pub(crate) fn counter_rules(sheets: &[Stylesheet], viewport: &Viewport) -> Vec<CounterRule> {
    let mut out = Vec::new();
    for sheet in sheets {
        for rule in &sheet.inner.rules {
            let Some(text) = &rule.selector_text else {
                continue;
            };
            if !rule.media.iter().all(|m| m.matches(viewport)) {
                continue;
            }
            let changes: Vec<_> = rule
                .declarations
                .iter()
                .filter_map(|d| {
                    let DeclValue::Expanded(longhand, v) = &d.value else {
                        return None;
                    };
                    let (property, default) = match longhand {
                        Longhand::CounterReset => ("counter-reset", 0),
                        Longhand::CounterIncrement => ("counter-increment", 1),
                        Longhand::CounterSet => ("counter-set", 0),
                        _ => return None,
                    };
                    let changes = crate::properties::parse_counter_value(v, default)?;
                    Some((property.to_owned(), changes, d.important))
                })
                .collect();
            if changes.is_empty() {
                continue;
            }
            for part in split_list(text) {
                let part = part.trim();
                // A pseudo-element's own counters are not followed.
                if part.contains("::") || part.ends_with(":before") || part.ends_with(":after") {
                    continue;
                }
                let Some(specificity) = part_specificity(part) else {
                    continue;
                };
                out.push(CounterRule {
                    selector: part.to_owned(),
                    specificity,
                    changes: changes.clone(),
                });
            }
        }
    }
    out
}

fn strip_pseudo<'a>(part: &'a str, name: &str) -> Option<&'a str> {
    let rest = part.strip_suffix(name)?;
    rest.strip_suffix("::")
        .or_else(|| rest.strip_suffix(':'))
        .map(str::trim_end)
}

fn pseudo_content(value: &str) -> Option<Vec<PseudoPart>> {
    let items = crate::properties::parse_content_value(value)?;
    let mut parts = Vec::new();
    for item in items.iter().flatten() {
        parts.push(match item {
            ContentItem::String(s) => PseudoPart::Text(s.clone()),
            ContentItem::Attr(name) => PseudoPart::Attr(name.clone()),
            ContentItem::OpenQuote => PseudoPart::Text("\u{201c}".to_owned()),
            ContentItem::CloseQuote => PseudoPart::Text("\u{201d}".to_owned()),
            ContentItem::Counter(name, style) => PseudoPart::Counter(name.clone(), *style),
            ContentItem::Counters(name, sep, style) => {
                PseudoPart::Counters(name.clone(), sep.clone(), *style)
            }
            _ => return None,
        });
    }
    Some(parts)
}
