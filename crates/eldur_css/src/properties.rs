//! Property classification, shorthand expansion, and value parsing (`eldur_css` part B).
//!
//! This module is the **single seam** between the cascade (`cascade.rs`) and
//! the value parsers. `cascade.rs` calls only the `pub(crate)` items exported
//! here. All CSS value grammars (`calc()`, `<color>`, `<length-percentage>`,
//! `<image>`, `<transform-list>`, shorthands, etc.) live behind this interface.

#![allow(dead_code)]

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
#[cfg(test)]
mod tests;
mod text;

/// `content`'s value as text, for what the runtime generates itself.
pub(crate) fn parse_content_value(value: &str) -> Option<Option<Vec<crate::ContentItem>>> {
    let cvs = cursor::parse_component_values(value).ok()?;
    text::parse_content(cursor::trim_ws(&cvs)).ok()
}
mod transform;

/// A `counter-*` value's changes, `default` for a name with no number.
pub(crate) fn parse_counter_value(value: &str, default: i32) -> Option<Vec<crate::CounterChange>> {
    let cvs = cursor::parse_component_values(value).ok()?;
    text::parse_counter_changes(cursor::trim_ws(&cvs), default).ok()
}

use crate::{
    BackgroundAttachment, BackgroundClip, BackgroundOrigin, BackgroundRepeat, ComputedStyle,
    LengthPercentage, Size, TransformFn, Viewport,
};

use self::{
    animation::{
        apply_animation_field, apply_animation_names, apply_transition_field,
        apply_transition_properties, copy_animation_field, copy_transition_field,
        parse_animation_direction_list, parse_animation_fill_mode_list,
        parse_animation_iteration_list, parse_animation_name_list, parse_animation_play_state_list,
        parse_easing_list, parse_time_list, parse_transition_property_list,
    },
    background::{
        apply_background_images, apply_background_prop, copy_background_field,
        parse_background_color, parse_background_image_list, parse_bg_attachment_list,
        parse_bg_clip_list, parse_bg_origin_list, parse_bg_position_axis_list,
        parse_bg_repeat_list, parse_bg_size_list, parse_filter, parse_object_fit, parse_opacity,
        parse_overflow, parse_position_2d, parse_shadow_list,
    },
    box_model::{
        apply_border_color, apply_border_style, apply_border_width, parse_aspect_ratio,
        parse_border_style, parse_box_sizing, parse_corner_radius, parse_display, parse_float,
        parse_inset_side, parse_line_width, parse_margin_side, parse_outline_offset,
        parse_padding_side, parse_position, parse_size, parse_z_index,
    },
    color::{is_current_color_cvs, parse_color_cvs},
    cursor::{parse_component_values, trim_ws},
    flex::{
        AlignTarget, parse_align, parse_flex_basis, parse_flex_direction, parse_flex_factor,
        parse_flex_wrap, parse_gap_side, parse_order,
    },
    font::{
        font_size_origin, parse_font_family, parse_font_size, parse_font_style,
        parse_font_variant_caps, parse_font_weight, parse_line_height,
    },
    grid::{
        parse_grid_auto_flow, parse_grid_auto_tracks, parse_grid_line, parse_grid_template_areas,
        parse_grid_template_tracks,
    },
    length::{UnitContext, parse_integer_cvs, parse_length_auto_cvs, parse_length_cvs},
    text::{
        compose_white_space, decompose_white_space, parse_accent_color, parse_border_collapse,
        parse_border_spacing, parse_caption_side, parse_content, parse_counter_changes,
        parse_cursor, parse_direction, parse_list_style_image, parse_list_style_position,
        parse_list_style_type, parse_overflow_wrap, parse_pointer_events, parse_spacing,
        parse_table_layout, parse_text_align, parse_text_decoration_color,
        parse_text_decoration_line, parse_text_decoration_style, parse_text_indent,
        parse_text_overflow, parse_text_shadow, parse_text_transform, parse_text_wrap_mode,
        parse_user_select, parse_vertical_align, parse_visibility, parse_white_space_collapse,
        parse_word_break,
    },
    transform::{
        parse_individual_rotate, parse_individual_scale, parse_individual_translate,
        parse_transform_list, parse_transform_origin,
    },
};

/// Suppresses the `dead_code` warning on `Stylesheet::inner` while `parse.rs` / `lib.rs`
/// stubs are not yet wired in this worktree.
fn _touch_stylesheet_inner(s: &crate::Stylesheet) {
    let _ = &s.inner;
}

/// Identifies a single longhand property (one field or sub-field of
/// [`ComputedStyle`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Longhand {
    // Priority 0: font-size (resolves `em` on the element)
    FontSize,

    // Priority 1: font properties & line-height (`lh` depends on `line_height` + `font_size`)
    FontFamily,
    FontWeight,
    FontStyle,
    FontVariantCaps,
    LineHeight,

    // Priority 2: color & direction (`currentColor` and `match-parent` depend on these)
    Color,
    Direction,

    // Priority 3: border & outline styles (must run before border/outline widths & colors)
    BorderTopStyle,
    BorderRightStyle,
    BorderBottomStyle,
    BorderLeftStyle,
    OutlineStyle,

    // Priority 4: border & outline widths and colors
    BorderTopWidth,
    BorderRightWidth,
    BorderBottomWidth,
    BorderLeftWidth,
    BorderTopColor,
    BorderRightColor,
    BorderBottomColor,
    BorderLeftColor,
    OutlineWidth,
    OutlineColor,
    OutlineOffset,

    // Priority 5: primary list-sizing longhands for background, transition, and animation
    BackgroundImage,
    TransitionProperty,
    AnimationName,

    // Priority 6: individual transform properties (`translate`, `rotate`, `scale`)
    Translate,
    Rotate,
    Scale,

    // Priority 7: `transform` and all remaining longhands
    Transform,
    TransformOrigin,

    // Display & positioning
    Display,
    Position,
    Top,
    Right,
    Bottom,
    Left,
    ZIndex,
    Float,
    Clear,

    // Box model & sizing
    BoxSizing,
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    AspectRatio,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    BorderTopLeftRadius,
    BorderTopRightRadius,
    BorderBottomRightRadius,
    BorderBottomLeftRadius,

    // Flexbox & box alignment
    FlexDirection,
    FlexWrap,
    JustifyContent,
    AlignItems,
    AlignContent,
    AlignSelf,
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
    GridAutoRows,
    GridAutoColumns,
    GridRowStart,
    GridRowEnd,
    GridColumnStart,
    GridColumnEnd,

    // Backgrounds & visual effects
    BackgroundColor,
    BackgroundPositionX,
    BackgroundPositionY,
    BackgroundSize,
    BackgroundRepeat,
    BackgroundAttachment,
    BackgroundOrigin,
    BackgroundClip,
    BoxShadow,
    Opacity,
    Visibility,
    ScrollBehavior,
    OverflowX,
    OverflowY,
    ObjectFit,
    ObjectPosition,
    Filter,

    // Typography & text
    LetterSpacing,
    WordSpacing,
    TextAlign,
    TextIndent,
    TextTransform,
    TextDecorationLine,
    TextDecorationStyle,
    TextDecorationColor,
    TextDecorationThickness,
    TextUnderlineOffset,
    TextOverflow,
    WebkitLineClamp,
    WhiteSpaceCollapse,
    TextWrapMode,
    WordBreak,
    OverflowWrap,
    VerticalAlign,
    TextShadow,

    // Lists, UI & generated content
    Cursor,
    PointerEvents,
    UserSelect,
    ListStyleType,
    ListStylePosition,
    ListStyleImage,
    Content,
    CounterReset,
    CounterIncrement,
    CounterSet,
    BorderCollapse,
    BorderSpacing,
    CaptionSide,
    AccentColor,
    TableLayout,

    // Transitions & animations sub-properties
    TransitionDuration,
    TransitionTimingFunction,
    TransitionDelay,
    AnimationDuration,
    AnimationTimingFunction,
    AnimationDelay,
    AnimationIterationCount,
    AnimationDirection,
    AnimationFillMode,
    AnimationPlayState,
}

/// CSS-wide keywords (handled uniformly by the cascade for every property).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CssWide {
    Initial,
    Inherit,
    Unset,
    Revert,
    RevertLayer,
}

impl CssWide {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "initial" => Some(Self::Initial),
            "inherit" => Some(Self::Inherit),
            "unset" => Some(Self::Unset),
            "revert" => Some(Self::Revert),
            "revert-layer" => Some(Self::RevertLayer),
            _ => None,
        }
    }
}

/// How the cascade should treat a property name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyKind {
    /// Known longhand.
    Longhand(Longhand),
    /// Known shorthand — call [`expand`] to split into longhands.
    Shorthand,
    /// Custom property (`--*`).
    Custom,
    /// Recognised CSS property that has no QML representation (emit an
    /// `unsupported-property` diagnostic).
    Unsupported,
    /// Unrecognised / ignored property name (drop silently or warn).
    Unknown,
}

