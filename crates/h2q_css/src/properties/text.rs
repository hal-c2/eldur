//! Typography, text layout, text decoration, lists, UI (`cursor`,
//! `pointer-events`, `user-select`, `visibility`, `direction`), and generated
//! `content`.

use swc_css_ast::ComponentValue;

use crate::{
    BorderCollapse, BorderStyle, CaptionSide, ComputedStyle, ContentItem, Cursor as CssCursor,
    Direction, Length, ListStyleType, PointerEvents, Rgba, Shadow, TableLayout, TextAlign,
    TextDecorationLine, TextOverflow, TextTransform, VerticalAlign, Visibility, WhiteSpace,
    WordBreak,
};

use super::{
    ComputeContext,
    background::parse_shadow_list,
    color::{eat_color, parse_color_cvs},
    cursor::{Cursor, split_comma_separated},
    length::{
        UnitContext, eat_length, eat_number_or_calc, eat_px, length_to_px, parse_length_cvs,
        parse_px_cvs,
    },
};

pub(crate) fn parse_spacing(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Option<f32>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("normal") {
        cur.expect_eof()?;
        return Ok(None);
    }
    Ok(Some(parse_px_cvs(cvs, uctx, false)?))
}

pub(crate) fn parse_text_align(
    cvs: &[ComponentValue],
    ctx: &ComputeContext<'_>,
) -> Result<TextAlign, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected text-align keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "start" => Ok(TextAlign::Start),
        "end" => Ok(TextAlign::End),
        "left" | "-webkit-left" => Ok(TextAlign::Left),
        "right" | "-webkit-right" => Ok(TextAlign::Right),
        "center" | "-webkit-center" => Ok(TextAlign::Center),
        "justify" | "justify-all" => Ok(TextAlign::Justify),
        "match-parent" => {
            let (parent_align, parent_dir) =
                ctx.parent.map_or((TextAlign::Start, Direction::Ltr), |p| {
                    (p.text_align, p.direction)
                });
            Ok(match (parent_align, parent_dir) {
                (TextAlign::Start, Direction::Ltr) | (TextAlign::End, Direction::Rtl) => {
                    TextAlign::Left
                }
                (TextAlign::Start, Direction::Rtl) | (TextAlign::End, Direction::Ltr) => {
                    TextAlign::Right
                }
                (other, _) => other,
            })
        }
        _ => Err(format!("invalid text-align '{id}'")),
    }
}

pub(crate) fn parse_text_indent(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Length, String> {
    let mut cur = Cursor::new(cvs);
    let mut len: Option<Length> = None;
    let mut hanging = false;
    let mut each_line = false;

    while !cur.is_eof() {
        if cur.eat_ident_eq("hanging") {
            if hanging {
                return Err("duplicate 'hanging' in text-indent".to_owned());
            }
            hanging = true;
            continue;
        }
        if cur.eat_ident_eq("each-line") {
            if each_line {
                return Err("duplicate 'each-line' in text-indent".to_owned());
            }
            each_line = true;
            continue;
        }
        if len.is_none()
            && let Some(l) = eat_length(&mut cur, uctx, false)?
        {
            len = Some(l);
            continue;
        }
        return Err("invalid token in text-indent".to_owned());
    }

    len.ok_or_else(|| "text-indent requires a <length-percentage>".to_owned())
}

pub(crate) fn parse_text_transform(cvs: &[ComponentValue]) -> Result<TextTransform, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected text-transform keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "none" => Ok(TextTransform::None),
        "uppercase" => Ok(TextTransform::Uppercase),
        "lowercase" => Ok(TextTransform::Lowercase),
        "capitalize" => Ok(TextTransform::Capitalize),
        _ => Err(format!("invalid text-transform '{id}'")),
    }
}

// ---------------------------------------------------------------------------
// `white-space`, `white-space-collapse`, and `text-wrap-mode`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CollapseMode {
    Collapse,
    Preserve,
    PreserveBreaks,
    BreakSpaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WrapMode {
    Wrap,
    NoWrap,
}

