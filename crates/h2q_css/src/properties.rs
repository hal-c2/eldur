//! CSS property table, shorthand expansion, and value parsing → [`ComputedStyle`].
//!
//! This module is the contract between the stylesheet parser / cascade and the
//! value parsers under `properties/*`:
//! - [`kind`] classifies any CSS property name.
//! - [`expand`] and [`longhands_of`] decompose shorthands into [`Longhand`]s.
//! - [`is_inherited`], [`priority`], [`compute`], and [`copy`] drive computed-value
//!   resolution into [`ComputedStyle`].

mod animation;
mod background;
mod box_model;
mod color;
mod cursor;
mod flex;
mod font;
mod grid;
mod length;
mod shorthand;
mod text;
mod transform;

use crate::{ComputedStyle, Viewport};

use self::{
    animation::{
        apply_animation_field, apply_animation_names, apply_transition_field,
        apply_transition_properties, parse_animation_direction_list,
        parse_animation_fill_mode_list, parse_animation_iteration_list, parse_animation_name_list,
        parse_animation_play_state_list, parse_easing_list, parse_time_list,
        parse_transition_property_list,
    },
    background::{
        apply_background_images, apply_background_prop, parse_background_color,
        parse_background_image_list, parse_bg_position_axis_list, parse_bg_repeat_list,
        parse_bg_size_list, parse_filter, parse_object_fit, parse_opacity, parse_overflow,
        parse_shadow_list, validate_bg_attachment_list, validate_bg_box_list,
    },
    box_model::{
        apply_border_style, apply_border_width, parse_aspect_ratio, parse_border_style,
        parse_box_sizing, parse_corner_radius, parse_display, parse_float, parse_inset_side,
        parse_line_width, parse_margin_side, parse_outline_offset, parse_padding_side,
        parse_position, parse_size, parse_z_index,
    },
    color::parse_color_cvs,
    cursor::{Cursor, parse_component_values, serialize_cvs},
    flex::{
        AlignTarget, parse_align, parse_flex_basis, parse_flex_direction, parse_flex_factor,
        parse_flex_wrap, parse_gap_side, parse_order,
    },
    font::{
        parse_font_family, parse_font_size, parse_font_style, parse_font_variant_caps,
        parse_font_weight, parse_line_height,
    },
    grid::{
        parse_grid_auto_flow, parse_grid_auto_tracks, parse_grid_line, parse_grid_template_areas,
        parse_grid_template_tracks,
    },
    length::UnitContext,
    shorthand::{expand_shorthand, shorthand_longhands},
    text::{
        compose_white_space, decompose_white_space, parse_content, parse_cursor, parse_direction,
        parse_list_style_image, parse_list_style_position, parse_list_style_type,
        parse_overflow_wrap, parse_pointer_events, parse_spacing, parse_text_align,
        parse_text_decoration_color, parse_text_decoration_line, parse_text_decoration_style,
        parse_text_indent, parse_text_overflow, parse_text_shadow, parse_text_transform,
        parse_text_wrap_mode, parse_user_select, parse_vertical_align, parse_visibility,
        parse_white_space_collapse, parse_word_break,
    },
    transform::{
        parse_individual_rotate, parse_individual_scale, parse_individual_translate,
        parse_transform_list, parse_transform_origin,
    },
};

/// Every supported CSS longhand property mapped onto [`ComputedStyle`].
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Longhand {
    // Existing contract variants (names preserved verbatim)
    Display,
    Color,
    FontSize,
    MarginTop,

    // Box generation & positioning
    Position,
    Top,
    Right,
    Bottom,
    Left,
    ZIndex,
    Float,
    BoxSizing,

    // Sizing
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    AspectRatio,

    // Margins & padding
    MarginRight,
    MarginBottom,
    MarginLeft,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,

    // Borders & radii
    BorderTopWidth,
    BorderRightWidth,
    BorderBottomWidth,
    BorderLeftWidth,
    BorderTopStyle,
    BorderRightStyle,
    BorderBottomStyle,
    BorderLeftStyle,
    BorderTopColor,
    BorderRightColor,
    BorderBottomColor,
    BorderLeftColor,
    BorderTopLeftRadius,
    BorderTopRightRadius,
    BorderBottomRightRadius,
    BorderBottomLeftRadius,

    // Outline
    OutlineWidth,
    OutlineStyle,
    OutlineColor,
    OutlineOffset,

    // Flexbox & Box Alignment
    FlexDirection,
    FlexWrap,
    JustifyContent,
    AlignItems,
    AlignSelf,
    AlignContent,
    JustifyItems,
    JustifySelf,
    RowGap,
    ColumnGap,
    FlexGrow,
    FlexShrink,
    FlexBasis,
    Order,

    // Grid
    GridTemplateColumns,
    GridTemplateRows,
    GridTemplateAreas,
    GridAutoFlow,
    GridAutoColumns,
    GridAutoRows,
    GridColumnStart,
    GridColumnEnd,
    GridRowStart,
    GridRowEnd,

    // Backgrounds & visual effects
    BackgroundColor,
    BackgroundImage,
    BackgroundPositionX,
    BackgroundPositionY,
    BackgroundSize,
    BackgroundRepeat,
    BackgroundAttachment,
    BackgroundOrigin,
    BackgroundClip,
    BoxShadow,
    Opacity,
    OverflowX,
    OverflowY,
    Transform,
    Translate,
    Rotate,
    Scale,
    TransformOrigin,
    ObjectFit,
    Filter,

    // Fonts & text (inherited)
    FontFamily,
    FontWeight,
    FontStyle,
    FontVariantCaps,
    LineHeight,
    LetterSpacing,
    WordSpacing,
    TextAlign,
    TextIndent,
    TextTransform,
    WhiteSpaceCollapse,
    TextWrapMode,
    WordBreak,
    OverflowWrap,
    TextShadow,
    Direction,
    Visibility,
    Cursor,
    PointerEvents,
    ListStyleType,
    ListStylePosition,
    ListStyleImage,

    // Non-inherited text decoration, UI & generated content
    UserSelect,
    TextDecorationLine,
    TextDecorationColor,
    TextDecorationStyle,
    TextOverflow,
    VerticalAlign,
    Content,

    // Transitions & animations
    TransitionProperty,
    TransitionDuration,
    TransitionDelay,
    TransitionTimingFunction,
    AnimationName,
    AnimationDuration,
    AnimationDelay,
    AnimationTimingFunction,
    AnimationIterationCount,
    AnimationDirection,
    AnimationFillMode,
    AnimationPlayState,
}

