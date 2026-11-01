//! An inline formatting context → one `Text` (layout.md §2).
//!
//! The block's own font and colour go on the `Text`; inline boxes that
//! differ from their parent become StyledText tags. StyledText has no
//! other font families, inline backgrounds or padding, so a text with such
//! a run (inline `<code>`) is RichText, with the line height on its block
//! since RichText ignores `lineHeight`. What neither draws is reported.
//! QML has no inheritance, so every value the block needs is written out.

use eldur_css::{
    ComputedStyle, Direction, FontStyle, LineHeight, Rgba, TextAlign, TextDecorationLine,
    TextTransform, VerticalAlign, WhiteSpace, WordBreak,
};
use eldur_dom::{Document, NodeId};
use eldur_qml::{Member, Object, Value};

use super::Emitter;
use super::units::{color, number, round};
use crate::boxes::{Inline, Span, describe, node_line};

/// Ascent + descent of the common fallback fonts (Liberation, DejaVu) as a
/// fraction of the font size; used to centre glyphs in the line box.
pub(super) const CONTENT_AREA: f32 = 1.15;

/// Below this line height (as a fraction of the font size) a fallback
/// font's own height may exceed the line: Liberation and DejaVu are
/// 1.13–1.17 in Qt, so this leaves a margin for other families.
const TIGHT_LINE: f32 = 1.25;

/// What StyledText needs to know about the enclosing inline box.
#[derive(Clone)]
struct State {
    bold: bool,
    italic: bool,
    color: Rgba,
    decoration: TextDecorationLine,
    font_size: f32,
    family: String,
    in_link: bool,
}

struct Markup {
    styled: String,
    plain: String,
    /// Tags were needed.
    rich: bool,
    /// The tags are RichText's HTML rather than StyledText's.
    html: bool,
    /// Preserved spaces must survive StyledText's own collapsing.
    preserve: bool,
    prev_space: bool,
    link_colors: Vec<Rgba>,
    /// The one link the whole text is, when it is not underlined: it is
    /// written as plain text and the Text opens its `href` when tapped,
    /// since StyledText underlines every link.
    bare_link: Option<(NodeId, String)>,
}

