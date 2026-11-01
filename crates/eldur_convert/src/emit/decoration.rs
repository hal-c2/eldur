//! Box decoration (layout.md §7): a `Rectangle` (or `Item` when nothing is
//! painted) around the box's content, which is anchored inside it, inset
//! by padding + border.

use eldur_css::{BorderSide, BorderStyle, BoxSizing, ComputedStyle, Overflow, Sides, Size};
use eldur_qml::{Member, Object, Value};

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

pub(crate) const ELDUR_ROUND_CLIP_QML: &str = include_str!("EldurRoundClip.qml");
pub(crate) const ELDUR_BORDER_QML: &str = include_str!("EldurBorder.qml");
pub(crate) const ELDUR_SCROLLER_QML: &str = include_str!("EldurScroller.qml");
pub(crate) const ELDUR_STICKY_QML: &str = include_str!("EldurSticky.qml");
pub(crate) const ELDUR_SCROLL_INPUT_QML: &str = include_str!("EldurScrollInput.qml");

fn style_name(style: BorderStyle) -> &'static str {
    match style {
        BorderStyle::None => "none",
        BorderStyle::Hidden => "hidden",
        BorderStyle::Solid => "solid",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Dotted => "dotted",
        BorderStyle::Double => "double",
        BorderStyle::Groove => "groove",
        BorderStyle::Ridge => "ridge",
        BorderStyle::Inset => "inset",
        BorderStyle::Outset => "outset",
    }
}

/// The picture a Rectangle's one border and four square strips cannot give: a
/// style other than solid, curves on sides that differ, or a join between colours.
pub(super) fn needs_drawn_border(style: &ComputedStyle) -> bool {
    let all = sides(style);
    let plain = |s: &BorderSide| !is_visible(s) || s.style == BorderStyle::Solid;
    if !all.iter().all(|s| plain(s)) {
        return true;
    }
    let uniform = all.iter().all(|s| **s == *all[0]);
    let r = &style.border_radius;
    let rounded = [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left]
        .iter()
        .any(|c| !c.is_zero());
    let meet = (0..4).any(|i| {
        let (a, b) = (all[i], all[(i + 1) % 4]);
        is_visible(a) && is_visible(b) && a.color != b.color
    });
    all.iter().any(|s| is_visible(s)) && !uniform && (rounded || meet)
}

fn argb(c: eldur_css::Rgba) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.a, c.r, c.g, c.b)
}

fn drawn_border(sides: [&BorderSide; 4], radii: [f32; 4]) -> Object {
    let mut drawn = Object::new("EldurBorder");
    let list = |items: Vec<String>| Value::expr(format!("[{}]", items.join(", ")));
    drawn.set(
        "sideWidths",
        list(
            sides
                .iter()
                .map(|s| fmt_number(f64::from(border_width(s))))
                .collect(),
        ),
    );
    drawn.set(
        "sideColors",
        list(
            sides
                .iter()
                .map(|s| format!("\"{}\"", argb(s.color)))
                .collect(),
        ),
    );
    drawn.set(
        "sideStyles",
        list(
            sides
                .iter()
                .map(|s| format!("\"{}\"", style_name(s.style)))
                .collect(),
        ),
    );
    drawn.set(
        "radii",
        list(radii.iter().map(|r| fmt_number(f64::from(*r))).collect()),
    );
    drawn
}

pub(crate) fn is_visible(side: &BorderSide) -> bool {
    side.width > 0.0 && !matches!(side.style, BorderStyle::None | BorderStyle::Hidden)
}

/// A `double` side wide enough for two lines and a gap; a thinner one is
/// drawn solid.
fn is_double(side: &BorderSide) -> bool {
    side.style == BorderStyle::Double && side.width >= 3.0
}

pub(crate) fn border_width(side: &BorderSide) -> f32 {
    if is_visible(side) { side.width } else { 0.0 }
}

pub(super) fn sides(style: &ComputedStyle) -> [&BorderSide; 4] {
    let b = &style.border;
    [&b.top, &b.right, &b.bottom, &b.left]
}

