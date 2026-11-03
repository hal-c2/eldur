//! What the runtime knows of the inline elements inside one `Text`
//! (`docs/spec/dom-api.md`, Inline nodes): StyledText has no objects for
//! them, so scripts get virtual nodes parsed from this JSON. The computed
//! values spare the runtime a cascade until a script changes something.

use eldur_css::{
    ComputedStyle, Display, FontStyle, LineHeight, Position, Rgba, TextDecorationLine,
    VerticalAlign, WhiteSpace,
};
use eldur_dom::{NodeData, NodeId};
use eldur_qml::{Member, Object, Value};
use serde_json::{Value as Json, json};

use super::Emitter;
use super::text::{line_height, needs_html, whole_span};
use crate::boxes::{Inline, LayoutBox, Span};

/// The way `getComputedStyle` writes a colour.
fn rgb(c: Rgba) -> String {
    if c.a == 255 {
        format!("rgb({}, {}, {})", c.r, c.g, c.b)
    } else {
        let alpha = (f64::from(c.a) / 255.0 * 1000.0).round() / 1000.0;
        format!("rgba({}, {}, {}, {alpha})", c.r, c.g, c.b)
    }
}

fn computed(style: &ComputedStyle) -> Json {
    let mut out = plain_computed(style);
    if let Some(unscaled) = style.font_size_unscaled {
        out["us"] = json!(unscaled);
    }
    if let Some(keyword) = style.font_size_keyword {
        out["kw"] = json!(keyword);
    }
    // An inline element's rect is its border box, which its line's text does not show.
    let edge = |padding: &eldur_css::Length, border: &eldur_css::BorderSide| {
        padding.as_px().unwrap_or(0.0).max(0.0) + super::decoration::border_width(border)
    };
    let edges = [
        edge(&style.padding.top, &style.border.top),
        edge(&style.padding.right, &style.border.right),
        edge(&style.padding.bottom, &style.border.bottom),
        edge(&style.padding.left, &style.border.left),
    ];
    if edges.iter().any(|e| *e != 0.0) {
        out["edge"] = json!(edges);
    }
    // What the runtime paints and spaces of the box when it lays the runs out
    // again: the same as `paint_run_boxes` writes.
    let (margin_left, margin_right) = (
        super::inline::side_px(&style.margin.left),
        super::inline::side_px(&style.margin.right),
    );
    if margin_left != 0.0 {
        out["ml"] = json!(margin_left);
    }
    if margin_right != 0.0 {
        out["mr"] = json!(margin_right);
    }
    let sides = super::decoration::sides(style);
    if sides.iter().any(|s| super::decoration::is_visible(s)) {
        out["bw"] = json!(sides.map(super::decoration::border_width));
        out["bc"] = json!(sides.map(|s| rgb(s.color)));
    }
    let r = &style.border_radius;
    let radii = [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left]
        .map(|c| c.as_px().unwrap_or(c.px));
    if radii.iter().any(|r| *r != 0.0) {
        out["rad"] = json!(radii);
    }
    let (rel_x, rel_y) = super::inline::relative_offset(style);
    if (rel_x, rel_y) != (0.0, 0.0) {
        out["rel"] = json!([rel_x, rel_y]);
    }
    match style.line_height {
        LineHeight::Normal => {}
        LineHeight::Number(n) => {
            out["lf"] = json!(n);
            out["lh"] = json!(n * style.font_size);
        }
        LineHeight::Px(px) => out["lh"] = json!(px),
    }
    match style.vertical_align {
        VerticalAlign::Baseline => {}
        VerticalAlign::Sub => out["va"] = json!("sub"),
        VerticalAlign::Super => out["va"] = json!("super"),
        VerticalAlign::Top => out["va"] = json!("top"),
        VerticalAlign::TextTop => out["va"] = json!("text-top"),
        VerticalAlign::Middle => out["va"] = json!("middle"),
        VerticalAlign::Bottom => out["va"] = json!("bottom"),
        VerticalAlign::TextBottom => out["va"] = json!("text-bottom"),
        VerticalAlign::Length(px) => out["vs"] = json!(px),
        VerticalAlign::Percent(p) => out["vp"] = json!(p),
    }
    out
}