/// Classifies `name` (already ASCII-lowercased or mixed-case).
pub(crate) fn kind(name: &str) -> PropertyKind {
    let trimmed = name.trim();
    if trimmed.starts_with("--") && trimmed.len() > 2 {
        return PropertyKind::Custom;
    }
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        // Longhands: Font & typography
        "font-size" => PropertyKind::Longhand(Longhand::FontSize),
        "font-family" => PropertyKind::Longhand(Longhand::FontFamily),
        "font-weight" => PropertyKind::Longhand(Longhand::FontWeight),
        "font-style" => PropertyKind::Longhand(Longhand::FontStyle),
        "font-variant-caps" => PropertyKind::Longhand(Longhand::FontVariantCaps),
        "line-height" => PropertyKind::Longhand(Longhand::LineHeight),
        "color" => PropertyKind::Longhand(Longhand::Color),
        "direction" => PropertyKind::Longhand(Longhand::Direction),
        "letter-spacing" => PropertyKind::Longhand(Longhand::LetterSpacing),
        "word-spacing" => PropertyKind::Longhand(Longhand::WordSpacing),
        "text-align" => PropertyKind::Longhand(Longhand::TextAlign),
        "text-indent" => PropertyKind::Longhand(Longhand::TextIndent),
        "text-transform" => PropertyKind::Longhand(Longhand::TextTransform),
        "text-decoration-line" => PropertyKind::Longhand(Longhand::TextDecorationLine),
        "text-decoration-style" => PropertyKind::Longhand(Longhand::TextDecorationStyle),
        "text-decoration-color" => PropertyKind::Longhand(Longhand::TextDecorationColor),
        "text-decoration-thickness" => PropertyKind::Longhand(Longhand::TextDecorationThickness),
        "text-underline-offset" => PropertyKind::Longhand(Longhand::TextUnderlineOffset),
        "text-overflow" => PropertyKind::Longhand(Longhand::TextOverflow),
        "-webkit-line-clamp" | "line-clamp" => PropertyKind::Longhand(Longhand::WebkitLineClamp),
        "white-space-collapse" => PropertyKind::Longhand(Longhand::WhiteSpaceCollapse),
        "text-wrap-mode" | "text-wrap" => PropertyKind::Longhand(Longhand::TextWrapMode),
        "word-break" => PropertyKind::Longhand(Longhand::WordBreak),
        "overflow-wrap" | "word-wrap" => PropertyKind::Longhand(Longhand::OverflowWrap),
        "vertical-align" => PropertyKind::Longhand(Longhand::VerticalAlign),
        "text-shadow" => PropertyKind::Longhand(Longhand::TextShadow),

        // Longhands: Display & positioning
        "display" => PropertyKind::Longhand(Longhand::Display),
        "position" => PropertyKind::Longhand(Longhand::Position),
        "top" | "inset-block-start" => PropertyKind::Longhand(Longhand::Top),
        "right" | "inset-inline-end" => PropertyKind::Longhand(Longhand::Right),
        "bottom" | "inset-block-end" => PropertyKind::Longhand(Longhand::Bottom),
        "left" | "inset-inline-start" => PropertyKind::Longhand(Longhand::Left),
        "z-index" => PropertyKind::Longhand(Longhand::ZIndex),
        "float" => PropertyKind::Longhand(Longhand::Float),
        "clear" => PropertyKind::Longhand(Longhand::Clear),

        // Longhands: Box model & sizing
        "box-sizing" | "-webkit-box-sizing" | "-moz-box-sizing" => {
            PropertyKind::Longhand(Longhand::BoxSizing)
        }
        "width" | "inline-size" => PropertyKind::Longhand(Longhand::Width),
        "height" | "block-size" => PropertyKind::Longhand(Longhand::Height),
        "min-width" | "min-inline-size" => PropertyKind::Longhand(Longhand::MinWidth),
        "min-height" | "min-block-size" => PropertyKind::Longhand(Longhand::MinHeight),
        "max-width" | "max-inline-size" => PropertyKind::Longhand(Longhand::MaxWidth),
        "max-height" | "max-block-size" => PropertyKind::Longhand(Longhand::MaxHeight),
        "aspect-ratio" => PropertyKind::Longhand(Longhand::AspectRatio),
        "margin-top" | "margin-block-start" => PropertyKind::Longhand(Longhand::MarginTop),
        "margin-right" | "margin-inline-end" => PropertyKind::Longhand(Longhand::MarginRight),
        "margin-bottom" | "margin-block-end" => PropertyKind::Longhand(Longhand::MarginBottom),
        "margin-left" | "margin-inline-start" => PropertyKind::Longhand(Longhand::MarginLeft),
        "padding-top" | "padding-block-start" => PropertyKind::Longhand(Longhand::PaddingTop),
        "padding-right" | "padding-inline-end" => PropertyKind::Longhand(Longhand::PaddingRight),
        "padding-bottom" | "padding-block-end" => PropertyKind::Longhand(Longhand::PaddingBottom),
        "padding-left" | "padding-inline-start" => PropertyKind::Longhand(Longhand::PaddingLeft),

        // Longhands: Borders & outline
        "border-top-width" | "border-block-start-width" => {
            PropertyKind::Longhand(Longhand::BorderTopWidth)
        }
        "border-right-width" | "border-inline-end-width" => {
            PropertyKind::Longhand(Longhand::BorderRightWidth)
        }
        "border-bottom-width" | "border-block-end-width" => {
            PropertyKind::Longhand(Longhand::BorderBottomWidth)
        }
        "border-left-width" | "border-inline-start-width" => {
            PropertyKind::Longhand(Longhand::BorderLeftWidth)
        }
        "border-top-style" | "border-block-start-style" => {
            PropertyKind::Longhand(Longhand::BorderTopStyle)
        }
        "border-right-style" | "border-inline-end-style" => {
            PropertyKind::Longhand(Longhand::BorderRightStyle)
        }
        "border-bottom-style" | "border-block-end-style" => {
            PropertyKind::Longhand(Longhand::BorderBottomStyle)
        }
        "border-left-style" | "border-inline-start-style" => {
            PropertyKind::Longhand(Longhand::BorderLeftStyle)
        }
        "border-top-color" | "border-block-start-color" => {
            PropertyKind::Longhand(Longhand::BorderTopColor)
        }
        "border-right-color" | "border-inline-end-color" => {
            PropertyKind::Longhand(Longhand::BorderRightColor)
        }
        "border-bottom-color" | "border-block-end-color" => {
            PropertyKind::Longhand(Longhand::BorderBottomColor)
        }
        "border-left-color" | "border-inline-start-color" => {
            PropertyKind::Longhand(Longhand::BorderLeftColor)
        }
        "border-top-left-radius" | "border-start-start-radius" => {
            PropertyKind::Longhand(Longhand::BorderTopLeftRadius)
        }
        "border-top-right-radius" | "border-start-end-radius" => {
            PropertyKind::Longhand(Longhand::BorderTopRightRadius)
        }
        "border-bottom-right-radius" | "border-end-end-radius" => {
            PropertyKind::Longhand(Longhand::BorderBottomRightRadius)
        }
        "border-bottom-left-radius" | "border-end-start-radius" => {
            PropertyKind::Longhand(Longhand::BorderBottomLeftRadius)
        }
        "outline-width" => PropertyKind::Longhand(Longhand::OutlineWidth),
        "outline-style" => PropertyKind::Longhand(Longhand::OutlineStyle),
        "outline-color" => PropertyKind::Longhand(Longhand::OutlineColor),
        "outline-offset" => PropertyKind::Longhand(Longhand::OutlineOffset),

        // Longhands: Flexbox & alignment
        "flex-direction" | "-webkit-flex-direction" => {
            PropertyKind::Longhand(Longhand::FlexDirection)
        }
        "flex-wrap" | "-webkit-flex-wrap" => PropertyKind::Longhand(Longhand::FlexWrap),
        "justify-content" | "-webkit-justify-content" => {
            PropertyKind::Longhand(Longhand::JustifyContent)
        }
        "align-items" | "-webkit-align-items" => PropertyKind::Longhand(Longhand::AlignItems),
        "align-content" | "-webkit-align-content" => PropertyKind::Longhand(Longhand::AlignContent),
        "align-self" | "-webkit-align-self" => PropertyKind::Longhand(Longhand::AlignSelf),
        "justify-items" => PropertyKind::Longhand(Longhand::JustifyItems),
        "justify-self" => PropertyKind::Longhand(Longhand::JustifySelf),
        "row-gap" | "grid-row-gap" => PropertyKind::Longhand(Longhand::RowGap),
        "column-gap" | "grid-column-gap" => PropertyKind::Longhand(Longhand::ColumnGap),
        "flex-grow" | "-webkit-flex-grow" => PropertyKind::Longhand(Longhand::FlexGrow),
        "flex-shrink" | "-webkit-flex-shrink" => PropertyKind::Longhand(Longhand::FlexShrink),
        "flex-basis" | "-webkit-flex-basis" => PropertyKind::Longhand(Longhand::FlexBasis),
        "order" | "-webkit-order" => PropertyKind::Longhand(Longhand::Order),

        // Longhands: Grid
        "grid-template-columns" => PropertyKind::Longhand(Longhand::GridTemplateColumns),
        "grid-template-rows" => PropertyKind::Longhand(Longhand::GridTemplateRows),
        "grid-template-areas" => PropertyKind::Longhand(Longhand::GridTemplateAreas),
        "grid-auto-flow" => PropertyKind::Longhand(Longhand::GridAutoFlow),
        "grid-auto-rows" => PropertyKind::Longhand(Longhand::GridAutoRows),
        "grid-auto-columns" => PropertyKind::Longhand(Longhand::GridAutoColumns),
        "grid-row-start" => PropertyKind::Longhand(Longhand::GridRowStart),
        "grid-row-end" => PropertyKind::Longhand(Longhand::GridRowEnd),
        "grid-column-start" => PropertyKind::Longhand(Longhand::GridColumnStart),
        "grid-column-end" => PropertyKind::Longhand(Longhand::GridColumnEnd),

        // Longhands: Backgrounds & visual
        "background-color" => PropertyKind::Longhand(Longhand::BackgroundColor),
        "background-image" => PropertyKind::Longhand(Longhand::BackgroundImage),
        "background-position-x" => PropertyKind::Longhand(Longhand::BackgroundPositionX),
        "background-position-y" => PropertyKind::Longhand(Longhand::BackgroundPositionY),
        "background-size" | "-webkit-background-size" => {
            PropertyKind::Longhand(Longhand::BackgroundSize)
        }
        "background-repeat" => PropertyKind::Longhand(Longhand::BackgroundRepeat),
        "background-attachment" => PropertyKind::Longhand(Longhand::BackgroundAttachment),
        "background-origin" => PropertyKind::Longhand(Longhand::BackgroundOrigin),
        "background-clip" | "-webkit-background-clip" => {
            PropertyKind::Longhand(Longhand::BackgroundClip)
        }
        "box-shadow" | "-webkit-box-shadow" => PropertyKind::Longhand(Longhand::BoxShadow),
        "opacity" => PropertyKind::Longhand(Longhand::Opacity),
        "visibility" => PropertyKind::Longhand(Longhand::Visibility),
        "scroll-behavior" => PropertyKind::Longhand(Longhand::ScrollBehavior),
        "overflow-x" => PropertyKind::Longhand(Longhand::OverflowX),
        "overflow-y" => PropertyKind::Longhand(Longhand::OverflowY),
        "object-fit" => PropertyKind::Longhand(Longhand::ObjectFit),
        "object-position" => PropertyKind::Longhand(Longhand::ObjectPosition),
        "filter" | "-webkit-filter" => PropertyKind::Longhand(Longhand::Filter),

        // Longhands: Transforms
        "transform" | "-webkit-transform" => PropertyKind::Longhand(Longhand::Transform),
        "transform-origin" | "-webkit-transform-origin" => {
            PropertyKind::Longhand(Longhand::TransformOrigin)
        }
        "translate" => PropertyKind::Longhand(Longhand::Translate),
        "rotate" => PropertyKind::Longhand(Longhand::Rotate),
        "scale" => PropertyKind::Longhand(Longhand::Scale),

        // Longhands: UI, lists, generated content
        "cursor" => PropertyKind::Longhand(Longhand::Cursor),
        "pointer-events" => PropertyKind::Longhand(Longhand::PointerEvents),
        "user-select" | "-webkit-user-select" | "-moz-user-select" | "-ms-user-select" => {
            PropertyKind::Longhand(Longhand::UserSelect)
        }
        "list-style-type" => PropertyKind::Longhand(Longhand::ListStyleType),
        "list-style-position" => PropertyKind::Longhand(Longhand::ListStylePosition),
        "list-style-image" => PropertyKind::Longhand(Longhand::ListStyleImage),
        "content" => PropertyKind::Longhand(Longhand::Content),
        "counter-reset" => PropertyKind::Longhand(Longhand::CounterReset),
        "counter-increment" => PropertyKind::Longhand(Longhand::CounterIncrement),
        "counter-set" => PropertyKind::Longhand(Longhand::CounterSet),
        "border-collapse" => PropertyKind::Longhand(Longhand::BorderCollapse),
        "border-spacing" => PropertyKind::Longhand(Longhand::BorderSpacing),
        "caption-side" => PropertyKind::Longhand(Longhand::CaptionSide),
        "accent-color" => PropertyKind::Longhand(Longhand::AccentColor),
        "table-layout" => PropertyKind::Longhand(Longhand::TableLayout),

        // Longhands: Transitions & animations
        "transition-property" | "-webkit-transition-property" => {
            PropertyKind::Longhand(Longhand::TransitionProperty)
        }
        "transition-duration" | "-webkit-transition-duration" => {
            PropertyKind::Longhand(Longhand::TransitionDuration)
        }
        "transition-timing-function" | "-webkit-transition-timing-function" => {
            PropertyKind::Longhand(Longhand::TransitionTimingFunction)
        }
        "transition-delay" | "-webkit-transition-delay" => {
            PropertyKind::Longhand(Longhand::TransitionDelay)
        }
        "animation-name" | "-webkit-animation-name" => {
            PropertyKind::Longhand(Longhand::AnimationName)
        }
        "animation-duration" | "-webkit-animation-duration" => {
            PropertyKind::Longhand(Longhand::AnimationDuration)
        }
        "animation-timing-function" | "-webkit-animation-timing-function" => {
            PropertyKind::Longhand(Longhand::AnimationTimingFunction)
        }
        "animation-delay" | "-webkit-animation-delay" => {
            PropertyKind::Longhand(Longhand::AnimationDelay)
        }
        "animation-iteration-count" | "-webkit-animation-iteration-count" => {
            PropertyKind::Longhand(Longhand::AnimationIterationCount)
        }
        "animation-direction" | "-webkit-animation-direction" => {
            PropertyKind::Longhand(Longhand::AnimationDirection)
        }
        "animation-fill-mode" | "-webkit-animation-fill-mode" => {
            PropertyKind::Longhand(Longhand::AnimationFillMode)
        }
        "animation-play-state" | "-webkit-animation-play-state" => {
            PropertyKind::Longhand(Longhand::AnimationPlayState)
        }

        // Shorthands
        "margin"
        | "margin-block"
        | "margin-inline"
        | "padding"
        | "padding-block"
        | "padding-inline"
        | "inset"
        | "inset-block"
        | "inset-inline"
        | "border"
        | "border-top"
        | "border-right"
        | "border-bottom"
        | "border-left"
        | "border-block"
        | "border-block-start"
        | "border-block-end"
        | "border-inline"
        | "border-inline-start"
        | "border-inline-end"
        | "border-width"
        | "border-block-width"
        | "border-inline-width"
        | "border-style"
        | "border-block-style"
        | "border-inline-style"
        | "border-color"
        | "border-block-color"
        | "border-inline-color"
        | "border-radius"
        | "-webkit-border-radius"
        | "outline"
        | "background"
        | "background-position"
        | "font"
        | "font-variant"
        | "flex"
        | "-webkit-flex"
        | "flex-flow"
        | "-webkit-flex-flow"
        | "gap"
        | "grid-gap"
        | "place-content"
        | "place-items"
        | "place-self"
        | "grid-template"
        | "grid"
        | "grid-row"
        | "grid-column"
        | "grid-area"
        | "overflow"
        | "list-style"
        | "text-decoration"
        | "-webkit-text-decoration"
        | "transition"
        | "-webkit-transition"
        | "animation"
        | "-webkit-animation"
        | "white-space" => PropertyKind::Shorthand,

        // Unsupported properties (recognised CSS properties with no QML mapping)
        "backdrop-filter"
        | "-webkit-backdrop-filter"
        | "mix-blend-mode"
        | "background-blend-mode"
        | "transform-style"
        | "-webkit-transform-style"
        | "perspective"
        | "-webkit-perspective"
        | "perspective-origin"
        | "-webkit-perspective-origin"
        | "backface-visibility"
        | "-webkit-backface-visibility"
        | "writing-mode"
        | "-webkit-writing-mode"
        | "text-orientation"
        | "columns"
        | "column-count"
        | "column-width"
        | "column-rule"
        | "column-rule-width"
        | "column-rule-style"
        | "column-rule-color"
        | "column-span"
        | "column-fill"
        | "shape-outside"
        | "shape-margin"
        | "shape-image-threshold"
        | "mask"
        | "mask-image"
        | "mask-mode"
        | "mask-repeat"
        | "mask-position"
        | "mask-clip"
        | "mask-origin"
        | "mask-size"
        | "mask-composite"
        | "-webkit-mask"
        | "-webkit-mask-image"
        | "clip-path"
        | "-webkit-clip-path"
        | "offset"
        | "offset-path"
        | "offset-distance"
        | "offset-rotate"
        | "offset-anchor"
        | "offset-position" => PropertyKind::Unsupported,

        _ => PropertyKind::Unknown,
    }
}

