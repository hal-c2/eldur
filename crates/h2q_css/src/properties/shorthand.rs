//! CSS shorthand metadata (`shorthand_longhands`) and AST-based shorthand
//! expansion (`expand_shorthand`).
//!
//! Omitted sub-properties in any shorthand are explicitly emitted with their
//! CSS initial values, and authored token slices are preserved verbatim (via
//! [`serialize_cvs`]) so relative units (`em`, `rem`, `lh`) and `currentColor`
//! resolve at `compute` time against the element's actual [`ComputeContext`].

use swc_css_ast::{ComponentValue, Token};

use crate::{ComputedStyle, Rgba, Viewport};

use super::{
    ComputeContext, Longhand,
    animation::{
        eat_anim_direction, eat_anim_fill_mode, eat_anim_iteration_count, eat_anim_play_state,
        eat_easing, eat_keyframes_name,
    },
    background::{eat_bg_image_item, eat_bg_repeat_in_shorthand, parse_bg_size_item},
    box_model::{eat_border_style, eat_line_width, parse_size},
    color::eat_color,
    cursor::{
        Cursor, is_css_wide_keyword, non_ws_items, serialize_cv, serialize_cvs,
        split_comma_separated, split_slash_separated,
    },
    flex::{AlignTarget, eat_align, eat_flex_direction, eat_flex_wrap},
    font::{
        eat_font_style, is_font_stretch_kw, parse_font_family, parse_font_size,
        parse_font_variant_caps, parse_font_weight, parse_line_height,
    },
    grid::{eat_track_size, parse_grid_line, parse_grid_template_areas, parse_grid_template_tracks},
    length::{UnitContext, eat_length, eat_time_ms, parse_length_cvs, parse_number_cvs},
    text::{
        eat_decoration_color_token, eat_list_style_position, eat_list_style_type,
        eat_text_decoration_style, parse_white_space_collapse, parse_text_wrap_mode,
    },
};

fn dummy_viewport() -> Viewport {
    Viewport::default()
}

fn dummy_uctx(allow_percent: bool) -> UnitContext {
    let vp = dummy_viewport();
    let ctx = ComputeContext {
        parent: None,
        root_font_size: 16.0,
        viewport: &vp,
    };
    let style = ComputedStyle::default();
    UnitContext::for_element(&style, &ctx, allow_percent)
}

/// Returns `Some(longhands)` if `lower_name` is a supported CSS shorthand property.
pub(crate) fn shorthand_longhands(lower_name: &str) -> Option<Vec<Longhand>> {
    match lower_name {
        "margin" => Some(vec![
            Longhand::MarginTop,
            Longhand::MarginRight,
            Longhand::MarginBottom,
            Longhand::MarginLeft,
        ]),
        "margin-block" => Some(vec![Longhand::MarginTop, Longhand::MarginBottom]),
        "margin-inline" => Some(vec![Longhand::MarginLeft, Longhand::MarginRight]),
        "padding" => Some(vec![
            Longhand::PaddingTop,
            Longhand::PaddingRight,
            Longhand::PaddingBottom,
            Longhand::PaddingLeft,
        ]),
        "padding-block" => Some(vec![Longhand::PaddingTop, Longhand::PaddingBottom]),
        "padding-inline" => Some(vec![Longhand::PaddingLeft, Longhand::PaddingRight]),
        "inset" => Some(vec![
            Longhand::Top,
            Longhand::Right,
            Longhand::Bottom,
            Longhand::Left,
        ]),
        "inset-block" => Some(vec![Longhand::Top, Longhand::Bottom]),
        "inset-inline" => Some(vec![Longhand::Left, Longhand::Right]),
        "border" => Some(vec![
            Longhand::BorderTopWidth,
            Longhand::BorderTopStyle,
            Longhand::BorderTopColor,
            Longhand::BorderRightWidth,
            Longhand::BorderRightStyle,
            Longhand::BorderRightColor,
            Longhand::BorderBottomWidth,
            Longhand::BorderBottomStyle,
            Longhand::BorderBottomColor,
            Longhand::BorderLeftWidth,
            Longhand::BorderLeftStyle,
            Longhand::BorderLeftColor,
        ]),
        "border-top" | "border-block-start" => Some(vec![
            Longhand::BorderTopWidth,
            Longhand::BorderTopStyle,
            Longhand::BorderTopColor,
        ]),
        "border-right" | "border-inline-end" => Some(vec![
            Longhand::BorderRightWidth,
            Longhand::BorderRightStyle,
            Longhand::BorderRightColor,
        ]),
        "border-bottom" | "border-block-end" => Some(vec![
            Longhand::BorderBottomWidth,
            Longhand::BorderBottomStyle,
            Longhand::BorderBottomColor,
        ]),
        "border-left" | "border-inline-start" => Some(vec![
            Longhand::BorderLeftWidth,
            Longhand::BorderLeftStyle,
            Longhand::BorderLeftColor,
        ]),
        "border-block" => Some(vec![
            Longhand::BorderTopWidth,
            Longhand::BorderTopStyle,
            Longhand::BorderTopColor,
            Longhand::BorderBottomWidth,
            Longhand::BorderBottomStyle,
            Longhand::BorderBottomColor,
        ]),
        "border-inline" => Some(vec![
            Longhand::BorderLeftWidth,
            Longhand::BorderLeftStyle,
            Longhand::BorderLeftColor,
            Longhand::BorderRightWidth,
            Longhand::BorderRightStyle,
            Longhand::BorderRightColor,
        ]),
        "border-width" => Some(vec![
            Longhand::BorderTopWidth,
            Longhand::BorderRightWidth,
            Longhand::BorderBottomWidth,
            Longhand::BorderLeftWidth,
        ]),
        "border-block-width" => Some(vec![Longhand::BorderTopWidth, Longhand::BorderBottomWidth]),
        "border-inline-width" => Some(vec![Longhand::BorderLeftWidth, Longhand::BorderRightWidth]),
        "border-style" => Some(vec![
            Longhand::BorderTopStyle,
            Longhand::BorderRightStyle,
            Longhand::BorderBottomStyle,
            Longhand::BorderLeftStyle,
        ]),
        "border-block-style" => Some(vec![Longhand::BorderTopStyle, Longhand::BorderBottomStyle]),
        "border-inline-style" => Some(vec![Longhand::BorderLeftStyle, Longhand::BorderRightStyle]),
        "border-color" => Some(vec![
            Longhand::BorderTopColor,
            Longhand::BorderRightColor,
            Longhand::BorderBottomColor,
            Longhand::BorderLeftColor,
        ]),
        "border-block-color" => Some(vec![Longhand::BorderTopColor, Longhand::BorderBottomColor]),
        "border-inline-color" => Some(vec![Longhand::BorderLeftColor, Longhand::BorderRightColor]),
        "border-radius" => Some(vec![
            Longhand::BorderTopLeftRadius,
            Longhand::BorderTopRightRadius,
            Longhand::BorderBottomRightRadius,
            Longhand::BorderBottomLeftRadius,
        ]),
        "outline" => Some(vec![
            Longhand::OutlineWidth,
            Longhand::OutlineStyle,
            Longhand::OutlineColor,
        ]),
        "background" => Some(vec![
            Longhand::BackgroundImage,
            Longhand::BackgroundPositionX,
            Longhand::BackgroundPositionY,
            Longhand::BackgroundSize,
            Longhand::BackgroundRepeat,
            Longhand::BackgroundAttachment,
            Longhand::BackgroundOrigin,
            Longhand::BackgroundClip,
            Longhand::BackgroundColor,
        ]),
        "background-position" => Some(vec![
            Longhand::BackgroundPositionX,
            Longhand::BackgroundPositionY,
        ]),
        "font" => Some(vec![
            Longhand::FontStyle,
            Longhand::FontVariantCaps,
            Longhand::FontWeight,
            Longhand::FontSize,
            Longhand::LineHeight,
            Longhand::FontFamily,
        ]),
        "font-variant" => Some(vec![Longhand::FontVariantCaps]),
        "flex" => Some(vec![
            Longhand::FlexGrow,
            Longhand::FlexShrink,
            Longhand::FlexBasis,
        ]),
        "flex-flow" => Some(vec![Longhand::FlexDirection, Longhand::FlexWrap]),
        "gap" | "grid-gap" => Some(vec![Longhand::RowGap, Longhand::ColumnGap]),
        "place-content" => Some(vec![Longhand::AlignContent, Longhand::JustifyContent]),
        "place-items" => Some(vec![Longhand::AlignItems, Longhand::JustifyItems]),
        "place-self" => Some(vec![Longhand::AlignSelf, Longhand::JustifySelf]),
        "grid-template" => Some(vec![
            Longhand::GridTemplateRows,
            Longhand::GridTemplateColumns,
            Longhand::GridTemplateAreas,
        ]),
        "grid" => Some(vec![
            Longhand::GridTemplateRows,
            Longhand::GridTemplateColumns,
            Longhand::GridTemplateAreas,
            Longhand::GridAutoFlow,
            Longhand::GridAutoRows,
            Longhand::GridAutoColumns,
        ]),
        "grid-row" => Some(vec![Longhand::GridRowStart, Longhand::GridRowEnd]),
        "grid-column" => Some(vec![Longhand::GridColumnStart, Longhand::GridColumnEnd]),
        "grid-area" => Some(vec![
            Longhand::GridRowStart,
            Longhand::GridColumnStart,
            Longhand::GridRowEnd,
            Longhand::GridColumnEnd,
        ]),
        "overflow" => Some(vec![Longhand::OverflowX, Longhand::OverflowY]),
        "list-style" => Some(vec![
            Longhand::ListStylePosition,
            Longhand::ListStyleImage,
            Longhand::ListStyleType,
        ]),
        "text-decoration" => Some(vec![
            Longhand::TextDecorationLine,
            Longhand::TextDecorationStyle,
            Longhand::TextDecorationColor,
        ]),
        "transition" | "-webkit-transition" => Some(vec![
            Longhand::TransitionDuration,
            Longhand::TransitionTimingFunction,
            Longhand::TransitionDelay,
            Longhand::TransitionProperty,
        ]),
        "animation" | "-webkit-animation" => Some(vec![
            Longhand::AnimationDuration,
            Longhand::AnimationTimingFunction,
            Longhand::AnimationDelay,
            Longhand::AnimationIterationCount,
            Longhand::AnimationDirection,
            Longhand::AnimationFillMode,
            Longhand::AnimationPlayState,
            Longhand::AnimationName,
        ]),
        "white-space" => Some(vec![Longhand::WhiteSpaceCollapse, Longhand::TextWrapMode]),
        "text-wrap" => Some(vec![Longhand::TextWrapMode]),
        _ => None,
    }
}