fn plain_computed(style: &ComputedStyle) -> Json {
    json!({
        "color": rgb(style.color),
        "weight": style.font_weight,
        "italic": style.font_style != FontStyle::Normal,
        "underline": style.text_decoration_line.underline,
        "strike": style.text_decoration_line.line_through,
        "size": style.font_size,
        "family": style.font_family.join(", "),
        "bg": rgb(style.background_color),
        "display": match style.display {
            Display::None => "none",
            Display::Inline => "inline",
            Display::Contents => "contents",
            _ => "block",
        },
        "visible": style.visibility == eldur_css::Visibility::Visible,
    })
}

/// A generated box's text, and its style, for the line it stays on.
fn generated(span: &Span<'_>) -> Json {
    fn flat(items: &[Inline<'_>], out: &mut String) {
        for item in items {
            match item {
                Inline::Text(t) => out.push_str(t),
                Inline::LineBreak => out.push('\n'),
                Inline::Span(s) => flat(&s.children, out),
                Inline::Atomic(_) => {}
            }
        }
    }
    let mut text = String::new();
    flat(&span.children, &mut text);
    json!({ "text": text, "c": computed(span.style) })
}

/// The generated boxes at the ends of each element's content.
fn collect_generated(
    items: &[Inline<'_>],
    out: &mut std::collections::BTreeMap<NodeId, (Option<Json>, Option<Json>)>,
) {
    for item in items {
        let Inline::Span(span) = item else { continue };
        if let Some(node) = span.node {
            let is_generated =
                |i: Option<&Inline<'_>>| matches!(i, Some(Inline::Span(s)) if s.node.is_none());
            let entry = out.entry(node).or_default();
            if is_generated(span.children.first())
                && let Some(Inline::Span(s)) = span.children.first()
            {
                entry.0 = Some(generated(s));
            }
            if span.children.len() > 1
                && is_generated(span.children.last())
                && let Some(Inline::Span(s)) = span.children.last()
            {
                entry.1 = Some(generated(s));
            }
        }
        collect_generated(&span.children, out);
    }
}

impl Emitter<'_, '_> {
    /// Stays on one line of text whatever the script writes: an inline or
    /// hidden element, all the way down, in normal flow.
    fn all_inline(&self, node: NodeId) -> bool {
        self.doc.element_children(node).all(|child| {
            let Some(c) = self.styles.computed(child) else {
                return true;
            };
            let s = &c.style;
            s.display == Display::None
                || (s.display == Display::Inline
                    && s.position == Position::Static
                    && s.float == eldur_css::Float::None
                    && self.doc.element(child).is_some_and(|e| {
                        !matches!(
                            e.local_name(),
                            "img" | "svg" | "input" | "button" | "select"
                        )
                    })
                    && self.all_inline(child))
        })
    }

    /// `node` as the runtime's model: every attribute in document order
    /// and raw text, since scripts read them as written.
    fn dom_node(
        &mut self,
        node: NodeId,
        gen_boxes: &std::collections::BTreeMap<NodeId, (Option<Json>, Option<Json>)>,
    ) -> Option<Json> {
        self.dom_node_with(node, gen_boxes, false)
    }

    /// `dom_node` without its children.
    fn dom_node_shallow(
        &mut self,
        node: NodeId,
        gen_boxes: &std::collections::BTreeMap<NodeId, (Option<Json>, Option<Json>)>,
    ) -> Option<Json> {
        self.dom_node_with(node, gen_boxes, true)
    }

    fn dom_node_with(
        &mut self,
        node: NodeId,
        gen_boxes: &std::collections::BTreeMap<NodeId, (Option<Json>, Option<Json>)>,
        shallow: bool,
    ) -> Option<Json> {
        match &self.doc.node(node).data {
            NodeData::Text(t) => Some(json!({ "text": t })),
            NodeData::Element(e) => {
                let attrs: Vec<Json> = e
                    .attrs
                    .iter()
                    .map(|a| json!([&*a.name.local, a.value.as_str()]))
                    .collect();
                let children: Vec<NodeId> = self.doc.children(node).collect();
                let kids: Vec<Json> = if shallow {
                    Vec::new()
                } else {
                    children
                        .into_iter()
                        .filter_map(|c| self.dom_node(c, gen_boxes))
                        .collect()
                };
                self.inline_covered.insert(node);
                let mut out = json!({ "tag": e.local_name(), "attrs": attrs, "kids": kids });
                // Its handler attribute finds it by this, id or no id.
                if e.attrs.iter().any(|a| a.name.local.starts_with("on")) {
                    out["key"] = json!(node.index());
                }
                if let Some(c) = self.styles.computed(node) {
                    out["c"] = computed(&c.style);
                }
                if let Some((before, after)) = gen_boxes.get(&node) {
                    if let Some(g) = before {
                        out["before"] = g.clone();
                    }
                    if let Some(g) = after {
                        out["after"] = g.clone();
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }

    fn item_nodes(
        &mut self,
        items: &[Inline<'_>],
        gen_boxes: &std::collections::BTreeMap<NodeId, (Option<Json>, Option<Json>)>,
    ) -> Vec<Json> {
        let mut out = Vec::new();
        for item in items {
            match item {
                Inline::Text(t) => out.push(json!({ "text": t })),
                Inline::LineBreak => {
                    out.push(json!({ "tag": "br", "attrs": [], "kids": [] }));
                }
                Inline::Span(s) => match s.node {
                    Some(n) => out.extend(self.dom_node(n, gen_boxes)),
                    None => out.push(json!({ "gen": generated(s) })),
                },
                Inline::Atomic(_) => {}
            }
        }
        out
    }

    /// Like `item_nodes`, for a run cut at boxes: a span the cut went through is
    /// built from the piece in this run, and only its first piece is the element.
    fn run_nodes(
        &mut self,
        items: &[Inline<'_>],
        gen_boxes: &std::collections::BTreeMap<NodeId, (Option<Json>, Option<Json>)>,
    ) -> Vec<Json> {
        let mut out = Vec::new();
        for item in items {
            let Inline::Span(s) = item else {
                out.extend(self.item_nodes(std::slice::from_ref(item), gen_boxes));
                continue;
            };
            match s.node {
                Some(n) if self.cut_nodes.contains(&n) || !self.all_inline(n) => {
                    if self.inline_covered.contains(&n) {
                        // The element is the first piece's; a script reaches this
                        // through it.
                        let mut rest = self.run_nodes(&s.children, gen_boxes);
                        for kid in &mut rest {
                            if kid.get("under").is_none() {
                                kid["under"] = json!(n.index());
                            }
                        }
                        out.extend(rest);
                        continue;
                    }
                    let Some(mut node) = self.dom_node_shallow(n, gen_boxes) else {
                        continue;
                    };
                    // Its generated boxes are pieces of the cut, among its
                    // kids: as `before`/`after` too they would be its twice.
                    if let Some(o) = node.as_object_mut() {
                        o.remove("before");
                        o.remove("after");
                    }
                    node["kids"] = Json::Array(self.run_nodes(&s.children, gen_boxes));
                    // The boxes cut out of it name it by this.
                    node["key"] = json!(n.index());
                    out.push(node);
                }
                _ => out.extend(self.item_nodes(std::slice::from_ref(item), gen_boxes)),
            }
        }
        out
    }

    /// `declare_inline_model` for one run of text in `b`'s lines.
    pub(super) fn declare_run_model(
        &mut self,
        text: &mut Object,
        b: &LayoutBox<'_>,
        decoration: TextDecorationLine,
        items: &[Inline<'_>],
    ) {
        if !items
            .iter()
            .any(|i| matches!(i, Inline::Span(_) | Inline::LineBreak))
        {
            return;
        }
        let mut gen_boxes = std::collections::BTreeMap::new();
        collect_generated(items, &mut gen_boxes);
        let mut model = self.model_head(&b.style, decoration, items);
        model["kids"] = Json::Array(self.run_nodes(items, &gen_boxes));
        declare(text, &model);
    }

    /// Declares `eldurInline` on the `Text` of `b`'s inline content, when
    /// there is an element in it for a script to reach.
    pub(super) fn declare_inline_model(
        &mut self,
        text: &mut Object,
        b: &LayoutBox<'_>,
        decoration: TextDecorationLine,
        items: &[Inline<'_>],
    ) {
        let style = b.style.as_ref();
        let whole_element = b.node.filter(|&n| self.all_inline(n));
        let has_element = match whole_element {
            Some(n) => self.doc.element_children(n).next().is_some(),
            None => items
                .iter()
                .any(|i| matches!(i, Inline::Span(_) | Inline::LineBreak)),
        };
        if !has_element {
            return;
        }
        let mut gen_boxes = std::collections::BTreeMap::new();
        collect_generated(items, &mut gen_boxes);
        let mut model = self.model_head(style, decoration, items);
        let kids = match whole_element {
            Some(n) => {
                let children: Vec<NodeId> = self.doc.children(n).collect();
                let mut kids: Vec<Json> = children
                    .into_iter()
                    .filter_map(|c| self.dom_node(c, &gen_boxes))
                    .collect();
                // The block's own generated boxes come through the items.
                if let Some(Inline::Span(s)) = items.first()
                    && s.node.is_none()
                {
                    kids.insert(0, json!({ "gen": generated(s) }));
                }
                if items.len() > 1
                    && let Some(Inline::Span(s)) = items.last()
                    && s.node.is_none()
                {
                    kids.push(json!({ "gen": generated(s) }));
                }
                kids
            }
            None => self.item_nodes(items, &gen_boxes),
        };
        model["kids"] = Json::Array(kids);
        declare(text, &model);
    }

    fn model_head(
        &self,
        style: &ComputedStyle,
        decoration: TextDecorationLine,
        items: &[Inline<'_>],
    ) -> Json {
        let mut model = json!({
            "k": computed(style),
            "hoisted": whole_span(style, items).is_some(),
            "bare": matches!(items, [Inline::Span(s)] if !decoration.underline
                && super::text::link_href(self.doc, s).is_some()
                && !s.style.text_decoration_line.underline),
            "html": needs_html(items, &super::text::font_family(&style.font_family)),
            "pre": !matches!(style.white_space, WhiteSpace::Normal | WhiteSpace::NoWrap),
            "underline": decoration.underline,
            "strike": decoration.line_through,
        });
        if let Some(lh) = line_height(style) {
            model["lh"] = json!(lh);
        }
        model
    }

    /// Declares `eldurInline` on the `ComboBox` of `<select>` `node`.
    pub(super) fn declare_select_model(&mut self, combo: &mut Object, node: NodeId) {
        if !self.runtime_dom {
            return;
        }
        let gen_boxes = std::collections::BTreeMap::new();
        let children: Vec<NodeId> = self.doc.children(node).collect();
        let kids: Vec<Json> = children
            .into_iter()
            .filter_map(|c| self.dom_node(c, &gen_boxes))
            .collect();
        declare(combo, &json!({ "select": true, "kids": kids }));
    }
}

/// A string, as `qmllint` has no complaint about a string however long.
fn declare(object: &mut Object, model: &Json) {
    object.members.insert(
        0,
        Member::PropertyDecl {
            ty: "string".to_owned(),
            name: "eldurInline".to_owned(),
            value: Some(Value::String(model.to_string())),
            readonly: true,
            required: false,
        },
    );
}