/// Expands a declaration (`name` is a shorthand or a longhand; `value` has no
/// `var()` and is not a CSS-wide keyword) into `(Longhand, String)` pairs.
pub(crate) fn expand(name: &str, value: &str) -> Result<Vec<(Longhand, String)>, String> {
    let lower = name.trim().to_ascii_lowercase();
    let raw_cvs = parse_component_values(value)?;
    let cvs = trim_ws(&raw_cvs);
    if cvs.is_empty() {
        return Err(format!("empty value for '{name}'"));
    }
    if shorthand::shorthand_longhands(&lower).is_some() {
        let pairs = shorthand::expand_shorthand(&lower, cvs)?;
        let vp = Viewport::default();
        let ctx = ComputeContext {
            parent: None,
            root_font_size: 16.0,
            viewport: &vp,
        };
        let mut style = ComputedStyle::default();
        for (l, v) in &pairs {
            if CssWide::parse(v).is_some() {
                return Err(format!("CSS-wide keyword in shorthand '{name}'"));
            }
            compute(&mut style, *l, v, &ctx)?;
        }
        return Ok(pairs);
    }
    if let PropertyKind::Longhand(l) = kind(&lower) {
        return Ok(vec![(l, value.trim().to_owned())]);
    }
    Err(format!("cannot expand property '{name}'"))
}

