//! An inline formatting context → one `Text` (layout.md §2).
//!
//! The block's own font and colour go on the `Text`; inline boxes that
//! differ from their parent become StyledText tags. What StyledText cannot
//! draw (inline backgrounds, other font families, `sub`/`sup`) is reported.
//! QML has no inheritance, so every value the block needs is written out.

use h2q_css::{
    ComputedStyle, Direction, FontStyle, LineHeight, Rgba, TextAlign, TextDecorationLine,
    TextTransform, VerticalAlign, WhiteSpace, WordBreak,
};
use h2q_qml::{Member, Object, Value};

use super::Emitter;
use super::units::{color, number, round};
use crate::boxes::{Inline, Span, describe, node_line};

/// Ascent + descent of the common fallback fonts (Liberation, DejaVu) as a
/// fraction of the font size; used to centre glyphs in the line box.
pub(super) const CONTENT_AREA: f32 = 1.15;

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
    /// Preserved spaces must survive StyledText's own collapsing.
    preserve: bool,
    prev_space: bool,
    link_colors: Vec<Rgba>,
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
            preserve: !matches!(style.white_space, WhiteSpace::Normal | WhiteSpace::NoWrap),
            prev_space: true,
            link_colors: Vec::new(),
        };
        self.markup(items, &state, &mut markup);

        if markup.rich {
            text.set("text", Value::String(markup.styled));
            text.set("textFormat", Value::expr("Text.StyledText"));
        } else {
            text.set("text", Value::String(markup.plain));
            text.set("textFormat", Value::expr("Text.PlainText"));
        }
        self.font(&mut text, style, decoration, line);
        if let Some(&link) = markup.link_colors.first() {
            text.set("linkColor", color(link));
            if markup.link_colors.iter().any(|&c| c != link) {
                self.diags.info(
                    "qml",
                    line,
                    "links with different colours in one paragraph all use the first link's colour"
                        .to_owned(),
                );
            }
            text.members.push(Member::Handler {
                name: "onLinkActivated".to_owned(),
                params: vec!["link".to_owned()],
                body: vec!["Qt.openUrlExternally(link)".to_owned()],
            });
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
        let line_height = match style.line_height {
            LineHeight::Normal => None,
            LineHeight::Number(n) => Some(n * style.font_size),
            LineHeight::Px(px) => Some(px),
        };
        if let Some(lh) = line_height {
            text.set("lineHeight", number(lh));
            text.set("lineHeightMode", Value::expr("Text.FixedHeight"));
            // CSS centres the glyphs in the line box (half-leading); a fixed
            // QML line puts them at its top.
            let shift = round((lh - CONTENT_AREA * style.font_size) / 2.0);
            if shift.abs() >= 0.5 {
                text.set(
                    "transform",
                    Value::Object(Box::new(Object::new("Translate").prop("y", shift))),
                );
            }
        }
        let rtl = style.direction == Direction::Rtl;
        let align = match style.text_align {
            TextAlign::Start if rtl => Some("Text.AlignRight"),
            TextAlign::End if !rtl => Some("Text.AlignRight"),
            TextAlign::Start | TextAlign::End | TextAlign::Left => None,
            TextAlign::Right => Some("Text.AlignRight"),
            TextAlign::Center => Some("Text.AlignHCenter"),
            TextAlign::Justify => Some("Text.AlignJustify"),
        };
        if let Some(a) = align {
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

        let href = span
            .node
            .and_then(|n| self.doc.element(n))
            .filter(|e| e.is_html() && e.local_name() == "a")
            .and_then(|e| e.attr("href"));
        if let Some(href) = href
            && !parent.in_link
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
        if (style.font_size - parent.font_size).abs() > 0.5 {
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
        if family != parent.family {
            self.diags.info(
                "qml",
                line,
                format!(
                    "{what}: font family `{family}` inside `{}` text is not converted",
                    parent.family
                ),
            );
        }
        if !style.background_color.is_transparent() || !style.background_layers.is_empty() {
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
                    h2q_css::BorderStyle::None | h2q_css::BorderStyle::Hidden
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
        if boxed || padded {
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

        if !open.is_empty() {
            m.rich = true;
        }
        for o in &open {
            m.styled.push_str(o);
        }
        self.markup(&span.children, &state, m);
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
