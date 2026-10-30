use crate::{
    Align, Animation, BackgroundRepeat, BackgroundSize, BorderStyle, BoxSizing, CalcNode,
    ColorStop, ComputedStyle, ContentItem, Cursor, Direction, Display, Easing, FlexDirection,
    FlexWrap, Float, FontStyle, GridLine, Image, Length, LengthAuto, LengthPercentage, LineHeight,
    ListStyleType, ObjectFit, Overflow, PointerEvents, Position, Rgba, Shadow, Size, TextAlign,
    TextOverflow, TextTransform, Track, TransformFn, Transition, VerticalAlign, Viewport,
    Visibility, WhiteSpace, WordBreak,
};

use super::{
    ComputeContext, CssWide, Longhand, PropertyKind, compute, copy, expand, is_inherited, kind,
    longhands_of, priority,
};

fn default_ctx(vp: &Viewport) -> ComputeContext<'_> {
    ComputeContext {
        parent: None,
        root_font_size: 16.0,
        viewport: vp,
    }
}

/// Helper that expands a declaration (shorthand or longhand), sorts by priority,
/// and applies each resulting `(Longhand, String)` to `style`.
fn apply_decl(
    style: &mut ComputedStyle,
    name: &str,
    value: &str,
    ctx: &ComputeContext<'_>,
) -> Result<(), String> {
    let mut pairs = expand(name, value)?;
    pairs.sort_by_key(|(l, _)| priority(*l));
    for (l, v) in pairs {
        compute(style, l, &v, ctx)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 1. Property classification, CSS-wide keywords, inheritance, priority, copy
// ---------------------------------------------------------------------------

#[test]
fn css_wide_keywords_and_classification() {
    assert_eq!(CssWide::parse("initial"), Some(CssWide::Initial));
    assert_eq!(CssWide::parse("  INHERIT "), Some(CssWide::Inherit));
    assert_eq!(CssWide::parse("Unset"), Some(CssWide::Unset));
    assert_eq!(CssWide::parse("revert"), Some(CssWide::Revert));
    assert_eq!(CssWide::parse("revert-layer"), Some(CssWide::RevertLayer));
    assert_eq!(CssWide::parse("auto"), None);

    assert_eq!(kind("--brand-color"), PropertyKind::Custom);
    assert_eq!(kind("--"), PropertyKind::Unknown);
    assert_eq!(kind("display"), PropertyKind::Longhand(Longhand::Display));
    assert_eq!(
        kind("MARGIN-BLOCK-START"),
        PropertyKind::Longhand(Longhand::MarginTop)
    );
    assert_eq!(kind("inline-size"), PropertyKind::Longhand(Longhand::Width));
    assert_eq!(kind("block-size"), PropertyKind::Longhand(Longhand::Height));
    assert_eq!(kind("margin"), PropertyKind::Shorthand);
    assert_eq!(kind("background"), PropertyKind::Shorthand);
    assert_eq!(kind("backdrop-filter"), PropertyKind::Unsupported);
    assert_eq!(kind("clip-path"), PropertyKind::Unsupported);
    assert_eq!(kind("not-a-css-prop"), PropertyKind::Unknown);

    assert_eq!(
        longhands_of("margin"),
        vec![
            Longhand::MarginTop,
            Longhand::MarginRight,
            Longhand::MarginBottom,
            Longhand::MarginLeft
        ]
    );
    assert_eq!(longhands_of("width"), vec![Longhand::Width]);
}

#[test]
fn inheritance_priority_and_copy() {
    assert!(is_inherited(Longhand::Color));
    assert!(is_inherited(Longhand::FontSize));
    assert!(is_inherited(Longhand::LineHeight));
    assert!(is_inherited(Longhand::WhiteSpaceCollapse));
    assert!(is_inherited(Longhand::Visibility));
    assert!(!is_inherited(Longhand::Display));
    assert!(!is_inherited(Longhand::MarginTop));
    assert!(!is_inherited(Longhand::BorderTopStyle));

    assert!(priority(Longhand::FontSize) < priority(Longhand::LineHeight));
    assert!(priority(Longhand::LineHeight) < priority(Longhand::Color));
    assert!(priority(Longhand::Color) < priority(Longhand::BorderTopStyle));
    assert!(priority(Longhand::BorderTopStyle) < priority(Longhand::BorderTopWidth));
    assert!(priority(Longhand::BorderTopStyle) < priority(Longhand::BorderTopColor));
    assert!(priority(Longhand::BackgroundImage) < priority(Longhand::BackgroundPositionX));
    assert!(priority(Longhand::TransitionProperty) < priority(Longhand::TransitionDuration));
    assert!(priority(Longhand::AnimationName) < priority(Longhand::AnimationDuration));
    assert!(priority(Longhand::Translate) < priority(Longhand::Transform));

    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut src = ComputedStyle::default();
    apply_decl(&mut src, "white-space", "pre", &ctx).expect("parse white-space");
    assert_eq!(src.white_space, WhiteSpace::Pre);

    let mut dst = ComputedStyle::default();
    copy(&mut dst, &src, Longhand::WhiteSpaceCollapse);
    assert_eq!(dst.white_space, WhiteSpace::PreWrap);
    copy(&mut dst, &src, Longhand::TextWrapMode);
    assert_eq!(dst.white_space, WhiteSpace::Pre);
}

// ---------------------------------------------------------------------------
// 2. Colors: named, hex, rgb/hsl/hwb/oklab/oklch/color(), color-mix(), currentColor
// ---------------------------------------------------------------------------

#[test]
fn color_parsing_all_syntaxes() {
    let vp = Viewport::default();
    let parent = ComputedStyle {
        color: Rgba::new(10, 20, 30, 255),
        ..ComputedStyle::default()
    };
    let ctx = ComputeContext {
        parent: Some(&parent),
        root_font_size: 16.0,
        viewport: &vp,
    };

    let mut s = ComputedStyle::default();
    compute(&mut s, Longhand::Color, "rebeccapurple", &ctx).expect("rebeccapurple");
    assert_eq!(s.color, Rgba::new(102, 51, 153, 255));

    compute(&mut s, Longhand::Color, "transparent", &ctx).expect("transparent");
    assert_eq!(s.color, Rgba::TRANSPARENT);

    compute(&mut s, Longhand::Color, "currentColor", &ctx).expect("currentColor");
    assert_eq!(s.color, Rgba::new(10, 20, 30, 255));

    compute(&mut s, Longhand::Color, "#0f8", &ctx).expect("#0f8");
    assert_eq!(s.color, Rgba::new(0, 255, 136, 255));

    compute(&mut s, Longhand::Color, "#ff000080", &ctx).expect("#ff000080");
    assert_eq!(s.color, Rgba::new(255, 0, 0, 128));

    compute(&mut s, Longhand::Color, "rgb(255, 128, 0)", &ctx).expect("rgb");
    assert_eq!(s.color, Rgba::new(255, 128, 0, 255));

    compute(&mut s, Longhand::Color, "rgb(100% 0% 50% / 50%)", &ctx).expect("rgb pct");
    assert_eq!(s.color, Rgba::new(255, 0, 128, 128));

    compute(&mut s, Longhand::Color, "hsl(120deg 100% 50% / 1)", &ctx).expect("hsl");
    assert_eq!(s.color, Rgba::new(0, 255, 0, 255));

    compute(&mut s, Longhand::Color, "hwb(0 0% 0%)", &ctx).expect("hwb");
    assert_eq!(s.color, Rgba::new(255, 0, 0, 255));

    compute(&mut s, Longhand::Color, "color(srgb 1 0 0 / 0.5)", &ctx).expect("color()");
    assert_eq!(s.color, Rgba::new(255, 0, 0, 128));

    compute(&mut s, Longhand::Color, "oklch(0.6 0.15 240)", &ctx).expect("oklch");
    assert_eq!(s.color.a, 255);

    compute(
        &mut s,
        Longhand::Color,
        "color-mix(in srgb, #ff0000 50%, #0000ff)",
        &ctx,
    )
    .expect("color-mix");
    assert_eq!(s.color, Rgba::new(128, 0, 128, 255));

    // Border color uses element's own `style.color` for `currentColor`
    s.color = Rgba::new(40, 50, 60, 255);
    compute(&mut s, Longhand::BorderTopColor, "currentColor", &ctx).expect("border currentColor");
    assert_eq!(s.border.top.color, Rgba::new(40, 50, 60, 255));

    // Error leaves style unchanged
    let prev = s.color;
    assert!(compute(&mut s, Longhand::Color, "not-a-color", &ctx).is_err());
    assert_eq!(s.color, prev);
}

// ---------------------------------------------------------------------------
// 3. Lengths, relative units, viewport units, and calc()/min()/max()/clamp()
// ---------------------------------------------------------------------------

#[test]
fn lengths_units_and_math_functions() {
    let vp = Viewport {
        width: 1000.0,
        height: 800.0,
        ..Viewport::default()
    };
    let mut style = ComputedStyle {
        font_size: 20.0,
        line_height: LineHeight::Px(30.0),
        ..ComputedStyle::default()
    };
    let ctx = ComputeContext {
        parent: None,
        root_font_size: 16.0,
        viewport: &vp,
    };

    compute(&mut style, Longhand::PaddingTop, "2em", &ctx).expect("2em");
    assert_eq!(style.padding.top, Length::px(40.0));

    compute(&mut style, Longhand::PaddingRight, "2rem", &ctx).expect("2rem");
    assert_eq!(style.padding.right, Length::px(32.0));

    compute(&mut style, Longhand::PaddingBottom, "1.5lh", &ctx).expect("1.5lh");
    assert_eq!(style.padding.bottom, Length::px(45.0));

    compute(&mut style, Longhand::PaddingLeft, "10vw", &ctx).expect("10vw");
    assert_eq!(
        style.padding.left,
        Length::Linear(LengthPercentage::vw(10.0))
    );

    compute(&mut style, Longhand::MarginTop, "5vmin", &ctx).expect("5vmin");
    assert_eq!(
        style.margin.top,
        LengthAuto::Length(Length::Linear(LengthPercentage::vh(5.0)))
    );

    compute(&mut style, Longhand::MarginBottom, "10vmax", &ctx).expect("10vmax");
    assert_eq!(
        style.margin.bottom,
        LengthAuto::Length(Length::Linear(LengthPercentage::vw(10.0)))
    );

    // calc() with mixed length units
    compute(
        &mut style,
        Longhand::Width,
        "calc(100px + 2em - 1rem)",
        &ctx,
    )
    .expect("calc px");
    assert_eq!(style.width, Size::Length(Length::px(124.0)));

    // calc() with length + percentage
    compute(&mut style, Longhand::Width, "calc(50% - 20px)", &ctx).expect("calc pct");
    assert!(matches!(
        style.width,
        Size::Length(Length::Linear(_)) | Size::Length(Length::Calc(_))
    ));

    // min / max / clamp with compatible px units
    compute(&mut style, Longhand::Height, "clamp(10px, 2em, 30px)", &ctx).expect("clamp");
    assert_eq!(style.height, Size::Length(Length::px(30.0)));

    // Negative padding rejected
    assert!(compute(&mut style, Longhand::PaddingTop, "-5px", &ctx).is_err());
}

// ---------------------------------------------------------------------------
// 4. Box model, display, position, sizing, borders, outline, shorthands
// ---------------------------------------------------------------------------

#[test]
fn box_model_and_border_shorthands() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    compute(&mut s, Longhand::Display, "inline-flex", &ctx).expect("inline-flex");
    assert_eq!(s.display, Display::InlineFlex);
    compute(&mut s, Longhand::Display, "flow-root", &ctx).expect("flow-root");
    assert_eq!(s.display, Display::FlowRoot);

    compute(&mut s, Longhand::Position, "-webkit-sticky", &ctx).expect("sticky");
    assert_eq!(s.position, Position::Sticky);

    compute(&mut s, Longhand::ZIndex, "42", &ctx).expect("z-index 42");
    assert_eq!(s.z_index, Some(42));
    compute(&mut s, Longhand::ZIndex, "auto", &ctx).expect("z-index auto");
    assert_eq!(s.z_index, None);

    compute(&mut s, Longhand::Float, "inline-end", &ctx).expect("float");
    assert_eq!(s.float, Float::Right);

    compute(&mut s, Longhand::BoxSizing, "border-box", &ctx).expect("box-sizing");
    assert_eq!(s.box_sizing, BoxSizing::BorderBox);

    compute(&mut s, Longhand::AspectRatio, "16 / 9", &ctx).expect("aspect-ratio");
    assert!((s.aspect_ratio.expect("aspect_ratio some") - 16.0 / 9.0).abs() < 1e-4);

    apply_decl(&mut s, "margin", "10px 20px 30px", &ctx).expect("margin");
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(10.0)));
    assert_eq!(s.margin.right, LengthAuto::Length(Length::px(20.0)));
    assert_eq!(s.margin.bottom, LengthAuto::Length(Length::px(30.0)));
    assert_eq!(s.margin.left, LengthAuto::Length(Length::px(20.0)));

    apply_decl(&mut s, "inset", "4px 8px", &ctx).expect("inset");
    assert_eq!(s.top, LengthAuto::Length(Length::px(4.0)));
    assert_eq!(s.right, LengthAuto::Length(Length::px(8.0)));
    assert_eq!(s.bottom, LengthAuto::Length(Length::px(4.0)));
    assert_eq!(s.left, LengthAuto::Length(Length::px(8.0)));

    // Border shorthand with style + width + color in arbitrary order
    apply_decl(&mut s, "border", "dashed #ff0000 2px", &ctx).expect("border");
    assert_eq!(s.border.top.style, BorderStyle::Dashed);
    assert_eq!(s.border.top.width, 2.0);
    assert_eq!(s.border.top.color, Rgba::new(255, 0, 0, 255));
    assert_eq!(s.border.left.width, 2.0);

    // Border style `none` forces width to 0 even if width is specified
    apply_decl(&mut s, "border-bottom", "5px none blue", &ctx).expect("border-bottom");
    assert_eq!(s.border.bottom.style, BorderStyle::None);
    assert_eq!(s.border.bottom.width, 0.0);

    // Border radius with slash syntax
    apply_decl(&mut s, "border-radius", "10px 20px / 5px", &ctx).expect("border-radius");
    assert_eq!(s.border_radius.top_left, LengthPercentage::px(10.0));
    assert_eq!(s.border_radius.top_right, LengthPercentage::px(20.0));
    assert_eq!(s.border_radius.bottom_right, LengthPercentage::px(10.0));
    assert_eq!(s.border_radius.bottom_left, LengthPercentage::px(20.0));

    // Outline shorthand (`auto` maps to Solid)
    apply_decl(&mut s, "outline", "auto 3px green", &ctx).expect("outline");
    assert_eq!(s.outline.style, BorderStyle::Solid);
    assert_eq!(s.outline.width, 3.0);
}