/// Returns the list of [`Longhand`]s that `name` sets (used when a shorthand is
/// given a CSS-wide keyword such as `margin: inherit`).
pub(crate) fn longhands_of(name: &str) -> Vec<Longhand> {
    let lower = name.trim().to_ascii_lowercase();
    if let Some(list) = shorthand::shorthand_longhands(&lower) {
        return list;
    }
    if let PropertyKind::Longhand(l) = kind(&lower) {
        return vec![l];
    }
    Vec::new()
}

/// Whether `l` is an inherited property by default in CSS.
pub(crate) fn is_inherited(l: Longhand) -> bool {
    matches!(
        l,
        Longhand::Color
            | Longhand::Direction
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
            | Longhand::Visibility
            | Longhand::Cursor
            | Longhand::PointerEvents
            | Longhand::ListStyleType
            | Longhand::ListStylePosition
            | Longhand::ListStyleImage
            | Longhand::BorderCollapse
            | Longhand::BorderSpacing
            | Longhand::CaptionSide
            | Longhand::AccentColor
    )
}

/// Computation priority for `l` (lower runs first).
///
/// * `0`: `font-size` (resolves `em` on the element)
/// * `1`: `line-height` and other font properties (resolves `lh` on the element)
/// * `2`: `color` and `direction` (resolves `currentColor` and `text-align: match-parent`)
/// * `3`: `border-*-style` and `outline-style` (must precede border/outline widths & colors)
/// * `4`: `border-*-width`, `border-*-color`, `outline-width`, `outline-color`, `outline-offset`
/// * `5`: `background-image`, `transition-property`, `animation-name` (determine list lengths)
/// * `6`: `translate`, `rotate`, `scale` (individual transforms in spec order before `transform`)
/// * `7`: `transform` and all remaining longhands
pub(crate) use font::settle_font_size;

pub(crate) fn priority(l: Longhand) -> u8 {
    match l {
        // The family says what the size comes to.
        Longhand::FontSize | Longhand::FontFamily => 0,
        Longhand::FontWeight
        | Longhand::FontStyle
        | Longhand::FontVariantCaps
        | Longhand::LineHeight => 1,
        Longhand::Color | Longhand::Direction => 2,
        Longhand::BorderTopStyle
        | Longhand::BorderRightStyle
        | Longhand::BorderBottomStyle
        | Longhand::BorderLeftStyle
        | Longhand::OutlineStyle => 3,
        Longhand::BorderTopWidth
        | Longhand::BorderRightWidth
        | Longhand::BorderBottomWidth
        | Longhand::BorderLeftWidth
        | Longhand::BorderTopColor
        | Longhand::BorderRightColor
        | Longhand::BorderBottomColor
        | Longhand::BorderLeftColor
        | Longhand::OutlineWidth
        | Longhand::OutlineColor
        | Longhand::OutlineOffset => 4,
        Longhand::BackgroundImage | Longhand::TransitionProperty | Longhand::AnimationName => 5,
        Longhand::Translate | Longhand::Rotate | Longhand::Scale => 6,
        _ => 7,
    }
}

/// Context needed to resolve relative units (`em`, `rem`, `vw`, `vh`,
/// `currentColor`, `bolder`, etc.) when computing a longhand value.
pub(crate) struct ComputeContext<'a> {
    pub parent: Option<&'a ComputedStyle>,
    pub root_font_size: f32,
    pub viewport: &'a Viewport,
}