impl Longhand {
    /// Complete list of supported longhands (used by `all` and exhaustive tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const ALL: &'static [Self] = &[
        Self::Display,
        Self::Color,
        Self::FontSize,
        Self::MarginTop,
        Self::Position,
        Self::Top,
        Self::Right,
        Self::Bottom,
        Self::Left,
        Self::ZIndex,
        Self::Float,
        Self::BoxSizing,
        Self::Width,
        Self::Height,
        Self::MinWidth,
        Self::MinHeight,
        Self::MaxWidth,
        Self::MaxHeight,
        Self::AspectRatio,
        Self::MarginRight,
        Self::MarginBottom,
        Self::MarginLeft,
        Self::PaddingTop,
        Self::PaddingRight,
        Self::PaddingBottom,
        Self::PaddingLeft,
        Self::BorderTopWidth,
        Self::BorderRightWidth,
        Self::BorderBottomWidth,
        Self::BorderLeftWidth,
        Self::BorderTopStyle,
        Self::BorderRightStyle,
        Self::BorderBottomStyle,
        Self::BorderLeftStyle,
        Self::BorderTopColor,
        Self::BorderRightColor,
        Self::BorderBottomColor,
        Self::BorderLeftColor,
        Self::BorderTopLeftRadius,
        Self::BorderTopRightRadius,
        Self::BorderBottomRightRadius,
        Self::BorderBottomLeftRadius,
        Self::OutlineWidth,
        Self::OutlineStyle,
        Self::OutlineColor,
        Self::OutlineOffset,
        Self::FlexDirection,
        Self::FlexWrap,
        Self::JustifyContent,
        Self::AlignItems,
        Self::AlignSelf,
        Self::AlignContent,
        Self::JustifyItems,
        Self::JustifySelf,
        Self::RowGap,
        Self::ColumnGap,
        Self::FlexGrow,
        Self::FlexShrink,
        Self::FlexBasis,
        Self::Order,
        Self::GridTemplateColumns,
        Self::GridTemplateRows,
        Self::GridTemplateAreas,
        Self::GridAutoFlow,
        Self::GridAutoColumns,
        Self::GridAutoRows,
        Self::GridColumnStart,
        Self::GridColumnEnd,
        Self::GridRowStart,
        Self::GridRowEnd,
        Self::BackgroundColor,
        Self::BackgroundImage,
        Self::BackgroundPositionX,
        Self::BackgroundPositionY,
        Self::BackgroundSize,
        Self::BackgroundRepeat,
        Self::BackgroundAttachment,
        Self::BackgroundOrigin,
        Self::BackgroundClip,
        Self::BoxShadow,
        Self::Opacity,
        Self::OverflowX,
        Self::OverflowY,
        Self::Transform,
        Self::Translate,
        Self::Rotate,
        Self::Scale,
        Self::TransformOrigin,
        Self::ObjectFit,
        Self::Filter,
        Self::FontFamily,
        Self::FontWeight,
        Self::FontStyle,
        Self::FontVariantCaps,
        Self::LineHeight,
        Self::LetterSpacing,
        Self::WordSpacing,
        Self::TextAlign,
        Self::TextIndent,
        Self::TextTransform,
        Self::WhiteSpaceCollapse,
        Self::TextWrapMode,
        Self::WordBreak,
        Self::OverflowWrap,
        Self::TextShadow,
        Self::Direction,
        Self::Visibility,
        Self::Cursor,
        Self::PointerEvents,
        Self::ListStyleType,
        Self::ListStylePosition,
        Self::ListStyleImage,
        Self::UserSelect,
        Self::TextDecorationLine,
        Self::TextDecorationColor,
        Self::TextDecorationStyle,
        Self::TextOverflow,
        Self::VerticalAlign,
        Self::Content,
        Self::TransitionProperty,
        Self::TransitionDuration,
        Self::TransitionDelay,
        Self::TransitionTimingFunction,
        Self::AnimationName,
        Self::AnimationDuration,
        Self::AnimationDelay,
        Self::AnimationTimingFunction,
        Self::AnimationIterationCount,
        Self::AnimationDirection,
        Self::AnimationFillMode,
        Self::AnimationPlayState,
    ];
}

/// CSS-wide keywords (CSS Cascade 5 §7.3), handled uniformly by the cascade.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CssWide {
    Inherit,
    Initial,
    Unset,
    Revert,
    RevertLayer,
}

impl CssWide {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        if let Some(kw) = Self::match_ident(trimmed) {
            return Some(kw);
        }
        // Fallback when comments surround the keyword (e.g. `/* reset */ inherit`).
        if trimmed.contains("/*")
            && let Ok(cvs) = parse_component_values(trimmed)
        {
            let mut cur = Cursor::new(&cvs);
            if let Some(id) = cur.eat_ident()
                && cur.is_eof()
            {
                return Self::match_ident(id);
            }
        }
        None
    }

    fn match_ident(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("inherit") {
            Some(Self::Inherit)
        } else if s.eq_ignore_ascii_case("initial") {
            Some(Self::Initial)
        } else if s.eq_ignore_ascii_case("unset") {
            Some(Self::Unset)
        } else if s.eq_ignore_ascii_case("revert") {
            Some(Self::Revert)
        } else if s.eq_ignore_ascii_case("revert-layer") {
            Some(Self::RevertLayer)
        } else {
            None
        }
    }
}

/// Classification of a CSS property name.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyKind {
    Longhand(Longhand),
    Shorthand,
    /// `--*`
    Custom,
    /// Known CSS property we do not model (report as unsupported, not unknown).
    Unsupported,
    Unknown,
}