/// Expands a parsed shorthand declaration into `(Longhand, String)` pairs.
pub(crate) fn expand_shorthand(
    lower_name: &str,
    cvs: &[ComponentValue],
) -> Result<Vec<(Longhand, String)>, String> {
    match lower_name {
        "margin" => expand_four_sides(
            cvs,
            [
                Longhand::MarginTop,
                Longhand::MarginRight,
                Longhand::MarginBottom,
                Longhand::MarginLeft,
            ],
        ),
        "margin-block" => expand_two_sides(cvs, [Longhand::MarginTop, Longhand::MarginBottom]),
        "margin-inline" => expand_two_sides(cvs, [Longhand::MarginLeft, Longhand::MarginRight]),
        "padding" => expand_four_sides(
            cvs,
            [
                Longhand::PaddingTop,
                Longhand::PaddingRight,
                Longhand::PaddingBottom,
                Longhand::PaddingLeft,
            ],
        ),
        "padding-block" => expand_two_sides(cvs, [Longhand::PaddingTop, Longhand::PaddingBottom]),
        "padding-inline" => expand_two_sides(cvs, [Longhand::PaddingLeft, Longhand::PaddingRight]),
        "inset" => expand_four_sides(
            cvs,
            [
                Longhand::Top,
                Longhand::Right,
                Longhand::Bottom,
                Longhand::Left,
            ],
        ),
        "inset-block" => expand_two_sides(cvs, [Longhand::Top, Longhand::Bottom]),
        "inset-inline" => expand_two_sides(cvs, [Longhand::Left, Longhand::Right]),
        "border-width" => expand_four_sides(
            cvs,
            [
                Longhand::BorderTopWidth,
                Longhand::BorderRightWidth,
                Longhand::BorderBottomWidth,
                Longhand::BorderLeftWidth,
            ],
        ),
        "border-block-width" => {
            expand_two_sides(cvs, [Longhand::BorderTopWidth, Longhand::BorderBottomWidth])
        }
        "border-inline-width" => {
            expand_two_sides(cvs, [Longhand::BorderLeftWidth, Longhand::BorderRightWidth])
        }
        "border-style" => expand_four_sides(
            cvs,
            [
                Longhand::BorderTopStyle,
                Longhand::BorderRightStyle,
                Longhand::BorderBottomStyle,
                Longhand::BorderLeftStyle,
            ],
        ),
        "border-block-style" => {
            expand_two_sides(cvs, [Longhand::BorderTopStyle, Longhand::BorderBottomStyle])
        }
        "border-inline-style" => {
            expand_two_sides(cvs, [Longhand::BorderLeftStyle, Longhand::BorderRightStyle])
        }
        "border-color" => expand_four_sides(
            cvs,
            [
                Longhand::BorderTopColor,
                Longhand::BorderRightColor,
                Longhand::BorderBottomColor,
                Longhand::BorderLeftColor,
            ],
        ),
        "border-block-color" => {
            expand_two_sides(cvs, [Longhand::BorderTopColor, Longhand::BorderBottomColor])
        }
        "border-inline-color" => {
            expand_two_sides(cvs, [Longhand::BorderLeftColor, Longhand::BorderRightColor])
        }
        "border" => {
            let (w, s, c) = parse_border_triple(cvs, false)?;
            Ok(vec![
                (Longhand::BorderTopWidth, w.clone()),
                (Longhand::BorderTopStyle, s.clone()),
                (Longhand::BorderTopColor, c.clone()),
                (Longhand::BorderRightWidth, w.clone()),
                (Longhand::BorderRightStyle, s.clone()),
                (Longhand::BorderRightColor, c.clone()),
                (Longhand::BorderBottomWidth, w.clone()),
                (Longhand::BorderBottomStyle, s.clone()),
                (Longhand::BorderBottomColor, c.clone()),
                (Longhand::BorderLeftWidth, w),
                (Longhand::BorderLeftStyle, s),
                (Longhand::BorderLeftColor, c),
            ])
        }
        "border-top" | "border-block-start" => {
            let (w, s, c) = parse_border_triple(cvs, false)?;
            Ok(vec![
                (Longhand::BorderTopWidth, w),
                (Longhand::BorderTopStyle, s),
                (Longhand::BorderTopColor, c),
            ])
        }
        "border-right" | "border-inline-end" => {
            let (w, s, c) = parse_border_triple(cvs, false)?;
            Ok(vec![
                (Longhand::BorderRightWidth, w),
                (Longhand::BorderRightStyle, s),
                (Longhand::BorderRightColor, c),
            ])
        }
        "border-bottom" | "border-block-end" => {
            let (w, s, c) = parse_border_triple(cvs, false)?;
            Ok(vec![
                (Longhand::BorderBottomWidth, w),
                (Longhand::BorderBottomStyle, s),
                (Longhand::BorderBottomColor, c),
            ])
        }
        "border-left" | "border-inline-start" => {
            let (w, s, c) = parse_border_triple(cvs, false)?;
            Ok(vec![
                (Longhand::BorderLeftWidth, w),
                (Longhand::BorderLeftStyle, s),
                (Longhand::BorderLeftColor, c),
            ])
        }
        "border-block" => {
            let (w, s, c) = parse_border_triple(cvs, false)?;
            Ok(vec![
                (Longhand::BorderTopWidth, w.clone()),
                (Longhand::BorderTopStyle, s.clone()),
                (Longhand::BorderTopColor, c.clone()),
                (Longhand::BorderBottomWidth, w),
                (Longhand::BorderBottomStyle, s),
                (Longhand::BorderBottomColor, c),
            ])
        }
        "border-inline" => {
            let (w, s, c) = parse_border_triple(cvs, false)?;
            Ok(vec![
                (Longhand::BorderLeftWidth, w.clone()),
                (Longhand::BorderLeftStyle, s.clone()),
                (Longhand::BorderLeftColor, c.clone()),
                (Longhand::BorderRightWidth, w),
                (Longhand::BorderRightStyle, s),
                (Longhand::BorderRightColor, c),
            ])
        }
        "outline" => {
            let (w, s, c) = parse_border_triple(cvs, true)?;
            Ok(vec![
                (Longhand::OutlineWidth, w),
                (Longhand::OutlineStyle, s),
                (Longhand::OutlineColor, c),
            ])
        }
        "border-radius" => expand_border_radius(cvs),
        "background" => expand_background(cvs),
        "background-position" => expand_background_position(cvs),
        "font" => expand_font(cvs),
        "font-variant" => expand_font_variant(cvs),
        "flex" => expand_flex(cvs),
        "flex-flow" => expand_flex_flow(cvs),
        "gap" | "grid-gap" => expand_two_sides(cvs, [Longhand::RowGap, Longhand::ColumnGap]),
        "place-content" => expand_place_content(cvs),
        "place-items" => expand_place_items(cvs),
        "place-self" => expand_place_self(cvs),
        "grid-template" => expand_grid_template(cvs),
        "grid" => expand_grid(cvs),
        "grid-row" => expand_grid_span_pair(cvs, Longhand::GridRowStart, Longhand::GridRowEnd),
        "grid-column" => {
            expand_grid_span_pair(cvs, Longhand::GridColumnStart, Longhand::GridColumnEnd)
        }
        "grid-area" => expand_grid_area(cvs),
        "overflow" => expand_two_sides(cvs, [Longhand::OverflowX, Longhand::OverflowY]),
        "list-style" => expand_list_style(cvs),
        "text-decoration" => expand_text_decoration(cvs),
        "transition" | "-webkit-transition" => expand_transition(cvs),
        "animation" | "-webkit-animation" => expand_animation(cvs),
        "white-space" => expand_white_space(cvs),
        "text-wrap" => expand_text_wrap(cvs),
        _ => Err(format!("unsupported shorthand '{lower_name}'")),
    }
}