/// Parses `value` for longhand `l` and writes the result into `style`.
///
/// On `Err`, `style` must be left unchanged.
pub(crate) fn compute(
    style: &mut ComputedStyle,
    l: Longhand,
    value: &str,
    ctx: &ComputeContext<'_>,
) -> Result<(), String> {
    let raw_cvs = parse_component_values(value)?;
    let cvs = trim_ws(&raw_cvs);
    if cvs.is_empty() {
        return Err(format!("empty value for {l:?}"));
    }

    let uctx_pct = UnitContext::for_element(style, ctx, true);
    let uctx_no_pct = UnitContext::for_element(style, ctx, false);

    match l {
        // Priority 0 & 1: Font & line-height
        Longhand::FontSize => {
            style.font_size = parse_font_size(cvs, ctx)?;
            (style.font_size_unscaled, style.font_size_keyword) =
                font_size_origin(cvs, style.font_size, ctx);
        }
        Longhand::FontFamily => {
            style.font_family = parse_font_family(cvs)?;
        }
        Longhand::FontWeight => {
            style.font_weight = parse_font_weight(cvs, ctx)?;
        }
        Longhand::FontStyle => {
            style.font_style = parse_font_style(cvs)?;
        }
        Longhand::FontVariantCaps => {
            style.small_caps = parse_font_variant_caps(cvs)?;
        }
        Longhand::LineHeight => {
            style.line_height = parse_line_height(cvs, style, ctx)?;
        }

        // Priority 2: Color & direction
        Longhand::Color => {
            let parent_color = ctx.parent.map_or(crate::Rgba::BLACK, |p| p.color);
            set_color(style, parse_color_cvs(cvs, parent_color)?);
        }
        Longhand::Direction => {
            style.direction = parse_direction(cvs)?;
        }

        // Priority 3: Border & outline styles
        Longhand::BorderTopStyle => {
            let st = parse_border_style(cvs, false)?;
            apply_border_style(&mut style.border.top, st, style.color);
        }
        Longhand::BorderRightStyle => {
            let st = parse_border_style(cvs, false)?;
            apply_border_style(&mut style.border.right, st, style.color);
        }
        Longhand::BorderBottomStyle => {
            let st = parse_border_style(cvs, false)?;
            apply_border_style(&mut style.border.bottom, st, style.color);
        }
        Longhand::BorderLeftStyle => {
            let st = parse_border_style(cvs, false)?;
            apply_border_style(&mut style.border.left, st, style.color);
        }
        Longhand::OutlineStyle => {
            let st = parse_border_style(cvs, true)?;
            apply_border_style(&mut style.outline, st, style.color);
        }

        // Priority 4: Border & outline widths, colors, offset
        Longhand::BorderTopWidth => {
            let w = parse_line_width(cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.top, w);
        }
        Longhand::BorderRightWidth => {
            let w = parse_line_width(cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.right, w);
        }
        Longhand::BorderBottomWidth => {
            let w = parse_line_width(cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.bottom, w);
        }
        Longhand::BorderLeftWidth => {
            let w = parse_line_width(cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.border.left, w);
        }
        Longhand::BorderTopColor => {
            let c = parse_color_cvs(cvs, style.color)?;
            apply_border_color(&mut style.border.top, c, is_current_color_cvs(cvs));
        }
        Longhand::BorderRightColor => {
            let c = parse_color_cvs(cvs, style.color)?;
            apply_border_color(&mut style.border.right, c, is_current_color_cvs(cvs));
        }
        Longhand::BorderBottomColor => {
            let c = parse_color_cvs(cvs, style.color)?;
            apply_border_color(&mut style.border.bottom, c, is_current_color_cvs(cvs));
        }
        Longhand::BorderLeftColor => {
            let c = parse_color_cvs(cvs, style.color)?;
            apply_border_color(&mut style.border.left, c, is_current_color_cvs(cvs));
        }
        Longhand::OutlineWidth => {
            let w = parse_line_width(cvs, &uctx_no_pct)?;
            apply_border_width(&mut style.outline, w);
        }
        Longhand::OutlineColor => {
            let c = parse_color_cvs(cvs, style.color)?;
            apply_border_color(&mut style.outline, c, is_current_color_cvs(cvs));
        }
        Longhand::OutlineOffset => {
            style.outline_offset = parse_outline_offset(cvs, &uctx_no_pct)?;
        }

        // Display & positioning
        Longhand::Display => {
            style.display = parse_display(cvs)?;
        }
        Longhand::Position => {
            style.position = parse_position(cvs)?;
        }
        Longhand::Top => {
            style.top = parse_inset_side(cvs, &uctx_pct)?;
        }
        Longhand::Right => {
            style.right = parse_inset_side(cvs, &uctx_pct)?;
        }
        Longhand::Bottom => {
            style.bottom = parse_inset_side(cvs, &uctx_pct)?;
        }
        Longhand::Left => {
            style.left = parse_inset_side(cvs, &uctx_pct)?;
        }
        Longhand::ZIndex => {
            style.z_index = parse_z_index(cvs)?;
        }
        Longhand::Float => {
            style.float = parse_float(cvs)?;
        }
        Longhand::Clear => {
            style.clear = parse_clear(cvs)?;
        }

        // Box model & sizing
        Longhand::BoxSizing => {
            style.box_sizing = parse_box_sizing(cvs)?;
        }
        Longhand::Width => {
            style.width = parse_size(cvs, &uctx_pct, false, false)?;
        }
        Longhand::Height => {
            style.height = parse_size(cvs, &uctx_pct, false, false)?;
        }
        Longhand::MinWidth => {
            let sz = parse_size(cvs, &uctx_pct, false, false)?;
            style.min_width = if sz == Size::None { Size::Auto } else { sz };
        }
        Longhand::MinHeight => {
            let sz = parse_size(cvs, &uctx_pct, false, false)?;
            style.min_height = if sz == Size::None { Size::Auto } else { sz };
        }
        Longhand::MaxWidth => {
            style.max_width = parse_size(cvs, &uctx_pct, true, false)?;
        }
        Longhand::MaxHeight => {
            style.max_height = parse_size(cvs, &uctx_pct, true, false)?;
        }
        Longhand::AspectRatio => {
            style.aspect_ratio = parse_aspect_ratio(cvs)?;
        }
        Longhand::MarginTop => {
            style.margin.top = parse_margin_side(cvs, &uctx_pct)?;
        }
        Longhand::MarginRight => {
            style.margin.right = parse_margin_side(cvs, &uctx_pct)?;
        }
        Longhand::MarginBottom => {
            style.margin.bottom = parse_margin_side(cvs, &uctx_pct)?;
        }
        Longhand::MarginLeft => {
            style.margin.left = parse_margin_side(cvs, &uctx_pct)?;
        }
        Longhand::PaddingTop => {
            style.padding.top = parse_padding_side(cvs, &uctx_pct)?;
        }
        Longhand::PaddingRight => {
            style.padding.right = parse_padding_side(cvs, &uctx_pct)?;
        }
        Longhand::PaddingBottom => {
            style.padding.bottom = parse_padding_side(cvs, &uctx_pct)?;
        }
        Longhand::PaddingLeft => {
            style.padding.left = parse_padding_side(cvs, &uctx_pct)?;
        }
        Longhand::BorderTopLeftRadius => {
            set_corner_radius(
                &mut style.border_radius.top_left,
                &mut style.border_radius_calc.top_left,
                parse_corner_radius(cvs, &uctx_pct)?,
            );
        }
        Longhand::BorderTopRightRadius => {
            set_corner_radius(
                &mut style.border_radius.top_right,
                &mut style.border_radius_calc.top_right,
                parse_corner_radius(cvs, &uctx_pct)?,
            );
        }
        Longhand::BorderBottomRightRadius => {
            set_corner_radius(
                &mut style.border_radius.bottom_right,
                &mut style.border_radius_calc.bottom_right,
                parse_corner_radius(cvs, &uctx_pct)?,
            );
        }
        Longhand::BorderBottomLeftRadius => {
            set_corner_radius(
                &mut style.border_radius.bottom_left,
                &mut style.border_radius_calc.bottom_left,
                parse_corner_radius(cvs, &uctx_pct)?,
            );
        }

        // Flexbox & alignment
        Longhand::FlexDirection => {
            style.flex_direction = parse_flex_direction(cvs)?;
        }
        Longhand::FlexWrap => {
            style.flex_wrap = parse_flex_wrap(cvs)?;
        }
        Longhand::JustifyContent => {
            style.justify_content = parse_align(cvs, AlignTarget::JustifyContent)?;
        }
        Longhand::AlignItems => {
            style.align_items = parse_align(cvs, AlignTarget::AlignItems)?;
        }
        Longhand::AlignContent => {
            style.align_content = parse_align(cvs, AlignTarget::AlignContent)?;
        }
        Longhand::AlignSelf => {
            style.align_self = parse_align(cvs, AlignTarget::AlignSelf)?;
        }
        Longhand::JustifyItems => {
            style.justify_items = parse_align(cvs, AlignTarget::JustifyItems)?;
        }
        Longhand::JustifySelf => {
            style.justify_self = parse_align(cvs, AlignTarget::JustifySelf)?;
        }
        Longhand::RowGap => {
            style.row_gap = parse_gap_side(cvs, &uctx_pct)?;
        }
        Longhand::ColumnGap => {
            style.column_gap = parse_gap_side(cvs, &uctx_pct)?;
        }
        Longhand::FlexGrow => {
            style.flex_grow = parse_flex_factor(cvs)?;
        }
        Longhand::FlexShrink => {
            style.flex_shrink = parse_flex_factor(cvs)?;
        }
        Longhand::FlexBasis => {
            style.flex_basis = parse_flex_basis(cvs, &uctx_pct)?;
        }
        Longhand::Order => {
            style.order = parse_order(cvs)?;
        }

        // Grid
        Longhand::GridTemplateColumns => {
            style.grid_template_columns = parse_grid_template_tracks(cvs, &uctx_pct)?;
        }
        Longhand::GridTemplateRows => {
            style.grid_template_rows = parse_grid_template_tracks(cvs, &uctx_pct)?;
        }
        Longhand::GridTemplateAreas => {
            style.grid_template_areas = parse_grid_template_areas(cvs)?;
        }
        Longhand::GridAutoFlow => {
            (style.grid_auto_flow_column, style.grid_auto_flow_dense) = parse_grid_auto_flow(cvs)?;
        }
        Longhand::GridAutoRows => {
            style.grid_auto_rows = parse_grid_auto_tracks(cvs, &uctx_pct)?;
        }
        Longhand::GridAutoColumns => {
            style.grid_auto_columns = parse_grid_auto_tracks(cvs, &uctx_pct)?;
        }
        Longhand::GridRowStart => {
            style.grid_row_start = parse_grid_line(cvs)?;
        }
        Longhand::GridRowEnd => {
            style.grid_row_end = parse_grid_line(cvs)?;
        }
        Longhand::GridColumnStart => {
            style.grid_column_start = parse_grid_line(cvs)?;
        }
        Longhand::GridColumnEnd => {
            style.grid_column_end = parse_grid_line(cvs)?;
        }

        // Backgrounds & visual
        Longhand::BackgroundColor => {
            style.background_color = parse_background_color(cvs, style.color)?;
        }
        Longhand::BackgroundImage => {
            let imgs = parse_background_image_list(cvs, &uctx_pct, style.color)?;
            apply_background_images(style, imgs);
        }
        Longhand::BackgroundPositionX => {
            let xs = parse_bg_position_axis_list(cvs, &uctx_pct, true)?;
            apply_background_prop(style, &xs, &LengthPercentage::ZERO, |layer, x| {
                layer.position_x = x
            });
        }
        Longhand::BackgroundPositionY => {
            let ys = parse_bg_position_axis_list(cvs, &uctx_pct, false)?;
            apply_background_prop(style, &ys, &LengthPercentage::ZERO, |layer, y| {
                layer.position_y = y
            });
        }
        Longhand::BackgroundSize => {
            let sizes = parse_bg_size_list(cvs, &uctx_pct)?;
            apply_background_prop(style, &sizes, &crate::BackgroundSize::Auto, |layer, sz| {
                layer.size = sz
            });
        }
        Longhand::BackgroundRepeat => {
            let reps = parse_bg_repeat_list(cvs)?;
            apply_background_prop(style, &reps, &BackgroundRepeat::Repeat, |layer, r| {
                layer.repeat = r
            });
        }
        Longhand::BackgroundAttachment => {
            let list = parse_bg_attachment_list(cvs)?;
            apply_background_prop(style, &list, &BackgroundAttachment::Scroll, |layer, v| {
                layer.attachment = v
            });
        }
        Longhand::BackgroundOrigin => {
            let list = parse_bg_origin_list(cvs)?;
            apply_background_prop(style, &list, &BackgroundOrigin::PaddingBox, |layer, v| {
                layer.origin = v
            });
        }
        Longhand::BackgroundClip => {
            let list = parse_bg_clip_list(cvs)?;
            apply_background_prop(style, &list, &BackgroundClip::BorderBox, |layer, v| {
                layer.clip = v
            });
        }
        Longhand::BoxShadow => {
            style.box_shadow = parse_shadow_list(cvs, &uctx_no_pct, style.color, true)?;
        }
        Longhand::Opacity => {
            style.opacity = parse_opacity(cvs)?;
        }
        Longhand::ScrollBehavior => {
            style.scroll_smooth = text::parse_scroll_behavior(cvs)?;
        }
        Longhand::Visibility => {
            style.visibility = parse_visibility(cvs)?;
        }
        Longhand::OverflowX => {
            style.overflow_x = parse_overflow(cvs)?;
        }
        Longhand::OverflowY => {
            style.overflow_y = parse_overflow(cvs)?;
        }
        Longhand::ObjectFit => {
            style.object_fit = parse_object_fit(cvs)?;
        }
        Longhand::ObjectPosition => {
            (style.object_position_x, style.object_position_y) =
                parse_position_2d(cvs, &uctx_pct, true)?;
        }
        Longhand::Filter => {
            style.filter = parse_filter(cvs, &uctx_no_pct, style.color)?;
        }

        // Transforms (`translate`, `rotate`, `scale` run at priority 6 before `transform` at 7)
        Longhand::Translate => {
            let tr = parse_individual_translate(cvs, &uctx_pct)?;
            let tail = transform_list_tail(style).to_vec();
            style.translate = tr;
            rebuild_transforms(style, tail);
        }
        Longhand::Rotate => {
            let rot = parse_individual_rotate(cvs)?;
            let tail = transform_list_tail(style).to_vec();
            style.rotate = rot;
            rebuild_transforms(style, tail);
        }
        Longhand::Scale => {
            let sc = parse_individual_scale(cvs)?;
            let tail = transform_list_tail(style).to_vec();
            style.scale = sc;
            rebuild_transforms(style, tail);
        }
        Longhand::Transform => {
            let fns = parse_transform_list(cvs, &uctx_pct)?;
            rebuild_transforms(style, fns);
        }
        Longhand::TransformOrigin => {
            style.transform_origin = parse_transform_origin(cvs, &uctx_pct)?;
        }

        // Typography & text
        Longhand::LetterSpacing => {
            style.letter_spacing = parse_spacing(cvs, &uctx_no_pct)?;
        }
        Longhand::WordSpacing => {
            style.word_spacing = parse_spacing(cvs, &uctx_no_pct)?;
        }
        Longhand::TextAlign => {
            style.text_align = parse_text_align(cvs, ctx)?;
        }
        Longhand::TextIndent => {
            style.text_indent = parse_text_indent(cvs, &uctx_pct)?;
        }
        Longhand::TextTransform => {
            style.text_transform = parse_text_transform(cvs)?;
        }
        Longhand::TextDecorationLine => {
            style.text_decoration_line = parse_text_decoration_line(cvs)?;
        }
        Longhand::TextDecorationStyle => {
            style.text_decoration_style = parse_text_decoration_style(cvs)?;
            style.text_decoration_wavy = text::is_wavy(cvs);
        }
        Longhand::TextDecorationColor => {
            style.text_decoration_color = parse_text_decoration_color(cvs, style.color)?;
        }
        Longhand::TextDecorationThickness => {
            parse_text_decoration_thickness(cvs, &uctx_pct)?;
        }
        Longhand::TextUnderlineOffset => {
            let _ = parse_length_auto_cvs(cvs, &uctx_pct, false)?;
        }
        Longhand::TextOverflow => {
            style.text_overflow = parse_text_overflow(cvs)?;
        }
        Longhand::WebkitLineClamp => {
            let _ = parse_line_clamp(cvs)?;
        }
        Longhand::WhiteSpaceCollapse => {
            let collapse = parse_white_space_collapse(cvs)?;
            let (_, wrap) = decompose_white_space(style.white_space);
            style.white_space = compose_white_space(collapse, wrap);
        }
        Longhand::TextWrapMode => {
            let wrap = parse_text_wrap_mode(cvs)?;
            let (collapse, _) = decompose_white_space(style.white_space);
            style.white_space = compose_white_space(collapse, wrap);
        }
        Longhand::WordBreak => {
            style.word_break = parse_word_break(cvs)?;
        }
        Longhand::OverflowWrap => {
            style.overflow_wrap_anywhere = parse_overflow_wrap(cvs)?;
        }
        Longhand::VerticalAlign => {
            style.vertical_align = parse_vertical_align(cvs, style, ctx)?;
        }
        Longhand::TextShadow => {
            style.text_shadow = parse_text_shadow(cvs, &uctx_no_pct, style.color)?;
        }

        // UI, lists, generated content
        Longhand::Cursor => {
            style.cursor = parse_cursor(cvs)?;
        }
        Longhand::PointerEvents => {
            style.pointer_events = parse_pointer_events(cvs)?;
        }
        Longhand::UserSelect => {
            style.user_select_none = parse_user_select(cvs)?;
        }
        Longhand::ListStyleType => {
            style.list_style_type = parse_list_style_type(cvs)?;
        }
        Longhand::ListStylePosition => {
            style.list_style_inside = parse_list_style_position(cvs)?;
        }
        Longhand::ListStyleImage => {
            style.list_style_image = parse_list_style_image(cvs)?;
        }
        Longhand::Content => {
            style.content = parse_content(cvs)?;
        }
        Longhand::CounterReset => style.counter_reset = parse_counter_changes(cvs, 0)?,
        Longhand::CounterIncrement => style.counter_increment = parse_counter_changes(cvs, 1)?,
        Longhand::CounterSet => style.counter_set = parse_counter_changes(cvs, 0)?,
        Longhand::BorderCollapse => {
            style.border_collapse = parse_border_collapse(cvs)?;
        }
        Longhand::BorderSpacing => {
            style.border_spacing = parse_border_spacing(cvs, &uctx_no_pct)?;
        }
        Longhand::CaptionSide => {
            style.caption_side = parse_caption_side(cvs)?;
        }
        Longhand::AccentColor => {
            style.accent_color = parse_accent_color(cvs, style.color)?;
        }
        Longhand::TableLayout => {
            style.table_layout = parse_table_layout(cvs)?;
        }

        // Transitions
        Longhand::TransitionProperty => {
            let props = parse_transition_property_list(cvs)?;
            apply_transition_properties(style, props);
        }
        Longhand::TransitionDuration => {
            let durs = parse_time_list(cvs, true, false)?;
            apply_transition_field(style, &durs, |tr, d| tr.duration_ms = d);
        }
        Longhand::TransitionTimingFunction => {
            let easings = parse_easing_list(cvs)?;
            apply_transition_field(style, &easings, |tr, e| tr.easing = e);
        }
        Longhand::TransitionDelay => {
            let delays = parse_time_list(cvs, false, false)?;
            apply_transition_field(style, &delays, |tr, d| tr.delay_ms = d);
        }

        // Animations
        Longhand::AnimationName => {
            let names = parse_animation_name_list(cvs)?;
            apply_animation_names(style, names);
        }
        Longhand::AnimationDuration => {
            let durs = parse_time_list(cvs, true, true)?;
            apply_animation_field(style, &durs, |anim, d| anim.duration_ms = d);
        }
        Longhand::AnimationTimingFunction => {
            let easings = parse_easing_list(cvs)?;
            apply_animation_field(style, &easings, |anim, e| anim.easing = e);
        }
        Longhand::AnimationDelay => {
            let delays = parse_time_list(cvs, false, false)?;
            apply_animation_field(style, &delays, |anim, d| anim.delay_ms = d);
        }
        Longhand::AnimationIterationCount => {
            let counts = parse_animation_iteration_list(cvs)?;
            apply_animation_field(style, &counts, |anim, c| anim.iteration_count = c);
        }
        Longhand::AnimationDirection => {
            let dirs = parse_animation_direction_list(cvs)?;
            apply_animation_field(style, &dirs, |anim, (alt, rev)| {
                anim.alternate = alt;
                anim.reverse = rev;
            });
        }
        Longhand::AnimationFillMode => {
            let modes = parse_animation_fill_mode_list(cvs)?;
            apply_animation_field(style, &modes, |anim, (fwd, bwd)| {
                anim.fill_forwards = fwd;
                anim.fill_backwards = bwd;
            });
        }
        Longhand::AnimationPlayState => {
            let states = parse_animation_play_state_list(cvs)?;
            apply_animation_field(style, &states, |anim, paused| anim.paused = paused);
        }
    }

    Ok(())
}