pub(crate) fn decompose_white_space(ws: WhiteSpace) -> (CollapseMode, WrapMode) {
    match ws {
        WhiteSpace::Normal => (CollapseMode::Collapse, WrapMode::Wrap),
        WhiteSpace::NoWrap => (CollapseMode::Collapse, WrapMode::NoWrap),
        WhiteSpace::Pre => (CollapseMode::Preserve, WrapMode::NoWrap),
        WhiteSpace::PreWrap => (CollapseMode::Preserve, WrapMode::Wrap),
        WhiteSpace::PreLine => (CollapseMode::PreserveBreaks, WrapMode::Wrap),
        WhiteSpace::BreakSpaces => (CollapseMode::BreakSpaces, WrapMode::Wrap),
    }
}

pub(crate) fn compose_white_space(collapse: CollapseMode, wrap: WrapMode) -> WhiteSpace {
    match (collapse, wrap) {
        (CollapseMode::Collapse, WrapMode::Wrap) => WhiteSpace::Normal,
        (CollapseMode::Collapse, WrapMode::NoWrap) => WhiteSpace::NoWrap,
        (CollapseMode::Preserve, WrapMode::NoWrap) => WhiteSpace::Pre,
        (CollapseMode::Preserve, WrapMode::Wrap) => WhiteSpace::PreWrap,
        (CollapseMode::PreserveBreaks, WrapMode::Wrap) => WhiteSpace::PreLine,
        (CollapseMode::PreserveBreaks, WrapMode::NoWrap) => WhiteSpace::Pre,
        (CollapseMode::BreakSpaces, WrapMode::Wrap) => WhiteSpace::BreakSpaces,
        (CollapseMode::BreakSpaces, WrapMode::NoWrap) => WhiteSpace::Pre,
    }
}

pub(crate) fn parse_white_space_collapse(cvs: &[ComponentValue]) -> Result<CollapseMode, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected white-space-collapse keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "collapse" => Ok(CollapseMode::Collapse),
        "preserve" => Ok(CollapseMode::Preserve),
        "preserve-breaks" => Ok(CollapseMode::PreserveBreaks),
        "break-spaces" => Ok(CollapseMode::BreakSpaces),
        _ => Err(format!("invalid white-space-collapse '{id}'")),
    }
}

pub(crate) fn parse_text_wrap_mode(cvs: &[ComponentValue]) -> Result<WrapMode, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected text-wrap-mode keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "wrap" => Ok(WrapMode::Wrap),
        "nowrap" => Ok(WrapMode::NoWrap),
        _ => Err(format!("invalid text-wrap-mode '{id}'")),
    }
}

pub(crate) fn parse_word_break(cvs: &[ComponentValue]) -> Result<WordBreak, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected word-break keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "normal" => Ok(WordBreak::Normal),
        "break-all" => Ok(WordBreak::BreakAll),
        "keep-all" => Ok(WordBreak::KeepAll),
        "break-word" => Ok(WordBreak::BreakWord),
        _ => Err(format!("invalid word-break '{id}'")),
    }
}

pub(crate) fn parse_overflow_wrap(cvs: &[ComponentValue]) -> Result<bool, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected overflow-wrap keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "normal" => Ok(false),
        "anywhere" | "break-word" => Ok(true),
        _ => Err(format!("invalid overflow-wrap '{id}'")),
    }
}

pub(crate) fn parse_text_shadow(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
    current_color: Rgba,
) -> Result<Vec<Shadow>, String> {
    parse_shadow_list(cvs, uctx, current_color, false)
}

pub(crate) fn parse_direction(cvs: &[ComponentValue]) -> Result<Direction, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected direction keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "ltr" => Ok(Direction::Ltr),
        "rtl" => Ok(Direction::Rtl),
        _ => Err(format!("invalid direction '{id}'")),
    }
}

pub(crate) fn parse_visibility(cvs: &[ComponentValue]) -> Result<Visibility, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected visibility keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "visible" => Ok(Visibility::Visible),
        "hidden" => Ok(Visibility::Hidden),
        "collapse" => Ok(Visibility::Collapse),
        _ => Err(format!("invalid visibility '{id}'")),
    }
}