// ---------------------------------------------------------------------------
// 5. Flexbox & Box Alignment
// ---------------------------------------------------------------------------

#[test]
fn flexbox_and_alignment_parsing() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    apply_decl(&mut s, "flex-flow", "column-reverse wrap", &ctx).expect("flex-flow");
    assert_eq!(s.flex_direction, FlexDirection::ColumnReverse);
    assert_eq!(s.flex_wrap, FlexWrap::Wrap);

    // flex: 1 -> 1 1 0%
    apply_decl(&mut s, "flex", "1", &ctx).expect("flex 1");
    assert_eq!(s.flex_grow, 1.0);
    assert_eq!(s.flex_shrink, 1.0);
    assert_eq!(
        s.flex_basis,
        Size::Length(Length::Linear(LengthPercentage::percent(0.0)))
    );

    // flex: auto -> 1 1 auto
    apply_decl(&mut s, "flex", "auto", &ctx).expect("flex auto");
    assert_eq!(s.flex_grow, 1.0);
    assert_eq!(s.flex_shrink, 1.0);
    assert_eq!(s.flex_basis, Size::Auto);

    // flex: none -> 0 0 auto
    apply_decl(&mut s, "flex", "none", &ctx).expect("flex none");
    assert_eq!(s.flex_grow, 0.0);
    assert_eq!(s.flex_shrink, 0.0);
    assert_eq!(s.flex_basis, Size::Auto);

    // flex: 2 3 120px
    apply_decl(&mut s, "flex", "2 3 120px", &ctx).expect("flex 3-val");
    assert_eq!(s.flex_grow, 2.0);
    assert_eq!(s.flex_shrink, 3.0);
    assert_eq!(s.flex_basis, Size::Length(Length::px(120.0)));

    apply_decl(&mut s, "gap", "12px 24px", &ctx).expect("gap");
    assert_eq!(s.row_gap, Some(Length::px(12.0)));
    assert_eq!(s.column_gap, Some(Length::px(24.0)));

    apply_decl(&mut s, "place-content", "center space-between", &ctx).expect("place-content");
    assert_eq!(s.align_content, Align::Center);
    assert_eq!(s.justify_content, Align::SpaceBetween);

    apply_decl(&mut s, "place-items", "safe flex-end start", &ctx).expect("place-items");
    assert_eq!(s.align_items, Align::FlexEnd);
    assert_eq!(s.justify_items, Align::Start);

    apply_decl(&mut s, "place-self", "first baseline stretch", &ctx).expect("place-self");
    assert_eq!(s.align_self, Align::Baseline);
    assert_eq!(s.justify_self, Align::Stretch);

    compute(&mut s, Longhand::Order, "-2", &ctx).expect("order");
    assert_eq!(s.order, -2);
}