/// Sets `color` and the border/outline sides whose color is still the
/// initial `currentColor`, so they track it whichever path set it.
fn set_color(style: &mut ComputedStyle, c: crate::Rgba) {
    style.color = c;
    for side in [
        &mut style.border.top,
        &mut style.border.right,
        &mut style.border.bottom,
        &mut style.border.left,
        &mut style.outline,
    ] {
        if side.explicit_color.is_none()
            && !matches!(
                side.style,
                crate::BorderStyle::None | crate::BorderStyle::Hidden
            )
        {
            side.color = c;
        }
    }
}

/// Copies the single longhand `l` from `src` to `dst` (used for `inherit`,
/// `initial`, `unset`, and `revert`).
pub(crate) fn copy(dst: &mut ComputedStyle, src: &ComputedStyle, l: Longhand) {
    match l {
        Longhand::FontSize => {
            dst.font_size = src.font_size;
            dst.font_size_unscaled = src.font_size_unscaled;
            dst.font_size_keyword = src.font_size_keyword;
        }
        Longhand::FontFamily => dst.font_family.clone_from(&src.font_family),
        Longhand::FontWeight => dst.font_weight = src.font_weight,
        Longhand::FontStyle => dst.font_style = src.font_style,
        Longhand::FontVariantCaps => dst.small_caps = src.small_caps,
        Longhand::LineHeight => dst.line_height = src.line_height,
        Longhand::Color => set_color(dst, src.color),
        Longhand::Direction => dst.direction = src.direction,

        Longhand::BorderTopStyle => dst.border.top.style = src.border.top.style,
        Longhand::BorderRightStyle => dst.border.right.style = src.border.right.style,
        Longhand::BorderBottomStyle => dst.border.bottom.style = src.border.bottom.style,
        Longhand::BorderLeftStyle => dst.border.left.style = src.border.left.style,
        Longhand::OutlineStyle => dst.outline.style = src.outline.style,

        Longhand::BorderTopWidth => {
            dst.border.top.width = src.border.top.width;
            dst.border.top.explicit_width = src.border.top.explicit_width;
        }
        Longhand::BorderRightWidth => {
            dst.border.right.width = src.border.right.width;
            dst.border.right.explicit_width = src.border.right.explicit_width;
        }
        Longhand::BorderBottomWidth => {
            dst.border.bottom.width = src.border.bottom.width;
            dst.border.bottom.explicit_width = src.border.bottom.explicit_width;
        }
        Longhand::BorderLeftWidth => {
            dst.border.left.width = src.border.left.width;
            dst.border.left.explicit_width = src.border.left.explicit_width;
        }
        Longhand::BorderTopColor => {
            dst.border.top.color = src.border.top.color;
            dst.border.top.explicit_color = src.border.top.explicit_color;
        }
        Longhand::BorderRightColor => {
            dst.border.right.color = src.border.right.color;
            dst.border.right.explicit_color = src.border.right.explicit_color;
        }
        Longhand::BorderBottomColor => {
            dst.border.bottom.color = src.border.bottom.color;
            dst.border.bottom.explicit_color = src.border.bottom.explicit_color;
        }
        Longhand::BorderLeftColor => {
            dst.border.left.color = src.border.left.color;
            dst.border.left.explicit_color = src.border.left.explicit_color;
        }
        Longhand::OutlineWidth => {
            dst.outline.width = src.outline.width;
            dst.outline.explicit_width = src.outline.explicit_width;
        }
        Longhand::OutlineColor => {
            dst.outline.color = src.outline.color;
            dst.outline.explicit_color = src.outline.explicit_color;
        }
        Longhand::OutlineOffset => dst.outline_offset = src.outline_offset,

        Longhand::Display => dst.display = src.display,
        Longhand::Position => dst.position = src.position,
        Longhand::Top => dst.top.clone_from(&src.top),
        Longhand::Right => dst.right.clone_from(&src.right),
        Longhand::Bottom => dst.bottom.clone_from(&src.bottom),
        Longhand::Left => dst.left.clone_from(&src.left),
        Longhand::ZIndex => dst.z_index = src.z_index,
        Longhand::Float => dst.float = src.float,
        Longhand::Clear => dst.clear = src.clear,

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
        Longhand::BorderTopLeftRadius => {
            dst.border_radius.top_left = src.border_radius.top_left;
            dst.border_radius_calc
                .top_left
                .clone_from(&src.border_radius_calc.top_left);
        }
        Longhand::BorderTopRightRadius => {
            dst.border_radius.top_right = src.border_radius.top_right;
            dst.border_radius_calc
                .top_right
                .clone_from(&src.border_radius_calc.top_right);
        }
        Longhand::BorderBottomRightRadius => {
            dst.border_radius.bottom_right = src.border_radius.bottom_right;
            dst.border_radius_calc
                .bottom_right
                .clone_from(&src.border_radius_calc.bottom_right);
        }
        Longhand::BorderBottomLeftRadius => {
            dst.border_radius.bottom_left = src.border_radius.bottom_left;
            dst.border_radius_calc
                .bottom_left
                .clone_from(&src.border_radius_calc.bottom_left);
        }

        Longhand::FlexDirection => dst.flex_direction = src.flex_direction,
        Longhand::FlexWrap => dst.flex_wrap = src.flex_wrap,
        Longhand::JustifyContent => dst.justify_content = src.justify_content,
        Longhand::AlignItems => dst.align_items = src.align_items,
        Longhand::AlignContent => dst.align_content = src.align_content,
        Longhand::AlignSelf => dst.align_self = src.align_self,
        Longhand::JustifyItems => dst.justify_items = src.justify_items,
        Longhand::JustifySelf => dst.justify_self = src.justify_self,
        Longhand::RowGap => dst.row_gap.clone_from(&src.row_gap),
        Longhand::ColumnGap => dst.column_gap.clone_from(&src.column_gap),
        Longhand::FlexGrow => dst.flex_grow = src.flex_grow,
        Longhand::FlexShrink => dst.flex_shrink = src.flex_shrink,
        Longhand::FlexBasis => dst.flex_basis.clone_from(&src.flex_basis),
        Longhand::Order => dst.order = src.order,

        Longhand::GridTemplateColumns => dst
            .grid_template_columns
            .clone_from(&src.grid_template_columns),
        Longhand::GridTemplateRows => dst.grid_template_rows.clone_from(&src.grid_template_rows),
        Longhand::GridTemplateAreas => dst.grid_template_areas.clone_from(&src.grid_template_areas),
        Longhand::GridAutoFlow => {
            dst.grid_auto_flow_column = src.grid_auto_flow_column;
            dst.grid_auto_flow_dense = src.grid_auto_flow_dense;
        }
        Longhand::GridAutoRows => dst.grid_auto_rows.clone_from(&src.grid_auto_rows),
        Longhand::GridAutoColumns => dst.grid_auto_columns.clone_from(&src.grid_auto_columns),
        Longhand::GridRowStart => dst.grid_row_start.clone_from(&src.grid_row_start),
        Longhand::GridRowEnd => dst.grid_row_end.clone_from(&src.grid_row_end),
        Longhand::GridColumnStart => dst.grid_column_start.clone_from(&src.grid_column_start),
        Longhand::GridColumnEnd => dst.grid_column_end.clone_from(&src.grid_column_end),

        Longhand::BackgroundColor => dst.background_color = src.background_color,
        Longhand::BackgroundImage => {
            if src.background_layers.is_empty()
                || src.background_layers.iter().all(|l| l.image.is_none())
            {
                apply_background_images(dst, vec![None]);
            } else {
                let imgs = src
                    .background_layers
                    .iter()
                    .map(|l| l.image.clone())
                    .collect();
                apply_background_images(dst, imgs);
            }
        }
        Longhand::BackgroundPositionX => {
            copy_background_field(
                dst,
                src,
                LengthPercentage::ZERO,
                |l| l.position_x,
                |l, x| l.position_x = x,
            );
        }
        Longhand::BackgroundPositionY => {
            copy_background_field(
                dst,
                src,
                LengthPercentage::ZERO,
                |l| l.position_y,
                |l, y| l.position_y = y,
            );
        }
        Longhand::BackgroundSize => {
            copy_background_field(
                dst,
                src,
                crate::BackgroundSize::Auto,
                |l| l.size.clone(),
                |l, sz| l.size = sz,
            );
        }
        Longhand::BackgroundRepeat => {
            copy_background_field(
                dst,
                src,
                BackgroundRepeat::Repeat,
                |l| l.repeat,
                |l, r| l.repeat = r,
            );
        }
        Longhand::BackgroundAttachment => {
            copy_background_field(
                dst,
                src,
                BackgroundAttachment::Scroll,
                |l| l.attachment,
                |l, v| l.attachment = v,
            );
        }
        Longhand::BackgroundOrigin => {
            copy_background_field(
                dst,
                src,
                BackgroundOrigin::PaddingBox,
                |l| l.origin,
                |l, v| l.origin = v,
            );
        }
        Longhand::BackgroundClip => {
            copy_background_field(
                dst,
                src,
                BackgroundClip::BorderBox,
                |l| l.clip,
                |l, v| l.clip = v,
            );
        }
        Longhand::BoxShadow => dst.box_shadow.clone_from(&src.box_shadow),
        Longhand::Opacity => dst.opacity = src.opacity,
        Longhand::Visibility => dst.visibility = src.visibility,
        Longhand::ScrollBehavior => dst.scroll_smooth = src.scroll_smooth,
        Longhand::OverflowX => dst.overflow_x = src.overflow_x,
        Longhand::OverflowY => dst.overflow_y = src.overflow_y,
        Longhand::ObjectFit => dst.object_fit = src.object_fit,
        Longhand::ObjectPosition => {
            dst.object_position_x = src.object_position_x;
            dst.object_position_y = src.object_position_y;
        }
        Longhand::Filter => dst.filter.clone_from(&src.filter),

        Longhand::Translate => {
            let tail = transform_list_tail(dst).to_vec();
            dst.translate.clone_from(&src.translate);
            rebuild_transforms(dst, tail);
        }
        Longhand::Rotate => {
            let tail = transform_list_tail(dst).to_vec();
            dst.rotate.clone_from(&src.rotate);
            rebuild_transforms(dst, tail);
        }
        Longhand::Scale => {
            let tail = transform_list_tail(dst).to_vec();
            dst.scale.clone_from(&src.scale);
            rebuild_transforms(dst, tail);
        }
        Longhand::Transform => {
            let tail = transform_list_tail(src).to_vec();
            rebuild_transforms(dst, tail);
        }
        Longhand::TransformOrigin => dst.transform_origin = src.transform_origin,

        Longhand::LetterSpacing => dst.letter_spacing = src.letter_spacing,
        Longhand::WordSpacing => dst.word_spacing = src.word_spacing,
        Longhand::TextAlign => dst.text_align = src.text_align,
        Longhand::TextIndent => dst.text_indent.clone_from(&src.text_indent),
        Longhand::TextTransform => dst.text_transform = src.text_transform,
        Longhand::TextDecorationLine => dst.text_decoration_line = src.text_decoration_line,
        Longhand::TextDecorationStyle => {
            dst.text_decoration_style = src.text_decoration_style;
            dst.text_decoration_wavy = src.text_decoration_wavy;
        }
        Longhand::TextDecorationColor => dst.text_decoration_color = src.text_decoration_color,
        Longhand::TextDecorationThickness | Longhand::TextUnderlineOffset => {}
        Longhand::TextOverflow => dst.text_overflow = src.text_overflow,
        Longhand::WebkitLineClamp => {}
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
        Longhand::VerticalAlign => dst.vertical_align = src.vertical_align,
        Longhand::TextShadow => dst.text_shadow.clone_from(&src.text_shadow),

        Longhand::Cursor => dst.cursor = src.cursor,
        Longhand::PointerEvents => dst.pointer_events = src.pointer_events,
        Longhand::UserSelect => dst.user_select_none = src.user_select_none,
        Longhand::ListStyleType => dst.list_style_type = src.list_style_type,
        Longhand::ListStylePosition => dst.list_style_inside = src.list_style_inside,
        Longhand::ListStyleImage => dst.list_style_image.clone_from(&src.list_style_image),
        Longhand::Content => dst.content.clone_from(&src.content),
        Longhand::CounterReset => dst.counter_reset.clone_from(&src.counter_reset),
        Longhand::CounterIncrement => dst.counter_increment.clone_from(&src.counter_increment),
        Longhand::CounterSet => dst.counter_set.clone_from(&src.counter_set),
        Longhand::BorderCollapse => dst.border_collapse = src.border_collapse,
        Longhand::BorderSpacing => dst.border_spacing = src.border_spacing,
        Longhand::CaptionSide => dst.caption_side = src.caption_side,
        Longhand::AccentColor => dst.accent_color = src.accent_color,
        Longhand::TableLayout => dst.table_layout = src.table_layout,

        Longhand::TransitionProperty => {
            if src.transitions.is_empty() {
                apply_transition_properties(dst, None);
            } else {
                let props = src.transitions.iter().map(|t| t.property.clone()).collect();
                apply_transition_properties(dst, Some(props));
            }
        }
        Longhand::TransitionDuration => {
            copy_transition_field(dst, src, |d, s| d.duration_ms = s.duration_ms);
        }
        Longhand::TransitionTimingFunction => {
            copy_transition_field(dst, src, |d, s| d.easing.clone_from(&s.easing));
        }
        Longhand::TransitionDelay => {
            copy_transition_field(dst, src, |d, s| d.delay_ms = s.delay_ms);
        }
        Longhand::AnimationName => {
            if src.animations.is_empty() {
                dst.animations.clear();
            } else {
                let names = src
                    .animations
                    .iter()
                    .map(|a| Some(a.name.clone()))
                    .collect();
                apply_animation_names(dst, names);
            }
        }
        Longhand::AnimationDuration => {
            copy_animation_field(dst, src, |d, s| d.duration_ms = s.duration_ms);
        }
        Longhand::AnimationTimingFunction => {
            copy_animation_field(dst, src, |d, s| d.easing.clone_from(&s.easing));
        }
        Longhand::AnimationDelay => {
            copy_animation_field(dst, src, |d, s| d.delay_ms = s.delay_ms);
        }
        Longhand::AnimationIterationCount => {
            copy_animation_field(dst, src, |d, s| d.iteration_count = s.iteration_count);
        }
        Longhand::AnimationDirection => {
            copy_animation_field(dst, src, |d, s| {
                d.alternate = s.alternate;
                d.reverse = s.reverse;
            });
        }
        Longhand::AnimationFillMode => {
            copy_animation_field(dst, src, |d, s| {
                d.fill_forwards = s.fill_forwards;
                d.fill_backwards = s.fill_backwards;
            });
        }
        Longhand::AnimationPlayState => {
            copy_animation_field(dst, src, |d, s| d.paused = s.paused);
        }
    }
}