/// Parses CSS UI 4 `cursor`: maps predefined names to [`CssCursor`], and maps
/// `url(...)` cursors or unmodelled `<custom-ident>` cursor names to [`CssCursor::Other`].
pub(crate) fn parse_cursor(cvs: &[ComponentValue]) -> Result<CssCursor, String> {
    let parts = split_comma_separated(cvs)?;
    let (last, url_fallbacks) = parts
        .split_last()
        .ok_or_else(|| "empty cursor value".to_owned())?;

    for item in url_fallbacks {
        validate_cursor_url_item(item)?;
    }

    // Also allow a standalone `url(...) [x y]?` without fallback keyword -> `Other`.
    if url_fallbacks.is_empty() && validate_cursor_url_item(last).is_ok() {
        return Ok(CssCursor::Other);
    }

    let mut cur = Cursor::new(last);
    let kw = cur
        .eat_custom_ident()
        .ok_or_else(|| "cursor value must be a cursor identifier or url()".to_owned())?;
    cur.expect_eof()?;

    if !url_fallbacks.is_empty() {
        return Ok(CssCursor::Other);
    }

    Ok(match kw.to_ascii_lowercase().as_str() {
        "auto" => CssCursor::Auto,
        "default" => CssCursor::Default,
        "pointer" => CssCursor::Pointer,
        "text" => CssCursor::Text,
        "move" => CssCursor::Move,
        "not-allowed" => CssCursor::NotAllowed,
        "wait" => CssCursor::Wait,
        "help" => CssCursor::Help,
        "crosshair" => CssCursor::Crosshair,
        "grab" | "-webkit-grab" => CssCursor::Grab,
        "grabbing" | "-webkit-grabbing" => CssCursor::Grabbing,
        "ew-resize" | "e-resize" | "w-resize" | "col-resize" => CssCursor::ResizeEw,
        "ns-resize" | "n-resize" | "s-resize" | "row-resize" => CssCursor::ResizeNs,
        "nesw-resize" | "ne-resize" | "sw-resize" => CssCursor::ResizeNesw,
        "nwse-resize" | "nw-resize" | "se-resize" => CssCursor::ResizeNwse,
        _ => CssCursor::Other,
    })
}

fn validate_cursor_url_item(item: &[ComponentValue]) -> Result<(), String> {
    let mut cur = Cursor::new(item);
    let _url = cur
        .eat_url()?
        .ok_or_else(|| "cursor fallback item must be url()".to_owned())?;
    if !cur.is_eof() {
        let _x = eat_number_or_calc(&mut cur, false)?
            .ok_or_else(|| "expected hotspot X coordinate after url() in cursor".to_owned())?;
        let _y = eat_number_or_calc(&mut cur, false)?
            .ok_or_else(|| "expected hotspot Y coordinate after url() in cursor".to_owned())?;
    }
    cur.expect_eof()
}

pub(crate) fn parse_pointer_events(cvs: &[ComponentValue]) -> Result<PointerEvents, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected pointer-events keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "none" => Ok(PointerEvents::None),
        "auto" | "visiblepainted" | "visiblefill" | "visiblestroke" | "visible" | "painted"
        | "fill" | "stroke" | "all" => Ok(PointerEvents::Auto),
        _ => Err(format!("invalid pointer-events '{id}'")),
    }
}

pub(crate) fn parse_border_collapse(cvs: &[ComponentValue]) -> Result<BorderCollapse, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected border-collapse keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "separate" => Ok(BorderCollapse::Separate),
        "collapse" => Ok(BorderCollapse::Collapse),
        _ => Err(format!("invalid border-collapse '{id}'")),
    }
}

