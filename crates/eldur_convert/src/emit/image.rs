//! `<img>` → `Image`, sized as CSS sizes a replaced element (CSS 2.2
//! §10.3.2, §10.6.2): a given width or height keeps the intrinsic ratio,
//! both stretch the image (`object-fit: fill`).

use eldur_css::{
    BoxSizing, ComputedStyle, HtmlDimension, LengthPercentage, ObjectFit, Rgba, Size,
    html_dimension,
};
use eldur_dom::{NodeId, collapse_whitespace};
use eldur_qml::{Object, Value};

use super::accessible;
use super::flow::{Emitted, MarginSet};
use super::units::{fmt_number, number, round};
use super::{Emitter, hint};
use crate::boxes::LayoutBox;
use crate::images::{ImageAsset, svg_natural};

impl Emitter<'_, '_> {
    pub(super) fn image(&mut self, b: &LayoutBox<'_>, node: NodeId, line: Option<u32>) -> Emitted {
        let style = &*b.style;
        let element = self.doc.element(node).expect("an img is an element");
        let inline_svg = element.is_svg();
        let insets = self.insets(style, line);
        let (extra_w, extra_h) = match style.box_sizing {
            BoxSizing::ContentBox => (0.0, 0.0),
            BoxSizing::BorderBox => (insets.horizontal(), insets.vertical()),
        };
        // The `width`/`height` attributes reached the cascade as
        // presentational hints, so `style` already holds them unless an
        // author rule beat them; here they only give a ratio to an image
        // whose file has none. Both go through the same dimension rule
        // (HTML §15.4.3), so `80junk` is 80 in each; a percentage is no ratio.
        let attr_px = |name: &str| match html_dimension(element.attr(name)?)? {
            HtmlDimension::Length(px) => Some(px),
            HtmlDimension::Percentage(_) => None,
        };
        let css_px = |s: &Size, extra: f32| match s {
            Size::Length(l) => l.as_px().map(|px| (px - extra).max(0.0)),
            _ => None,
        };
        let width = css_px(&style.width, extra_w);
        let height = css_px(&style.height, extra_h);
        // An inline svg's file is written at the size CSS gives it, when CSS
        // gives both; a file has no box of its own to fill.
        let asset = if inline_svg {
            self.svg_asset(node, style.color, width.zip(height))
        } else {
            self.images.get(&node).cloned()
        };
        let natural = asset.as_ref().and_then(|a| a.natural);
        let size = natural.and_then(|n| n.size());
        let ratio = natural
            .and_then(|n| n.ratio)
            .or_else(|| {
                // An `<svg>`'s own attributes are presentation attributes,
                // already read into `natural` with its `viewBox` (SVG 2 §8.2).
                if inline_svg {
                    None
                } else {
                    Some(attr_px("height")? / attr_px("width")?)
                }
            })
            .filter(|r| r.is_finite() && *r > 0.0);
        // A read file or an inline svg lacking a natural width or height
        // takes the default object size's, 300×150, for it (CSS 2.2 §10.3.2,
        // §10.6.2): `<svg width="80" style="width:200px">` is 200×150. A file
        // that was not read has no size of its own until Qt loads it.
        let defaults = natural.is_some();
        let (nw, nh) = natural.map_or((None, None), |n| (n.width, n.height));
        let default_w = nw.unwrap_or(if defaults { 300.0 } else { 0.0 });
        let default_h = nh.unwrap_or(if defaults { 150.0 } else { 0.0 });
        let content = match (width, height, ratio) {
            (Some(w), Some(h), _) => Some((w, h)),
            (Some(w), None, Some(r)) => Some((w, w * r)),
            (None, Some(h), Some(r)) => Some((h / r, h)),
            (Some(w), None, None) => Some((w, default_h)),
            (None, Some(h), None) => Some((default_w, h)),
            (None, None, _) if size.is_some() => size,
            (None, None, Some(r)) => match (nw, nh) {
                (Some(w), _) => Some((w, w * r)),
                (None, Some(h)) => Some((h / r, h)),
                // Only a ratio: the width is the layout's.
                (None, None) => None,
            },
            (None, None, None) => defaults.then_some((default_w, default_h)),
        };
        // A percentage or `max-width`, or no intrinsic size, makes the
        // width the layout's: the height follows it through the ratio.
        let percent_width = matches!(&style.width, Size::Length(l) if l.as_px().is_none());
        let fills = content.is_none() && width.is_none();
        let follows =
            height.is_none() && (percent_width || fills || !matches!(style.max_width, Size::None));

        let mut image = Object::new("Image");
        if let Some(a) = &asset {
            image.set("source", Value::string(a.source.clone()));
        } else if !inline_svg && element.attr("src").is_none_or(|s| s.trim().is_empty()) {
            // A missing file was reported when the files were read.
            let what = super::describe(self.doc, node);
            self.diags.warning(
                "html",
                line,
                format!("{what} has no `src`; an empty placeholder keeps its space"),
            );
        }
        let fill_mode = match style.object_fit {
            ObjectFit::Fill => None,
            ObjectFit::Contain => Some("Image.PreserveAspectFit"),
            ObjectFit::Cover => Some("Image.PreserveAspectCrop"),
            ObjectFit::None => Some("Image.Pad"),
            ObjectFit::ScaleDown => None,
        };
        if let Some(mode) = fill_mode {
            image.set("fillMode", Value::expr(mode));
        }
        if asset.as_ref().is_some_and(|a| is_svg(&a.source)) {
            // An SVG is drawn at its source size and then scaled: render it
            // at the size it is shown so that it stays sharp.
            image.set("sourceSize.width", Value::expr("width"));
            image.set("sourceSize.height", Value::expr("height"));
        }
        if inline_svg {
            self.svg_accessible(&mut image, node);
        } else if let Some(alt) = element.attr("alt").filter(|a| !a.is_empty()) {
            image.set("Accessible.name", Value::string(alt.to_owned()));
        }
        image.set("anchors.fill", Value::expr("parent"));
        for (name, v) in [
            ("anchors.leftMargin", insets.left),
            ("anchors.rightMargin", insets.right),
            ("anchors.topMargin", insets.top),
            ("anchors.bottomMargin", insets.bottom),
        ] {
            if v != 0.0 {
                image.set(name, number(v));
            }
        }

        // The image is a child of its box: an `Image` sets its own implicit
        // size when it loads, which would undo the CSS size.
        let mut deco = self.decoration(style, true, line);
        deco.hint = hint(self.doc, b);
        let rounded = {
            let r = &style.border_radius;
            [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left]
                .iter()
                .any(|c| !c.is_zero())
        };
        // An image is clipped to its rounded border box (CSS Backgrounds 3
        // §5.3).
        deco.clips |= rounded;
        // `Image` has no rounded corners, no `object-position` and no
        // scale-down; a background layer does all three in one draw. An SVG
        // has to stay an `Image`, which renders it at the size it is shown.
        let placed = asset.as_ref().filter(|a| {
            !is_svg(&a.source)
                && (rounded || style.object_fit == ObjectFit::ScaleDown || !is_centred(style))
        });
        deco.clips &= placed.is_none();
        let shown = match (placed, &asset) {
            (Some(a), _) => {
                let hint = deco.hint.clone();
                let mut drawn = self.image_layer(style, &a.source, &hint, line);
                if let Some(alt) = element.attr("alt").filter(|a| !a.is_empty()) {
                    drawn.set("Accessible.name", Value::string(alt.to_owned()));
                }
                vec![drawn]
            }
            (None, Some(_)) => vec![image],
            (None, None) => Vec::new(),
        };
        let mut obj = self.wrap_with(style, &deco, true, None, shown, false, line);
        let (cw, ch) = content.unwrap_or((0.0, 0.0));
        obj.set("implicitWidth", number(cw + insets.horizontal()));
        obj.set("implicitHeight", number(ch + insets.vertical()));
        if fills {
            obj.set("Layout.fillWidth", true);
        }
        if follows && let Some(r) = ratio {
            let inner = if insets.horizontal() == 0.0 {
                "width".to_owned()
            } else {
                format!("(width - {})", fmt_number(round(insets.horizontal())))
            };
            let mut expr = format!("{inner} * {}", fmt_number(f64::from(r)));
            if insets.vertical() != 0.0 {
                expr = format!("{expr} + {}", fmt_number(round(insets.vertical())));
            }
            obj.set("Layout.preferredHeight", Value::expr(expr));
        }
        if let Some(id) = element.id() {
            obj.id = Some(self.claim_html_id(id));
            if self.wanted.contains(&node) {
                obj.set("objectName", Value::String(id.to_owned()));
            }
        }
        self.visibility(&mut obj, style);
        let mt = self.vertical_margin(&style.margin.top, line);
        let mb = self.vertical_margin(&style.margin.bottom, line);
        Emitted {
            obj,
            top: MarginSet::of(mt),
            bottom: MarginSet::of(mb),
            collapse_through: false,
            top_cases: None,
        }
    }
}