// ---------------------------------------------------------------------------
// Small helpers for `clear`, `-webkit-line-clamp`, and individual transforms
// ---------------------------------------------------------------------------

fn parse_clear(cvs: &[swc_css_ast::ComponentValue]) -> Result<crate::Clear, String> {
    let mut cur = cursor::Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected clear keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "none" => Ok(crate::Clear::None),
        "left" | "inline-start" => Ok(crate::Clear::Left),
        "right" | "inline-end" => Ok(crate::Clear::Right),
        "both" => Ok(crate::Clear::Both),
        _ => Err(format!("invalid clear keyword '{id}'")),
    }
}

fn parse_text_decoration_thickness(
    cvs: &[swc_css_ast::ComponentValue],
    uctx: &UnitContext,
) -> Result<(), String> {
    let mut cur = cursor::Cursor::new(cvs);
    if cur.eat_ident_eq("auto") || cur.eat_ident_eq("from-font") {
        return cur.expect_eof();
    }
    let _ = parse_length_cvs(cvs, uctx, false)?;
    Ok(())
}

fn parse_line_clamp(cvs: &[swc_css_ast::ComponentValue]) -> Result<Option<u32>, String> {
    let mut cur = cursor::Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(None);
    }
    let n = parse_integer_cvs(cvs)?;
    if n < 1 {
        return Err("-webkit-line-clamp requires a positive integer (>= 1) or 'none'".to_owned());
    }
    Ok(Some(n as u32))
}