/// One length applies to both axes; two are horizontal, then vertical.
pub(crate) fn parse_border_spacing(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<(f32, f32), String> {
    let mut cur = Cursor::new(cvs);
    let h = eat_px(&mut cur, uctx, true)?.ok_or_else(|| "expected a <length>".to_owned())?;
    let v = eat_px(&mut cur, uctx, true)?.unwrap_or(h);
    cur.expect_eof()?;
    Ok((h, v))
}

pub(crate) fn parse_caption_side(cvs: &[ComponentValue]) -> Result<CaptionSide, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected caption-side keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "top" => Ok(CaptionSide::Top),
        "bottom" => Ok(CaptionSide::Bottom),
        _ => Err(format!("invalid caption-side '{id}'")),
    }
}

pub(crate) fn parse_table_layout(cvs: &[ComponentValue]) -> Result<TableLayout, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected table-layout keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "auto" => Ok(TableLayout::Auto),
        "fixed" => Ok(TableLayout::Fixed),
        _ => Err(format!("invalid table-layout '{id}'")),
    }
}

pub(crate) fn eat_list_style_type(cur: &mut Cursor<'_>) -> Option<ListStyleType> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    let ty = match id.to_ascii_lowercase().as_str() {
        "disc" => ListStyleType::Disc,
        "circle" => ListStyleType::Circle,
        "square" => ListStyleType::Square,
        "decimal" => ListStyleType::Decimal,
        "decimal-leading-zero" => ListStyleType::DecimalLeadingZero,
        "lower-alpha" | "lower-latin" => ListStyleType::LowerAlpha,
        "upper-alpha" | "upper-latin" => ListStyleType::UpperAlpha,
        "lower-roman" => ListStyleType::LowerRoman,
        "upper-roman" => ListStyleType::UpperRoman,
        "none" => ListStyleType::None,
        _ => {
            cur.restore(saved);
            return None;
        }
    };
    Some(ty)
}

pub(crate) fn parse_list_style_type(cvs: &[ComponentValue]) -> Result<ListStyleType, String> {
    let mut cur = Cursor::new(cvs);
    let ty = eat_list_style_type(&mut cur).ok_or_else(|| "invalid list-style-type".to_owned())?;
    cur.expect_eof()?;
    Ok(ty)
}

pub(crate) fn eat_list_style_position(cur: &mut Cursor<'_>) -> Option<bool> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    match id.to_ascii_lowercase().as_str() {
        "inside" => Some(true),
        "outside" => Some(false),
        _ => {
            cur.restore(saved);
            None
        }
    }
}

pub(crate) fn parse_list_style_position(cvs: &[ComponentValue]) -> Result<bool, String> {
    let mut cur = Cursor::new(cvs);
    let inside = eat_list_style_position(&mut cur)
        .ok_or_else(|| "expected 'inside' or 'outside' for list-style-position".to_owned())?;
    cur.expect_eof()?;
    Ok(inside)
}

pub(crate) fn parse_list_style_image(cvs: &[ComponentValue]) -> Result<Option<String>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(None);
    }
    let url = cur
        .eat_url()?
        .ok_or_else(|| "expected url() or 'none' for list-style-image".to_owned())?;
    cur.expect_eof()?;
    Ok(Some(url))
}

pub(crate) fn parse_user_select(cvs: &[ComponentValue]) -> Result<bool, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected user-select keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "none" => Ok(true),
        "auto" | "text" | "contain" | "all" => Ok(false),
        _ => Err(format!("invalid user-select '{id}'")),
    }
}

// ---------------------------------------------------------------------------
// Text decoration, `text-overflow`, `vertical-align`, and `content`
// ---------------------------------------------------------------------------

pub(crate) fn parse_text_decoration_line(
    cvs: &[ComponentValue],
) -> Result<TextDecorationLine, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(TextDecorationLine::default());
    }

    let mut line = TextDecorationLine::default();
    let mut blink = false;
    let mut count = 0_usize;

    while !cur.is_eof() {
        let id = cur
            .eat_ident()
            .ok_or_else(|| "expected text-decoration-line keyword".to_owned())?;
        match id.to_ascii_lowercase().as_str() {
            "underline" if !line.underline => line.underline = true,
            "overline" if !line.overline => line.overline = true,
            "line-through" if !line.line_through => line.line_through = true,
            "blink" if !blink => blink = true,
            _ => return Err(format!("invalid or duplicate text-decoration-line '{id}'")),
        }
        count += 1;
    }

    if count == 0 {
        return Err("empty text-decoration-line".to_owned());
    }
    Ok(line)
}

