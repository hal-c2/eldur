//! `<img>` → `Image`, sized as CSS sizes a replaced element (CSS 2.2
//! §10.3.2, §10.6.2): a given width or height keeps the intrinsic ratio,
//! both stretch the image (`object-fit: fill`).

use h2q_css::{BoxSizing, ComputedStyle, ObjectFit, Size};
use h2q_dom::NodeId;
use h2q_qml::{Object, Value};

use super::flow::{Emitted, MarginSet};
use super::units::{fmt_number, number, round};
use super::{Emitter, hint};
use crate::boxes::LayoutBox;

impl Emitter<'_, '_> {
    pub(super) fn image(&mut self, b: &LayoutBox<'_>, node: NodeId, line: Option<u32>) -> Emitted {
        let style = &*b.style;
        let element = self.doc.element(node).expect("an img is an element");
        let asset = self.images.get(&node).cloned();
        let insets = self.insets(style, line);
        let (extra_w, extra_h) = match style.box_sizing {
            BoxSizing::ContentBox => (0.0, 0.0),
            BoxSizing::BorderBox => (insets.horizontal(), insets.vertical()),
        };
        // `width`/`height` attributes are presentational hints, below any
        // CSS value.
        let attr_px = |name: &str| {
            element
                .attr(name)
                .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
        };
        let css_px = |s: &Size, extra: f32| match s {
            Size::Length(l) => l.as_px().map(|px| (px - extra).max(0.0)),
            _ => None,
        };
        let width = css_px(&style.width, extra_w).or_else(|| match style.width {
            Size::Auto => attr_px("width"),
            _ => None,
        });
        let height = css_px(&style.height, extra_h).or_else(|| match style.height {
            Size::Auto => attr_px("height"),
            _ => None,
        });
        let size = asset.as_ref().and_then(|a| a.size);
        let ratio = size
            .map(|(w, h)| h / w)
            .or_else(|| asset.as_ref().and_then(|a| a.ratio))
            .or_else(|| Some(attr_px("height")? / attr_px("width")?))
            .filter(|r| r.is_finite() && *r > 0.0);
        let content = match (width, height, ratio) {
            (Some(w), Some(h), _) => Some((w, h)),
            (Some(w), None, Some(r)) => Some((w, w * r)),
            (None, Some(h), Some(r)) => Some((h / r, h)),
            (None, None, _) => size,
            (Some(w), None, None) => Some((w, size.map_or(0.0, |s| s.1))),
            (None, Some(h), None) => Some((size.map_or(0.0, |s| s.0), h)),
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
        } else if element.attr("src").is_none_or(|s| s.trim().is_empty()) {
            // A missing file was reported when the files were read.
            let what = super::describe(self.doc, node);
            self.diags.warning(
                "html",
                line,
                format!("{what} has no `src`; an empty placeholder keeps its space"),
            );
        }
        let (fill_mode, fit) = match style.object_fit {
            ObjectFit::Fill => (None, None),
            ObjectFit::Contain => (Some("Image.PreserveAspectFit"), None),
            ObjectFit::Cover => (Some("Image.PreserveAspectCrop"), None),
            ObjectFit::None => (Some("Image.Pad"), None),
            ObjectFit::ScaleDown => (Some("Image.PreserveAspectFit"), Some("scale-down")),
        };
        if let Some(mode) = fill_mode {
            image.set("fillMode", Value::expr(mode));
        }
        if let Some(fit) = fit {
            self.diags.info(
                "css",
                line,
                format!("`object-fit: {fit}` is drawn as `contain`"),
            );
        }
        if asset.as_ref().is_some_and(|a| is_svg(&a.source)) {
            // An SVG is drawn at its source size and then scaled: render it
            // at the size it is shown so that it stays sharp.
            image.set("sourceSize.width", Value::expr("width"));
            image.set("sourceSize.height", Value::expr("height"));
        }
        if let Some(alt) = element.attr("alt").filter(|a| !a.is_empty()) {
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
        let shown = if asset.is_some() {
            vec![image]
        } else {
            Vec::new()
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
        }
    }
}

fn is_svg(source: &str) -> bool {
    let path = source.split(['?', '#']).next().unwrap_or(source);
    path.to_ascii_lowercase().ends_with(".svg") || source.starts_with("data:image/svg")
}

/// Whether a `style` gives an image a percentage width.
pub(super) fn is_percent_width(style: &ComputedStyle) -> bool {
    matches!(&style.width, Size::Length(l) if l.as_px().is_none())
}