// ---------------------------------------------------------------------------
// 6. CSS Grid: track lists, repeat(), minmax(), template areas, placement
// ---------------------------------------------------------------------------

#[test]
fn grid_tracks_areas_and_placement() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    compute(
        &mut s,
        Longhand::GridTemplateColumns,
        "[nav-start] 200px repeat(2, 1fr) minmax(100px, auto) [main-end]",
        &ctx,
    )
    .expect("grid-template-columns");
    assert_eq!(
        s.grid_template_columns,
        vec![
            Track::Length(Length::px(200.0)),
            Track::Fr(1.0),
            Track::Fr(1.0),
            Track::MinMax(
                Box::new(Track::Length(Length::px(100.0))),
                Box::new(Track::Auto),
            ),
        ]
    );

    compute(
        &mut s,
        Longhand::GridTemplateAreas,
        "\"header header\" \"nav main\" \". footer\"",
        &ctx,
    )
    .expect("grid-template-areas");
    assert_eq!(s.grid_template_areas.len(), 3);
    assert_eq!(
        s.grid_template_areas[2],
        vec![None, Some("footer".to_owned())]
    );

    // Non-rectangular area is rejected
    assert!(compute(&mut s, Longhand::GridTemplateAreas, "\"a a\" \"a b\"", &ctx).is_err());

    compute(&mut s, Longhand::GridAutoFlow, "column dense", &ctx).expect("grid-auto-flow");
    assert!(s.grid_auto_flow_column);

    apply_decl(&mut s, "grid-column", "2 / span 3", &ctx).expect("grid-column");
    assert_eq!(s.grid_column_start, GridLine::Line(2));
    assert_eq!(s.grid_column_end, GridLine::Span(3));

    apply_decl(&mut s, "grid-area", "header", &ctx).expect("grid-area");
    assert_eq!(s.grid_row_start, GridLine::Named("header".to_owned()));
    assert_eq!(s.grid_row_end, GridLine::Named("header".to_owned()));
    assert_eq!(s.grid_column_start, GridLine::Named("header".to_owned()));
    assert_eq!(s.grid_column_end, GridLine::Named("header".to_owned()));

    apply_decl(
        &mut s,
        "grid-template",
        "\"a b\" 50px \"c d\" 1fr / 100px 2fr",
        &ctx,
    )
    .expect("grid-template");
    assert_eq!(s.grid_template_areas.len(), 2);
    assert_eq!(
        s.grid_template_rows,
        vec![Track::Length(Length::px(50.0)), Track::Fr(1.0)]
    );
    assert_eq!(
        s.grid_template_columns,
        vec![Track::Length(Length::px(100.0)), Track::Fr(2.0)]
    );
}