/// Maps a lowercase CSS property name (including logical LTR `horizontal-tb`
/// aliases and trivial `-webkit-*` aliases) to its [`Longhand`].
fn longhand_by_name(lower: &str) -> Option<Longhand> {
    match lower {
        "display" => Some(Longhand::Display),
        "position" => Some(Longhand::Position),
        "top" | "inset-block-start" => Some(Longhand::Top),
        "right" | "inset-inline-end" => Some(Longhand::Right),
        "bottom" | "inset-block-end" => Some(Longhand::Bottom),
        "left" | "inset-inline-start" => Some(Longhand::Left),
        "z-index" => Some(Longhand::ZIndex),
        "float" => Some(Longhand::Float),
        "box-sizing" | "-webkit-box-sizing" | "-moz-box-sizing" => Some(Longhand::BoxSizing),

        "width" | "inline-size" => Some(Longhand::Width),
        "height" | "block-size" => Some(Longhand::Height),
        "min-width" | "min-inline-size" => Some(Longhand::MinWidth),
        "min-height" | "min-block-size" => Some(Longhand::MinHeight),
        "max-width" | "max-inline-size" => Some(Longhand::MaxWidth),
        "max-height" | "max-block-size" => Some(Longhand::MaxHeight),
        "aspect-ratio" => Some(Longhand::AspectRatio),

        "margin-top" | "margin-block-start" => Some(Longhand::MarginTop),
        "margin-right" | "margin-inline-end" => Some(Longhand::MarginRight),
        "margin-bottom" | "margin-block-end" => Some(Longhand::MarginBottom),
        "margin-left" | "margin-inline-start" => Some(Longhand::MarginLeft),

        "padding-top" | "padding-block-start" => Some(Longhand::PaddingTop),
        "padding-right" | "padding-inline-end" => Some(Longhand::PaddingRight),
        "padding-bottom" | "padding-block-end" => Some(Longhand::PaddingBottom),
        "padding-left" | "padding-inline-start" => Some(Longhand::PaddingLeft),

        "border-top-width" | "border-block-start-width" => Some(Longhand::BorderTopWidth),
        "border-right-width" | "border-inline-end-width" => Some(Longhand::BorderRightWidth),
        "border-bottom-width" | "border-block-end-width" => Some(Longhand::BorderBottomWidth),
        "border-left-width" | "border-inline-start-width" => Some(Longhand::BorderLeftWidth),

        "border-top-style" | "border-block-start-style" => Some(Longhand::BorderTopStyle),
        "border-right-style" | "border-inline-end-style" => Some(Longhand::BorderRightStyle),
        "border-bottom-style" | "border-block-end-style" => Some(Longhand::BorderBottomStyle),
        "border-left-style" | "border-inline-start-style" => Some(Longhand::BorderLeftStyle),

        "border-top-color" | "border-block-start-color" => Some(Longhand::BorderTopColor),
        "border-right-color" | "border-inline-end-color" => Some(Longhand::BorderRightColor),
        "border-bottom-color" | "border-block-end-color" => Some(Longhand::BorderBottomColor),
        "border-left-color" | "border-inline-start-color" => Some(Longhand::BorderLeftColor),

        "border-top-left-radius" | "border-start-start-radius" => {
            Some(Longhand::BorderTopLeftRadius)
        }
        "border-top-right-radius" | "border-start-end-radius" => {
            Some(Longhand::BorderTopRightRadius)
        }
        "border-bottom-right-radius" | "border-end-end-radius" => {
            Some(Longhand::BorderBottomRightRadius)
        }
        "border-bottom-left-radius" | "border-end-start-radius" => {
            Some(Longhand::BorderBottomLeftRadius)
        }

        "outline-width" => Some(Longhand::OutlineWidth),
        "outline-style" => Some(Longhand::OutlineStyle),
        "outline-color" => Some(Longhand::OutlineColor),
        "outline-offset" => Some(Longhand::OutlineOffset),

        "flex-direction" => Some(Longhand::FlexDirection),
        "flex-wrap" => Some(Longhand::FlexWrap),
        "justify-content" => Some(Longhand::JustifyContent),
        "align-items" => Some(Longhand::AlignItems),
        "align-self" => Some(Longhand::AlignSelf),
        "align-content" => Some(Longhand::AlignContent),
        "justify-items" => Some(Longhand::JustifyItems),
        "justify-self" => Some(Longhand::JustifySelf),
        "row-gap" | "grid-row-gap" => Some(Longhand::RowGap),
        "column-gap" | "grid-column-gap" => Some(Longhand::ColumnGap),
        "flex-grow" => Some(Longhand::FlexGrow),
        "flex-shrink" => Some(Longhand::FlexShrink),
        "flex-basis" => Some(Longhand::FlexBasis),
        "order" => Some(Longhand::Order),

        "grid-template-columns" => Some(Longhand::GridTemplateColumns),
        "grid-template-rows" => Some(Longhand::GridTemplateRows),
        "grid-template-areas" => Some(Longhand::GridTemplateAreas),
        "grid-auto-flow" => Some(Longhand::GridAutoFlow),
        "grid-auto-columns" => Some(Longhand::GridAutoColumns),
        "grid-auto-rows" => Some(Longhand::GridAutoRows),
        "grid-column-start" => Some(Longhand::GridColumnStart),
        "grid-column-end" => Some(Longhand::GridColumnEnd),
        "grid-row-start" => Some(Longhand::GridRowStart),
        "grid-row-end" => Some(Longhand::GridRowEnd),

        "background-color" => Some(Longhand::BackgroundColor),
        "background-image" => Some(Longhand::BackgroundImage),
        "background-position-x" => Some(Longhand::BackgroundPositionX),
        "background-position-y" => Some(Longhand::BackgroundPositionY),
        "background-size" => Some(Longhand::BackgroundSize),
        "background-repeat" => Some(Longhand::BackgroundRepeat),
        "background-attachment" => Some(Longhand::BackgroundAttachment),
        "background-origin" => Some(Longhand::BackgroundOrigin),
        "background-clip" | "-webkit-background-clip" => Some(Longhand::BackgroundClip),

        "box-shadow" | "-webkit-box-shadow" => Some(Longhand::BoxShadow),
        "opacity" => Some(Longhand::Opacity),
        "overflow-x" | "overflow-inline" => Some(Longhand::OverflowX),
        "overflow-y" | "overflow-block" => Some(Longhand::OverflowY),
        "transform" | "-webkit-transform" => Some(Longhand::Transform),
        "translate" => Some(Longhand::Translate),
        "rotate" => Some(Longhand::Rotate),
        "scale" => Some(Longhand::Scale),
        "transform-origin" | "-webkit-transform-origin" => Some(Longhand::TransformOrigin),
        "object-fit" => Some(Longhand::ObjectFit),
        "filter" | "-webkit-filter" => Some(Longhand::Filter),

        "color" => Some(Longhand::Color),
        "font-family" => Some(Longhand::FontFamily),
        "font-size" => Some(Longhand::FontSize),
        "font-weight" => Some(Longhand::FontWeight),
        "font-style" => Some(Longhand::FontStyle),
        "font-variant-caps" => Some(Longhand::FontVariantCaps),
        "line-height" => Some(Longhand::LineHeight),
        "letter-spacing" => Some(Longhand::LetterSpacing),
        "word-spacing" => Some(Longhand::WordSpacing),
        "text-align" => Some(Longhand::TextAlign),
        "text-indent" => Some(Longhand::TextIndent),
        "text-transform" => Some(Longhand::TextTransform),
        "white-space-collapse" => Some(Longhand::WhiteSpaceCollapse),
        "text-wrap-mode" => Some(Longhand::TextWrapMode),
        "word-break" => Some(Longhand::WordBreak),
        "overflow-wrap" | "word-wrap" => Some(Longhand::OverflowWrap),
        "text-shadow" => Some(Longhand::TextShadow),
        "direction" => Some(Longhand::Direction),
        "visibility" => Some(Longhand::Visibility),
        "cursor" => Some(Longhand::Cursor),
        "pointer-events" => Some(Longhand::PointerEvents),
        "list-style-type" => Some(Longhand::ListStyleType),
        "list-style-position" => Some(Longhand::ListStylePosition),
        "list-style-image" => Some(Longhand::ListStyleImage),

        "user-select" | "-webkit-user-select" => Some(Longhand::UserSelect),
        "text-decoration-line" => Some(Longhand::TextDecorationLine),
        "text-decoration-color" => Some(Longhand::TextDecorationColor),
        "text-decoration-style" => Some(Longhand::TextDecorationStyle),
        "text-overflow" => Some(Longhand::TextOverflow),
        "vertical-align" => Some(Longhand::VerticalAlign),
        "content" => Some(Longhand::Content),

        "transition-property" | "-webkit-transition-property" => Some(Longhand::TransitionProperty),
        "transition-duration" | "-webkit-transition-duration" => Some(Longhand::TransitionDuration),
        "transition-delay" | "-webkit-transition-delay" => Some(Longhand::TransitionDelay),
        "transition-timing-function" | "-webkit-transition-timing-function" => {
            Some(Longhand::TransitionTimingFunction)
        }

        "animation-name" | "-webkit-animation-name" => Some(Longhand::AnimationName),
        "animation-duration" | "-webkit-animation-duration" => Some(Longhand::AnimationDuration),
        "animation-delay" | "-webkit-animation-delay" => Some(Longhand::AnimationDelay),
        "animation-timing-function" | "-webkit-animation-timing-function" => {
            Some(Longhand::AnimationTimingFunction)
        }
        "animation-iteration-count" | "-webkit-animation-iteration-count" => {
            Some(Longhand::AnimationIterationCount)
        }
        "animation-direction" | "-webkit-animation-direction" => {
            Some(Longhand::AnimationDirection)
        }
        "animation-fill-mode" | "-webkit-animation-fill-mode" => Some(Longhand::AnimationFillMode),
        "animation-play-state" | "-webkit-animation-play-state" => {
            Some(Longhand::AnimationPlayState)
        }

        _ => None,
    }
}