// ---------------------------------------------------------------------------
// 1–4 and 1–2 side shorthands
// ---------------------------------------------------------------------------

fn expand_four_sides(
    cvs: &[ComponentValue],
    targets: [Longhand; 4],
) -> Result<Vec<(Longhand, String)>, String> {
    let items = non_ws_items(cvs);
    let [t, r, b, l] = match items.as_slice() {
        [a] => {
            let s = serialize_cv(a);
            [s.clone(), s.clone(), s.clone(), s]
        }
        [a, b] => {
            let sa = serialize_cv(a);
            let sb = serialize_cv(b);
            [sa.clone(), sb.clone(), sa, sb]
        }
        [a, b, c] => {
            let sa = serialize_cv(a);
            let sb = serialize_cv(b);
            let sc = serialize_cv(c);
            [sa, sb.clone(), sc, sb]
        }
        [a, b, c, d] => [
            serialize_cv(a),
            serialize_cv(b),
            serialize_cv(c),
            serialize_cv(d),
        ],
        _ => return Err("shorthand expects 1 to 4 values".to_owned()),
    };
    Ok(vec![
        (targets[0], t),
        (targets[1], r),
        (targets[2], b),
        (targets[3], l),
    ])
}

fn expand_two_sides(
    cvs: &[ComponentValue],
    targets: [Longhand; 2],
) -> Result<Vec<(Longhand, String)>, String> {
    let items = non_ws_items(cvs);
    let (a, b) = match items.as_slice() {
        [x] => {
            let s = serialize_cv(x);
            (s.clone(), s)
        }
        [x, y] => (serialize_cv(x), serialize_cv(y)),
        _ => return Err("shorthand expects 1 or 2 values".to_owned()),
    };
    Ok(vec![(targets[0], a), (targets[1], b)])
}

// ---------------------------------------------------------------------------
// `border`, `border-*`, `outline`, `border-radius`
// ---------------------------------------------------------------------------