impl Emitter<'_, '_> {
    /// The file an inline `<svg>` is shown from: its markup as a document of
    /// its own, with `currentColor` resolved to `color`, the colour of the
    /// element that holds it (SVG 2 §13.2, CSS Color 4 §6.4). The name is a
    /// hash of the content, so each colour the svg takes in some state is a
    /// file of its own and the same icon twice is one file.
    fn svg_asset(
        &mut self,
        node: NodeId,
        color: Rgba,
        used: Option<(f32, f32)>,
    ) -> Option<ImageAsset> {
        let colour = svg_colour(color);
        let resolve = |name: &str, value: &str, own: Option<&str>| {
            if takes_a_colour(name) {
                replace_current_color(value, own.unwrap_or(&colour))
            } else {
                value.to_owned()
            }
        };
        let markup = self.doc.svg_markup(node, used, &resolve)?;
        let source = format!("assets/svg-{:016x}.svg", fnv1a(markup.as_bytes()));
        let natural = svg_natural(&markup);
        self.svgs
            .entry(source.clone())
            .or_insert_with(|| markup.into_bytes());
        Some(ImageAsset {
            source,
            natural: Some(natural),
        })
    }

    /// An `aria-hidden` svg is drawn but not read (WAI-ARIA 1.2 §6.6); one
    /// that is named, by ARIA or by its `<title>` (SVG-AAM §5.2), is a
    /// graphic with that name.
    fn svg_accessible(&self, image: &mut Object, node: NodeId) {
        if accessible::hidden(self.doc, self.styles, node) {
            image.set("Accessible.ignored", true);
            return;
        }
        let title = || {
            self.doc
                .children(node)
                .find(|&c| {
                    self.doc
                        .element(c)
                        .is_some_and(|e| e.is_svg() && e.local_name() == "title")
                })
                .map(|t| collapse_whitespace(&self.doc.text_content(t)))
                .filter(|t| !t.is_empty())
        };
        if let Some(name) = accessible::aria_name(self.doc, self.styles, node).or_else(title) {
            image.set("Accessible.role", Value::expr("Accessible.Graphic"));
            image.set("Accessible.name", Value::string(name));
        }
    }
}