/// Known standard or vendor-prefixed CSS properties not modelled in [`ComputedStyle`].
fn is_known_unsupported(lower: &str) -> bool {
    if lower.starts_with("-webkit-")
        || lower.starts_with("-moz-")
        || lower.starts_with("-ms-")
        || lower.starts_with("-o-")
        || lower.starts_with("scroll-margin")
        || lower.starts_with("scroll-padding")
        || lower.starts_with("scroll-snap")
        || lower.starts_with("mask-")
        || lower.starts_with("border-image")
        || lower.starts_with("contain-intrinsic-")
        || lower.starts_with("font-synthesis")
        || lower.starts_with("font-variant-")
        || lower.starts_with("text-emphasis")
        || lower.starts_with("overscroll-behavior")
        || lower.starts_with("page-break-")
        || lower.starts_with("counter-")
        || lower.starts_with("column-rule")
        || lower.starts_with("offset-")
        || lower.starts_with("view-timeline")
        || lower.starts_with("scroll-timeline")
        || lower.starts_with("animation-range")
    {
        return true;
    }

    matches!(
        lower,
        "mask"
            | "clip"
            | "clip-path"
            | "mix-blend-mode"
            | "background-blend-mode"
            | "backdrop-filter"
            | "isolation"
            | "will-change"
            | "contain"
            | "content-visibility"
            | "columns"
            | "column-count"
            | "column-width"
            | "column-span"
            | "column-fill"
            | "scroll-behavior"
            | "scrollbar-width"
            | "scrollbar-color"
            | "scrollbar-gutter"
            | "touch-action"
            | "resize"
            | "appearance"
            | "accent-color"
            | "caret-color"
            | "color-scheme"
            | "forced-color-adjust"
            | "print-color-adjust"
            | "field-sizing"
            | "interactivity"
            | "tab-size"
            | "hyphens"
            | "hyphenate-character"
            | "hyphenate-limit-chars"
            | "quotes"
            | "unicode-bidi"
            | "writing-mode"
            | "text-orientation"
            | "text-combine-upright"
            | "text-justify"
            | "text-align-last"
            | "text-underline-offset"
            | "text-underline-position"
            | "text-decoration-thickness"
            | "text-decoration-skip-ink"
            | "text-wrap-style"
            | "line-break"
            | "hanging-punctuation"
            | "initial-letter"
            | "ruby-position"
            | "ruby-align"
            | "font-feature-settings"
            | "font-variation-settings"
            | "font-kerning"
            | "font-stretch"
            | "font-width"
            | "font-optical-sizing"
            | "font-size-adjust"
            | "font-palette"
            | "font-language-override"
            | "text-rendering"
            | "page"
            | "break-before"
            | "break-after"
            | "break-inside"
            | "orphans"
            | "widows"
            | "box-decoration-break"
            | "table-layout"
            | "border-collapse"
            | "border-spacing"
            | "caption-side"
            | "empty-cells"
            | "perspective"
            | "perspective-origin"
            | "backface-visibility"
            | "transform-style"
            | "transform-box"
            | "offset"
            | "view-transition-name"
            | "transition-behavior"
            | "animation-composition"
            | "animation-timeline"
            | "timeline-scope"
            | "object-position"
            | "image-rendering"
            | "image-orientation"
            | "shape-outside"
            | "shape-margin"
            | "shape-image-threshold"
            | "clear"
            | "zoom"
            | "container"
            | "container-name"
            | "container-type"
            | "anchor-name"
            | "position-anchor"
            | "position-area"
            | "fill"
            | "fill-opacity"
            | "fill-rule"
            | "stroke"
            | "stroke-width"
            | "stroke-opacity"
            | "stroke-linecap"
            | "stroke-linejoin"
            | "stroke-miterlimit"
            | "stroke-dasharray"
            | "stroke-dashoffset"
            | "paint-order"
            | "vector-effect"
            | "stop-color"
            | "stop-opacity"
            | "flood-color"
            | "flood-opacity"
            | "lighting-color"
            | "color-interpolation"
            | "color-interpolation-filters"
            | "shape-rendering"
            | "dominant-baseline"
            | "alignment-baseline"
            | "text-anchor"
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn kind(name: &str) -> PropertyKind {
    let trimmed = name.trim();
    if trimmed.starts_with("--") && trimmed.len() > 2 {
        return PropertyKind::Custom;
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(lh) = longhand_by_name(&lower) {
        return PropertyKind::Longhand(lh);
    }
    if lower == "all" || shorthand_longhands(&lower).is_some() {
        return PropertyKind::Shorthand;
    }
    if is_known_unsupported(&lower) {
        return PropertyKind::Unsupported;
    }
    PropertyKind::Unknown
}

/// Expand a shorthand (or validate a single longhand) declaration into
/// `(Longhand, raw_value)` pairs.
///
/// `value` has already had `var()` substituted and is not a CSS-wide keyword.
/// Returns `Err(reason)` if the shorthand value is syntactically invalid.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn expand(name: &str, value: &str) -> Result<Vec<(Longhand, String)>, String> {
    let trimmed_name = name.trim();
    let lower = trimmed_name.to_ascii_lowercase();

    if lower == "all" {
        return Err("the 'all' shorthand only accepts CSS-wide keywords".to_owned());
    }

    let vp = Viewport::default();
    let dummy_ctx = ComputeContext {
        parent: None,
        root_font_size: 16.0,
        viewport: &vp,
    };

    if let Some(lh) = longhand_by_name(&lower) {
        let cvs = parse_component_values(value)?;
        let canonical = serialize_cvs(&cvs);
        let mut style = ComputedStyle::default();
        compute(&mut style, lh, &canonical, &dummy_ctx)?;
        return Ok(vec![(lh, canonical)]);
    }

    if shorthand_longhands(&lower).is_some() {
        let cvs = parse_component_values(value)?;
        let pairs = expand_shorthand(&lower, &cvs)?;

        // Validate every expanded sub-declaration against `compute` in priority order.
        let mut ordered: Vec<(Longhand, &str)> =
            pairs.iter().map(|(l, v)| (*l, v.as_str())).collect();
        ordered.sort_by_key(|(l, _)| priority(*l));
        let mut style = ComputedStyle::default();
        for (lh, sub_val) in ordered {
            compute(&mut style, lh, sub_val, &dummy_ctx)?;
        }
        return Ok(pairs);
    }

    Err(format!("unsupported or unknown property '{trimmed_name}'"))
}

/// Longhands covered by a shorthand (for `margin: inherit` etc.).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn longhands_of(name: &str) -> Vec<Longhand> {
    let lower = name.trim().to_ascii_lowercase();
    if lower == "all" {
        // CSS Cascade 4 §3.1: `all` resets every property except `direction` and `unicode-bidi`.
        return Longhand::ALL
            .iter()
            .copied()
            .filter(|&l| l != Longhand::Direction)
            .collect();
    }
    if let Some(lhs) = shorthand_longhands(&lower) {
        return lhs;
    }
    if let Some(lh) = longhand_by_name(&lower) {
        return vec![lh];
    }
    Vec::new()
}

/// Whether `l` inherits by default in CSS.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn is_inherited(l: Longhand) -> bool {
    matches!(
        l,
        Longhand::Color
            | Longhand::FontFamily
            | Longhand::FontSize
            | Longhand::FontWeight
            | Longhand::FontStyle
            | Longhand::FontVariantCaps
            | Longhand::LineHeight
            | Longhand::LetterSpacing
            | Longhand::WordSpacing
            | Longhand::TextAlign
            | Longhand::TextIndent
            | Longhand::TextTransform
            | Longhand::WhiteSpaceCollapse
            | Longhand::TextWrapMode
            | Longhand::WordBreak
            | Longhand::OverflowWrap
            | Longhand::TextShadow
            | Longhand::Direction
            | Longhand::Visibility
            | Longhand::Cursor
            | Longhand::PointerEvents
            | Longhand::ListStyleType
            | Longhand::ListStylePosition
            | Longhand::ListStyleImage
    )
}