pub(crate) fn eat_text_decoration_style(cur: &mut Cursor<'_>) -> Option<BorderStyle> {
    let saved = cur.save();
    let id = cur.eat_ident()?;
    let st = match id.to_ascii_lowercase().as_str() {
        "solid" | "wavy" => BorderStyle::Solid,
        "double" => BorderStyle::Double,
        "dotted" => BorderStyle::Dotted,
        "dashed" => BorderStyle::Dashed,
        _ => {
            cur.restore(saved);
            return None;
        }
    };
    Some(st)
}

pub(crate) fn parse_text_decoration_style(cvs: &[ComponentValue]) -> Result<BorderStyle, String> {
    let mut cur = Cursor::new(cvs);
    let st = eat_text_decoration_style(&mut cur)
        .ok_or_else(|| "invalid text-decoration-style".to_owned())?;
    cur.expect_eof()?;
    Ok(st)
}

pub(crate) fn parse_text_decoration_color(
    cvs: &[ComponentValue],
    current_color: Rgba,
) -> Result<Option<Rgba>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("currentcolor") {
        cur.expect_eof()?;
        return Ok(None);
    }
    Ok(Some(parse_color_cvs(cvs, current_color)?))
}

/// Helper for `text-decoration` shorthand: checks whether `cur` begins with a `<color>`.
pub(crate) fn eat_decoration_color_token(
    cur: &mut Cursor<'_>,
    current_color: Rgba,
) -> Result<Option<Option<Rgba>>, String> {
    let saved = cur.save();
    if cur.eat_ident_eq("currentcolor") {
        return Ok(Some(None));
    }
    cur.restore(saved);
    Ok(eat_color(cur, current_color)?.map(Some))
}

pub(crate) fn parse_text_overflow(cvs: &[ComponentValue]) -> Result<TextOverflow, String> {
    let mut cur = Cursor::new(cvs);
    let id = cur
        .eat_ident()
        .ok_or_else(|| "expected text-overflow keyword".to_owned())?;
    cur.expect_eof()?;
    match id.to_ascii_lowercase().as_str() {
        "clip" => Ok(TextOverflow::Clip),
        "ellipsis" => Ok(TextOverflow::Ellipsis),
        _ => Err(format!("invalid text-overflow '{id}'")),
    }
}

pub(crate) fn parse_vertical_align(
    cvs: &[ComponentValue],
    style: &ComputedStyle,
    ctx: &ComputeContext<'_>,
) -> Result<VerticalAlign, String> {
    let mut cur = Cursor::new(cvs);
    let saved = cur.save();
    if let Some(id) = cur.eat_ident() {
        let kw = match id.to_ascii_lowercase().as_str() {
            "baseline" => Some(VerticalAlign::Baseline),
            "sub" => Some(VerticalAlign::Sub),
            "super" => Some(VerticalAlign::Super),
            "top" => Some(VerticalAlign::Top),
            "text-top" => Some(VerticalAlign::TextTop),
            "middle" => Some(VerticalAlign::Middle),
            "bottom" => Some(VerticalAlign::Bottom),
            "text-bottom" => Some(VerticalAlign::TextBottom),
            _ => None,
        };
        if let Some(va) = kw {
            cur.expect_eof()?;
            return Ok(va);
        }
        cur.restore(saved);
    }

    let uctx = UnitContext::for_vertical_align(style, ctx);
    let len = parse_length_cvs(cvs, &uctx, false)?;
    let px = length_to_px(&len, &uctx, false)?;
    Ok(VerticalAlign::Length(px))
}