/// What a box needs to be wrapped for.
pub(crate) struct Decoration {
    pub insets: Insets,
    pub paints: bool,
    pub clips: bool,
    /// `overflow: auto/scroll`, per axis: the content sits in an `EldurScroller`.
    pub scroll: Option<(bool, bool)>,
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
    /// Draw no `EldurBorder` or outline: states would change the box's objects.
    pub plain_edges: bool,
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
        let gradients = style
            .background_layers
            .iter()
            .any(|l| l.image.is_some() || !matches!(l.clip, eldur_css::BackgroundClip::BorderBox));
        let inset_shadows = style.box_shadow.iter().any(|s| s.inset);
        let shadows = style.box_shadow.iter().any(|s| !s.inset);
        let scrolls = |o: Overflow| matches!(o, Overflow::Auto | Overflow::Scroll);
        let scroll = (scrolls(style.overflow_x) || scrolls(style.overflow_y))
            .then_some((scrolls(style.overflow_x), scrolls(style.overflow_y)));
        Decoration {
            insets,
            paints: background || border || is_visible(&style.outline) || self.force_paint,
            clips,
            scroll,
            gradients,
            shadows,
            inset_shadows,
            hint: "box".to_owned(),
            strips: None,
            plain_edges: false,
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

    /// The content of an `overflow: auto/scroll` box, in a scroller filling its
    /// padding box. The content keeps the width of that box; what is wider makes
    /// `contentWidth` wider. Overlays keep the wrapper's coordinates, and their
    /// percentages and far edges keep its size: they scroll with the content but
    /// are not sized by it.
    #[allow(clippy::too_many_arguments)]
    fn scroller(
        &mut self,
        wrapper: &mut Object,
        style: &ComputedStyle,
        deco: &Decoration,
        (scroll_x, scroll_y): (bool, bool),
        inner: Option<(Object, String)>,
        extra: Vec<Object>,
        fill_height: bool,
    ) {
        let [top, right, bottom, left] = sides(style).map(border_width);
        let id = self.ids.claim(&format!("{} scroller", deco.hint));
        let mut scroller = Object::new("EldurScroller");
        if style.scroll_smooth {
            scroller.set("eldurSmooth", true);
        }
        scroller.id = Some(id.clone());
        scroller.set("x", number(left));
        scroller.set("y", number(top));
        scroller.set("width", Value::expr(plus("parent.width", -(left + right))));
        scroller.set(
            "height",
            Value::expr(plus("parent.height", -(top + bottom))),
        );
        if !scroll_x {
            scroller.set("scrollsX", false);
        }
        if !scroll_y {
            scroller.set("scrollsY", false);
        }
        let pad_x = deco.insets.left - left;
        let pad_y = deco.insets.top - top;
        let pad_right = deco.insets.right - right;
        let pad_bottom = deco.insets.bottom - bottom;
        for (name, pad) in [
            ("stickPadTop", pad_y),
            ("stickPadRight", pad_right),
            ("stickPadBottom", pad_bottom),
            ("stickPadLeft", pad_x),
        ] {
            if pad != 0.0 {
                scroller.set(name, number(pad));
            }
        }
        let mut wide = vec!["width".to_owned()];
        let mut tall = vec!["height".to_owned()];
        if let Some((mut inner, inner_id)) = inner {
            inner.set("x", number(pad_x));
            inner.set("y", number(pad_y));
            inner.set(
                "width",
                Value::expr(plus(&format!("{id}.width"), -(pad_x + pad_right))),
            );
            if fill_height {
                inner.set(
                    "height",
                    Value::expr(format!(
                        "Math.max(implicitHeight, {})",
                        plus(&format!("{id}.height"), -(pad_y + pad_bottom))
                    )),
                );
            }
            // A `Text` draws past its width without children to measure.
            let reach = if inner.type_name == "Text" {
                format!("{inner_id}.contentWidth")
            } else {
                format!("{inner_id}.childrenRect.x + {inner_id}.childrenRect.width")
            };
            wide.push(plus(&reach, pad_x + pad_right));
            tall.push(plus(
                &format!("{inner_id}.implicitHeight"),
                pad_y + pad_bottom,
            ));
            scroller.push_child(inner);
        }
        if !extra.is_empty() {
            let holder = format!("{id}Overlays");
            let mut overlays = Object::new("Item");
            overlays.id = Some(holder.clone());
            overlays.set("x", number(-left));
            overlays.set("y", number(-top));
            overlays.set("width", Value::expr("parent.parent.parent.width"));
            overlays.set("height", Value::expr("parent.parent.parent.height"));
            for child in extra {
                overlays.push_child(child);
            }
            wide.push(plus(
                &format!("{holder}.childrenRect.x + {holder}.childrenRect.width"),
                -left,
            ));
            tall.push(plus(
                &format!("{holder}.childrenRect.y + {holder}.childrenRect.height"),
                -top,
            ));
            scroller.push_child(overlays);
        }
        let extent = |parts: &[String]| Value::expr(format!("Math.max({})", parts.join(", ")));
        scroller.set(
            "contentWidth",
            if scroll_x {
                extent(&wide)
            } else {
                Value::expr("width")
            },
        );
        scroller.set(
            "contentHeight",
            if scroll_y {
                extent(&tall)
            } else {
                Value::expr("height")
            },
        );
        wrapper.members.push(Member::PropertyDecl {
            ty: "Item".to_owned(),
            name: "eldurScroller".to_owned(),
            value: Some(Value::expr(id)),
            readonly: false,
            required: false,
        });
        wrapper.push_child(scroller);
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
            let bg = if paint_background && colour_clip_inset(style).is_none() {
                style.background_color
            } else {
                eldur_css::Rgba::TRANSPARENT
            };
            wrapper.set("color", color(bg));
            self.radius(&mut wrapper, style, line);
        }
        if deco.clips {
            wrapper.set("clip", true);
        }
        // EldurSticky's limit: the column inside is only as tall as its content.
        if let Some((content, _)) = &inner
            && content.members.iter().any(|m| {
                matches!(m, Member::Child(c) if matches!(
                    c.get("transform"),
                    Some(Value::Object(t)) if t.type_name == "EldurSticky"
                ) || matches!(
                    c.get("transform"),
                    Some(Value::List(list)) if list.iter().any(
                        |t| matches!(t, Value::Object(t) if t.type_name == "EldurSticky"))
                ))
            })
        {
            wrapper.members.push(Member::PropertyDecl {
                name: "eldurInsetBottom".to_owned(),
                ty: "real".to_owned(),
                value: Some(number(deco.insets.bottom)),
                readonly: true,
                required: false,
            });
        }
        match &inner {
            Some((content, id)) => {
                let width = if super::flex::holds_control_row(content) {
                    format!(
                        "Math.ceil(Array.from({id}.children).reduce((sum, item) => sum + item.implicitWidth, 0))"
                    )
                } else {
                    format!("{id}.implicitWidth")
                };
                wrapper.set(
                    "implicitWidth",
                    Value::expr(plus(&width, insets.horizontal())),
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
        let colour_box = (paint_background && !style.background_color.is_transparent())
            .then(|| colour_clip_inset(style))
            .flatten();
        let painted = !gradients.is_empty() || colour_box.is_some();
        if let Some(by_content) = colour_box {
            let colour = self.clipped_colour(style, by_content, line);
            wrapper.push_child(colour);
        }
        for shape in gradients {
            wrapper.push_child(shape);
        }
        if deco.inset_shadows {
            let shape = self.inset_shadows(style, &deco.hint, line);
            wrapper.push_child(shape);
        }
        let borders: Vec<&BorderSide> = sides(style).to_vec();
        let uniform = borders.iter().all(|s| **s == *borders[0]) && is_visible(borders[0]);
        let stripped = deco.strips.is_some() || !uniform;
        let drawn = deco.strips.is_none() && !deco.plain_edges && needs_drawn_border(style);
        if let Some(forced) = deco.strips {
            self.border_strips(&mut wrapper, style, forced, line);
        } else if drawn {
            let radii = self.corner_radii(style, line);
            let mut border = drawn_border(sides(style), radii);
            border.set("anchors.fill", Value::expr("parent"));
            wrapper.push_child(border);
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
            let two_lines = stripped && is_double(side);
            if !drawn && !matches!(side.style, BorderStyle::Solid) && !two_lines {
                self.diags.info(
                    "css",
                    line,
                    format!("border style `{:?}` is drawn solid", side.style).to_lowercase(),
                );
            }
        }
        let (inner, extra) = match deco.scroll {
            Some(axes) if inner.is_some() || !extra.is_empty() => {
                self.scroller(&mut wrapper, style, deco, axes, inner, extra, fill_height);
                (None, Vec::new())
            }
            _ => (inner, extra),
        };
        if let Some((mut inner, _)) = inner {
            inner.set("anchors.left", Value::expr("parent.left"));
            inner.set("anchors.right", Value::expr("parent.right"));
            // A cell's content sits where `vertical-align` says within the row
            // (CSS 2.1 §17.5.4); `baseline` is the top but for fonts that
            // differ across the row.
            let cell_align =
                (style.display == eldur_css::Display::TableCell).then_some(style.vertical_align);
            let mut margins = vec![
                ("anchors.leftMargin", insets.left),
                ("anchors.rightMargin", insets.right),
            ];
            match cell_align {
                Some(eldur_css::VerticalAlign::Middle) => {
                    inner.set(
                        "anchors.verticalCenter",
                        Value::expr("parent.verticalCenter"),
                    );
                    margins.push((
                        "anchors.verticalCenterOffset",
                        (insets.top - insets.bottom) / 2.0,
                    ));
                    // By default Qt rounds an odd height's centre up, on both
                    // the cell and the content, where CSS centres exactly
                    // (layout.md §1.2).
                    inner.set("anchors.alignWhenCentered", false);
                    wrapper.set("anchors.alignWhenCentered", false);
                }
                Some(eldur_css::VerticalAlign::Bottom) => {
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
        self.outline(&mut wrapper, style, line);
        if deco.clips {
            let holds = wrapper.children().next().is_some();
            self.clip_edges(&mut wrapper, style, &deco.hint, holds, line);
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
            // The outer item is the element a script holds.
            let scroller = wrapper.members.iter().find(
                |m| matches!(m, Member::PropertyDecl { name, .. } if name == "eldurScroller"),
            );
            if let Some(scroller) = scroller.cloned() {
                outer.members.push(scroller);
            }
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
    /// top. Over anything else the box is masked, which takes a layer and so
    /// is kept for a box that holds something to clip.
    fn clip_edges(
        &mut self,
        wrapper: &mut Object,
        style: &ComputedStyle,
        hint: &str,
        holds: bool,
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
                None if holds => {
                    self.imports.insert("QtQuick.Effects");
                    let mut mask = Object::new("EldurRoundClip");
                    self.radius(&mut mask, style, line);
                    wrapper.set("layer.enabled", true);
                    wrapper.set("layer.effect", Value::Object(Box::new(mask)));
                }
                None => {}
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

    /// The background colour, painted in the box the bottom layer clips to.
    fn clipped_colour(
        &mut self,
        style: &ComputedStyle,
        to_content: bool,
        line: Option<u32>,
    ) -> Object {
        let by = if to_content {
            self.insets(style, line)
        } else {
            Insets {
                top: border_width(&style.border.top),
                right: border_width(&style.border.right),
                bottom: border_width(&style.border.bottom),
                left: border_width(&style.border.left),
            }
        };
        let mut rect = Object::new("Rectangle");
        rect.set("anchors.fill", Value::expr("parent"));
        for (name, v) in [
            ("anchors.topMargin", by.top),
            ("anchors.rightMargin", by.right),
            ("anchors.bottomMargin", by.bottom),
            ("anchors.leftMargin", by.left),
        ] {
            if v != 0.0 {
                rect.set(name, number(v));
            }
        }
        rect.set("color", color(style.background_color));
        let radii = self.corner_radii(style, line);
        let less = |r: f32, a: f32, b: f32| (r - a.max(b)).max(0.0);
        set_radii(
            &mut rect,
            [
                less(radii[0], by.top, by.left),
                less(radii[1], by.top, by.right),
                less(radii[2], by.bottom, by.right),
                less(radii[3], by.bottom, by.left),
            ],
        );
        rect
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

    /// An outline is above what the box holds and takes no room. It follows the
    /// border's curve, grown by its distance, where a corner is rounded.
    fn outline(&mut self, wrapper: &mut Object, style: &ComputedStyle, line: Option<u32>) {
        let side = &style.outline;
        if !is_visible(side) {
            return;
        }
        let out = style.outline_offset + side.width;
        let radii = self
            .corner_radii(style, line)
            .map(|r| if r > 0.0 { r + out } else { 0.0 });
        let mut drawn = if side.style == BorderStyle::Solid {
            let mut rect = Object::new("Rectangle");
            rect.set("color", Value::string("transparent"));
            rect.set("border.width", number(side.width));
            rect.set("border.color", color(side.color));
            set_radii(&mut rect, radii);
            rect
        } else {
            drawn_border([side; 4], radii)
        };
        drawn.set("anchors.fill", Value::expr("parent"));
        drawn.set("anchors.margins", number(-out));
        drawn.set("z", 1.0);
        wrapper.push_child(drawn);
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
            if visible && is_double(side) {
                // Two lines at the strip's edges, as Chromium draws them.
                strip.set("color", color(eldur_css::Rgba::TRANSPARENT));
                let across = if size == "height" {
                    ["top", "bottom"]
                } else {
                    ["left", "right"]
                };
                for edge in across {
                    let mut line = Object::new("Rectangle");
                    for a in anchors.iter().filter(|a| !across.contains(a)) {
                        line.set(&format!("anchors.{a}"), Value::expr(format!("parent.{a}")));
                    }
                    line.set(
                        &format!("anchors.{edge}"),
                        Value::expr(format!("parent.{edge}")),
                    );
                    line.set(size, number((side.width / 3.0).round()));
                    line.set("color", color(side.color));
                    strip.push_child(line);
                }
            } else {
                strip.set(
                    "color",
                    color(if visible {
                        side.color
                    } else {
                        eldur_css::Rgba::TRANSPARENT
                    }),
                );
            }
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
/// `Some(to_content)` when the background colour stops inside the border box:
/// the bottom layer's `background-clip` says for the colour too (CSS
/// Backgrounds 3 §3.7).
fn colour_clip_inset(style: &ComputedStyle) -> Option<bool> {
    match style.background_layers.last()?.clip {
        eldur_css::BackgroundClip::PaddingBox => Some(false),
        eldur_css::BackgroundClip::ContentBox => Some(true),
        eldur_css::BackgroundClip::BorderBox | eldur_css::BackgroundClip::Text => None,
    }
}

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