fn parse_border_triple(
    cvs: &[ComponentValue],
    allow_auto_style: bool,
) -> Result<(String, String, String), String> {
    let uctx = dummy_uctx(false);
    let mut cur = Cursor::new(cvs);

    let mut width: Option<String> = None;
    let mut style: Option<String> = None;
    let mut color: Option<String> = None;
    let mut count = 0_usize;

    while !cur.is_eof() {
        let start = cur.save();
        if style.is_none() && eat_border_style(&mut cur, allow_auto_style).is_some() {
            let end = cur.save();
            style = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        cur.restore(start);

        if width.is_none()
            && let Some(_) = eat_line_width(&mut cur, &uctx)?
        {
            let end = cur.save();
            width = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        cur.restore(start);

        if color.is_none()
            && let Some(_) = eat_color(&mut cur, Rgba::BLACK)?
        {
            let end = cur.save();
            color = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }

        return Err("invalid or duplicate component in border/outline shorthand".to_owned());
    }

    if count == 0 {
        return Err("empty border/outline shorthand".to_owned());
    }

    Ok((
        width.unwrap_or_else(|| "medium".to_owned()),
        style.unwrap_or_else(|| "none".to_owned()),
        color.unwrap_or_else(|| "currentcolor".to_owned()),
    ))
}

fn expand_border_radius(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let parts = split_slash_separated(cvs)?;
    if parts.len() > 2 {
        return Err("border-radius accepts at most one '/'".to_owned());
    }

    let uctx = dummy_uctx(true);
    let h_items = non_ws_items(parts[0]);
    if !(1..=4).contains(&h_items.len()) {
        return Err("border-radius expects 1 to 4 horizontal radii".to_owned());
    }
    for item in &h_items {
        let _ = parse_length_cvs(std::slice::from_ref(*item), &uctx, true)?;
    }

    if let Some(vert_part) = parts.get(1) {
        let v_items = non_ws_items(vert_part);
        if !(1..=4).contains(&v_items.len()) {
            return Err("border-radius expects 1 to 4 vertical radii after '/'".to_owned());
        }
        for item in &v_items {
            let _ = parse_length_cvs(std::slice::from_ref(*item), &uctx, true)?;
        }
    }

    expand_four_sides(
        parts[0],
        [
            Longhand::BorderTopLeftRadius,
            Longhand::BorderTopRightRadius,
            Longhand::BorderBottomRightRadius,
            Longhand::BorderBottomLeftRadius,
        ],
    )
}

// ---------------------------------------------------------------------------
// `background` and `background-position`
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum PosKind {
    Horiz,
    Vert,
    Center,
    Lp,
}

fn classify_pos_token(cv: &ComponentValue, uctx: &UnitContext) -> Result<Option<PosKind>, String> {
    if let ComponentValue::PreservedToken(ts) = cv
        && let Token::Ident { value, .. } = &ts.token
    {
        return Ok(match value.to_ascii_lowercase().as_str() {
            "left" | "right" => Some(PosKind::Horiz),
            "top" | "bottom" => Some(PosKind::Vert),
            "center" => Some(PosKind::Center),
            _ => None,
        });
    }
    let mut cur = Cursor::new(std::slice::from_ref(cv));
    if eat_length(&mut cur, uctx, false)?.is_some() && cur.is_eof() {
        return Ok(Some(PosKind::Lp));
    }
    Ok(None)
}

/// Splits 1–4 `<bg-position>` tokens into `(position_x_css, position_y_css)` strings
/// while keeping relative units (`em`, `rem`, `calc()`) un-evaluated.
fn split_bg_position_tokens(
    toks: &[(&ComponentValue, PosKind)],
) -> Result<(String, String), String> {
    let join2 = |a: &ComponentValue, b: &ComponentValue| {
        format!("{} {}", serialize_cv(a), serialize_cv(b))
    };

    match toks {
        [(c0, k0)] => match k0 {
            PosKind::Vert => Ok(("center".to_owned(), serialize_cv(c0))),
            PosKind::Horiz | PosKind::Center | PosKind::Lp => {
                Ok((serialize_cv(c0), "center".to_owned()))
            }
        },
        [(c0, k0), (c1, k1)] => {
            if *k0 != PosKind::Lp && *k1 != PosKind::Lp {
                let first_h = matches!(k0, PosKind::Horiz | PosKind::Center);
                let second_v = matches!(k1, PosKind::Vert | PosKind::Center);
                if first_h && second_v {
                    return Ok((serialize_cv(c0), serialize_cv(c1)));
                }
                let first_v = matches!(k0, PosKind::Vert | PosKind::Center);
                let second_h = matches!(k1, PosKind::Horiz | PosKind::Center);
                if first_v && second_h {
                    return Ok((serialize_cv(c1), serialize_cv(c0)));
                }
                return Err("conflicting keywords in 2-value background-position".to_owned());
            }
            let ok_h = matches!(k0, PosKind::Horiz | PosKind::Center | PosKind::Lp);
            let ok_v = matches!(k1, PosKind::Vert | PosKind::Center | PosKind::Lp);
            if ok_h && ok_v {
                Ok((serialize_cv(c0), serialize_cv(c1)))
            } else {
                Err("invalid 2-value background-position".to_owned())
            }
        }
        [(c0, k0), (c1, k1), (c2, k2)] => match (*k0, *k1, *k2) {
            (PosKind::Horiz, PosKind::Lp, PosKind::Vert | PosKind::Center) => {
                Ok((join2(c0, c1), serialize_cv(c2)))
            }
            (PosKind::Vert, PosKind::Lp, PosKind::Horiz | PosKind::Center) => {
                Ok((serialize_cv(c2), join2(c0, c1)))
            }
            (PosKind::Horiz | PosKind::Center, PosKind::Vert, PosKind::Lp) => {
                Ok((serialize_cv(c0), join2(c1, c2)))
            }
            (PosKind::Vert | PosKind::Center, PosKind::Horiz, PosKind::Lp) => {
                Ok((join2(c1, c2), serialize_cv(c0)))
            }
            _ => Err("invalid 3-value background-position".to_owned()),
        },
        [(c0, k0), (c1, k1), (c2, k2), (c3, k3)] => match (*k0, *k1, *k2, *k3) {
            (PosKind::Horiz, PosKind::Lp, PosKind::Vert, PosKind::Lp) => {
                Ok((join2(c0, c1), join2(c2, c3)))
            }
            (PosKind::Vert, PosKind::Lp, PosKind::Horiz, PosKind::Lp) => {
                Ok((join2(c2, c3), join2(c0, c1)))
            }
            _ => Err("invalid 4-value background-position".to_owned()),
        },
        _ => Err("invalid background-position".to_owned()),
    }
}

fn eat_bg_position_strings(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
) -> Result<Option<(String, String)>, String> {
    let mut checkpoints = Vec::with_capacity(4);
    let mut toks = Vec::with_capacity(4);
    for _ in 0..4 {
        let before = cur.save();
        let Some(cv) = cur.next_cv() else {
            break;
        };
        match classify_pos_token(cv, uctx)? {
            Some(kind) => {
                toks.push((cv, kind));
                checkpoints.push(cur.save());
            }
            None => {
                cur.restore(before);
                break;
            }
        }
    }
    if toks.is_empty() {
        return Ok(None);
    }
    for len in (1..=toks.len()).rev() {
        if let Ok(pair) = split_bg_position_tokens(&toks[..len]) {
            cur.restore(checkpoints[len - 1]);
            return Ok(Some(pair));
        }
    }
    Err("invalid <bg-position> in background shorthand".to_owned())
}

fn eat_bg_size_string(
    cur: &mut Cursor<'_>,
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<String, String> {
    let start = cur.save();
    if cur.eat_ident_eq("cover") || cur.eat_ident_eq("contain") {
        let end = cur.save();
        return Ok(serialize_cvs(&cvs[start..end]));
    }
    let Some(_first) = cur.next_cv() else {
        return Err("expected <bg-size> after '/'".to_owned());
    };
    let after_first = cur.save();
    parse_bg_size_item(&cvs[start..after_first], uctx)?;

    // Check if a second `<length-percentage> | auto` immediately follows.
    if let Some(_second) = cur.next_cv() {
        let after_second = cur.save();
        if parse_bg_size_item(&cvs[start..after_second], uctx).is_ok() {
            return Ok(serialize_cvs(&cvs[start..after_second]));
        }
        cur.restore(after_first);
    }
    Ok(serialize_cvs(&cvs[start..after_first]))
}

fn expand_background_position(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let uctx = dummy_uctx(true);
    let parts = split_comma_separated(cvs)?;
    let mut xs = Vec::with_capacity(parts.len());
    let mut ys = Vec::with_capacity(parts.len());

    for part in parts {
        let mut cur = Cursor::new(part);
        let (x, y) = eat_bg_position_strings(&mut cur, &uctx)?
            .ok_or_else(|| "expected <position> in background-position".to_owned())?;
        cur.expect_eof()?;
        xs.push(x);
        ys.push(y);
    }

    Ok(vec![
        (Longhand::BackgroundPositionX, xs.join(", ")),
        (Longhand::BackgroundPositionY, ys.join(", ")),
    ])
}

fn expand_background(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let uctx = dummy_uctx(true);
    let layers = split_comma_separated(cvs)?;
    let num_layers = layers.len();

    let mut images = Vec::with_capacity(num_layers);
    let mut pos_xs = Vec::with_capacity(num_layers);
    let mut pos_ys = Vec::with_capacity(num_layers);
    let mut sizes = Vec::with_capacity(num_layers);
    let mut repeats = Vec::with_capacity(num_layers);
    let mut attachments = Vec::with_capacity(num_layers);
    let mut origins = Vec::with_capacity(num_layers);
    let mut clips = Vec::with_capacity(num_layers);
    let mut final_color: Option<String> = None;

    for (idx, layer) in layers.iter().enumerate() {
        let is_final = idx + 1 == num_layers;
        let mut cur = Cursor::new(layer);

        let mut img: Option<String> = None;
        let mut pos: Option<(String, String)> = None;
        let mut size: Option<String> = None;
        let mut rep: Option<String> = None;
        let mut att: Option<String> = None;
        let mut boxes: Vec<String> = Vec::new();
        let mut layer_color: Option<String> = None;
        let mut matched_any = false;

        while !cur.is_eof() {
            let start = cur.save();

            // 1. `<bg-image>` (`none`, `url(...)`, or gradient)
            if img.is_none()
                && let Some(_) = eat_bg_image_item(&mut cur, &uctx, Rgba::BLACK)?
            {
                let end = cur.save();
                img = Some(serialize_cvs(&layer[start..end]));
                matched_any = true;
                continue;
            }
            cur.restore(start);

            // 2. `<repeat-style>`
            if rep.is_none() && eat_bg_repeat_in_shorthand(&mut cur).is_some() {
                let end = cur.save();
                rep = Some(serialize_cvs(&layer[start..end]));
                matched_any = true;
                continue;
            }
            cur.restore(start);

            // 3. `<attachment>`
            if att.is_none()
                && let Some(id) = cur.eat_ident()
                && matches!(id.to_ascii_lowercase().as_str(), "scroll" | "fixed" | "local")
            {
                att = Some(id.to_ascii_lowercase());
                matched_any = true;
                continue;
            }
            cur.restore(start);

            // 4. `<visual-box>` (up to 2; 2nd or lone `text` allowed for `background-clip: text`)
            if boxes.len() < 2
                && let Some(id) = cur.eat_ident()
            {
                let lower = id.to_ascii_lowercase();
                if matches!(lower.as_str(), "border-box" | "padding-box" | "content-box")
                    || (lower == "text" && boxes.len() <= 1)
                {
                    boxes.push(lower);
                    matched_any = true;
                    continue;
                }
            }
            cur.restore(start);

            // 5. `<bg-position> [ / <bg-size> ]?`
            if pos.is_none()
                && let Some((px, py)) = eat_bg_position_strings(&mut cur, &uctx)?
            {
                pos = Some((px, py));
                if cur.eat_slash() {
                    size = Some(eat_bg_size_string(&mut cur, layer, &uctx)?);
                }
                matched_any = true;
                continue;
            }
            cur.restore(start);

            // 6. `<background-color>` (valid ONLY in the final layer)
            if layer_color.is_none()
                && let Some(_) = eat_color(&mut cur, Rgba::BLACK)?
            {
                if !is_final {
                    return Err(
                        "background-color is only permitted in the final background layer"
                            .to_owned(),
                    );
                }
                let end = cur.save();
                layer_color = Some(serialize_cvs(&layer[start..end]));
                matched_any = true;
                continue;
            }

            return Err("invalid or duplicate token in background shorthand".to_owned());
        }

        if !matched_any {
            return Err("empty layer in background shorthand".to_owned());
        }

        let (origin_box, clip_box) = match boxes.as_slice() {
            [] => ("padding-box".to_owned(), "border-box".to_owned()),
            [single] if single == "text" => ("padding-box".to_owned(), "text".to_owned()),
            [single] => (single.clone(), single.clone()),
            [first, second] if first != "text" => (first.clone(), second.clone()),
            _ => return Err("background-origin cannot be 'text'".to_owned()),
        };

        let (px, py) = pos.unwrap_or_else(|| ("0%".to_owned(), "0%".to_owned()));
        images.push(img.unwrap_or_else(|| "none".to_owned()));
        pos_xs.push(px);
        pos_ys.push(py);
        sizes.push(size.unwrap_or_else(|| "auto".to_owned()));
        repeats.push(rep.unwrap_or_else(|| "repeat".to_owned()));
        attachments.push(att.unwrap_or_else(|| "scroll".to_owned()));
        origins.push(origin_box);
        clips.push(clip_box);
        if is_final {
            final_color = layer_color;
        }
    }

    Ok(vec![
        (Longhand::BackgroundImage, images.join(", ")),
        (Longhand::BackgroundPositionX, pos_xs.join(", ")),
        (Longhand::BackgroundPositionY, pos_ys.join(", ")),
        (Longhand::BackgroundSize, sizes.join(", ")),
        (Longhand::BackgroundRepeat, repeats.join(", ")),
        (Longhand::BackgroundAttachment, attachments.join(", ")),
        (Longhand::BackgroundOrigin, origins.join(", ")),
        (Longhand::BackgroundClip, clips.join(", ")),
        (
            Longhand::BackgroundColor,
            final_color.unwrap_or_else(|| "transparent".to_owned()),
        ),
    ])
}

// ---------------------------------------------------------------------------
// `font` and `font-variant`
// ---------------------------------------------------------------------------

fn expand_font_variant(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("normal") || cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(vec![(Longhand::FontVariantCaps, "normal".to_owned())]);
    }
    let _ = parse_font_variant_caps(cvs)?;
    Ok(vec![(Longhand::FontVariantCaps, serialize_cvs(cvs))])
}

fn expand_font(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let vp = dummy_viewport();
    let ctx = ComputeContext {
        parent: None,
        root_font_size: 16.0,
        viewport: &vp,
    };
    let dummy_style = ComputedStyle::default();

    // System font keywords (CSS Fonts 4 §2.7).
    let mut sys_cur = Cursor::new(cvs);
    if let Some(id) = sys_cur.eat_ident()
        && sys_cur.is_eof()
    {
        let sys_size = match id.to_ascii_lowercase().as_str() {
            "caption" | "icon" | "menu" | "message-box" => Some("14px"),
            "small-caption" | "status-bar" => Some("12px"),
            _ => None,
        };
        if let Some(sz) = sys_size {
            return Ok(vec![
                (Longhand::FontStyle, "normal".to_owned()),
                (Longhand::FontVariantCaps, "normal".to_owned()),
                (Longhand::FontWeight, "normal".to_owned()),
                (Longhand::FontSize, sz.to_owned()),
                (Longhand::LineHeight, "normal".to_owned()),
                (Longhand::FontFamily, "system-ui, sans-serif".to_owned()),
            ]);
        }
    }

    let mut cur = Cursor::new(cvs);
    let mut f_style: Option<String> = None;
    let mut f_variant: Option<String> = None;
    let mut f_weight: Option<String> = None;
    let mut f_stretch_seen = false;
    let mut normals_seen = 0_usize;

    let mut f_size: Option<String> = None;

    while !cur.is_eof() {
        let non_normal_count = usize::from(f_style.is_some())
            + usize::from(f_variant.is_some())
            + usize::from(f_weight.is_some())
            + usize::from(f_stretch_seen);

        let start = cur.save();

        // Check `"normal"` in prefix (at most 4 prefix slots total).
        if non_normal_count + normals_seen < 4 && cur.eat_ident_eq("normal") {
            normals_seen += 1;
            continue;
        }
        cur.restore(start);

        // `font-style` (`italic` or `oblique [<angle>]`)
        if f_style.is_none()
            && non_normal_count + normals_seen < 4
            && let Some(_) = eat_font_style(&mut cur)?
        {
            let end = cur.save();
            f_style = Some(serialize_cvs(&cvs[start..end]));
            continue;
        }
        cur.restore(start);

        // `font-variant-css2` (`small-caps` only in CSS Fonts 4 `font` shorthand)
        if f_variant.is_none()
            && non_normal_count + normals_seen < 4
            && cur.eat_ident_eq("small-caps")
        {
            f_variant = Some("small-caps".to_owned());
            continue;
        }
        cur.restore(start);

        // `font-stretch-css3` keyword
        if !f_stretch_seen
            && non_normal_count + normals_seen < 4
            && let Some(id) = cur.eat_ident()
            && is_font_stretch_kw(id)
        {
            f_stretch_seen = true;
            continue;
        }
        cur.restore(start);

        // `font-weight` (`bold`, `bolder`, `lighter`, or `<number [1,1000]>`).
        // Note: unitless `0` is NOT a valid `font-weight` (must be >= 1), so `0` falls through to `font-size`.
        if f_weight.is_none()
            && non_normal_count + normals_seen < 4
            && let Some(cv) = cur.next_cv()
            && parse_font_weight(std::slice::from_ref(cv), &ctx).is_ok()
        {
            f_weight = Some(serialize_cv(cv));
            continue;
        }
        cur.restore(start);

        // Mandatory `<font-size>`
        let cv = cur
            .next_cv()
            .ok_or_else(|| "expected <font-size> in font shorthand".to_owned())?;
        if parse_font_size(std::slice::from_ref(cv), &ctx).is_ok() {
            f_size = Some(serialize_cv(cv));
            break;
        }
        return Err("invalid token before <font-size> in font shorthand".to_owned());
    }

    let font_size = f_size.ok_or_else(|| "missing <font-size> in font shorthand".to_owned())?;

    let line_height = if cur.eat_slash() {
        let lh_cv = cur
            .next_cv()
            .ok_or_else(|| "expected <line-height> after '/' in font shorthand".to_owned())?;
        let _ = parse_line_height(std::slice::from_ref(lh_cv), &dummy_style, &ctx)?;
        serialize_cv(lh_cv)
    } else {
        "normal".to_owned()
    };

    let family_slice = cur.remaining();
    if family_slice.is_empty() {
        return Err("font shorthand requires <font-family>".to_owned());
    }
    let _ = parse_font_family(family_slice)?;

    Ok(vec![
        (Longhand::FontStyle, f_style.unwrap_or_else(|| "normal".to_owned())),
        (
            Longhand::FontVariantCaps,
            f_variant.unwrap_or_else(|| "normal".to_owned()),
        ),
        (
            Longhand::FontWeight,
            f_weight.unwrap_or_else(|| "normal".to_owned()),
        ),
        (Longhand::FontSize, font_size),
        (Longhand::LineHeight, line_height),
        (Longhand::FontFamily, serialize_cvs(family_slice)),
    ])
}

// ---------------------------------------------------------------------------
// Flexbox & Box Alignment shorthands (`flex`, `flex-flow`, `place-*`)
// ---------------------------------------------------------------------------

fn expand_flex(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    if let Some(id) = cur.eat_ident()
        && cur.is_eof()
    {
        match id.to_ascii_lowercase().as_str() {
            "none" => {
                return Ok(vec![
                    (Longhand::FlexGrow, "0".to_owned()),
                    (Longhand::FlexShrink, "0".to_owned()),
                    (Longhand::FlexBasis, "auto".to_owned()),
                ]);
            }
            "auto" => {
                return Ok(vec![
                    (Longhand::FlexGrow, "1".to_owned()),
                    (Longhand::FlexShrink, "1".to_owned()),
                    (Longhand::FlexBasis, "auto".to_owned()),
                ]);
            }
            "initial" => {
                return Ok(vec![
                    (Longhand::FlexGrow, "0".to_owned()),
                    (Longhand::FlexShrink, "1".to_owned()),
                    (Longhand::FlexBasis, "auto".to_owned()),
                ]);
            }
            _ => {}
        }
    }

    let uctx = dummy_uctx(true);
    let items = non_ws_items(cvs);
    if items.is_empty() || items.len() > 3 {
        return Err("flex shorthand expects 1 to 3 components".to_owned());
    }

    let is_flex_number = |cv: &ComponentValue| parse_number_cvs(std::slice::from_ref(cv), true).is_ok();
    let is_flex_basis =
        |cv: &ComponentValue| parse_size(std::slice::from_ref(cv), &uctx, false, true).is_ok();

    let (grow, shrink, basis) = match items.as_slice() {
        [a] => {
            // Unitless number (including unitless `0`) is `<flex-grow>` with basis `0px`.
            if is_flex_number(a) {
                (serialize_cv(a), "1".to_owned(), "0px".to_owned())
            } else if is_flex_basis(a) {
                ("1".to_owned(), "1".to_owned(), serialize_cv(a))
            } else {
                return Err("invalid 1-value flex shorthand".to_owned());
            }
        }
        [a, b] => {
            if is_flex_number(a) && is_flex_number(b) {
                (serialize_cv(a), serialize_cv(b), "0px".to_owned())
            } else if is_flex_number(a) && is_flex_basis(b) {
                (serialize_cv(a), "1".to_owned(), serialize_cv(b))
            } else if !is_flex_number(a) && is_flex_basis(a) && is_flex_number(b) {
                (serialize_cv(b), "1".to_owned(), serialize_cv(a))
            } else {
                return Err("invalid 2-value flex shorthand".to_owned());
            }
        }
        [a, b, c] => {
            if is_flex_number(a) && is_flex_number(b) && is_flex_basis(c) {
                (serialize_cv(a), serialize_cv(b), serialize_cv(c))
            } else if !is_flex_number(a)
                && is_flex_basis(a)
                && is_flex_number(b)
                && is_flex_number(c)
            {
                (serialize_cv(b), serialize_cv(c), serialize_cv(a))
            } else {
                return Err("invalid 3-value flex shorthand".to_owned());
            }
        }
        _ => unreachable!(),
    };

    Ok(vec![
        (Longhand::FlexGrow, grow),
        (Longhand::FlexShrink, shrink),
        (Longhand::FlexBasis, basis),
    ])
}

fn expand_flex_flow(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    let mut dir: Option<String> = None;
    let mut wrap: Option<String> = None;
    let mut count = 0_usize;

    while !cur.is_eof() {
        let start = cur.save();
        if dir.is_none() && eat_flex_direction(&mut cur).is_some() {
            let end = cur.save();
            dir = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        cur.restore(start);

        if wrap.is_none() && eat_flex_wrap(&mut cur).is_some() {
            let end = cur.save();
            wrap = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        return Err("invalid or duplicate token in flex-flow".to_owned());
    }

    if count == 0 {
        return Err("empty flex-flow".to_owned());
    }

    Ok(vec![
        (Longhand::FlexDirection, dir.unwrap_or_else(|| "row".to_owned())),
        (Longhand::FlexWrap, wrap.unwrap_or_else(|| "nowrap".to_owned())),
    ])
}

fn expand_place_content(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    let start1 = cur.save();
    let first_align = eat_align(&mut cur, AlignTarget::AlignContent)
        .ok_or_else(|| "invalid align-content in place-content".to_owned())?;
    let end1 = cur.save();
    let first_str = serialize_cvs(&cvs[start1..end1]);

    let second_str = if cur.is_eof() {
        // CSS Box Alignment 3 §5.3: if 1st is a baseline keyword, omitted 2nd defaults to `start`.
        if first_align == crate::Align::Baseline {
            "start".to_owned()
        } else {
            let mut check = Cursor::new(&cvs[start1..end1]);
            if eat_align(&mut check, AlignTarget::JustifyContent).is_some() && check.is_eof() {
                first_str.clone()
            } else {
                return Err("single place-content value is not valid for justify-content".to_owned());
            }
        }
    } else {
        let start2 = cur.save();
        let _ = eat_align(&mut cur, AlignTarget::JustifyContent)
            .ok_or_else(|| "invalid justify-content in place-content".to_owned())?;
        let end2 = cur.save();
        cur.expect_eof()?;
        serialize_cvs(&cvs[start2..end2])
    };

    Ok(vec![
        (Longhand::AlignContent, first_str),
        (Longhand::JustifyContent, second_str),
    ])
}

fn expand_place_items(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    let start1 = cur.save();
    let _ = eat_align(&mut cur, AlignTarget::AlignItems)
        .ok_or_else(|| "invalid align-items in place-items".to_owned())?;
    let end1 = cur.save();
    let first_str = serialize_cvs(&cvs[start1..end1]);

    let second_str = if cur.is_eof() {
        first_str.clone()
    } else {
        let start2 = cur.save();
        let _ = eat_align(&mut cur, AlignTarget::JustifyItems)
            .ok_or_else(|| "invalid justify-items in place-items".to_owned())?;
        let end2 = cur.save();
        cur.expect_eof()?;
        serialize_cvs(&cvs[start2..end2])
    };

    Ok(vec![
        (Longhand::AlignItems, first_str),
        (Longhand::JustifyItems, second_str),
    ])
}

fn expand_place_self(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    let start1 = cur.save();
    let _ = eat_align(&mut cur, AlignTarget::AlignSelf)
        .ok_or_else(|| "invalid align-self in place-self".to_owned())?;
    let end1 = cur.save();
    let first_str = serialize_cvs(&cvs[start1..end1]);

    let second_str = if cur.is_eof() {
        first_str.clone()
    } else {
        let start2 = cur.save();
        let _ = eat_align(&mut cur, AlignTarget::JustifySelf)
            .ok_or_else(|| "invalid justify-self in place-self".to_owned())?;
        let end2 = cur.save();
        cur.expect_eof()?;
        serialize_cvs(&cvs[start2..end2])
    };

    Ok(vec![
        (Longhand::AlignSelf, first_str),
        (Longhand::JustifySelf, second_str),
    ])
}

// ---------------------------------------------------------------------------
// Grid shorthands (`grid-template`, `grid`, `grid-row`, `grid-column`, `grid-area`)
// ---------------------------------------------------------------------------

fn expand_grid_template(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(vec![
            (Longhand::GridTemplateRows, "none".to_owned()),
            (Longhand::GridTemplateColumns, "none".to_owned()),
            (Longhand::GridTemplateAreas, "none".to_owned()),
        ]);
    }

    let uctx = dummy_uctx(true);
    let parts = split_slash_separated(cvs)?;
    if parts.len() > 2 {
        return Err("grid-template accepts at most one '/'".to_owned());
    }

    // Check if `parts[0]` contains any `<string>` token -> areas syntax vs `<rows> / <cols>`.
    let has_area_strings = parts[0].iter().any(|cv| {
        matches!(
            cv,
            ComponentValue::PreservedToken(ts) if matches!(ts.token, Token::String { .. })
        )
    });

    if !has_area_strings {
        if parts.len() != 2 {
            return Err("grid-template without area strings requires '<rows> / <columns>'".to_owned());
        }
        let _ = parse_grid_template_tracks(parts[0], &uctx)?;
        let _ = parse_grid_template_tracks(parts[1], &uctx)?;
        return Ok(vec![
            (Longhand::GridTemplateRows, serialize_cvs(parts[0])),
            (Longhand::GridTemplateColumns, serialize_cvs(parts[1])),
            (Longhand::GridTemplateAreas, "none".to_owned()),
        ]);
    }

    // Areas form: `[ <line-names>? <string> <track-size>? <line-names>? ]+ [ / <explicit-track-list> ]?`
    let mut rcur = Cursor::new(parts[0]);
    let mut area_tokens = Vec::new();
    let mut row_tracks = Vec::new();

    while !rcur.is_eof() {
        while rcur.eat_bracket_block().is_some() {}
        let start_str = rcur.save();
        let Some(_area_str) = rcur.eat_string() else {
            return Err("expected area <string> in grid-template".to_owned());
        };
        let end_str = rcur.save();
        area_tokens.push(serialize_cvs(&parts[0][start_str..end_str]));

        let start_track = rcur.save();
        if eat_track_size(&mut rcur, &uctx)?.is_some() {
            let end_track = rcur.save();
            row_tracks.push(serialize_cvs(&parts[0][start_track..end_track]));
        } else {
            row_tracks.push("auto".to_owned());
        }
        while rcur.eat_bracket_block().is_some() {}
    }

    if area_tokens.is_empty() {
        return Err("empty areas in grid-template".to_owned());
    }

    let areas_joined = area_tokens.join(" ");
    let rows_joined = row_tracks.join(" ");

    let cols_str = if let Some(col_part) = parts.get(1) {
        let mut ccur = Cursor::new(col_part);
        if ccur.eat_ident_eq("none") && ccur.is_eof() {
            return Err("'none' is not allowed after '/' in grid-template areas syntax".to_owned());
        }
        let _ = parse_grid_template_tracks(col_part, &uctx)?;
        serialize_cvs(col_part)
    } else {
        "none".to_owned()
    };

    // Validate area rectangles immediately.
    let area_cvs = super::cursor::parse_component_values(&areas_joined)?;
    let _ = parse_grid_template_areas(&area_cvs)?;

    Ok(vec![
        (Longhand::GridTemplateRows, rows_joined),
        (Longhand::GridTemplateColumns, cols_str),
        (Longhand::GridTemplateAreas, areas_joined),
    ])
}

fn expand_grid(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut out = expand_grid_template(cvs)
        .map_err(|_| "unsupported or invalid 'grid' shorthand (only template forms are supported)".to_owned())?;
    out.push((Longhand::GridAutoFlow, "row".to_owned()));
    out.push((Longhand::GridAutoRows, "auto".to_owned()));
    out.push((Longhand::GridAutoColumns, "auto".to_owned()));
    Ok(out)
}

fn bare_custom_grid_ident(cvs: &[ComponentValue]) -> Option<String> {
    let mut cur = Cursor::new(cvs);
    let id = cur.eat_custom_ident()?;
    if !cur.is_eof() || id.eq_ignore_ascii_case("auto") || id.eq_ignore_ascii_case("span") {
        return None;
    }
    Some(serialize_cvs(cvs))
}

fn expand_grid_span_pair(
    cvs: &[ComponentValue],
    start_lh: Longhand,
    end_lh: Longhand,
) -> Result<Vec<(Longhand, String)>, String> {
    let parts = split_slash_separated(cvs)?;
    if parts.len() > 2 {
        return Err("grid-row/grid-column accepts at most one '/'".to_owned());
    }
    let _ = parse_grid_line(parts[0])?;
    let start_str = serialize_cvs(parts[0]);
    let end_str = if let Some(end_part) = parts.get(1) {
        let _ = parse_grid_line(end_part)?;
        serialize_cvs(end_part)
    } else {
        bare_custom_grid_ident(parts[0]).unwrap_or_else(|| "auto".to_owned())
    };

    Ok(vec![(start_lh, start_str), (end_lh, end_str)])
}

fn expand_grid_area(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let parts = split_slash_separated(cvs)?;
    if parts.len() > 4 {
        return Err("grid-area accepts at most 4 '/' segments".to_owned());
    }
    for part in &parts {
        let _ = parse_grid_line(part)?;
    }

    let r_start = serialize_cvs(parts[0]);
    let r_start_ident = bare_custom_grid_ident(parts[0]);

    let (c_start, c_start_ident) = if let Some(p1) = parts.get(1) {
        (serialize_cvs(p1), bare_custom_grid_ident(p1))
    } else {
        let fallback = r_start_ident.clone().unwrap_or_else(|| "auto".to_owned());
        (fallback, r_start_ident.clone())
    };

    let r_end = if let Some(p2) = parts.get(2) {
        serialize_cvs(p2)
    } else {
        r_start_ident.unwrap_or_else(|| "auto".to_owned())
    };

    let c_end = if let Some(p3) = parts.get(3) {
        serialize_cvs(p3)
    } else {
        c_start_ident.unwrap_or_else(|| "auto".to_owned())
    };

    Ok(vec![
        (Longhand::GridRowStart, r_start),
        (Longhand::GridColumnStart, c_start),
        (Longhand::GridRowEnd, r_end),
        (Longhand::GridColumnEnd, c_end),
    ])
}

// ---------------------------------------------------------------------------
// `list-style`, `text-decoration`, `white-space`, `text-wrap`
// ---------------------------------------------------------------------------

fn expand_list_style(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    let mut pos: Option<String> = None;
    let mut img: Option<String> = None;
    let mut ty: Option<String> = None;
    let mut nones = 0_usize;
    let mut count = 0_usize;

    while !cur.is_eof() {
        let start = cur.save();
        if cur.eat_ident_eq("none") {
            nones += 1;
            if nones > 2 {
                return Err("too many 'none' keywords in list-style".to_owned());
            }
            count += 1;
            continue;
        }
        cur.restore(start);

        if pos.is_none() && eat_list_style_position(&mut cur).is_some() {
            let end = cur.save();
            pos = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        cur.restore(start);

        if img.is_none() && cur.eat_url()?.is_some() {
            let end = cur.save();
            img = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        cur.restore(start);

        if ty.is_none() && eat_list_style_type(&mut cur).is_some() {
            let end = cur.save();
            ty = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }

        return Err("invalid or duplicate token in list-style".to_owned());
    }

    if count == 0 {
        return Err("empty list-style".to_owned());
    }

    // Disambiguate `none` per CSS Lists 3 §6.
    match nones {
        0 => {}
        1 => match (&img, &ty) {
            (None, None) => {
                img = Some("none".to_owned());
                ty = Some("none".to_owned());
            }
            (Some(_), None) => {
                ty = Some("none".to_owned());
            }
            (None, Some(_)) => {
                img = Some("none".to_owned());
            }
            (Some(_), Some(_)) => {
                return Err("'none' conflicts with both list-style-image and list-style-type".to_owned());
            }
        },
        2 => {
            if img.is_some() || ty.is_some() {
                return Err("too many image/type values with two 'none' keywords in list-style".to_owned());
            }
            img = Some("none".to_owned());
            ty = Some("none".to_owned());
        }
        _ => unreachable!(),
    }

    Ok(vec![
        (
            Longhand::ListStylePosition,
            pos.unwrap_or_else(|| "outside".to_owned()),
        ),
        (
            Longhand::ListStyleImage,
            img.unwrap_or_else(|| "none".to_owned()),
        ),
        (
            Longhand::ListStyleType,
            ty.unwrap_or_else(|| "disc".to_owned()),
        ),
    ])
}

fn expand_text_decoration(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let uctx = dummy_uctx(true);
    let mut cur = Cursor::new(cvs);

    let mut line_words: Vec<String> = Vec::new();
    let mut line_none = false;
    let mut style: Option<String> = None;
    let mut color: Option<String> = None;
    let mut thickness_seen = false;
    let mut count = 0_usize;

    while !cur.is_eof() {
        let start = cur.save();

        // `<text-decoration-line>`
        if let Some(id) = cur.eat_ident() {
            let lower = id.to_ascii_lowercase();
            if lower == "none" && line_words.is_empty() && !line_none {
                line_none = true;
                count += 1;
                continue;
            }
            if matches!(
                lower.as_str(),
                "underline" | "overline" | "line-through" | "blink"
            ) && !line_none
                && !line_words.contains(&lower)
            {
                line_words.push(lower);
                count += 1;
                continue;
            }
        }
        cur.restore(start);

        // `<text-decoration-style>`
        if style.is_none() && eat_text_decoration_style(&mut cur).is_some() {
            let end = cur.save();
            style = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        cur.restore(start);

        // `<text-decoration-color>`
        if color.is_none() && eat_decoration_color_token(&mut cur, Rgba::BLACK)?.is_some() {
            let end = cur.save();
            color = Some(serialize_cvs(&cvs[start..end]));
            count += 1;
            continue;
        }
        cur.restore(start);

        // Optional CSS Text Decoration 4 `<text-decoration-thickness>` (parsed and ignored)
        if !thickness_seen {
            if cur.eat_ident_eq("auto") || cur.eat_ident_eq("from-font") {
                thickness_seen = true;
                count += 1;
                continue;
            }
            cur.restore(start);
            if eat_length(&mut cur, &uctx, false)?.is_some() {
                thickness_seen = true;
                count += 1;
                continue;
            }
        }

        return Err("invalid or duplicate token in text-decoration".to_owned());
    }

    if count == 0 {
        return Err("empty text-decoration".to_owned());
    }

    let line_str = if line_words.is_empty() {
        "none".to_owned()
    } else {
        line_words.join(" ")
    };

    Ok(vec![
        (Longhand::TextDecorationLine, line_str),
        (
            Longhand::TextDecorationStyle,
            style.unwrap_or_else(|| "solid".to_owned()),
        ),
        (
            Longhand::TextDecorationColor,
            color.unwrap_or_else(|| "currentcolor".to_owned()),
        ),
    ])
}

fn expand_white_space(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    if let Some(id) = cur.eat_ident()
        && cur.is_eof()
    {
        let pair = match id.to_ascii_lowercase().as_str() {
            "normal" => Some(("collapse", "wrap")),
            "nowrap" => Some(("collapse", "nowrap")),
            "pre" => Some(("preserve", "nowrap")),
            "pre-wrap" => Some(("preserve", "wrap")),
            "pre-line" => Some(("preserve-breaks", "wrap")),
            "break-spaces" => Some(("break-spaces", "wrap")),
            _ => None,
        };
        if let Some((c, w)) = pair {
            return Ok(vec![
                (Longhand::WhiteSpaceCollapse, c.to_owned()),
                (Longhand::TextWrapMode, w.to_owned()),
            ]);
        }
    }

    // CSS Text 4 multi-keyword syntax: `<white-space-collapse> || <text-wrap-mode> || <text-wrap-style>`
    let mut cur = Cursor::new(cvs);
    let mut collapse: Option<String> = None;
    let mut wrap_mode: Option<String> = None;
    let mut wrap_style_seen = false;
    let mut count = 0_usize;

    while !cur.is_eof() {
        let Some(cv) = cur.next_cv() else { break };
        if collapse.is_none() && parse_white_space_collapse(std::slice::from_ref(cv)).is_ok() {
            collapse = Some(serialize_cv(cv).to_ascii_lowercase());
            count += 1;
            continue;
        }
        if wrap_mode.is_none() && parse_text_wrap_mode(std::slice::from_ref(cv)).is_ok() {
            wrap_mode = Some(serialize_cv(cv).to_ascii_lowercase());
            count += 1;
            continue;
        }
        let mut icur = Cursor::new(std::slice::from_ref(cv));
        if !wrap_style_seen
            && let Some(id) = icur.eat_ident()
            && matches!(
                id.to_ascii_lowercase().as_str(),
                "auto" | "balance" | "stable" | "pretty"
            )
        {
            wrap_style_seen = true;
            count += 1;
            continue;
        }
        return Err("invalid white-space value".to_owned());
    }

    if count == 0 {
        return Err("empty white-space value".to_owned());
    }

    Ok(vec![
        (
            Longhand::WhiteSpaceCollapse,
            collapse.unwrap_or_else(|| "collapse".to_owned()),
        ),
        (
            Longhand::TextWrapMode,
            wrap_mode.unwrap_or_else(|| "wrap".to_owned()),
        ),
    ])
}

fn expand_text_wrap(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let mut cur = Cursor::new(cvs);
    let mut wrap_mode: Option<String> = None;
    let mut wrap_style_seen = false;
    let mut count = 0_usize;

    while !cur.is_eof() {
        let Some(cv) = cur.next_cv() else { break };
        if wrap_mode.is_none() && parse_text_wrap_mode(std::slice::from_ref(cv)).is_ok() {
            wrap_mode = Some(serialize_cv(cv).to_ascii_lowercase());
            count += 1;
            continue;
        }
        let mut icur = Cursor::new(std::slice::from_ref(cv));
        if !wrap_style_seen
            && let Some(id) = icur.eat_ident()
            && matches!(
                id.to_ascii_lowercase().as_str(),
                "auto" | "balance" | "stable" | "pretty"
            )
        {
            wrap_style_seen = true;
            count += 1;
            continue;
        }
        return Err("invalid text-wrap value".to_owned());
    }

    if count == 0 {
        return Err("empty text-wrap value".to_owned());
    }

    Ok(vec![(
        Longhand::TextWrapMode,
        wrap_mode.unwrap_or_else(|| "wrap".to_owned()),
    )])
}

// ---------------------------------------------------------------------------
// `transition` and `animation` shorthands
// ---------------------------------------------------------------------------

fn expand_transition(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let items = split_comma_separated(cvs)?;
    let mut durations = Vec::with_capacity(items.len());
    let mut easings = Vec::with_capacity(items.len());
    let mut delays = Vec::with_capacity(items.len());
    let mut props = Vec::with_capacity(items.len());
    let multi = items.len() > 1;

    for item in items {
        let mut cur = Cursor::new(item);
        let mut dur: Option<String> = None;
        let mut delay: Option<String> = None;
        let mut easing: Option<String> = None;
        let mut prop: Option<String> = None;
        let mut behavior_seen = false;
        let mut count = 0_usize;

        while !cur.is_eof() {
            let start = cur.save();

            // 1st `<time>` is non-negative `duration`, 2nd `<time>` is `delay`.
            if dur.is_none() {
                if eat_time_ms(&mut cur, true)?.is_some() {
                    let end = cur.save();
                    dur = Some(serialize_cvs(&item[start..end]));
                    count += 1;
                    continue;
                }
            } else if delay.is_none() && eat_time_ms(&mut cur, false)?.is_some() {
                let end = cur.save();
                delay = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            if easing.is_none() && eat_easing(&mut cur)?.is_some() {
                let end = cur.save();
                easing = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            // Optional CSS Transitions 2 `<transition-behavior>`
            if !behavior_seen
                && let Some(id) = cur.eat_ident()
                && matches!(id.to_ascii_lowercase().as_str(), "normal" | "allow-discrete")
            {
                behavior_seen = true;
                count += 1;
                continue;
            }
            cur.restore(start);

            if prop.is_none()
                && let Some(id) = cur.eat_custom_ident()
            {
                if multi && id.eq_ignore_ascii_case("none") {
                    return Err("'none' is not allowed in a multi-item transition list".to_owned());
                }
                let end = cur.save();
                prop = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }

            return Err("invalid or duplicate token in transition shorthand".to_owned());
        }

        if count == 0 {
            return Err("empty item in transition shorthand".to_owned());
        }

        durations.push(dur.unwrap_or_else(|| "0s".to_owned()));
        easings.push(easing.unwrap_or_else(|| "ease".to_owned()));
        delays.push(delay.unwrap_or_else(|| "0s".to_owned()));
        props.push(prop.unwrap_or_else(|| "all".to_owned()));
    }

    Ok(vec![
        (Longhand::TransitionDuration, durations.join(", ")),
        (Longhand::TransitionTimingFunction, easings.join(", ")),
        (Longhand::TransitionDelay, delays.join(", ")),
        (Longhand::TransitionProperty, props.join(", ")),
    ])
}

fn expand_animation(cvs: &[ComponentValue]) -> Result<Vec<(Longhand, String)>, String> {
    let items = split_comma_separated(cvs)?;
    let mut durations = Vec::with_capacity(items.len());
    let mut easings = Vec::with_capacity(items.len());
    let mut delays = Vec::with_capacity(items.len());
    let mut iterations = Vec::with_capacity(items.len());
    let mut directions = Vec::with_capacity(items.len());
    let mut fill_modes = Vec::with_capacity(items.len());
    let mut play_states = Vec::with_capacity(items.len());
    let mut names = Vec::with_capacity(items.len());

    for item in items {
        let mut cur = Cursor::new(item);
        let mut dur: Option<String> = None;
        let mut delay: Option<String> = None;
        let mut easing: Option<String> = None;
        let mut iter_cnt: Option<String> = None;
        let mut dir: Option<String> = None;
        let mut fill: Option<String> = None;
        let mut play: Option<String> = None;
        let mut name: Option<String> = None;
        let mut count = 0_usize;

        while !cur.is_eof() {
            let start = cur.save();

            // 1. `<time>`: 1st is `duration` (non-negative, or CSS Animations 2 `auto`), 2nd is `delay`.
            if dur.is_none() {
                if cur.eat_ident_eq("auto") {
                    dur = Some("auto".to_owned());
                    count += 1;
                    continue;
                }
                cur.restore(start);
                if eat_time_ms(&mut cur, true)?.is_some() {
                    let end = cur.save();
                    dur = Some(serialize_cvs(&item[start..end]));
                    count += 1;
                    continue;
                }
            } else if delay.is_none() && eat_time_ms(&mut cur, false)?.is_some() {
                let end = cur.save();
                delay = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            // 2. `<easing-function>` (tried before `<keyframes-name>` per CSS Animations 1 §3.9)
            if easing.is_none() && eat_easing(&mut cur)?.is_some() {
                let end = cur.save();
                easing = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            // 3. `<single-animation-iteration-count>` (`infinite | <number [0,∞]>`)
            if iter_cnt.is_none() && eat_anim_iteration_count(&mut cur)?.is_some() {
                let end = cur.save();
                iter_cnt = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            // 4. `<single-animation-direction>`
            if dir.is_none() && eat_anim_direction(&mut cur).is_some() {
                let end = cur.save();
                dir = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            // 5. `<single-animation-fill-mode>` (`none | forwards | backwards | both`)
            if fill.is_none() && eat_anim_fill_mode(&mut cur).is_some() {
                let end = cur.save();
                fill = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            // 6. `<single-animation-play-state>` (`running | paused`)
            if play.is_none() && eat_anim_play_state(&mut cur).is_some() {
                let end = cur.save();
                play = Some(serialize_cvs(&item[start..end]));
                count += 1;
                continue;
            }
            cur.restore(start);

            // 7. `<keyframes-name>` (`none`, `<custom-ident>`, or `<string>`)
            if name.is_none() {
                if cur.eat_ident_eq("none") {
                    name = Some("none".to_owned());
                    count += 1;
                    continue;
                }
                cur.restore(start);
                if eat_keyframes_name(&mut cur).is_some() {
                    let end = cur.save();
                    name = Some(serialize_cvs(&item[start..end]));
                    count += 1;
                    continue;
                }
                cur.restore(start);
                // A custom-ident that wasn't `"none"` and wasn't CSS-wide
                if let Some(id) = cur.eat_ident()
                    && !is_css_wide_keyword(id)
                {
                    let end = cur.save();
                    name = Some(serialize_cvs(&item[start..end]));
                    count += 1;
                    continue;
                }
            }

            return Err("invalid or duplicate token in animation shorthand".to_owned());
        }

        if count == 0 {
            return Err("empty item in animation shorthand".to_owned());
        }

        durations.push(dur.unwrap_or_else(|| "0s".to_owned()));
        easings.push(easing.unwrap_or_else(|| "ease".to_owned()));
        delays.push(delay.unwrap_or_else(|| "0s".to_owned()));
        iterations.push(iter_cnt.unwrap_or_else(|| "1".to_owned()));
        directions.push(dir.unwrap_or_else(|| "normal".to_owned()));
        fill_modes.push(fill.unwrap_or_else(|| "none".to_owned()));
        play_states.push(play.unwrap_or_else(|| "running".to_owned()));
        names.push(name.unwrap_or_else(|| "none".to_owned()));
    }

    Ok(vec![
        (Longhand::AnimationDuration, durations.join(", ")),
        (Longhand::AnimationTimingFunction, easings.join(", ")),
        (Longhand::AnimationDelay, delays.join(", ")),
        (Longhand::AnimationIterationCount, iterations.join(", ")),
        (Longhand::AnimationDirection, directions.join(", ")),
        (Longhand::AnimationFillMode, fill_modes.join(", ")),
        (Longhand::AnimationPlayState, play_states.join(", ")),
        (Longhand::AnimationName, names.join(", ")),
    ])
}