/// Parses CSS Generated Content 3 `content`.
pub(crate) fn parse_content(cvs: &[ComponentValue]) -> Result<Option<Vec<ContentItem>>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") || cur.eat_ident_eq("normal") {
        cur.expect_eof()?;
        return Ok(None);
    }

    let mut items = Vec::new();
    let mut saw_item = false;

    while !cur.is_eof() {
        if cur.eat_slash() {
            if !saw_item {
                return Err("content alt-text '/' cannot appear without content items".to_owned());
            }
            // Accessibility alt-text: one or more `<string>` or `attr()` functions.
            let mut alt_count = 0_usize;
            while !cur.is_eof() {
                if cur.eat_string().is_some() {
                    alt_count += 1;
                    continue;
                }
                if let Some((fname, fargs)) = cur.eat_function()
                    && fname.eq_ignore_ascii_case("attr")
                {
                    let _ = parse_attr_fn(fargs)?;
                    alt_count += 1;
                    continue;
                }
                return Err("invalid token in content alt-text".to_owned());
            }
            if alt_count == 0 {
                return Err("empty alt-text after '/' in content".to_owned());
            }
            break;
        }

        if let Some(s) = cur.eat_string() {
            items.push(ContentItem::String(s.to_owned()));
            saw_item = true;
            continue;
        }

        if let Some(url) = cur.eat_url()? {
            items.push(ContentItem::Url(url));
            saw_item = true;
            continue;
        }

        let saved = cur.save();
        if let Some(id) = cur.eat_ident() {
            match id.to_ascii_lowercase().as_str() {
                "open-quote" => {
                    items.push(ContentItem::OpenQuote);
                    saw_item = true;
                    continue;
                }
                "close-quote" => {
                    items.push(ContentItem::CloseQuote);
                    saw_item = true;
                    continue;
                }
                "no-open-quote" | "no-close-quote" => {
                    saw_item = true;
                    continue;
                }
                _ => cur.restore(saved),
            }
        }

        if let Some((fname, fargs)) = cur.eat_function() {
            match fname.to_ascii_lowercase().as_str() {
                "attr" => {
                    items.push(ContentItem::Attr(parse_attr_fn(fargs)?));
                    saw_item = true;
                    continue;
                }
                "counter" => {
                    let (name, sty) = parse_counter_fn(fargs, false)?;
                    items.push(ContentItem::Counter(name, sty));
                    saw_item = true;
                    continue;
                }
                "counters" => {
                    let (name, sty) = parse_counter_fn(fargs, true)?;
                    items.push(ContentItem::Counter(name, sty));
                    saw_item = true;
                    continue;
                }
                _ => return Err(format!("unsupported function '{fname}()' in content")),
            }
        }

        return Err("invalid token in content property".to_owned());
    }

    if !saw_item {
        return Err("empty content value".to_owned());
    }
    Ok(Some(items))
}

fn parse_attr_fn(args: &[ComponentValue]) -> Result<String, String> {
    let mut cur = Cursor::new(args);
    let name = cur
        .eat_ident()
        .ok_or_else(|| "attr() requires an attribute name identifier".to_owned())?;
    cur.expect_eof()?;
    Ok(name.to_owned())
}

fn parse_counter_fn(
    args: &[ComponentValue],
    has_separator: bool,
) -> Result<(String, ListStyleType), String> {
    let parts = split_comma_separated(args)?;
    let expected = if has_separator { 2..=3 } else { 1..=2 };
    if !expected.contains(&parts.len()) {
        return Err("invalid argument count in counter()/counters()".to_owned());
    }

    let mut name_cur = Cursor::new(parts[0]);
    let name = name_cur
        .eat_custom_ident()
        .ok_or_else(|| "counter() requires a <custom-ident> name".to_owned())?;
    name_cur.expect_eof()?;

    if has_separator {
        let mut sep_cur = Cursor::new(parts[1]);
        let _ = sep_cur
            .eat_string()
            .ok_or_else(|| "counters() requires a separator <string>".to_owned())?;
        sep_cur.expect_eof()?;
    }

    let style_idx = if has_separator { 2 } else { 1 };
    let list_style = if let Some(style_part) = parts.get(style_idx) {
        parse_list_style_type(style_part)?
    } else {
        ListStyleType::Decimal
    };

    Ok((name.to_owned(), list_style))
}