// ---------------------------------------------------------------------------
// 7. Fonts & Typography (`font`, `font-size`, `font-weight`, `line-height`, etc.)
// ---------------------------------------------------------------------------

#[test]
fn font_and_typography_parsing() {
    let vp = Viewport::default();
    let parent = ComputedStyle {
        font_size: 20.0,
        font_weight: 400,
        ..ComputedStyle::default()
    };
    let ctx = ComputeContext {
        parent: Some(&parent),
        root_font_size: 16.0,
        viewport: &vp,
    };

    let mut s = ComputedStyle::default();
    compute(&mut s, Longhand::FontSize, "150%", &ctx).expect("font-size");
    assert_eq!(s.font_size, 30.0);

    compute(&mut s, Longhand::FontWeight, "bolder", &ctx).expect("font-weight");
    assert_eq!(s.font_weight, 700);

    apply_decl(
        &mut s,
        "font",
        "italic small-caps bold 18px/1.5 \"Fira Sans\", sans-serif",
        &ctx,
    )
    .expect("font shorthand");
    assert_eq!(s.font_style, FontStyle::Italic);
    assert!(s.small_caps);
    assert_eq!(s.font_weight, 700);
    assert_eq!(s.font_size, 18.0);
    assert_eq!(s.line_height, LineHeight::Number(1.5));
    assert_eq!(
        s.font_family,
        vec!["Fira Sans".to_owned(), "sans-serif".to_owned()]
    );

    apply_decl(&mut s, "white-space", "nowrap", &ctx).expect("white-space");
    assert_eq!(s.white_space, WhiteSpace::NoWrap);

    apply_decl(&mut s, "text-decoration", "underline wavy #ff0000", &ctx).expect("text-decoration");
    assert!(s.text_decoration_line.underline);
    assert!(!s.text_decoration_line.line_through);
    assert_eq!(s.text_decoration_color, Some(Rgba::new(255, 0, 0, 255)));

    compute(&mut s, Longhand::TextAlign, "end", &ctx).expect("text-align");
    assert_eq!(s.text_align, TextAlign::End);
    compute(&mut s, Longhand::TextTransform, "uppercase", &ctx).expect("text-transform");
    assert_eq!(s.text_transform, TextTransform::Uppercase);
    compute(&mut s, Longhand::TextOverflow, "ellipsis", &ctx).expect("text-overflow");
    assert_eq!(s.text_overflow, TextOverflow::Ellipsis);
    compute(&mut s, Longhand::WordBreak, "break-all", &ctx).expect("word-break");
    assert_eq!(s.word_break, WordBreak::BreakAll);
    compute(&mut s, Longhand::OverflowWrap, "anywhere", &ctx).expect("overflow-wrap");
    assert!(s.overflow_wrap_anywhere);
    compute(&mut s, Longhand::VerticalAlign, "middle", &ctx).expect("vertical-align");
    assert_eq!(s.vertical_align, VerticalAlign::Middle);

    compute(&mut s, Longhand::Content, "\"hello\" \" \" \"world\"", &ctx).expect("content");
    assert_eq!(
        s.content,
        Some(vec![
            ContentItem::String("hello".to_owned()),
            ContentItem::String(" ".to_owned()),
            ContentItem::String("world".to_owned()),
        ])
    );

    apply_decl(&mut s, "list-style", "square inside none", &ctx).expect("list-style");
    assert_eq!(s.list_style_type, ListStyleType::Square);
    assert!(s.list_style_inside);
    assert_eq!(s.list_style_image, None);

    compute(&mut s, Longhand::Direction, "rtl", &ctx).expect("direction");
    assert_eq!(s.direction, Direction::Rtl);
    compute(&mut s, Longhand::Visibility, "hidden", &ctx).expect("visibility");
    assert_eq!(s.visibility, Visibility::Hidden);
    compute(&mut s, Longhand::Cursor, "pointer", &ctx).expect("cursor pointer");
    assert_eq!(s.cursor, Cursor::Pointer);
    compute(&mut s, Longhand::Cursor, "url(hand.cur), pointer", &ctx).expect("cursor url");
    assert_eq!(s.cursor, Cursor::Other);
    compute(&mut s, Longhand::PointerEvents, "none", &ctx).expect("pointer-events");
    assert_eq!(s.pointer_events, PointerEvents::None);
    compute(&mut s, Longhand::UserSelect, "none", &ctx).expect("user-select");
    assert!(s.user_select_none);
    compute(&mut s, Longhand::TextShadow, "2px 4px 6px #000000", &ctx).expect("text-shadow");
    assert_eq!(
        s.text_shadow,
        vec![Shadow {
            offset_x: 2.0,
            offset_y: 4.0,
            blur: 6.0,
            spread: 0.0,
            color: Rgba::BLACK,
            inset: false,
        }]
    );
}