fn individual_transform_count(style: &ComputedStyle) -> usize {
    usize::from(style.translate.is_some())
        + usize::from(style.rotate.is_some())
        + usize::from(style.scale.is_some())
}

fn transform_list_tail(style: &ComputedStyle) -> &[TransformFn] {
    let prefix = individual_transform_count(style).min(style.transform.len());
    &style.transform[prefix..]
}

/// Rebuilds `style.transform` in CSS Transforms 2 order (`translate`, `rotate`,
/// `scale`, then the `transform` list `tail`).
fn rebuild_transforms(style: &mut ComputedStyle, tail: Vec<TransformFn>) {
    let mut out = Vec::with_capacity(individual_transform_count(style) + tail.len());
    if let Some(tr) = &style.translate {
        out.push(tr.clone());
    }
    if let Some(rot) = &style.rotate {
        out.push(rot.clone());
    }
    if let Some(sc) = &style.scale {
        out.push(sc.clone());
    }
    out.extend(tail);
    style.transform = out;
}

fn set_corner_radius(
    lp_slot: &mut LengthPercentage,
    calc_slot: &mut Option<crate::CalcNode>,
    len: crate::Length,
) {
    match len {
        crate::Length::Linear(lp) => {
            *lp_slot = lp;
            *calc_slot = None;
        }
        crate::Length::Calc(node) => {
            *lp_slot = LengthPercentage::ZERO;
            *calc_slot = Some(node);
        }
    }
}

/// A longhand that turns a control's native look off when the page sets it.
pub(crate) fn is_background(l: Longhand) -> bool {
    matches!(l, Longhand::BackgroundColor | Longhand::BackgroundImage)
}

/// See [`is_background`].
pub(crate) fn is_border(l: Longhand) -> bool {
    matches!(
        l,
        Longhand::BorderTopStyle
            | Longhand::BorderRightStyle
            | Longhand::BorderBottomStyle
            | Longhand::BorderLeftStyle
            | Longhand::BorderTopWidth
            | Longhand::BorderRightWidth
            | Longhand::BorderBottomWidth
            | Longhand::BorderLeftWidth
            | Longhand::BorderTopColor
            | Longhand::BorderRightColor
            | Longhand::BorderBottomColor
            | Longhand::BorderLeftColor
    )
}