/// Properties that other properties depend on (`font-size` before `em`,
/// `line-height` before `lh`, `color` before `currentColor`, border styles
/// before border widths, `background-image` before layer props, and individual
/// transforms in `translate` → `rotate` → `scale` → `transform` order).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn priority(l: Longhand) -> u8 {
    match l {
        Longhand::FontSize => 0,
        Longhand::FontWeight
        | Longhand::FontStyle
        | Longhand::FontFamily
        | Longhand::FontVariantCaps
        | Longhand::LineHeight => 1,
        Longhand::Color => 2,
        Longhand::BorderTopStyle
        | Longhand::BorderRightStyle
        | Longhand::BorderBottomStyle
        | Longhand::BorderLeftStyle
        | Longhand::OutlineStyle
        | Longhand::BackgroundImage => 3,
        Longhand::Rotate | Longhand::TransitionProperty | Longhand::AnimationName => 5,
        Longhand::Scale => 6,
        Longhand::Transform => 7,
        _ => 4,
    }
}

/// Context needed to turn specified values into computed values.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ComputeContext<'a> {
    pub parent: Option<&'a ComputedStyle>,
    pub root_font_size: f32,
    pub viewport: &'a Viewport,
}

/// Parse `value` for longhand `l` and write the computed value into `style`.
/// `value` is not a CSS-wide keyword and contains no `var()`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compute(
    style: &mut ComputedStyle,
    l: Longhand,
    value: &str,
    ctx: &ComputeContext<'_>,
) -> Result<(), String> {
    let cvs = parse_component_values(value)?;
    if cursor::trim_ws(&cvs).is_empty() {
        return Err("empty property value".to_owned());
    }

    let uctx_pct = UnitContext::for_element(style, ctx, true);
    let uctx_no_pct = UnitContext::for_element(style, ctx, false);

    match l {
        // Box generation & positioning
        Longhand::Display => style.display = parse_display(&cvs)?,
        Longhand::Position => style.position = parse_position(&cvs)?,
        Longhand::Top => style.top = parse_inset_side(&cvs, &uctx_pct)?,
        Longhand::Right => style.right = parse_inset_side(&cvs, &uctx_pct)?,
        Longhand::Bottom => style.bottom = parse_inset_side(&cvs, &uctx_pct)?,
        Longhand::Left => style.left = parse_inset_side(&cvs, &uctx_pct)?,
        Longhand::ZIndex => style.z_index = parse_z_index(&cvs)?,
        Longhand::Float => style.float = parse_float(&cvs)?,
        Longhand::BoxSizing => style.box_sizing = parse_box_sizing(&cvs)?,

        // Sizing
        Longhand::Width => style.width = parse_size(&cvs, &uctx_pct, false, false)?,
        Longhand::Height => style.height = parse_size(&cvs, &uctx_pct, false, false)?,
        Longhand::MinWidth => style.min_width = parse_size(&cvs, &uctx_pct, false, false)?,
        Longhand::MinHeight => style.min_height = parse_size(&cvs, &uctx_pct, false, false)?,
        Longhand::MaxWidth => style.max_width = parse_size(&cvs, &uctx_pct, true, false)?,
        Longhand::MaxHeight => style.max_height = parse_size(&cvs, &uctx_pct, true, false)?,
        Longhand::AspectRatio => style.aspect_ratio = parse_aspect_ratio(&cvs)?,

        // Margins & padding
        Longhand::MarginTop => style.margin.top = parse_margin_side(&cvs, &uctx_pct)?,
        Longhand::MarginRight => style.margin.right = parse_margin_side(&cvs, &uctx_pct)?,
        Longhand::MarginBottom => style.margin.bottom = parse_margin_side(&cvs, &uctx_pct)?,
        Longhand::MarginLeft => style.margin.left = parse_margin_side(&cvs, &uctx_pct)?,
        Longhand::PaddingTop => style.padding.top = parse_padding_side(&cvs, &uctx_pct)?,
        Longhand::PaddingRight => style.padding.right = parse_padding_side(&cvs, &uctx_pct)?,
        Longhand::PaddingBottom => style.padding.bottom = parse_padding_side(&cvs, &uctx_pct)?,
        Longhand::PaddingLeft => style.padding.left = parse_padding_side(&cvs, &uctx_pct)?,

        // Borders & radii
        Longhand::BorderTopStyle => {
            let st = parse_border_style(&cvs, false)?;
            apply_border_style(&mut style.border.top, st, style.color);
        }
        Longhand::BorderRightStyle => {
            let st = parse_border_style(&cvs, false)?;
            apply_border_style(&mut style.border.right, st, style.color);
        }
        Longhand::BorderBottomStyle => {
            let st = parse_border_style(&cvs, false)?;
            apply_border_style(&mut style.border.bottom, st, style.color);
        }
        Longhand::BorderLeftStyle => {
            let st = parse_border_style(&cvs, false)?;
            apply_border_style(&mut style.border.left, st, style.color);
        }
        Longhand::BorderTopWidth => {
            let w = parse_line_width(&cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.top, w);
        }
        Longhand::BorderRightWidth => {
            let w = parse_line_width(&cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.right, w);
        }
        Longhand::BorderBottomWidth => {
            let w = parse_line_width(&cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.bottom, w);
        }
        Longhand::BorderLeftWidth => {
            let w = parse_line_width(&cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.left, w);
        }
        Longhand::BorderTopColor => {
            style.border.top.color = parse_color_cvs(&cvs, style.color)?;
        }
        Longhand::BorderRightColor => {
            style.border.right.color = parse_color_cvs(&cvs, style.color)?;
        }
        Longhand::BorderBottomColor => {
            style.border.bottom.color = parse_color_cvs(&cvs, style.color)?;
        }
        Longhand::BorderLeftColor => {
            style.border.left.color = parse_color_cvs(&cvs, style.color)?;
        }
        Longhand::BorderTopLeftRadius => {
            style.border_radius.top_left = parse_corner_radius(&cvs, &uctx_pct)?;
        }
        Longhand::BorderTopRightRadius => {
            style.border_radius.top_right = parse_corner_radius(&cvs, &uctx_pct)?;
        }
        Longhand::BorderBottomRightRadius => {
            style.border_radius.bottom_right = parse_corner_radius(&cvs, &uctx_pct)?;
        }
        Longhand::BorderBottomLeftRadius => {
            style.border_radius.bottom_left = parse_corner_radius(&cvs, &uctx_pct)?;
        }

        // Outline
        Longhand::OutlineStyle => {
            let st = parse_border_style(&cvs, true)?;
            apply_border_style(&mut style.outline, st, style.color);
        }
        Longhand::OutlineWidth => {
            let w = parse_line_width(&cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.outline, w);
        }
        Longhand::OutlineColor => {
            style.outline.color = parse_color_cvs(&cvs, style.color)?;
        }
        Longhand::OutlineOffset => {
            style.outline_offset = parse_outline_offset(&cvs, &uctx_no_pct)?;
        }

        // Flexbox & Box Alignment
        Longhand::FlexDirection => style.flex_direction = parse_flex_direction(&cvs)?,
        Longhand::FlexWrap => style.flex_wrap = parse_flex_wrap(&cvs)?,
        Longhand::JustifyContent => {
            style.justify_content = parse_align(&cvs, AlignTarget::JustifyContent)?;
        }
        Longhand::AlignItems => style.align_items = parse_align(&cvs, AlignTarget::AlignItems)?,
        Longhand::AlignSelf => style.align_self = parse_align(&cvs, AlignTarget::AlignSelf)?,
        Longhand::AlignContent => {
            style.align_content = parse_align(&cvs, AlignTarget::AlignContent)?;
        }
        Longhand::JustifyItems => {
            style.justify_items = parse_align(&cvs, AlignTarget::JustifyItems)?;
        }
        Longhand::JustifySelf => {
            style.justify_self = parse_align(&cvs, AlignTarget::JustifySelf)?;
        }
        Longhand::RowGap => style.row_gap = parse_gap_side(&cvs, &uctx_pct)?,
        Longhand::ColumnGap => style.column_gap = parse_gap_side(&cvs, &uctx_pct)?,
        Longhand::FlexGrow => style.flex_grow = parse_flex_factor(&cvs)?,
        Longhand::FlexShrink => style.flex_shrink = parse_flex_factor(&cvs)?,
        Longhand::FlexBasis => style.flex_basis = parse_flex_basis(&cvs, &uctx_pct)?,
        Longhand::Order => style.order = parse_order(&cvs)?,

        // Grid
        Longhand::GridTemplateColumns => {
            style.grid_template_columns = parse_grid_template_tracks(&cvs, &uctx_pct)?;
        }
        Longhand::GridTemplateRows => {
            style.grid_template_rows = parse_grid_template_tracks(&cvs, &uctx_pct)?;
        }
        Longhand::GridTemplateAreas => {
            style.grid_template_areas = parse_grid_template_areas(&cvs)?;
        }
        Longhand::GridAutoFlow => {
            style.grid_auto_flow_column = parse_grid_auto_flow(&cvs)?;
        }
        Longhand::GridAutoColumns => {
            style.grid_auto_columns = parse_grid_auto_tracks(&cvs, &uctx_pct)?;
        }
        Longhand::GridAutoRows => {
            style.grid_auto_rows = parse_grid_auto_tracks(&cvs, &uctx_pct)?;
        }
        Longhand::GridColumnStart => style.grid_column_start = parse_grid_line(&cvs)?,
        Longhand::GridColumnEnd => style.grid_column_end = parse_grid_line(&cvs)?,
        Longhand::GridRowStart => style.grid_row_start = parse_grid_line(&cvs)?,
        Longhand::GridRowEnd => style.grid_row_end = parse_grid_line(&cvs)?,

        // Backgrounds & visual effects
        Longhand::BackgroundColor => {
            style.background_color = parse_background_color(&cvs, style.color)?;
        }
        Longhand::BackgroundImage => {
            let images = parse_background_image_list(&cvs, &uctx_pct, style.color)?;
            apply_background_images(style, images);
        }
        Longhand::BackgroundPositionX => {
            let xs = parse_bg_position_axis_list(&cvs, &uctx_pct, true)?;
            apply_background_prop(
                style,
                &xs,
                &crate::LengthPercentage::ZERO,
                |layer, x| layer.position_x = x,
            );
        }
        Longhand::BackgroundPositionY => {
            let ys = parse_bg_position_axis_list(&cvs, &uctx_pct, false)?;
            apply_background_prop(
                style,
                &ys,
                &crate::LengthPercentage::ZERO,
                |layer, y| layer.position_y = y,
            );
        }
        Longhand::BackgroundSize => {
            let sizes = parse_bg_size_list(&cvs, &uctx_pct)?;
            apply_background_prop(
                style,
                &sizes,
                &crate::BackgroundSize::Auto,
                |layer, sz| layer.size = sz,
            );
        }
        Longhand::BackgroundRepeat => {
            let repeats = parse_bg_repeat_list(&cvs)?;
            apply_background_prop(
                style,
                &repeats,
                &crate::BackgroundRepeat::Repeat,
                |layer, rep| layer.repeat = rep,
            );
        }
        Longhand::BackgroundAttachment => validate_bg_attachment_list(&cvs)?,
        Longhand::BackgroundOrigin => validate_bg_box_list(&cvs, false)?,
        Longhand::BackgroundClip => validate_bg_box_list(&cvs, true)?,
        Longhand::BoxShadow => {
            style.box_shadow = parse_shadow_list(&cvs, &uctx_no_pct, style.color, true)?;
        }
        Longhand::Opacity => style.opacity = parse_opacity(&cvs)?,
        Longhand::OverflowX => style.overflow_x = parse_overflow(&cvs)?,
        Longhand::OverflowY => style.overflow_y = parse_overflow(&cvs)?,
        Longhand::Translate => {
            if let Some(tf) = parse_individual_translate(&cvs, &uctx_pct)? {
                style.transform.push(tf);
            }
        }
        Longhand::Rotate => {
            if let Some(tf) = parse_individual_rotate(&cvs)? {
                style.transform.push(tf);
            }
        }
        Longhand::Scale => {
            if let Some(tf) = parse_individual_scale(&cvs)? {
                style.transform.push(tf);
            }
        }
        Longhand::Transform => {
            let fns = parse_transform_list(&cvs, &uctx_pct)?;
            style.transform.extend(fns);
        }
        Longhand::TransformOrigin => {
            style.transform_origin = parse_transform_origin(&cvs, &uctx_pct)?;
        }
        Longhand::ObjectFit => style.object_fit = parse_object_fit(&cvs)?,
        Longhand::Filter => style.filter = parse_filter(&cvs, &uctx_no_pct, style.color)?,

        // Inherited: fonts, text, lists, UI
        Longhand::Color => {
            style.color =
                parse_color_cvs(&cvs, ctx.parent.map_or(crate::Rgba::BLACK, |p| p.color))?;
        }
        Longhand::FontFamily => style.font_family = parse_font_family(&cvs)?,
        Longhand::FontSize => style.font_size = parse_font_size(&cvs, ctx)?,
        Longhand::FontWeight => style.font_weight = parse_font_weight(&cvs, ctx)?,
        Longhand::FontStyle => style.font_style = parse_font_style(&cvs)?,
        Longhand::FontVariantCaps => style.small_caps = parse_font_variant_caps(&cvs)?,
        Longhand::LineHeight => style.line_height = parse_line_height(&cvs, style, ctx)?,
        Longhand::LetterSpacing => style.letter_spacing = parse_spacing(&cvs, &uctx_no_pct)?,
        Longhand::WordSpacing => style.word_spacing = parse_spacing(&cvs, &uctx_no_pct)?,
        Longhand::TextAlign => style.text_align = parse_text_align(&cvs, ctx)?,
        Longhand::TextIndent => style.text_indent = parse_text_indent(&cvs, &uctx_pct)?,
        Longhand::TextTransform => style.text_transform = parse_text_transform(&cvs)?,
        Longhand::WhiteSpaceCollapse => {
            let collapse = parse_white_space_collapse(&cvs)?;
            let (_, wrap) = decompose_white_space(style.white_space);
            style.white_space = compose_white_space(collapse, wrap);
        }
        Longhand::TextWrapMode => {
            let wrap = parse_text_wrap_mode(&cvs)?;
            let (collapse, _) = decompose_white_space(style.white_space);
            style.white_space = compose_white_space(collapse, wrap);
        }
        Longhand::WordBreak => style.word_break = parse_word_break(&cvs)?,
        Longhand::OverflowWrap => style.overflow_wrap_anywhere = parse_overflow_wrap(&cvs)?,
        Longhand::TextShadow => {
            style.text_shadow = parse_text_shadow(&cvs, &uctx_no_pct, style.color)?;
        }
        Longhand::Direction => style.direction = parse_direction(&cvs)?,
        Longhand::Visibility => style.visibility = parse_visibility(&cvs)?,
        Longhand::Cursor => style.cursor = parse_cursor(&cvs)?,
        Longhand::PointerEvents => style.pointer_events = parse_pointer_events(&cvs)?,
        Longhand::ListStyleType => style.list_style_type = parse_list_style_type(&cvs)?,
        Longhand::ListStylePosition => {
            style.list_style_inside = parse_list_style_position(&cvs)?;
        }
        Longhand::ListStyleImage => style.list_style_image = parse_list_style_image(&cvs)?,

        // Non-inherited text decoration, UI & generated content
        Longhand::UserSelect => style.user_select_none = parse_user_select(&cvs)?,
        Longhand::TextDecorationLine => {
            style.text_decoration_line = parse_text_decoration_line(&cvs)?;
        }
        Longhand::TextDecorationColor => {
            style.text_decoration_color = parse_text_decoration_color(&cvs, style.color)?;
        }
        Longhand::TextDecorationStyle => {
            style.text_decoration_style = parse_text_decoration_style(&cvs)?;
        }
        Longhand::TextOverflow => style.text_overflow = parse_text_overflow(&cvs)?,
        Longhand::VerticalAlign => {
            style.vertical_align = parse_vertical_align(&cvs, style, ctx)?;
        }
        Longhand::Content => style.content = parse_content(&cvs)?,

        // Transitions
        Longhand::TransitionProperty => {
            let props = parse_transition_property_list(&cvs)?;
            apply_transition_properties(style, props);
        }
        Longhand::TransitionDuration => {
            let durs = parse_time_list(&cvs, true, false)?;
            apply_transition_field(style, &durs, |tr, ms| tr.duration_ms = ms);
        }
        Longhand::TransitionDelay => {
            let delays = parse_time_list(&cvs, false, false)?;
            apply_transition_field(style, &delays, |tr, ms| tr.delay_ms = ms);
        }
        Longhand::TransitionTimingFunction => {
            let easings = parse_easing_list(&cvs)?;
            apply_transition_field(style, &easings, |tr, e| tr.easing = e);
        }

        // Animations
        Longhand::AnimationName => {
            let names = parse_animation_name_list(&cvs)?;
            apply_animation_names(style, names);
        }
        Longhand::AnimationDuration => {
            let durs = parse_time_list(&cvs, true, true)?;
            apply_animation_field(style, &durs, |anim, ms| anim.duration_ms = ms);
        }
        Longhand::AnimationDelay => {
            let delays = parse_time_list(&cvs, false, false)?;
            apply_animation_field(style, &delays, |anim, ms| anim.delay_ms = ms);
        }
        Longhand::AnimationTimingFunction => {
            let easings = parse_easing_list(&cvs)?;
            apply_animation_field(style, &easings, |anim, e| anim.easing = e);
        }
        Longhand::AnimationIterationCount => {
            let counts = parse_animation_iteration_list(&cvs)?;
            apply_animation_field(style, &counts, |anim, c| anim.iteration_count = c);
        }
        Longhand::AnimationDirection => {
            let dirs = parse_animation_direction_list(&cvs)?;
            apply_animation_field(style, &dirs, |anim, (alt, rev)| {
                anim.alternate = alt;
                anim.reverse = rev;
            });
        }
        Longhand::AnimationFillMode => {
            let modes = parse_animation_fill_mode_list(&cvs)?;
            apply_animation_field(style, &modes, |anim, (fwd, bwd)| {
                anim.fill_forwards = fwd;
                anim.fill_backwards = bwd;
            });
        }
        Longhand::AnimationPlayState => {
            let states = parse_animation_play_state_list(&cvs)?;
            apply_animation_field(style, &states, |anim, paused| anim.paused = paused);
        }
    }

    Ok(())
}