// ---------------------------------------------------------------------------
// 8. Backgrounds, Gradients, Shadows, Filter, Overflow
// ---------------------------------------------------------------------------

#[test]
fn backgrounds_gradients_shadows_and_filters() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    apply_decl(
        &mut s,
        "background",
        "linear-gradient(to right, #ff0000 0%, #0000ff 100%) center / cover no-repeat, #112233",
        &ctx,
    )
    .expect("background shorthand");
    assert_eq!(s.background_color, Rgba::new(0x11, 0x22, 0x33, 255));
    assert_eq!(s.background_layers.len(), 2);
    assert_eq!(
        s.background_layers[0].image,
        Some(Image::LinearGradient {
            angle_deg: 90.0,
            stops: vec![
                ColorStop {
                    color: Rgba::new(255, 0, 0, 255),
                    position: Some(LengthPercentage::percent(0.0)),
                },
                ColorStop {
                    color: Rgba::new(0, 0, 255, 255),
                    position: Some(LengthPercentage::percent(100.0)),
                },
            ],
            repeating: false,
        })
    );
    assert_eq!(
        s.background_layers[0].position_x,
        LengthPercentage::percent(50.0)
    );
    assert_eq!(
        s.background_layers[0].position_y,
        LengthPercentage::percent(50.0)
    );
    assert_eq!(s.background_layers[0].size, BackgroundSize::Cover);
    assert_eq!(s.background_layers[0].repeat, BackgroundRepeat::NoRepeat);

    compute(
        &mut s,
        Longhand::BoxShadow,
        "inset 0 2px 4px 1px rgba(0, 0, 0, 0.5)",
        &ctx,
    )
    .expect("box-shadow");
    assert_eq!(s.box_shadow.len(), 1);
    assert!(s.box_shadow[0].inset);
    assert_eq!(s.box_shadow[0].offset_y, 2.0);
    assert_eq!(s.box_shadow[0].blur, 4.0);
    assert_eq!(s.box_shadow[0].spread, 1.0);

    compute(&mut s, Longhand::Opacity, "50%", &ctx).expect("opacity");
    assert!((s.opacity - 0.5).abs() < 1e-5);

    apply_decl(&mut s, "overflow", "hidden scroll", &ctx).expect("overflow");
    assert_eq!(s.overflow_x, Overflow::Hidden);
    assert_eq!(s.overflow_y, Overflow::Scroll);

    compute(&mut s, Longhand::ObjectFit, "scale-down", &ctx).expect("object-fit");
    assert_eq!(s.object_fit, ObjectFit::ScaleDown);

    compute(
        &mut s,
        Longhand::Filter,
        "blur(4px) brightness(120%) drop-shadow(2px 3px 4px #000)",
        &ctx,
    )
    .expect("filter");
    assert!(s.filter.is_some());
}

