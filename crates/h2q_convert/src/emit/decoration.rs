//! Box decoration (layout.md §7): a `Rectangle` (or `Item` when nothing is
//! painted) around the box's content, which is anchored inside it, inset
//! by padding + border.

use h2q_css::{BorderSide, BorderStyle, BoxSizing, ComputedStyle, Overflow, Sides, Size};
use h2q_qml::{Object, Value};

use super::Emitter;
use super::units::{color, fmt_number, number, px_part, round};

/// Padding + border widths, in px.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Insets {
    pub(crate) fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub(crate) fn vertical(self) -> f32 {
        self.top + self.bottom
    }

    fn is_zero(self) -> bool {
        self == Self::default()
    }
}

pub(crate) fn is_visible(side: &BorderSide) -> bool {
    side.width > 0.0 && !matches!(side.style, BorderStyle::None | BorderStyle::Hidden)
}

pub(crate) fn border_width(side: &BorderSide) -> f32 {
    if is_visible(side) { side.width } else { 0.0 }
}

fn sides(style: &ComputedStyle) -> [&BorderSide; 4] {
    let b = &style.border;
    [&b.top, &b.right, &b.bottom, &b.left]
}

/// What a box needs to be wrapped for.
pub(crate) struct Decoration {
    pub insets: Insets,
    pub paints: bool,
    pub clips: bool,
    /// Gradient layers to paint, bottom first.
    pub gradients: bool,
    /// Outer `box-shadow`s to draw behind the box.
    pub shadows: bool,
    /// Inset `box-shadow`s to draw inside the padding box.
    pub inset_shadows: bool,
    /// Base for the ids of generated parts (gradient shapes).
    pub hint: String,
    /// Draw the borders as strips on these sides (top, right, bottom,
    /// left), visible or not, so every combination of conditions has
    /// the same objects.
    pub strips: Option<[bool; 4]>,
}

impl Decoration {
    pub(crate) fn needed(&self) -> bool {
        self.paints
            || self.gradients
            || self.shadows
            || self.inset_shadows
            || self.clips
            || !self.insets.is_zero()
    }
}

impl Emitter<'_, '_> {
    /// Padding (px part; percentages are reported) plus border widths.
    pub(super) fn insets(&mut self, style: &ComputedStyle, line: Option<u32>) -> Insets {
        let Sides {
            top,
            right,
            bottom,
            left,
        } = &style.padding;
        let mut lossy = false;
        let mut px = |l| {
            let (v, dropped) = px_part(l);
            lossy |= dropped;
            v.max(0.0)
        };
        let insets = Insets {
            top: px(top) + border_width(&style.border.top),
            right: px(right) + border_width(&style.border.right),
            bottom: px(bottom) + border_width(&style.border.bottom),
            left: px(left) + border_width(&style.border.left),
        };
        if lossy {
            self.diags.info(
                "css",
                line,
                "percentage or `calc()` padding uses its px part only".to_owned(),
            );
        }
        insets
    }

    pub(super) fn decoration(
        &mut self,
        style: &ComputedStyle,
        paint_background: bool,
        line: Option<u32>,
    ) -> Decoration {
        let insets = self.insets(style, line);
        let background = paint_background && !style.background_color.is_transparent();
        let border = sides(style).iter().any(|s| is_visible(s));
        let clips = style.overflow_x != Overflow::Visible || style.overflow_y != Overflow::Visible;
        // The canvas propagation only takes the colour; gradients stay on
        // the box, which is close for the body of a page.
        let gradients = style.background_layers.iter().any(|l| l.image.is_some());
        let inset_shadows = style.box_shadow.iter().any(|s| s.inset);
        let shadows = style.box_shadow.iter().any(|s| !s.inset);
        if matches!(style.overflow_x, Overflow::Auto | Overflow::Scroll)
            || matches!(style.overflow_y, Overflow::Auto | Overflow::Scroll)
        {
            self.diags.warning(
                "css",
                line,
                "`overflow: auto/scroll` clips but does not scroll yet".to_owned(),
            );
        }
        Decoration {
            insets,
            paints: background || border || self.force_paint,
            clips,
            gradients,
            shadows,
            inset_shadows,
            hint: "box".to_owned(),
            strips: None,
        }
    }

    /// Wraps `inner` (anchored inside, id `inner_id`) in the box's
    /// decoration. `inner` is `None` for a box without content. With
    /// `fill_height`, `inner` is anchored to the bottom edge too.
    pub(super) fn wrap(
        &mut self,
        style: &ComputedStyle,
        deco: &Decoration,
        paint_background: bool,
        inner: Option<(Object, String)>,
        fill_height: bool,
        line: Option<u32>,
    ) -> Object {
        self.wrap_with(
            style,
            deco,
            paint_background,
            inner,
            Vec::new(),
            fill_height,
            line,
        )
    }