/// Copy one longhand's field from `src` to `dst` (used by `inherit` and `initial`).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn copy(dst: &mut ComputedStyle, src: &ComputedStyle, l: Longhand) {
    match l {
        Longhand::Display => dst.display = src.display,
        Longhand::Position => dst.position = src.position,
        Longhand::Top => dst.top.clone_from(&src.top),
        Longhand::Right => dst.right.clone_from(&src.right),
        Longhand::Bottom => dst.bottom.clone_from(&src.bottom),
        Longhand::Left => dst.left.clone_from(&src.left),
        Longhand::ZIndex => dst.z_index = src.z_index,
        Longhand::Float => dst.float = src.float,
        Longhand::BoxSizing => dst.box_sizing = src.box_sizing,

        Longhand::Width => dst.width.clone_from(&src.width),
        Longhand::Height => dst.height.clone_from(&src.height),
        Longhand::MinWidth => dst.min_width.clone_from(&src.min_width),
        Longhand::MinHeight => dst.min_height.clone_from(&src.min_height),
        Longhand::MaxWidth => dst.max_width.clone_from(&src.max_width),
        Longhand::MaxHeight => dst.max_height.clone_from(&src.max_height),
        Longhand::AspectRatio => dst.aspect_ratio = src.aspect_ratio,

        Longhand::MarginTop => dst.margin.top.clone_from(&src.margin.top),
        Longhand::MarginRight => dst.margin.right.clone_from(&src.margin.right),
        Longhand::MarginBottom => dst.margin.bottom.clone_from(&src.margin.bottom),
        Longhand::MarginLeft => dst.margin.left.clone_from(&src.margin.left),

        Longhand::PaddingTop => dst.padding.top.clone_from(&src.padding.top),
        Longhand::PaddingRight => dst.padding.right.clone_from(&src.padding.right),
        Longhand::PaddingBottom => dst.padding.bottom.clone_from(&src.padding.bottom),
        Longhand::PaddingLeft => dst.padding.left.clone_from(&src.padding.left),

        Longhand::BorderTopStyle => {
            apply_border_style(&mut dst.border.top, src.border.top.style, dst.color);
        }
        Longhand::BorderRightStyle => {
            apply_border_style(&mut dst.border.right, src.border.right.style, dst.color);
        }
        Longhand::BorderBottomStyle => {
            apply_border_style(&mut dst.border.bottom, src.border.bottom.style, dst.color);
        }
        Longhand::BorderLeftStyle => {
            apply_border_style(&mut dst.border.left, src.border.left.style, dst.color);
        }
        Longhand::BorderTopWidth => apply_border_width(&mut dst.border.top, src.border.top.width),
        Longhand::BorderRightWidth => {
            apply_border_width(&mut dst.border.right, src.border.right.width);
        }
        Longhand::BorderBottomWidth => {
            apply_border_width(&mut dst.border.bottom, src.border.bottom.width);
        }
        Longhand::BorderLeftWidth => {
            apply_border_width(&mut dst.border.left, src.border.left.width);
        }
        Longhand::BorderTopColor => dst.border.top.color = src.border.top.color,
        Longhand::BorderRightColor => dst.border.right.color = src.border.right.color,
        Longhand::BorderBottomColor => dst.border.bottom.color = src.border.bottom.color,
        Longhand::BorderLeftColor => dst.border.left.color = src.border.left.color,

        Longhand::BorderTopLeftRadius => dst.border_radius.top_left = src.border_radius.top_left,
        Longhand::BorderTopRightRadius => dst.border_radius.top_right = src.border_radius.top_right,
        Longhand::BorderBottomRightRadius => {
            dst.border_radius.bottom_right = src.border_radius.bottom_right;
        }
        Longhand::BorderBottomLeftRadius => {
            dst.border_radius.bottom_left = src.border_radius.bottom_left;
        }

        Longhand::OutlineStyle => {
            apply_border_style(&mut dst.outline, src.outline.style, dst.color);
        }
        Longhand::OutlineWidth => apply_border_width(&mut dst.outline, src.outline.width),
        Longhand::OutlineColor => dst.outline.color = src.outline.color,
        Longhand::OutlineOffset => dst.outline_offset = src.outline_offset,

        Longhand::FlexDirection => dst.flex_direction = src.flex_direction,
        Longhand::FlexWrap => dst.flex_wrap = src.flex_wrap,
        Longhand::JustifyContent => dst.justify_content = src.justify_content,
        Longhand::AlignItems => dst.align_items = src.align_items,
        Longhand::AlignSelf => dst.align_self = src.align_self,
        Longhand::AlignContent => dst.align_content = src.align_content,
        Longhand::JustifyItems => dst.justify_items = src.justify_items,
        Longhand::JustifySelf => dst.justify_self = src.justify_self,
        Longhand::RowGap => dst.row_gap.clone_from(&src.row_gap),
        Longhand::ColumnGap => dst.column_gap.clone_from(&src.column_gap),
        Longhand::FlexGrow => dst.flex_grow = src.flex_grow,
        Longhand::FlexShrink => dst.flex_shrink = src.flex_shrink,
        Longhand::FlexBasis => dst.flex_basis.clone_from(&src.flex_basis),
        Longhand::Order => dst.order = src.order,

        Longhand::GridTemplateColumns => {
            dst.grid_template_columns.clone_from(&src.grid_template_columns);
        }
        Longhand::GridTemplateRows => dst.grid_template_rows.clone_from(&src.grid_template_rows),
        Longhand::GridTemplateAreas => dst.grid_template_areas.clone_from(&src.grid_template_areas),
        Longhand::GridAutoFlow => dst.grid_auto_flow_column = src.grid_auto_flow_column,
        Longhand::GridAutoColumns => dst.grid_auto_columns.clone_from(&src.grid_auto_columns),
        Longhand::GridAutoRows => dst.grid_auto_rows.clone_from(&src.grid_auto_rows),
        Longhand::GridColumnStart => dst.grid_column_start.clone_from(&src.grid_column_start),
        Longhand::GridColumnEnd => dst.grid_column_end.clone_from(&src.grid_column_end),
        Longhand::GridRowStart => dst.grid_row_start.clone_from(&src.grid_row_start),
        Longhand::GridRowEnd => dst.grid_row_end.clone_from(&src.grid_row_end),

        Longhand::BackgroundColor => dst.background_color = src.background_color,
        Longhand::BackgroundImage
        | Longhand::BackgroundPositionX
        | Longhand::BackgroundPositionY
        | Longhand::BackgroundSize
        | Longhand::BackgroundRepeat => {
            dst.background_layers.clone_from(&src.background_layers);
        }
        Longhand::BackgroundAttachment
        | Longhand::BackgroundOrigin
        | Longhand::BackgroundClip => {}

        Longhand::BoxShadow => dst.box_shadow.clone_from(&src.box_shadow),
        Longhand::Opacity => dst.opacity = src.opacity,
        Longhand::OverflowX => dst.overflow_x = src.overflow_x,
        Longhand::OverflowY => dst.overflow_y = src.overflow_y,
        Longhand::Transform | Longhand::Translate | Longhand::Rotate | Longhand::Scale => {
            dst.transform.clone_from(&src.transform);
        }
        Longhand::TransformOrigin => dst.transform_origin = src.transform_origin,
        Longhand::ObjectFit => dst.object_fit = src.object_fit,
        Longhand::Filter => dst.filter.clone_from(&src.filter),

        Longhand::Color => dst.color = src.color,
        Longhand::FontFamily => dst.font_family.clone_from(&src.font_family),
        Longhand::FontSize => dst.font_size = src.font_size,
        Longhand::FontWeight => dst.font_weight = src.font_weight,
        Longhand::FontStyle => dst.font_style = src.font_style,
        Longhand::FontVariantCaps => dst.small_caps = src.small_caps,
        Longhand::LineHeight => dst.line_height = src.line_height,
        Longhand::LetterSpacing => dst.letter_spacing = src.letter_spacing,
        Longhand::WordSpacing => dst.word_spacing = src.word_spacing,
        Longhand::TextAlign => dst.text_align = src.text_align,
        Longhand::TextIndent => dst.text_indent.clone_from(&src.text_indent),
        Longhand::TextTransform => dst.text_transform = src.text_transform,
        Longhand::WhiteSpaceCollapse => {
            let (src_collapse, _) = decompose_white_space(src.white_space);
            let (_, dst_wrap) = decompose_white_space(dst.white_space);
            dst.white_space = compose_white_space(src_collapse, dst_wrap);
        }
        Longhand::TextWrapMode => {
            let (_, src_wrap) = decompose_white_space(src.white_space);
            let (dst_collapse, _) = decompose_white_space(dst.white_space);
            dst.white_space = compose_white_space(dst_collapse, src_wrap);
        }
        Longhand::WordBreak => dst.word_break = src.word_break,
        Longhand::OverflowWrap => dst.overflow_wrap_anywhere = src.overflow_wrap_anywhere,
        Longhand::TextShadow => dst.text_shadow.clone_from(&src.text_shadow),
        Longhand::Direction => dst.direction = src.direction,
        Longhand::Visibility => dst.visibility = src.visibility,
        Longhand::Cursor => dst.cursor = src.cursor,
        Longhand::PointerEvents => dst.pointer_events = src.pointer_events,
        Longhand::ListStyleType => dst.list_style_type = src.list_style_type,
        Longhand::ListStylePosition => dst.list_style_inside = src.list_style_inside,
        Longhand::ListStyleImage => dst.list_style_image.clone_from(&src.list_style_image),

        Longhand::UserSelect => dst.user_select_none = src.user_select_none,
        Longhand::TextDecorationLine => dst.text_decoration_line = src.text_decoration_line,
        Longhand::TextDecorationColor => dst.text_decoration_color = src.text_decoration_color,
        Longhand::TextDecorationStyle => dst.text_decoration_style = src.text_decoration_style,
        Longhand::TextOverflow => dst.text_overflow = src.text_overflow,
        Longhand::VerticalAlign => dst.vertical_align = src.vertical_align,
        Longhand::Content => dst.content.clone_from(&src.content),

        Longhand::TransitionProperty
        | Longhand::TransitionDuration
        | Longhand::TransitionDelay
        | Longhand::TransitionTimingFunction => {
            dst.transitions.clone_from(&src.transitions);
        }
        Longhand::AnimationName
        | Longhand::AnimationDuration
        | Longhand::AnimationDelay
        | Longhand::AnimationTimingFunction
        | Longhand::AnimationIterationCount
        | Longhand::AnimationDirection
        | Longhand::AnimationFillMode
        | Longhand::AnimationPlayState => {
            dst.animations.clone_from(&src.animations);
        }
    }
}