// ---------------------------------------------------------------------------
// 9. Transforms (`translate`, `rotate`, `scale`, `transform`, `transform-origin`)
// ---------------------------------------------------------------------------

#[test]
fn transforms_and_individual_transform_order() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    // Compute in priority order: Translate, Rotate, Scale, Transform
    let mut decls = vec![
        (Longhand::Transform, "skewX(15deg)"),
        (Longhand::Scale, "2 0.5"),
        (Longhand::Rotate, "0.25turn"),
        (Longhand::Translate, "10px 20%"),
    ];
    decls.sort_by_key(|(l, _)| priority(*l));
    for (l, v) in decls {
        compute(&mut s, l, v, &ctx).expect("transform decl");
    }

    assert_eq!(
        s.transform,
        vec![
            TransformFn::Translate(LengthPercentage::px(10.0), LengthPercentage::percent(20.0),),
            TransformFn::Rotate(90.0),
            TransformFn::Scale(2.0, 0.5),
            TransformFn::Skew(15.0, 0.0),
        ]
    );

    compute(&mut s, Longhand::TransformOrigin, "right bottom", &ctx).expect("transform-origin");
    assert_eq!(
        s.transform_origin,
        (
            LengthPercentage::percent(100.0),
            LengthPercentage::percent(100.0),
        )
    );
}

// ---------------------------------------------------------------------------
// 10. Transitions & Animations (shorthands, list cycling, easing)
// ---------------------------------------------------------------------------

#[test]
fn transitions_and_animations_parsing() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    apply_decl(
        &mut s,
        "transition",
        "opacity 200ms ease-in 50ms, transform 0.5s cubic-bezier(0.4, 0, 0.2, 1)",
        &ctx,
    )
    .expect("transition shorthand");
    assert_eq!(
        s.transitions,
        vec![
            Transition {
                property: "opacity".to_owned(),
                duration_ms: 200.0,
                delay_ms: 50.0,
                easing: Easing::CubicBezier(0.42, 0.0, 1.0, 1.0),
            },
            Transition {
                property: "transform".to_owned(),
                duration_ms: 500.0,
                delay_ms: 0.0,
                easing: Easing::CubicBezier(0.4, 0.0, 0.2, 1.0),
            },
        ]
    );

    apply_decl(
        &mut s,
        "animation",
        "300ms ease-out 100ms infinite alternate both paused fade-in",
        &ctx,
    )
    .expect("animation shorthand");
    assert_eq!(
        s.animations,
        vec![Animation {
            name: "fade-in".to_owned(),
            duration_ms: 300.0,
            delay_ms: 100.0,
            easing: Easing::CubicBezier(0.0, 0.0, 0.58, 1.0),
            iteration_count: f32::INFINITY,
            alternate: true,
            reverse: false,
            fill_forwards: true,
            fill_backwards: true,
            paused: true,
        }]
    );

    // List cycling when animation-name has 2 items and animation-duration has 1
    let mut s2 = ComputedStyle::default();
    compute(&mut s2, Longhand::AnimationName, "slide, pulse", &ctx).expect("animation-name");
    compute(&mut s2, Longhand::AnimationDuration, "400ms", &ctx).expect("animation-duration");
    assert_eq!(s2.animations.len(), 2);
    assert_eq!(s2.animations[0].name, "slide");
    assert_eq!(s2.animations[0].duration_ms, 400.0);
    assert_eq!(s2.animations[1].name, "pulse");
    assert_eq!(s2.animations[1].duration_ms, 400.0);
}

#[test]
fn atomic_shorthand_validation() {
    assert!(expand("margin", "10px bogus").is_err());
    assert!(expand("padding", "12px -4px").is_err());
    assert!(expand("margin", "10px inherit").is_err());
    assert!(expand("margin", "1px 2px 3px 4px").is_ok());
}

#[test]
fn flex_unitless_zero_basis() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    apply_decl(&mut s, "flex", "1 1 0", &ctx).expect("flex: 1 1 0");
    assert_eq!(s.flex_grow, 1.0);
    assert_eq!(s.flex_shrink, 1.0);
    assert_eq!(s.flex_basis, Size::Length(Length::px(0.0)));

    apply_decl(&mut s, "flex", "0 0 0", &ctx).expect("flex: 0 0 0");
    assert_eq!(s.flex_grow, 0.0);
    assert_eq!(s.flex_shrink, 0.0);
    assert_eq!(s.flex_basis, Size::Length(Length::px(0.0)));

    apply_decl(&mut s, "flex", "1 0", &ctx).expect("flex: 1 0");
    assert_eq!(s.flex_grow, 1.0);
    assert_eq!(s.flex_shrink, 0.0);
    assert_eq!(s.flex_basis, Size::Length(Length::px(0.0)));
}