impl Emitter<'_, '_> {
    /// `items` laid out with `style` (the block container's), with text
    /// decorations from ancestor blocks in `decoration`.
    pub(super) fn text(
        &mut self,
        style: &ComputedStyle,
        decoration: TextDecorationLine,
        items: &[Inline<'_>],
        line: Option<u32>,
    ) -> Object {
        let hoisted = whole_span(style, items);
        let (style, raise) = match &hoisted {
            Some((s, raise)) => (s, *raise),
            None => (style, 0.0),
        };
        let mut text = Object::new("Text");
        let state = State {
            bold: style.font_weight >= 600,
            italic: style.font_style != FontStyle::Normal,
            color: style.color,
            decoration,
            font_size: style.font_size,
            family: font_family(&style.font_family),
            in_link: false,
        };
        let mut markup = Markup {
            styled: String::new(),
            plain: String::new(),
            rich: false,
            html: if self.boxes_drawn {
                needs_html_around_boxes(items, &state.family)
            } else {
                needs_html(items, &state.family)
            },
            preserve: !matches!(style.white_space, WhiteSpace::Normal | WhiteSpace::NoWrap),
            prev_space: true,
            link_colors: Vec::new(),
            bare_link: match items {
                [Inline::Span(span)] if !decoration.underline => link_href(self.doc, span)
                    .filter(|_| !span.style.text_decoration_line.underline)
                    .zip(span.node)
                    .map(|(href, node)| (node, href.to_owned())),
                _ => None,
            },
        };
        self.markup(items, &state, &mut markup);
        let fragments = markup.styled.contains("<a href=\"#");

        if markup.html {
            let styled = match line_height(style) {
                Some(lh) => format!(
                    "<div style=\"line-height:{}px\">{}</div>",
                    super::units::fmt_number(round(lh)),
                    markup.styled
                ),
                None => markup.styled,
            };
            text.set("text", Value::String(styled));
            text.set("textFormat", Value::expr("Text.RichText"));
        } else if markup.rich {
            text.set("text", Value::String(markup.styled));
            text.set("textFormat", Value::expr("Text.StyledText"));
        } else {
            text.set("text", Value::String(markup.plain));
            text.set("textFormat", Value::expr("Text.PlainText"));
        }
        self.font(&mut text, style, decoration, line);
        if markup.html {
            // A `lineHeight` on the Text replaces the block's in RichText.
            text.members.retain(|m| {
                !matches!(m, Member::Property { name, .. }
                    if name == "lineHeight" || name == "lineHeightMode")
            });
            // RichText puts a fixed line's leading above the glyphs, where
            // `lineHeight` puts it below; both round to a whole row, so a
            // whole shift lands them on Chromium's (layout.md §1.2).
            if let Some(lh) = line_height(style) {
                let shift = rich_shift(style, items, lh) + raise;
                text.members
                    .retain(|m| !matches!(m, Member::Property { name, .. } if name == "transform"));
                if shift != 0.0 {
                    text.set(
                        "transform",
                        Value::Object(Box::new(Object::new("Translate").prop("y", round(shift)))),
                    );
                }
            }
        }
        if raise != 0.0 && !markup.html {
            let shift = match text.get("transform") {
                Some(Value::Object(t)) => match t.get("y") {
                    Some(Value::Number(y)) => *y as f32,
                    _ => 0.0,
                },
                _ => 0.0,
            };
            text.members
                .retain(|m| !matches!(m, Member::Property { name, .. } if name == "transform"));
            if shift + raise != 0.0 {
                text.set(
                    "transform",
                    Value::Object(Box::new(
                        Object::new("Translate").prop("y", round(shift + raise)),
                    )),
                );
            }
        }
        if let Some(&link) = markup.link_colors.first() {
            // RichText colours each link itself.
            if !markup.html {
                text.set("linkColor", color(link));
            }
            if !markup.html && markup.link_colors.iter().any(|&c| c != link) {
                self.diags.info(
                    "qml",
                    line,
                    "links with different colours in one paragraph all use the first link's colour"
                        .to_owned(),
                );
            }
            self.link_handler(&mut text, fragments);
        }
        if let Some((_, href)) = markup.bare_link {
            let mut tap = Object::new("TapHandler");
            let follow = self.follow(&href);
            // This handler may hear the tap before the one a listener is on.
            let (params, body) = if self.runtime_dom {
                (
                    vec!["eventPoint".to_owned()],
                    vec![
                        "if (Eldur.tapPrevented(eventPoint))".to_owned(),
                        "    return;".to_owned(),
                        follow,
                    ],
                )
            } else {
                (Vec::new(), vec![follow])
            };
            tap.members.push(Member::Handler {
                name: "onTapped".to_owned(),
                params,
                body,
            });
            text.push_child(tap);
            let mut hover = Object::new("HoverHandler");
            hover.set("cursorShape", Value::expr("Qt.PointingHandCursor"));
            text.push_child(hover);
        }
        text
    }

    /// Font, colour, line height, alignment and wrapping of a `Text`.
    pub(super) fn font(
        &mut self,
        text: &mut Object,
        style: &ComputedStyle,
        decoration: TextDecorationLine,
        line: Option<u32>,
    ) {
        text.set("color", color(style.color));
        text.set(
            "font.family",
            Value::string(font_family(&style.font_family)),
        );
        // `font.pixelSize` is an int in Qt.
        text.set(
            "font.pixelSize",
            Value::Number(f64::from(style.font_size).round().max(1.0)),
        );
        if style.font_weight != 400 {
            text.set("font.weight", Value::Number(f64::from(style.font_weight)));
        }
        if style.font_style != FontStyle::Normal {
            text.set("font.italic", true);
        }
        if let Some(spacing) = style.letter_spacing.filter(|s| *s != 0.0) {
            text.set("font.letterSpacing", number(spacing));
        }
        if let Some(spacing) = style.word_spacing.filter(|s| *s != 0.0) {
            text.set("font.wordSpacing", number(spacing));
        }
        let capitalization = match style.text_transform {
            TextTransform::None if style.small_caps => Some("Font.SmallCaps"),
            TextTransform::None => None,
            TextTransform::Uppercase => Some("Font.AllUppercase"),
            TextTransform::Lowercase => Some("Font.AllLowercase"),
            TextTransform::Capitalize => Some("Font.Capitalize"),
        };
        if let Some(c) = capitalization {
            text.set("font.capitalization", Value::expr(c));
        }
        if decoration.underline {
            text.set("font.underline", true);
        }
        if decoration.line_through {
            text.set("font.strikeout", true);
        }
        if decoration.overline {
            text.set("font.overline", true);
        }
        if let Some(lh) = line_height(style) {
            text.set("lineHeight", number(lh));
            text.set("lineHeightMode", Value::expr("Text.FixedHeight"));
            // CSS centres the glyphs in the line box (half-leading); a fixed
            // QML line puts them at its top. Both round the glyphs' baseline
            // to a whole pixel, so a whole shift lands them on Chromium's
            // row wherever the line is.
            let shift = whole_baseline(style, lh) - qt_ascent(style);
            if shift != 0.0 {
                text.set(
                    "transform",
                    Value::Object(Box::new(Object::new("Translate").prop("y", round(shift)))),
                );
            }
            // Qt gives the last fixed-height line the font's own height when
            // that is taller, where CSS keeps every line box `line-height`
            // tall. The font's height is only known at run time, so a
            // `FontMetrics` of the same font measures it; a binding on
            // `contentHeight` instead would loop through the layout's width.
            if lh < TIGHT_LINE * style.font_size {
                let mut metrics = Object::new("FontMetrics");
                for name in [
                    "font.family",
                    "font.pixelSize",
                    "font.weight",
                    "font.italic",
                ] {
                    if let Some(value) = text.get(name) {
                        metrics.set(name, value.clone());
                    }
                }
                text.members.push(Member::PropertyDecl {
                    ty: "FontMetrics".to_owned(),
                    name: "metrics".to_owned(),
                    value: Some(Value::Object(Box::new(metrics))),
                    readonly: true,
                    required: false,
                });
                text.set(
                    "bottomPadding",
                    Value::expr("Math.min(0, lineHeight - Math.ceil(metrics.height))"),
                );
            }
        }
        if let Some(a) = text_align(style) {
            text.set("horizontalAlignment", Value::expr(a));
        }
        let wrap = match style.white_space {
            WhiteSpace::NoWrap | WhiteSpace::Pre => "Text.NoWrap",
            _ if style.word_break == WordBreak::BreakAll => "Text.WrapAnywhere",
            _ if style.overflow_wrap_anywhere || style.word_break == WordBreak::BreakWord => {
                "Text.WrapAtWordBoundaryOrAnywhere"
            }
            _ => "Text.WordWrap",
        };
        text.set("wrapMode", Value::expr(wrap));
        if style.text_indent.as_px() != Some(0.0) {
            self.diags
                .info("css", line, "`text-indent` is not converted".to_owned());
        }
        if !style.text_shadow.is_empty() {
            self.diags
                .info("css", line, "`text-shadow` is not converted".to_owned());
        }
    }

    fn markup(&mut self, items: &[Inline<'_>], state: &State, m: &mut Markup) {
        for item in items {
            match item {
                Inline::Text(t) => push_text(m, t),
                Inline::LineBreak => {
                    m.styled.push_str("<br>");
                    m.plain.push('\n');
                    m.prev_space = true;
                }
                Inline::Span(span) => self.span(span, state, m),
                // A line with boxes is a Flow (see `inline`), not one Text.
                Inline::Atomic(_) => {}
            }
        }
    }

    fn span(&mut self, span: &Span<'_>, parent: &State, m: &mut Markup) {
        let style = span.style;
        let line = span.node.and_then(|n| node_line(self.doc, n));
        let what = span
            .node
            .map_or_else(|| "generated content".to_owned(), |n| describe(self.doc, n));
        let mut state = parent.clone();
        let mut open = Vec::<String>::new();
        let mut close = Vec::<&str>::new();
        let mut tag = |open: &mut Vec<String>, o: String, c: &'static str| {
            open.push(o);
            close.push(c);
        };

        let href = link_href(self.doc, span);
        let bare = m
            .bare_link
            .as_ref()
            .is_some_and(|(node, _)| span.node == Some(*node));
        if let Some(href) = href
            && !parent.in_link
            && !bare
            && m.html
        {
            // RichText would draw a link blue and underlined.
            tag(
                &mut open,
                format!(
                    "<a href=\"{}\" style=\"color:{};text-decoration:none\">",
                    escape_attr(href),
                    css_color(style.color)
                ),
                "</a>",
            );
            m.link_colors.push(style.color);
        } else if let Some(href) = href
            && !parent.in_link
            && !bare
        {
            tag(
                &mut open,
                format!("<a href=\"{}\">", escape_attr(href)),
                "</a>",
            );
            state.in_link = true;
            m.link_colors.push(style.color);
            // StyledText underlines links itself.
            state.decoration.underline = true;
            if !style.text_decoration_line.underline && !parent.decoration.underline {
                self.diags.info(
                    "qml",
                    line,
                    format!("{what} is underlined: StyledText always underlines links"),
                );
            }
        }
        let bold = style.font_weight >= 600;
        if bold && !parent.bold {
            tag(&mut open, "<b>".to_owned(), "</b>");
        } else if !bold && parent.bold {
            self.diags.info(
                "qml",
                line,
                format!("{what}: normal weight inside bold text is not converted"),
            );
        }
        state.bold = bold;
        let italic = style.font_style != FontStyle::Normal;
        if italic && !parent.italic {
            tag(&mut open, "<i>".to_owned(), "</i>");
        } else if !italic && parent.italic {
            self.diags.info(
                "qml",
                line,
                format!("{what}: upright text inside italic text is not converted"),
            );
        }
        state.italic = italic;
        let deco = style.text_decoration_line;
        if deco.underline && !state.decoration.underline {
            tag(&mut open, "<u>".to_owned(), "</u>");
            state.decoration.underline = true;
        }
        if deco.line_through && !state.decoration.line_through {
            tag(&mut open, "<s>".to_owned(), "</s>");
            state.decoration.line_through = true;
        }
        if deco.overline && !state.decoration.overline {
            self.diags.info(
                "qml",
                line,
                format!("{what}: `overline` on inline text is not converted"),
            );
        }
        // Inside a link StyledText paints with `linkColor`.
        if style.color != parent.color && !state.in_link {
            let c = super::units::qml_color(style.color).to_qml_string();
            tag(&mut open, format!("<font color=\"{c}\">"), "</font>");
        }
        state.color = style.color;
        let mut css = Vec::<String>::new();
        if m.html && (style.font_size - parent.font_size).abs() > 0.5 {
            // Whole pixels, as `font.pixelSize`.
            css.push(format!("font-size:{}px", style.font_size.round().max(1.0)));
            state.font_size = style.font_size;
        } else if (style.font_size - parent.font_size).abs() > 0.5 {
            let ratio = style.font_size / parent.font_size.max(1.0);
            if ratio < 1.0 {
                tag(&mut open, "<small>".to_owned(), "</small>");
            } else {
                tag(&mut open, "<big>".to_owned(), "</big>");
            }
            self.diags.info(
                "qml",
                line,
                format!(
                    "{what}: font size {}px inside {}px text is approximated with <{}>",
                    round(style.font_size),
                    round(parent.font_size),
                    if ratio < 1.0 { "small" } else { "big" }
                ),
            );
            state.font_size = style.font_size;
        }
        let family = font_family(&style.font_family);
        if m.html && family != parent.family {
            css.push(format!("font-family:'{}'", family.replace(['\'', '"'], "")));
            state.family = family;
        } else if family != parent.family {
            self.diags.info(
                "qml",
                line,
                format!(
                    "{what}: font family `{family}` inside `{}` text is not converted",
                    parent.family
                ),
            );
        }
        let drawn = self.boxes_drawn && super::inline::paints_box(style);
        if m.html && !drawn && !style.background_color.is_transparent() {
            css.push(format!(
                "background-color:{}",
                css_color(style.background_color)
            ));
        }
        if !style.background_layers.is_empty()
            || (!m.html && !drawn && !style.background_color.is_transparent())
        {
            self.diags.info(
                "qml",
                line,
                format!("{what}: inline background is not converted"),
            );
        }
        let boxed = [
            &style.border.top,
            &style.border.right,
            &style.border.bottom,
            &style.border.left,
        ]
        .iter()
        .any(|b| {
            b.width > 0.0
                && !matches!(
                    b.style,
                    eldur_css::BorderStyle::None | eldur_css::BorderStyle::Hidden
                )
        });
        let padded = [
            &style.padding.top,
            &style.padding.right,
            &style.padding.bottom,
            &style.padding.left,
        ]
        .iter()
        .any(|p| p.as_px() != Some(0.0));
        let (lead, trail) = (
            inline_space(style, Side::Left),
            inline_space(style, Side::Right),
        );
        let rounded = {
            let r = &style.border_radius;
            [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left]
                .iter()
                .any(|c| !c.is_zero())
        };
        if drawn {
            // `EldurInlineBox` paints it, and the line leaves its room.
        } else if m.html {
            // The space padding and borders take on the line, as a zero-width
            // joiner widened by `letter-spacing`, inside the background.
            if boxed {
                self.diags.info(
                    "qml",
                    line,
                    format!("{what}: inline borders take their space but are not drawn"),
                );
            }
            if rounded && !style.background_color.is_transparent() {
                self.diags.info(
                    "qml",
                    line,
                    format!("{what}: an inline background has square corners"),
                );
            }
            if [&style.padding.left, &style.padding.right]
                .iter()
                .any(|p| p.as_px().is_none())
            {
                self.diags.info(
                    "qml",
                    line,
                    format!("{what}: percentage inline padding is not converted"),
                );
            }
        } else if boxed || padded {
            self.diags.info(
                "qml",
                line,
                format!("{what}: inline padding and borders are not converted"),
            );
        }
        if !matches!(style.vertical_align, VerticalAlign::Baseline) {
            self.diags.info(
                "qml",
                line,
                format!("{what}: `vertical-align` on inline text is not converted"),
            );
        }

        if !css.is_empty() {
            open.push(format!("<span style=\"{}\">", css.join(";")));
            close.push("</span>");
        }
        if !open.is_empty() {
            m.rich = true;
        }
        for o in &open {
            m.styled.push_str(o);
        }
        if m.html && !drawn && lead > 0.0 {
            push_space(m, lead);
        }
        self.markup(&span.children, &state, m);
        if m.html && !drawn && trail > 0.0 {
            push_space(m, trail);
        }
        for c in close.iter().rev() {
            m.styled.push_str(c);
        }
    }
}

fn push_text(m: &mut Markup, text: &str) {
    m.plain.push_str(text);
    for c in text.chars() {
        match c {
            '&' => m.styled.push_str("&amp;"),
            '<' => m.styled.push_str("&lt;"),
            '>' => m.styled.push_str("&gt;"),
            // StyledText collapses runs of spaces, so preserved ones are
            // made non-breaking.
            ' ' if m.preserve && m.prev_space => m.styled.push('\u{a0}'),
            _ => m.styled.push(c),
        }
        m.prev_space = c == ' ';
    }
}

/// `px` of horizontal space that cannot start or end a line.
fn push_space(m: &mut Markup, px: f32) {
    m.rich = true;
    m.styled.push_str(&format!(
        "<span style=\"letter-spacing:{}px\">&#8288;</span>",
        super::units::fmt_number(round(px))
    ));
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// The padding and border width on one side of an inline box.
fn inline_space(style: &ComputedStyle, side: Side) -> f32 {
    let (padding, border) = match side {
        Side::Left => (&style.padding.left, &style.border.left),
        Side::Right => (&style.padding.right, &style.border.right),
    };
    let border = if matches!(
        border.style,
        eldur_css::BorderStyle::None | eldur_css::BorderStyle::Hidden
    ) {
        0.0
    } else {
        border.width
    };
    padding.as_px().unwrap_or(0.0).max(0.0) + border
}

/// When `items` is one inline box that draws nothing of its own, the
/// block's style with that box's font, and how far to move the text down
/// from where `font` puts it. Its text is then the block's own, so Qt
/// applies its `letter-spacing` and `text-transform`; the line box is the
/// strut and the inline box aligned on their baseline (CSS 2.2 §10.8).
pub(super) fn whole_span(
    block: &ComputedStyle,
    items: &[Inline<'_>],
) -> Option<(ComputedStyle, f32)> {
    let [Inline::Span(span)] = items else {
        return None;
    };
    let s = span.style;
    let bordered = [
        &s.border.top,
        &s.border.right,
        &s.border.bottom,
        &s.border.left,
    ]
    .iter()
    .any(|b| {
        b.width > 0.0
            && !matches!(
                b.style,
                eldur_css::BorderStyle::None | eldur_css::BorderStyle::Hidden
            )
    });
    let padded = [
        &s.padding.top,
        &s.padding.right,
        &s.padding.bottom,
        &s.padding.left,
    ]
    .iter()
    .any(|p| p.as_px() != Some(0.0));
    if bordered
        || padded
        || !s.background_color.is_transparent()
        || !s.background_layers.is_empty()
        || s.vertical_align != VerticalAlign::Baseline
        || span.children.iter().any(|c| matches!(c, Inline::Atomic(_)))
    {
        return None;
    }
    let mut style = block.clone();
    style.font_size = s.font_size;
    style.font_family.clone_from(&s.font_family);
    style.font_weight = s.font_weight;
    style.font_style = s.font_style;
    style.small_caps = s.small_caps;
    style.color = s.color;
    style.letter_spacing = s.letter_spacing;
    style.word_spacing = s.word_spacing;
    style.text_transform = s.text_transform;
    style.line_height = s.line_height;
    // Ascent and descent of each inline box, half-leading included.
    let metrics = |st: &ComputedStyle| {
        let lh = super::inline::line_height(st);
        let ascent = super::inline::text_baseline(st);
        (ascent, lh - ascent)
    };
    let (strut_up, strut_down) = metrics(block);
    let (span_up, span_down) = metrics(s);
    let up = strut_up.max(span_up);
    let line = up + strut_down.max(span_down);
    if (line - super::inline::line_height(s)).abs() < 0.5 {
        return Some((style, 0.0));
    }
    style.line_height = LineHeight::Px(line);
    // `font` puts the glyphs on the line's own baseline; Chromium's is the
    // taller box's, each whole (`whole_baseline`).
    let up = whole_baseline(block, super::inline::line_height(block))
        .max(whole_baseline(s, super::inline::line_height(s)));
    Some((style, up - whole_baseline(s, line)))
}

/// Ascent and descent, as fractions of the size, of the font `family`
/// resolves to. A stopgap, and the one place that knows: it is right only
/// where fontconfig resolves these names to Liberation Serif, Liberation Sans
/// and, for `monospace`, JetBrains Mono, as on the machine it was measured on
/// (layout.md §1.2). Any other font gets Liberation Sans's numbers, and its
/// glyphs can land a row off. How to obtain real metrics, also for
/// `line-height: normal`, is undecided; replace this function then.
pub(super) fn font_extents(family: &str) -> (f32, f32) {
    match family {
        "serif" | "Times" | "Times New Roman" | "Georgia" | "Liberation Serif" => (0.891, 0.216),
        "monospace" => (1.02, 0.3),
        _ => (0.905, 0.212),
    }
}

/// Chromium's baseline in a `line` px line box of `style`'s font, from the
/// line's top. It rounds the font's ascent and descent, and floors the
/// half-leading, so the offset is whole and the glyphs' row is the line's
/// top rounded half up (`tools/visual/paint`, layout.md §1.2).
pub(super) fn whole_baseline(style: &ComputedStyle, line: f32) -> f32 {
    let (a, d) = font_extents(&font_family(&style.font_family));
    let ascent = (a * style.font_size).round();
    let descent = (d * style.font_size).round();
    // In Chromium's 1/64 px units, so that 19.2 - 17 is not just under 2.2.
    let line = (line * 64.0).round() / 64.0;
    ascent + ((line - ascent - descent) / 2.0 + 1e-4).floor()
}

/// Where a fixed-height Qt line puts the baseline: its font's ascent at the
/// whole `font.pixelSize`, rounded.
fn qt_ascent(style: &ComputedStyle) -> f32 {
    let (a, _) = font_extents(&font_family(&style.font_family));
    (a * style.font_size.round().max(1.0)).round()
}

/// Chromium's baseline less Qt's, for the RichText line holding every run in
/// `items`. Chromium aligns each inline box's whole baseline on the line's;
/// Qt puts a fixed line's bottom at `lh`, its descent being its tallest
/// run's, and when the runs do not fit it grows the line down from their
/// ascent. A paragraph has one shift, so a line without its tallest run can
/// land a row off (layout.md §1.2).
fn rich_shift(style: &ComputedStyle, items: &[Inline<'_>], lh: f32) -> f32 {
    let mut runs = vec![style];
    text_runs(items, &mut runs);
    let chromium = runs
        .iter()
        .map(|s| whole_baseline(s, super::inline::line_height(s)))
        .fold(whole_baseline(style, lh), f32::max);
    let (mut ascent, mut descent) = (0.0_f32, 0.0_f32);
    for s in &runs {
        let (a, d) = font_extents(&font_family(&s.font_family));
        let size = s.font_size.round().max(1.0);
        ascent = ascent.max(a * size);
        descent = descent.max(d * size);
    }
    // In Qt's 1/64 px units.
    let lh = (lh * 64.0).floor() / 64.0;
    let qt = if ascent + descent > lh {
        ascent.round()
    } else {
        (lh - descent).floor()
    };
    chromium - qt
}

/// The style of each inline box in `items` that holds text directly.
fn text_runs<'a>(items: &'a [Inline<'_>], runs: &mut Vec<&'a ComputedStyle>) {
    for item in items {
        if let Inline::Span(span) = item {
            if span.children.iter().any(|c| matches!(c, Inline::Text(_))) {
                runs.push(span.style);
            }
            text_runs(&span.children, runs);
        }
    }
}

/// Whether a run in `items` needs RichText: another font family, a
/// background colour, or horizontal padding or borders.
pub(super) fn needs_html(items: &[Inline<'_>], family: &str) -> bool {
    items.iter().any(|item| match item {
        Inline::Span(span) => {
            let style = span.style;
            let own = font_family(&style.font_family);
            own != family
                || !style.background_color.is_transparent()
                || inline_space(style, Side::Left) > 0.0
                || inline_space(style, Side::Right) > 0.0
                || needs_html(&span.children, &own)
        }
        _ => false,
    })
}

/// As `needs_html`, with what `EldurInlineBox` paints and the line makes room for
/// left out.
fn needs_html_around_boxes(items: &[Inline<'_>], family: &str) -> bool {
    items.iter().any(|item| match item {
        Inline::Span(span) => {
            let own = font_family(&span.style.font_family);
            let bare = !super::inline::paints_box(span.style)
                && (!span.style.background_color.is_transparent()
                    || inline_space(span.style, Side::Left) > 0.0
                    || inline_space(span.style, Side::Right) > 0.0);
            own != family || bare || needs_html_around_boxes(&span.children, &own)
        }
        _ => false,
    })
}

pub(super) fn line_height(style: &ComputedStyle) -> Option<f32> {
    match style.line_height {
        LineHeight::Normal => None,
        LineHeight::Number(n) => Some(n * style.font_size),
        LineHeight::Px(px) => Some(px),
    }
}

/// A colour as RichText's CSS reads it.
fn css_color(c: Rgba) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!(
            "rgba({},{},{},{})",
            c.r,
            c.g,
            c.b,
            super::units::fmt_number(f64::from(c.a) / 255.0)
        )
    }
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// QML's `font` has one family, not a fallback list: the first family that
/// is not an engine-specific alias, with the CSS `ui-*`/`system-ui`
/// aliases mapped to the generic families fontconfig knows.
pub(crate) fn font_family(families: &[String]) -> String {
    for family in families {
        let generic = match family.as_str() {
            "-apple-system" | "BlinkMacSystemFont" => continue,
            "system-ui" | "ui-sans-serif" | "ui-rounded" => "sans-serif",
            "ui-serif" => "serif",
            "ui-monospace" => "monospace",
            other => other,
        };
        return generic.to_owned();
    }
    "serif".to_owned()
}

/// `text-align` as a Qt flag; `None` for the left edge, Qt's default.
pub(super) fn text_align(style: &ComputedStyle) -> Option<&'static str> {
    let rtl = style.direction == Direction::Rtl;
    match style.text_align {
        TextAlign::Start if rtl => Some("Text.AlignRight"),
        TextAlign::End if !rtl => Some("Text.AlignRight"),
        TextAlign::Start | TextAlign::End | TextAlign::Left => None,
        TextAlign::Right => Some("Text.AlignRight"),
        TextAlign::Center => Some("Text.AlignHCenter"),
        TextAlign::Justify => Some("Text.AlignJustify"),
    }
}

/// The `href` of an `<a>` span.
pub(super) fn link_href<'a>(doc: &'a Document, span: &Span<'_>) -> Option<&'a str> {
    span.node
        .and_then(|n| doc.element(n))
        .filter(|e| e.is_html() && e.local_name() == "a")
        .and_then(|e| e.attr("href"))
}