/// `color` as SVG writes it: `#rrggbb`, or `rgba()` when it is translucent.
fn svg_colour(c: Rgba) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!(
            "rgba({},{},{},{})",
            c.r,
            c.g,
            c.b,
            fmt_number(f64::from(c.a) / 255.0)
        )
    }
}

/// `value` with each `currentColor`, in any case, replaced by `colour`.
/// Whether an svg attribute (or `"#style"`, a `<style>` text) can hold a
/// colour, and so `currentColor`: the paint and colour properties (SVG 2
/// §13.2, Filter Effects §9.7, §9.13) and style text. Anywhere else, an
/// `id` or a `<text>` attribute, the word is just a word.
fn takes_a_colour(name: &str) -> bool {
    matches!(
        name,
        "fill"
            | "stroke"
            | "stop-color"
            | "flood-color"
            | "lighting-color"
            | "color"
            | "style"
            | "#style"
    )
}

/// `value` with each `currentColor` keyword, in any case, made `colour`.
/// Only a whole identifier is the keyword (CSS Syntax 3 §4.3.11):
/// `--currentColorX` or `my-currentcolor` stay as they are.
fn replace_current_color(value: &str, colour: &str) -> String {
    const KEY: &str = "currentcolor";
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii();
    let lower = value.to_ascii_lowercase();
    let mut out = String::with_capacity(value.len());
    let mut rest = 0;
    for (at, _) in lower.match_indices(KEY) {
        let end = at + KEY.len();
        if value[..at].chars().next_back().is_some_and(ident)
            || value[end..].chars().next().is_some_and(ident)
        {
            continue;
        }
        out.push_str(&value[rest..at]);
        out.push_str(colour);
        rest = at + KEY.len();
    }
    out.push_str(&value[rest..]);
    out
}

/// FNV-1a: a stable name for a file from its content.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn is_centred(style: &ComputedStyle) -> bool {
    let centre = LengthPercentage::percent(50.0);
    style.object_position_x == centre && style.object_position_y == centre
}

fn is_svg(source: &str) -> bool {
    let path = source.split(['?', '#']).next().unwrap_or(source);
    path.to_ascii_lowercase().ends_with(".svg") || source.starts_with("data:image/svg")
}

/// Whether a `style` gives an image a percentage width.
pub(super) fn is_percent_width(style: &ComputedStyle) -> bool {
    matches!(&style.width, Size::Length(l) if l.as_px().is_none())
}

#[cfg(test)]
mod tests {
    use super::{replace_current_color, takes_a_colour};

    #[test]
    fn only_the_current_color_keyword_is_replaced() {
        assert_eq!(
            replace_current_color("CurrentColor; stroke:currentcolor", "#c00"),
            "#c00; stroke:#c00"
        );
        assert_eq!(
            replace_current_color("var(--currentColorX) my-currentcolor _currentColor", "#c00"),
            "var(--currentColorX) my-currentcolor _currentColor"
        );
        assert_eq!(replace_current_color("(currentColor)", "#c00"), "(#c00)");
    }

    #[test]
    fn only_colour_attributes_take_the_colour() {
        assert!(takes_a_colour("fill") && takes_a_colour("#style") && takes_a_colour("stop-color"));
        assert!(!takes_a_colour("id") && !takes_a_colour("aria-label") && !takes_a_colour("d"));
    }
}