#[test]
fn border_color_transparent_vs_unset() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);

    let mut s1 = ComputedStyle::default();
    compute(&mut s1, Longhand::Color, "red", &ctx).expect("color: red");
    compute(&mut s1, Longhand::BorderTopColor, "transparent", &ctx)
        .expect("border-top-color: transparent");
    compute(&mut s1, Longhand::BorderTopStyle, "solid", &ctx).expect("border-top-style: solid");
    assert_eq!(s1.border.top.color, Rgba::TRANSPARENT);

    let mut s2 = ComputedStyle::default();
    compute(&mut s2, Longhand::Color, "red", &ctx).expect("color: red");
    compute(&mut s2, Longhand::BorderTopStyle, "solid", &ctx).expect("border-top-style: solid");
    assert_eq!(s2.border.top.color, Rgba::new(255, 0, 0, 255));
}

#[test]
fn border_width_before_and_after_style() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);

    let mut s1 = ComputedStyle::default();
    compute(&mut s1, Longhand::BorderTopWidth, "10px", &ctx).expect("border-top-width: 10px");
    compute(&mut s1, Longhand::BorderTopStyle, "solid", &ctx).expect("border-top-style: solid");
    assert_eq!(s1.border.top.width, 10.0);

    let mut s2 = ComputedStyle::default();
    compute(&mut s2, Longhand::BorderTopStyle, "solid", &ctx).expect("border-top-style: solid");
    compute(&mut s2, Longhand::BorderTopWidth, "10px", &ctx).expect("border-top-width: 10px");
    assert_eq!(s2.border.top.width, 10.0);

    let mut s3 = ComputedStyle::default();
    compute(&mut s3, Longhand::BorderTopWidth, "10px", &ctx).expect("border-top-width: 10px");
    compute(&mut s3, Longhand::BorderTopStyle, "none", &ctx).expect("border-top-style: none");
    assert_eq!(s3.border.top.width, 0.0);

    let mut s4 = ComputedStyle::default();
    compute(&mut s4, Longhand::BorderTopStyle, "none", &ctx).expect("border-top-style: none");
    compute(&mut s4, Longhand::BorderTopWidth, "10px", &ctx).expect("border-top-width: 10px");
    assert_eq!(s4.border.top.width, 0.0);
}

#[test]
fn inherited_color_updates_current_color_borders_but_not_explicit_ones() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);

    let mut parent = ComputedStyle::default();
    compute(&mut parent, Longhand::Color, "blue", &ctx).expect("parent color");

    let mut s = ComputedStyle::default();
    compute(&mut s, Longhand::BorderTopStyle, "solid", &ctx).expect("border-top-style");
    compute(&mut s, Longhand::BorderLeftStyle, "solid", &ctx).expect("border-left-style");
    compute(&mut s, Longhand::BorderLeftColor, "green", &ctx).expect("border-left-color");
    // `color: inherit` reaches the style through `copy`, not `compute`.
    copy(&mut s, &parent, Longhand::Color);
    assert_eq!(s.color, Rgba::new(0, 0, 255, 255));
    assert_eq!(s.border.top.color, Rgba::new(0, 0, 255, 255));
    assert_eq!(s.border.left.color, Rgba::new(0, 128, 0, 255));
}

#[test]
fn copy_sub_properties_preserves_siblings() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let default_style = ComputedStyle::default();

    let mut bg = ComputedStyle::default();
    compute(&mut bg, Longhand::BackgroundImage, "url(hero.png)", &ctx).expect("bg image");
    compute(&mut bg, Longhand::BackgroundRepeat, "no-repeat", &ctx).expect("bg repeat");
    copy(&mut bg, &default_style, Longhand::BackgroundRepeat);
    assert_eq!(bg.background_layers.len(), 1);
    assert_eq!(
        bg.background_layers[0].image,
        Some(Image::Url("hero.png".to_owned()))
    );
    assert_eq!(bg.background_layers[0].repeat, BackgroundRepeat::Repeat);

    let mut tf = ComputedStyle::default();
    compute(&mut tf, Longhand::Translate, "10px 20px", &ctx).expect("translate");
    compute(&mut tf, Longhand::Rotate, "45deg", &ctx).expect("rotate");
    copy(&mut tf, &default_style, Longhand::Rotate);
    assert_eq!(
        tf.transform,
        vec![TransformFn::Translate(
            LengthPercentage::px(10.0),
            LengthPercentage::px(20.0),
        )]
    );

    let mut tr = ComputedStyle::default();
    compute(&mut tr, Longhand::TransitionProperty, "opacity", &ctx).expect("transition-property");
    compute(&mut tr, Longhand::TransitionDuration, "200ms", &ctx).expect("transition-duration");
    copy(&mut tr, &default_style, Longhand::TransitionDuration);
    assert_eq!(tr.transitions.len(), 1);
    assert_eq!(tr.transitions[0].property, "opacity");
    assert_eq!(tr.transitions[0].duration_ms, 0.0);

    let mut anim = ComputedStyle::default();
    compute(&mut anim, Longhand::AnimationName, "fade", &ctx).expect("animation-name");
    compute(&mut anim, Longhand::AnimationDuration, "300ms", &ctx).expect("animation-duration");
    copy(&mut anim, &default_style, Longhand::AnimationDuration);
    assert_eq!(anim.animations.len(), 1);
    assert_eq!(anim.animations[0].name, "fade");
    assert_eq!(anim.animations[0].duration_ms, 0.0);
}