    /// [`Self::wrap`], with `extra` children (markers, overlays) after the
    /// content, inside the clipping item.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn wrap_with(
        &mut self,
        style: &ComputedStyle,
        deco: &Decoration,
        paint_background: bool,
        inner: Option<(Object, String)>,
        extra: Vec<Object>,
        fill_height: bool,
        line: Option<u32>,
    ) -> Object {
        let mut wrapper = Object::new(if deco.paints { "Rectangle" } else { "Item" });
        let insets = deco.insets;
        if deco.paints {
            let bg = if paint_background {
                style.background_color
            } else {
                h2q_css::Rgba::TRANSPARENT
            };
            wrapper.set("color", color(bg));
            self.radius(&mut wrapper, style, line);
        }
        if deco.clips {
            wrapper.set("clip", true);
        }
        match &inner {
            Some((_, id)) => {
                wrapper.set(
                    "implicitWidth",
                    Value::expr(plus(&format!("{id}.implicitWidth"), insets.horizontal())),
                );
                wrapper.set(
                    "implicitHeight",
                    Value::expr(plus(&format!("{id}.implicitHeight"), insets.vertical())),
                );
            }
            None => {
                wrapper.set("implicitWidth", number(insets.horizontal()));
                wrapper.set("implicitHeight", number(insets.vertical()));
            }
        }
        if deco.shadows && !deco.clips {
            self.shadows(&mut wrapper, style, line);
        }
        let gradients = if deco.gradients {
            self.gradient_layers(style, &deco.hint, line)
        } else {
            Vec::new()
        };
        let painted = !gradients.is_empty();
        for shape in gradients {
            wrapper.push_child(shape);
        }
        if deco.inset_shadows {
            let shape = self.inset_shadows(style, &deco.hint, line);
            wrapper.push_child(shape);
        }
        let borders: Vec<&BorderSide> = sides(style).to_vec();
        let uniform = borders.iter().all(|s| **s == *borders[0]) && is_visible(borders[0]);
        if let Some(forced) = deco.strips {
            self.border_strips(&mut wrapper, style, forced, line);
        } else if uniform && painted {
            // The background is painted under the border (`background-clip:
            // border-box`), and a Rectangle draws its border below its
            // children, so the border goes on top as its own outline.
            let mut outline = Object::new("Rectangle");
            outline.set("anchors.fill", Value::expr("parent"));
            outline.set("color", Value::string("transparent"));
            self.radius(&mut outline, style, line);
            outline.set("border.width", number(borders[0].width));
            outline.set("border.color", color(borders[0].color));
            wrapper.push_child(outline);
        } else if uniform {
            wrapper.set("border.width", number(borders[0].width));
            wrapper.set("border.color", color(borders[0].color));
        } else {
            self.border_strips(&mut wrapper, style, [false; 4], line);
        }
        for side in borders.iter().filter(|s| is_visible(s)) {
            if !matches!(side.style, BorderStyle::Solid) {
                self.diags.info(
                    "css",
                    line,
                    format!("border style `{:?}` is drawn solid", side.style).to_lowercase(),
                );
            }
        }
        if let Some((mut inner, _)) = inner {
            inner.set("anchors.left", Value::expr("parent.left"));
            inner.set("anchors.right", Value::expr("parent.right"));
            // A cell's content sits where `vertical-align` says within the row
            // (CSS 2.1 §17.5.4); `baseline` is the top but for fonts that
            // differ across the row.
            let cell_align =
                (style.display == h2q_css::Display::TableCell).then_some(style.vertical_align);
            let mut margins = vec![
                ("anchors.leftMargin", insets.left),
                ("anchors.rightMargin", insets.right),
            ];
            match cell_align {
                Some(h2q_css::VerticalAlign::Middle) => {
                    inner.set(
                        "anchors.verticalCenter",
                        Value::expr("parent.verticalCenter"),
                    );
                    margins.push((
                        "anchors.verticalCenterOffset",
                        (insets.top - insets.bottom) / 2.0,
                    ));
                }
                Some(h2q_css::VerticalAlign::Bottom) => {
                    inner.set("anchors.bottom", Value::expr("parent.bottom"));
                    margins.push(("anchors.bottomMargin", insets.bottom));
                }
                _ => {
                    inner.set("anchors.top", Value::expr("parent.top"));
                    margins.push(("anchors.topMargin", insets.top));
                }
            }
            if fill_height {
                inner.set("anchors.bottom", Value::expr("parent.bottom"));
                margins.push(("anchors.bottomMargin", insets.bottom));
            }
            for (name, v) in margins {
                if v != 0.0 {
                    inner.set(name, number(v));
                }
            }
            wrapper.push_child(inner);
        }
        for child in extra {
            wrapper.push_child(child);
        }
        if deco.clips {
            self.clip_edges(&mut wrapper, style, &deco.hint, line);
        }
        if deco.shadows && deco.clips {
            // `overflow` clips the content, not the box's own shadow, and a
            // QML clip takes children with it: the shadow goes on an item
            // around the clipping box.
            let id = match &wrapper.id {
                Some(id) => id.clone(),
                None => self.ids.claim(&format!("{} clip", deco.hint)),
            };
            wrapper.id = Some(id.clone());
            wrapper.set("anchors.fill", Value::expr("parent"));
            let mut outer = Object::new("Item");
            outer.set("implicitWidth", Value::expr(format!("{id}.implicitWidth")));
            outer.set(
                "implicitHeight",
                Value::expr(format!("{id}.implicitHeight")),
            );
            self.shadows(&mut outer, style, line);
            outer.push_child(wrapper);
            return outer;
        }
        wrapper
    }

    /// What QML's square `clip` cannot do for `overflow`: round the clip
    /// with the box's corners, and keep the content inside the border
    /// (CSS clips to the padding box). No masking renders under the
    /// software backend (`MultiEffect` masks and `ShapePath.fillItem` draw
    /// nothing there), so the corners outside the rounded rect are painted
    /// over in the colour behind the box, and the border is drawn again on
    /// top. Over anything but a plain colour the clip stays square.
    fn clip_edges(
        &mut self,
        wrapper: &mut Object,
        style: &ComputedStyle,
        hint: &str,
        line: Option<u32>,
    ) {
        let r = &style.border_radius;
        let rounded = [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left]
            .iter()
            .any(|c| !c.is_zero());
        let above = wrapper
            .children()
            .filter_map(|c| match c.get("z") {
                Some(Value::Number(z)) => Some(*z + 1.0),
                _ => None,
            })
            .fold(0.0_f64, f64::max);
        if rounded {
            match self.backdrop {
                Some(backdrop) => {
                    self.imports.insert("QtQuick.Shapes");
                    let id = self.ids.claim(&format!("{hint} corners"));
                    let mut path = Object::new("ShapePath");
                    path.set("strokeWidth", Value::Number(-1.0));
                    path.set("fillColor", color(backdrop));
                    path.set("fillRule", Value::expr("ShapePath.OddEvenFill"));
                    let mut outer = Object::new("PathRectangle");
                    outer.set("width", Value::expr(format!("{id}.width")));
                    outer.set("height", Value::expr(format!("{id}.height")));
                    let mut inner = outer.clone();
                    self.radius(&mut inner, style, line);
                    path.push_child(outer);
                    path.push_child(inner);
                    let mut shape = Object::new("Shape").with_id(id);
                    shape.set("anchors.fill", Value::expr("parent"));
                    if above > 0.0 {
                        shape.set("z", above);
                    }
                    shape.push_child(path);
                    wrapper.push_child(shape);
                }
                None => self.diags.info(
                    "css",
                    line,
                    "rounded `overflow` clip is square: what is behind the box is not a plain colour"
                        .to_owned(),
                ),
            }
        }
        let borders = sides(style);
        if borders.iter().all(|s| *s == borders[0]) && is_visible(borders[0]) {
            let mut outline = Object::new("Rectangle");
            outline.set("anchors.fill", Value::expr("parent"));
            outline.set("color", Value::string("transparent"));
            self.radius(&mut outline, style, line);
            outline.set("border.width", number(borders[0].width));
            outline.set("border.color", color(borders[0].color));
            if above > 0.0 {
                outline.set("z", above);
            }
            wrapper.push_child(outline);
        }
    }

    /// Outer shadows as `RectangularShadow`s below the box (negative `z`
    /// draws under the parent's own paint), the first in CSS on top. The
    /// software renderer does not draw them.
    fn shadows(&mut self, wrapper: &mut Object, style: &ComputedStyle, line: Option<u32>) {
        self.imports.insert("QtQuick.Effects");
        for s in style.box_shadow.iter().rev().filter(|s| !s.inset) {
            let mut shadow = Object::new("RectangularShadow");
            shadow.set("anchors.fill", Value::expr("parent"));
            shadow.set("z", -1.0);
            shadow.set(
                "offset",
                Value::expr(format!(
                    "Qt.vector2d({}, {})",
                    fmt_number(round(s.offset_x)),
                    fmt_number(round(s.offset_y))
                )),
            );
            if s.blur > 0.0 {
                // Qt's blur fades over a shorter distance than the CSS
                // blur radius; fitted to Chromium's for radii 8 to 40.
                shadow.set("blur", number(1.2 * s.blur + 2.0));
            } else {
                shadow.set("blur", 0.0);
            }
            if s.spread != 0.0 {
                shadow.set("spread", number(s.spread));
            }
            shadow.set("color", color(s.color));
            self.radius(&mut shadow, style, line);
            wrapper.push_child(shadow);
        }
    }

    pub(super) fn radius(
        &mut self,
        wrapper: &mut Object,
        style: &ComputedStyle,
        line: Option<u32>,
    ) {
        let px = self.corner_radii(style, line);
        set_radii(wrapper, px);
    }

    /// The outer radius of each corner in px: top left, top right, bottom
    /// right, bottom left.
    pub(super) fn corner_radii(&mut self, style: &ComputedStyle, line: Option<u32>) -> [f32; 4] {
        let r = &style.border_radius;
        let corners = [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left];
        // A percentage is of the box, known here when the box is sized. Qt has
        // one radius per corner, so an uneven box uses its shorter side.
        let side = self.definite_side(style, line);
        let mut px = [0.0; 4];
        for (out, c) in px.iter_mut().zip(corners) {
            *out = match (c.as_px(), side) {
                (Some(v), _) => v,
                (None, Some(side)) => c.px + c.percent / 100.0 * side,
                (None, None) => {
                    self.diags.info(
                        "css",
                        line,
                        "percentage `border-radius` uses its px part only".to_owned(),
                    );
                    c.px
                }
            };
        }
        px
    }

    /// The shorter side of a box whose width and height are lengths.
    fn definite_side(&mut self, style: &ComputedStyle, line: Option<u32>) -> Option<f32> {
        let (Size::Length(w), Size::Length(h)) = (&style.width, &style.height) else {
            return None;
        };
        let (w, h) = (w.as_px()?, h.as_px()?);
        let (extra_w, extra_h) = match style.box_sizing {
            BoxSizing::ContentBox => {
                let insets = self.insets(style, line);
                (insets.horizontal(), insets.vertical())
            }
            BoxSizing::BorderBox => (0.0, 0.0),
        };
        Some((w + extra_w).min(h + extra_h))
    }

    /// Non-uniform borders: one thin `Rectangle` per visible side, and per
    /// `forced` one (corner joins are not mitred).
    fn border_strips(
        &mut self,
        wrapper: &mut Object,
        style: &ComputedStyle,
        forced: [bool; 4],
        line: Option<u32>,
    ) {
        let b = &style.border;
        let strips = [
            ("top", &b.top, ["left", "right", "top"], "height"),
            ("right", &b.right, ["top", "bottom", "right"], "width"),
            ("bottom", &b.bottom, ["left", "right", "bottom"], "height"),
            ("left", &b.left, ["top", "bottom", "left"], "width"),
        ];
        let mut any = false;
        for ((_, side, anchors, size), forced) in strips.into_iter().zip(forced) {
            let visible = is_visible(side);
            if !visible && !forced {
                continue;
            }
            any |= visible;
            let mut strip = Object::new("Rectangle");
            for a in anchors {
                strip.set(&format!("anchors.{a}"), Value::expr(format!("parent.{a}")));
            }
            strip.set(size, number(border_width(side)));
            strip.set(
                "color",
                color(if visible {
                    side.color
                } else {
                    h2q_css::Rgba::TRANSPARENT
                }),
            );
            wrapper.push_child(strip);
        }
        let r = &style.border_radius;
        if any
            && [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left]
                .iter()
                .any(|c| !c.is_zero())
        {
            self.diags.info(
                "css",
                line,
                "rounded corners with uneven borders: the borders are drawn square".to_owned(),
            );
        }
    }
}

/// `radius`, or one radius per corner when they differ.
pub(super) fn set_radii(obj: &mut Object, px: [f32; 4]) {
    let names = [
        "topLeftRadius",
        "topRightRadius",
        "bottomRightRadius",
        "bottomLeftRadius",
    ];
    if px.iter().all(|v| *v == px[0]) {
        if px[0] > 0.0 {
            obj.set("radius", number(px[0]));
        }
    } else {
        for (name, v) in names.into_iter().zip(px) {
            obj.set(name, number(v));
        }
    }
}

/// `expr + n`, or just `expr` when `n` is 0.
fn plus(expr: &str, n: f32) -> String {
    let n = round(n);
    if n == 0.0 {
        expr.to_owned()
    } else {
        format!("{expr} + {}", fmt_number(n))
    }
}