#[test]
fn transform_replaces_and_keeps_individual_transforms_independent() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);

    let mut s1 = ComputedStyle::default();
    compute(&mut s1, Longhand::Transform, "skewX(15deg)", &ctx).expect("skewX");
    compute(&mut s1, Longhand::Transform, "none", &ctx).expect("transform: none");
    assert!(s1.transform.is_empty());

    let mut s2 = ComputedStyle::default();
    compute(&mut s2, Longhand::Transform, "skewX(15deg)", &ctx).expect("skewX");
    compute(&mut s2, Longhand::Transform, "rotate(30deg)", &ctx).expect("rotate(30deg)");
    assert_eq!(s2.transform, vec![TransformFn::Rotate(30.0)]);

    let mut s3 = ComputedStyle::default();
    compute(&mut s3, Longhand::Transform, "translate(5px)", &ctx).expect("transform translate");
    compute(&mut s3, Longhand::Translate, "10px", &ctx).expect("individual translate");
    assert_eq!(
        s3.transform,
        vec![
            TransformFn::Translate(LengthPercentage::px(10.0), LengthPercentage::ZERO),
            TransformFn::Translate(LengthPercentage::px(5.0), LengthPercentage::ZERO),
        ]
    );

    let mut s4 = ComputedStyle::default();
    compute(&mut s4, Longhand::Rotate, "10deg", &ctx).expect("individual rotate");
    compute(&mut s4, Longhand::Transform, "rotate(20deg)", &ctx).expect("transform rotate");
    assert_eq!(
        s4.transform,
        vec![TransformFn::Rotate(10.0), TransformFn::Rotate(20.0)]
    );
}

#[test]
fn min_max_clamp_with_percentages_preserved_as_calc_tree() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    compute(
        &mut s,
        Longhand::BorderTopLeftRadius,
        "min(50%, 20px)",
        &ctx,
    )
    .expect("border-top-left-radius with min()");
    assert_eq!(
        s.border_radius_calc.top_left,
        Some(CalcNode::Min(vec![
            CalcNode::Leaf(LengthPercentage::percent(50.0)),
            CalcNode::Leaf(LengthPercentage::px(20.0)),
        ]))
    );

    compute(
        &mut s,
        Longhand::Transform,
        "translateX(clamp(10px, 50%, 100px))",
        &ctx,
    )
    .expect("transform translateX with clamp()");
    assert_eq!(
        s.transform,
        vec![TransformFn::TranslateCalc(
            Length::Calc(CalcNode::Clamp(
                Box::new(CalcNode::Leaf(LengthPercentage::px(10.0))),
                Box::new(CalcNode::Leaf(LengthPercentage::percent(50.0))),
                Box::new(CalcNode::Leaf(LengthPercentage::px(100.0))),
            )),
            Length::Linear(LengthPercentage::ZERO),
        )]
    );
}

#[test]
fn grid_template_areas_dots_and_adjacent_tokens() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    compute(&mut s, Longhand::GridTemplateAreas, "\"a . b\"", &ctx).expect("single dot");
    assert_eq!(
        s.grid_template_areas,
        vec![vec![Some("a".to_owned()), None, Some("b".to_owned()),]]
    );

    compute(&mut s, Longhand::GridTemplateAreas, "\"a .. b\"", &ctx).expect("double dot");
    assert_eq!(
        s.grid_template_areas,
        vec![vec![Some("a".to_owned()), None, Some("b".to_owned()),]]
    );

    compute(&mut s, Longhand::GridTemplateAreas, "\".a\"", &ctx).expect("dot before ident");
    assert_eq!(
        s.grid_template_areas,
        vec![vec![None, Some("a".to_owned())]]
    );

    compute(&mut s, Longhand::GridTemplateAreas, "\"a.b\"", &ctx).expect("dot between idents");
    assert_eq!(
        s.grid_template_areas,
        vec![vec![Some("a".to_owned()), None, Some("b".to_owned()),]]
    );
}

#[test]
fn border_radius_rejects_multiple_slashes() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    assert!(apply_decl(&mut s, "border-radius", "10px / 20px / 30px", &ctx).is_err());
}

#[test]
fn negative_calc_in_non_negative_properties() {
    let vp = Viewport::default();
    let ctx = default_ctx(&vp);
    let mut s = ComputedStyle::default();

    compute(&mut s, Longhand::PaddingTop, "calc(10% - 30%)", &ctx)
        .expect("padding-top negative percentage calc");
    assert_eq!(s.padding.top, Length::Linear(LengthPercentage::ZERO));

    compute(&mut s, Longhand::BorderTopLeftRadius, "calc(-20%)", &ctx)
        .expect("border-top-left-radius negative percentage calc");
    assert_eq!(s.border_radius.top_left, LengthPercentage::ZERO);
    assert_eq!(s.border_radius_calc.top_left, None);

    compute(&mut s, Longhand::PaddingTop, "calc(10px - 5%)", &ctx)
        .expect("padding-top mixed-sign calc");
    assert_eq!(
        s.padding.top,
        Length::Calc(CalcNode::Max(vec![
            CalcNode::Leaf(LengthPercentage::ZERO),
            CalcNode::Leaf(LengthPercentage {
                px: 10.0,
                percent: -5.0,
                vw: 0.0,
                vh: 0.0,
            }),
        ]))
    );

    compute(&mut s, Longhand::PaddingTop, "calc(20px + 5%)", &ctx)
        .expect("padding-top positive calc");
    assert_eq!(
        s.padding.top,
        Length::Linear(LengthPercentage {
            px: 20.0,
            percent: 5.0,
            vw: 0.0,
            vh: 0.0,
        })
    );

    compute(&mut s, Longhand::MarginTop, "calc(10% - 30%)", &ctx)
        .expect("margin-top negative percentage calc");
    assert_eq!(
        s.margin.top,
        LengthAuto::Length(Length::Linear(LengthPercentage::percent(-20.0)))
    );
}
