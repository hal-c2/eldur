//! Form controls → Qt Quick Controls (html-elements.md, Forms).
//!
//! The Basic style's own look is replaced by the element's CSS: its
//! `background`, padding and font. What CSS cannot reach in a browser either
//! (the check mark, the slider's handle) is drawn like Chromium draws it.

use std::collections::BTreeMap;

use h2q_css::{BorderSide, BorderStyle, ComputedStyle, Length, Rgba, Size, TextAlign};
use h2q_dom::{Document, NodeId};
use h2q_js::TargetKind;
use h2q_qml::{Member, Object, Value};

use super::flow::{Emitted, MarginSet};
use super::text::font_family;
use super::units::{color, number, round};
use super::{Emitter, Target};
use crate::boxes::{LayoutBox, describe};

/// Chromium's default accent. `accent-color` is not in `ComputedStyle`.
const ACCENT: Rgba = Rgba::new(0x00, 0x75, 0xff, 0xff);
/// The UA's `::placeholder` colour, which h2q_css's UA sheet does not set.
const PLACEHOLDER: Rgba = Rgba::new(0x75, 0x75, 0x75, 0xff);
const NATIVE_BORDER: Rgba = Rgba::new(0x76, 0x76, 0x76, 0xff);
const TRACK: Rgba = Rgba::new(0xef, 0xef, 0xef, 0xff);
/// Average glyph advance of the fallback fonts, in em: `size` and `cols`
/// count these (HTML §4.10.5.1.2 "average character width").
const AVG_CHAR: f32 = 0.55;
/// Room for the drop-down arrow, which CSS padding does not include.
const ARROW_ROOM: f32 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    Text,
    TextArea,
    Number,
    Range,
    CheckBox,
    Radio,
    Button { submit: bool, reset: bool },
    Select,
    Progress,
    Meter,
}

impl Control {
    fn target_kind(self) -> TargetKind {
        match self {
            Self::Text => TargetKind::TextField,
            Self::TextArea => TargetKind::TextArea,
            Self::Number | Self::Range => TargetKind::Range,
            Self::CheckBox => TargetKind::CheckBox,
            Self::Radio => TargetKind::RadioButton,
            Self::Button { .. } => TargetKind::Button,
            Self::Select => TargetKind::ComboBox,
            Self::Progress | Self::Meter => TargetKind::Progress,
        }
    }

    fn is_toggle(self) -> bool {
        matches!(self, Self::CheckBox | Self::Radio)
    }

    fn is_clickable(self) -> bool {
        self.is_toggle() || matches!(self, Self::Button { .. })
    }
}

pub(crate) fn classify(doc: &Document, node: NodeId) -> Option<Control> {
    let element = doc.element(node)?;
    if !element.is_html() {
        return None;
    }
    match element.local_name() {
        "input" => {
            let ty = element.attr("type").unwrap_or("text").to_ascii_lowercase();
            Some(match ty.as_str() {
                "number" => Control::Number,
                "range" => Control::Range,
                "checkbox" => Control::CheckBox,
                "radio" => Control::Radio,
                "submit" | "image" => Control::Button {
                    submit: true,
                    reset: false,
                },
                "reset" => Control::Button {
                    submit: false,
                    reset: true,
                },
                "button" => Control::Button {
                    submit: false,
                    reset: false,
                },
                _ => Control::Text,
            })
        }
        "button" => {
            let ty = element
                .attr("type")
                .unwrap_or("submit")
                .to_ascii_lowercase();
            Some(Control::Button {
                submit: ty != "button" && ty != "reset",
                reset: ty == "reset",
            })
        }
        "textarea" => Some(Control::TextArea),
        "select" => Some(Control::Select),
        "progress" => Some(Control::Progress),
        "meter" => Some(Control::Meter),
        _ => None,
    }
}

/// What a `<form>` needs once all its controls are emitted.
#[derive(Debug, Default)]
pub(crate) struct Form {
    /// Text fields (`onAccepted`) that submit it implicitly.
    pub accepting: Vec<String>,
    /// Submit buttons (`onClicked`).
    pub submitters: Vec<String>,
    pub reset_buttons: Vec<String>,
    /// Statements restoring each control's initial state.
    pub reset: Vec<String>,
}

/// The form owning `node`: its `form` attribute, else its nearest ancestor.
fn form_owner(doc: &Document, node: NodeId) -> Option<NodeId> {
    if let Some(id) = doc.element(node).and_then(|e| e.attr("form")) {
        return doc.element_by_id(id);
    }
    doc.ancestors(node)
        .find(|&a| doc.element(a).is_some_and(|e| e.local_name() == "form"))
}

/// The control a `<label>` is for (HTML §4.10.4): its `for` target, else
/// its first labelable descendant.
pub(crate) fn labeled_control(doc: &Document, label: NodeId) -> Option<NodeId> {
    let element = doc.element(label)?;
    if !element.is_html() || element.local_name() != "label" {
        return None;
    }
    match element.attr("for") {
        Some(id) => doc
            .element_by_id(id)
            .filter(|&n| classify(doc, n).is_some()),
        None => doc
            .descendants(label)
            .find(|&n| n != label && classify(doc, n).is_some()),
    }
}

/// The text of the first `<label>` of `control`, for `Accessible.name`.
fn label_text(doc: &Document, control: NodeId) -> Option<String> {
    let label = doc
        .descendants(doc.root())
        .find(|&n| labeled_control(doc, n) == Some(control))?;
    let text = h2q_dom::collapse_whitespace(&doc.text_content(label));
    (!text.is_empty()).then_some(text)
}

fn attr_f64(doc: &Document, node: NodeId, name: &str) -> Option<f64> {
    doc.element(node)?
        .attr(name)?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn line_box(style: &ComputedStyle) -> Option<f32> {
    match style.line_height {
        h2q_css::LineHeight::Normal => None,
        h2q_css::LineHeight::Number(n) => Some(n * style.font_size),
        h2q_css::LineHeight::Px(px) => Some(px),
    }
}

fn is_zero_length(l: &Length) -> bool {
    l.as_px() == Some(0.0)
}

/// h2q_css's UA sheet has no rules for the controls' native box, so a
/// control with no border, background or padding computes to nothing at
/// all, and a browser's default look is what the page means. An author's
/// explicit `border: 0; padding: 0; background: none` looks the same from
/// here and gets the native box too; that is reported as a limitation.
fn with_native_box(style: &ComputedStyle, control: Control) -> Option<ComputedStyle> {
    let bare = style.background_color.is_transparent()
        && style.background_layers.iter().all(|l| l.image.is_none())
        && [
            &style.border.top,
            &style.border.right,
            &style.border.bottom,
            &style.border.left,
        ]
        .iter()
        .all(|s| s.width == 0.0 || matches!(s.style, BorderStyle::None | BorderStyle::Hidden))
        && [
            &style.padding.top,
            &style.padding.right,
            &style.padding.bottom,
            &style.padding.left,
        ]
        .iter()
        .all(|l| is_zero_length(l));
    if !bare {
        return None;
    }
    let (v, h, background) = match control {
        Control::Button { .. } => (1.0, 6.0, TRACK),
        Control::Text | Control::TextArea | Control::Number | Control::Select => {
            (1.0, 2.0, Rgba::WHITE)
        }
        _ => return None,
    };
    let mut native = style.clone();
    native.background_color = background;
    native.padding.top = Length::px(v);
    native.padding.bottom = Length::px(v);
    native.padding.left = Length::px(h);
    native.padding.right = Length::px(h);
    let side = BorderSide {
        width: 1.0,
        style: BorderStyle::Solid,
        color: NATIVE_BORDER,
        explicit_color: Some(NATIVE_BORDER),
        explicit_width: Some(1.0),
    };
    native.border.top = side;
    native.border.right = side;
    native.border.bottom = side;
    native.border.left = side;
    let radius = h2q_css::LengthPercentage::px(2.0);
    native.border_radius = h2q_css::Corners {
        top_left: radius,
        top_right: radius,
        bottom_right: radius,
        bottom_left: radius,
    };
    Some(native)
}

fn set_font(obj: &mut Object, style: &ComputedStyle) {
    obj.set(
        "font.family",
        Value::string(font_family(&style.font_family)),
    );
    obj.set(
        "font.pixelSize",
        Value::Number(f64::from(style.font_size).round().max(1.0)),
    );
    if style.font_weight != 400 {
        obj.set("font.weight", Value::Number(f64::from(style.font_weight)));
    }
    if style.font_style != h2q_css::FontStyle::Normal {
        obj.set("font.italic", true);
    }
}

fn object_value(obj: Object) -> Value {
    Value::Object(Box::new(obj))
}

impl Emitter<'_, '_> {
    /// Every control has an id: labels, radio groups and form resets refer
    /// to it, possibly before it is emitted.
    pub(super) fn control_id(&mut self, node: NodeId) -> String {
        if let Some(id) = self.control_ids.get(&node) {
            return id.clone();
        }
        let base = self
            .doc
            .element(node)
            .and_then(|e| e.id().or(e.attr("name")).map(str::to_owned))
            .or_else(|| {
                self.doc
                    .element(node)
                    .map(|e| format!("{} control", e.local_name()))
            })
            .unwrap_or_else(|| "control".to_owned());
        let id = self.ids.claim(&base);
        self.control_ids.insert(node, id.clone());
        id
    }

    /// A `<label>` focuses or toggles its control when tapped. A tap on the
    /// control itself never reaches this handler: the control takes the
    /// press, and `TapHandler` gives up when another item grabs the point.
    pub(super) fn label_tap(&mut self, obj: &mut Object, node: Option<NodeId>) {
        let Some(label) = node else { return };
        let Some(control) = labeled_control(self.doc, label) else {
            return;
        };
        let Some(kind) = classify(self.doc, control) else {
            return;
        };
        self.tapped_labels.insert(label);
        let id = self.control_id(control);
        let action = if kind.is_clickable() {
            format!("{id}.click();")
        } else {
            format!("{id}.forceActiveFocus();")
        };
        let mut tap = Object::new("TapHandler");
        tap.members.push(Member::Handler {
            name: "onTapped".to_owned(),
            params: vec!["eventPoint".to_owned(), "button".to_owned()],
            body: vec![format!("if ({id}.enabled) {{ {action} }}")],
        });
        obj.push_child(tap);
    }

    /// Labels that became part of a paragraph's `Text` have no object to
    /// handle a tap.
    pub(super) fn report_inline_labels(&mut self) {
        let labels: Vec<NodeId> = self
            .doc
            .descendants(self.doc.root())
            .filter(|&n| labeled_control(self.doc, n).is_some())
            .filter(|n| !self.tapped_labels.contains(n))
            .collect();
        for label in labels {
            let line = crate::boxes::node_line(self.doc, label);
            self.diags.info(
                "html",
                line,
                format!(
                    "{} is inline, part of its paragraph's text: tapping it does not reach its control",
                    describe(self.doc, label)
                ),
            );
        }
    }

    /// Reset buttons, added once every control of their form has said how
    /// to restore itself: the button may come first.
    pub(super) fn finish_resets(&mut self, root: &mut Object) {
        for form in self.forms.values() {
            for button in &form.reset_buttons {
                if let Some(obj) = crate::scripts::find_mut(root, button) {
                    crate::scripts::add_handler(obj, "onClicked", &[], form.reset.clone());
                }
            }
        }
    }

    pub(super) fn control(
        &mut self,
        b: &LayoutBox<'_>,
        node: NodeId,
        control: Control,
        line: Option<u32>,
    ) -> Emitted {
        let native = with_native_box(&b.style, control);
        let style = native.as_ref().unwrap_or(&*b.style);
        let id = self.control_id(node);
        let reset = form_owner(self.doc, node);
        let mut obj = match control {
            Control::Text => self.text_field(node, style, &id, reset, line),
            Control::TextArea => self.text_area(node, style, &id, reset, line),
            Control::Number => self.spin_box(node, style, &id, reset, line),
            Control::Range => self.slider(node, &id, reset),
            Control::CheckBox | Control::Radio => self.toggle(node, style, control, &id, reset),
            Control::Button { .. } => self.button(node, style, line),
            Control::Select => self.combo_box(node, style, &id, reset, line),
            Control::Progress | Control::Meter => self.progress(node, style, control, &id, line),
        };
        if let Control::Button {
            submit,
            reset: is_reset,
        } = control
            && let Some(form) = reset
        {
            let entry = self.forms.entry(form).or_default();
            if submit {
                entry.submitters.push(id.clone());
            }
            if is_reset {
                entry.reset_buttons.push(id.clone());
            }
        }
        let element = self.doc.element(node);
        if element.is_some_and(|e| e.has_attr("disabled")) {
            obj.set("enabled", false);
        }
        if let Some(name) = element.and_then(|e| e.attr("name")) {
            obj.members.push(Member::PropertyDecl {
                ty: "string".to_owned(),
                name: "name".to_owned(),
                value: Some(Value::string(name)),
                readonly: false,
                required: false,
            });
        }
        if let Some(text) = label_text(self.doc, node) {
            obj.set("Accessible.name", Value::String(text));
        }

        // The TextArea scrolls inside a ScrollView that takes the layout
        // position; everything above belongs to the TextArea itself.
        let wanted = self.wanted.contains(&node);
        let mut outer = if control == Control::TextArea && !wanted {
            self.scrolling(obj, style)
        } else {
            obj
        };
        if outer.id.is_none() {
            outer.id = Some(id.clone());
        }
        if wanted {
            if let Some(html_id) = element.and_then(|e| e.id()) {
                outer.set("objectName", Value::string(html_id));
            }
            self.targets.insert(
                node,
                Target {
                    qml_id: id,
                    kind: control.target_kind(),
                },
            );
        }
        self.visibility(&mut outer, &b.style);
        let mt = self.vertical_margin(&style.margin.top, line);
        let mb = self.vertical_margin(&style.margin.bottom, line);
        Emitted {
            obj: outer,
            top: MarginSet::of(mt),
            bottom: MarginSet::of(mb),
            collapse_through: false,
        }
    }

    /// The element's box as the control's `background` (and the control's
    /// padding from its insets), so the Basic style's look never shows.
    fn apply_box(&mut self, obj: &mut Object, style: &ComputedStyle, line: Option<u32>) -> f32 {
        let deco = self.decoration(style, true, line);
        let insets = deco.insets;
        let mut background = self.wrap(style, &deco, true, None, false, line);
        // The control sizes its background; the insets are its padding.
        background.members.retain(
            |m| !matches!(m, Member::Property { name, .. } if name.starts_with("implicit")),
        );
        obj.set("background", object_value(background));
        obj.set("topPadding", number(insets.top));
        obj.set("rightPadding", number(insets.right));
        obj.set("bottomPadding", number(insets.bottom));
        obj.set("leftPadding", number(insets.left));
        insets.vertical()
    }

    /// `line-height` has no equivalent on text controls, so it decides the
    /// control's height instead, as it decides a browser's.
    fn line_height_implicit(&mut self, obj: &mut Object, style: &ComputedStyle, vertical: f32) {
        if let Some(lh) = line_box(style) {
            obj.set("implicitHeight", Value::Number(round(lh + vertical)));
        }
    }

    fn text_align(obj: &mut Object, style: &ComputedStyle) {
        let align = match style.text_align {
            TextAlign::Right | TextAlign::End => Some("Text.AlignRight"),
            TextAlign::Center => Some("Text.AlignHCenter"),
            _ => None,
        };
        if let Some(a) = align {
            obj.set("horizontalAlignment", Value::expr(a));
        }
    }

    fn placeholder(&mut self, obj: &mut Object, node: NodeId) {
        let Some(element) = self.doc.element(node) else {
            return;
        };
        let Some(text) = element.attr("placeholder") else {
            return;
        };
        obj.set("placeholderText", Value::string(text));
        let styled = self
            .styles
            .computed(node)
            .and_then(|s| s.placeholder.as_deref());
        obj.set(
            "placeholderTextColor",
            color(styled.map_or(PLACEHOLDER, |s| s.color)),
        );
    }

    fn text_field(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        id: &str,
        form: Option<NodeId>,
        line: Option<u32>,
    ) -> Object {
        let element = self.doc.element(node);
        let ty = element
            .and_then(|e| e.attr("type"))
            .unwrap_or("text")
            .to_ascii_lowercase();
        let mut obj = Object::new("TextField");
        let value = element.and_then(|e| e.attr("value")).unwrap_or("");
        if !value.is_empty() {
            obj.set("text", Value::string(value));
        }
        obj.set("color", color(style.color));
        set_font(&mut obj, style);
        Self::text_align(&mut obj, style);
        obj.set("verticalAlignment", Value::expr("TextInput.AlignVCenter"));
        let vertical = self.apply_box(&mut obj, style, line);
        self.line_height_implicit(&mut obj, style, vertical);
        let insets = self.insets(style, line);
        let size = attr_f64(self.doc, node, "size")
            .filter(|s| *s > 0.0)
            .unwrap_or(20.0);
        obj.set(
            "implicitWidth",
            Value::Number(round(
                size as f32 * AVG_CHAR * style.font_size + insets.horizontal(),
            )),
        );
        self.placeholder(&mut obj, node);
        match ty.as_str() {
            "password" => {
                obj.set("echoMode", Value::expr("TextInput.Password"));
            }
            "email" => {
                obj.set("inputMethodHints", Value::expr("Qt.ImhEmailCharactersOnly"));
            }
            "tel" => {
                obj.set(
                    "inputMethodHints",
                    Value::expr("Qt.ImhDialableCharactersOnly"),
                );
            }
            "url" => {
                obj.set("inputMethodHints", Value::expr("Qt.ImhUrlCharactersOnly"));
            }
            "text" | "search" | "hidden" => {}
            other => self.diags.warning(
                "html",
                line,
                format!("`<input type={other}>` has no picker yet; it is a plain text field"),
            ),
        }
        if element.is_some_and(|e| e.has_attr("readonly")) {
            obj.set("readOnly", true);
        }
        if let Some(max) = attr_f64(self.doc, node, "maxlength").filter(|m| *m >= 0.0) {
            obj.set("maximumLength", Value::Number(max.trunc()));
        }
        if let Some(form) = form {
            let entry = self.forms.entry(form).or_default();
            entry.accepting.push(id.to_owned());
            entry
                .reset
                .push(format!("{id}.text = {};", js_string(value)));
        }
        obj
    }

    fn text_area(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        id: &str,
        form: Option<NodeId>,
        line: Option<u32>,
    ) -> Object {
        let mut obj = Object::new("TextArea");
        // The first newline right after the start tag is not content
        // (HTML §13.2.6.4.7), and html5ever already dropped it.
        let value = self.doc.text_content(node);
        if !value.is_empty() {
            obj.set("text", Value::string(value.as_str()));
        }
        obj.set("color", color(style.color));
        set_font(&mut obj, style);
        Self::text_align(&mut obj, style);
        obj.set("wrapMode", Value::expr("TextEdit.Wrap"));
        self.apply_box(&mut obj, style, line);
        self.placeholder(&mut obj, node);
        let element = self.doc.element(node);
        if element.is_some_and(|e| e.has_attr("readonly")) {
            obj.set("readOnly", true);
        }
        if element.is_some_and(|e| e.has_attr("maxlength")) {
            self.diags.warning(
                "html",
                line,
                "`maxlength` on `<textarea>` is not enforced: TextArea has no such limit"
                    .to_owned(),
            );
        }
        if self.wanted.contains(&node) {
            // Scripts address the object that is laid out, so this one
            // cannot sit in a ScrollView.
            obj.set("clip", true);
            self.diags.info(
                "html",
                line,
                format!(
                    "{} is used by a script: it clips rather than scrolls",
                    describe(self.doc, node)
                ),
            );
            let (w, h) = self.text_area_size(node, style, line);
            obj.set("implicitWidth", number(w));
            obj.set("implicitHeight", number(h));
        }
        if let Some(form) = form {
            self.forms
                .entry(form)
                .or_default()
                .reset
                .push(format!("{id}.text = {};", js_string(&value)));
        }
        obj
    }

    fn text_area_size(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        line: Option<u32>,
    ) -> (f32, f32) {
        let insets = self.insets(style, line);
        let rows = attr_f64(self.doc, node, "rows")
            .filter(|r| *r > 0.0)
            .unwrap_or(2.0);
        let cols = attr_f64(self.doc, node, "cols")
            .filter(|c| *c > 0.0)
            .unwrap_or(20.0);
        let lh = line_box(style).unwrap_or(style.font_size * 1.15);
        (
            cols as f32 * AVG_CHAR * style.font_size + insets.horizontal(),
            rows as f32 * lh + insets.vertical(),
        )
    }

    fn scrolling(&mut self, area: Object, style: &ComputedStyle) -> Object {
        let node = self
            .control_ids
            .iter()
            .find(|(_, id)| Some(id.as_str()) == area.id.as_deref())
            .map(|(n, _)| *n);
        let mut view = Object::new("ScrollView");
        if let Some(node) = node {
            let (w, h) = self.text_area_size(node, style, None);
            view.set("implicitWidth", number(w));
            view.set("implicitHeight", number(h));
        }
        view.push_child(area);
        view
    }

    fn spin_box(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        id: &str,
        form: Option<NodeId>,
        line: Option<u32>,
    ) -> Object {
        let mut obj = Object::new("SpinBox");
        let number_attr = |name| attr_f64(self.doc, node, name);
        let (min, max, step, value) = (
            number_attr("min"),
            number_attr("max"),
            number_attr("step"),
            number_attr("value"),
        );
        if [min, max, step, value]
            .iter()
            .flatten()
            .any(|v| v.fract() != 0.0)
        {
            self.diags.warning(
                "html",
                line,
                format!(
                    "{} has fractional values; SpinBox holds integers, so they are rounded",
                    describe(self.doc, node)
                ),
            );
        }
        // Without limits a number input takes any value; SpinBox's default
        // is 0–99.
        let from = min.map_or(-2_147_483_648.0, f64::round);
        let to = max.map_or(2_147_483_647.0, f64::round);
        let initial = value.map_or(from.max(0.0).min(to), f64::round);
        obj.set("from", Value::Number(from));
        obj.set("to", Value::Number(to));
        if let Some(step) = step.filter(|s| *s >= 1.0) {
            obj.set("stepSize", Value::Number(step.round()));
        }
        obj.set("value", Value::Number(initial));
        obj.set("editable", true);
        set_font(&mut obj, style);
        let vertical = self.apply_box(&mut obj, style, line);
        self.line_height_implicit(&mut obj, style, vertical);
        let insets = self.insets(style, line);
        obj.set(
            "implicitWidth",
            Value::Number(round(
                20.0 * AVG_CHAR * style.font_size + insets.horizontal(),
            )),
        );
        // The Basic style's +/- buttons take the field's space; a browser
        // shows only small spinners on hover, and arrow keys step here too.
        obj.set("up.indicator", object_value(Object::new("Item")));
        obj.set("down.indicator", object_value(Object::new("Item")));
        let mut input = Object::new("TextInput");
        input.set("text", Value::expr(format!("{id}.displayText")));
        input.set("font", Value::expr(format!("{id}.font")));
        input.set("color", color(style.color));
        input.set("verticalAlignment", Value::expr("Qt.AlignVCenter"));
        input.set("readOnly", Value::expr(format!("!{id}.editable")));
        input.set("validator", Value::expr(format!("{id}.validator")));
        input.set(
            "inputMethodHints",
            Value::expr("Qt.ImhFormattedNumbersOnly"),
        );
        obj.set("contentItem", object_value(input));
        if let Some(form) = form {
            self.forms
                .entry(form)
                .or_default()
                .reset
                .push(format!("{id}.value = {initial};"));
        }
        obj
    }

    fn slider(&mut self, node: NodeId, id: &str, form: Option<NodeId>) -> Object {
        let mut obj = Object::new("Slider");
        let from = attr_f64(self.doc, node, "min").unwrap_or(0.0);
        let to = attr_f64(self.doc, node, "max").unwrap_or(100.0);
        // HTML's default value is the midpoint.
        let initial = attr_f64(self.doc, node, "value").unwrap_or(from + (to - from) / 2.0);
        obj.set("from", Value::Number(from));
        obj.set("to", Value::Number(to));
        obj.set(
            "stepSize",
            Value::Number(attr_f64(self.doc, node, "step").unwrap_or(1.0)),
        );
        obj.set("snapMode", Value::expr("Slider.SnapAlways"));
        obj.set("value", Value::Number(initial));
        obj.set("implicitWidth", Value::Number(129.0));
        obj.set("implicitHeight", Value::Number(16.0));
        obj.set("padding", Value::Number(0.0));
        let mut track = Object::new("Rectangle");
        track.set("y", Value::expr(format!("({id}.height - height) / 2")));
        track.set("width", Value::expr(format!("{id}.width")));
        track.set("height", Value::Number(8.0));
        track.set("radius", Value::Number(4.0));
        track.set("color", color(TRACK));
        track.set("border.color", color(NATIVE_BORDER));
        track.push_child({
            let mut fill = Object::new("Rectangle");
            fill.set(
                "width",
                Value::expr(format!("{id}.visualPosition * parent.width")),
            );
            fill.set("height", Value::expr("parent.height"));
            fill.set("radius", Value::Number(4.0));
            fill.set("color", color(ACCENT));
            fill
        });
        obj.set("background", object_value(track));
        let mut handle = Object::new("Rectangle");
        handle.set(
            "x",
            Value::expr(format!("{id}.visualPosition * ({id}.width - width)")),
        );
        handle.set("y", Value::expr(format!("({id}.height - height) / 2")));
        handle.set("width", Value::Number(16.0));
        handle.set("height", Value::Number(16.0));
        handle.set("radius", Value::Number(8.0));
        handle.set("color", color(ACCENT));
        obj.set("handle", object_value(handle));
        if let Some(form) = form {
            self.forms
                .entry(form)
                .or_default()
                .reset
                .push(format!("{id}.value = {initial};"));
        }
        obj
    }

    fn toggle(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        control: Control,
        id: &str,
        form: Option<NodeId>,
    ) -> Object {
        let radio = control == Control::Radio;
        let mut obj = Object::new(if radio { "RadioButton" } else { "CheckBox" });
        let element = self.doc.element(node);
        let checked = element.is_some_and(|e| e.has_attr("checked"));
        if checked {
            obj.set("checked", true);
        }
        obj.members.push(Member::PropertyDecl {
            ty: "string".to_owned(),
            name: "value".to_owned(),
            value: Some(Value::string(
                element.and_then(|e| e.attr("value")).unwrap_or("on"),
            )),
            readonly: false,
            required: false,
        });
        obj.set("implicitWidth", Value::Number(13.0));
        obj.set("implicitHeight", Value::Number(13.0));
        obj.set("padding", Value::Number(0.0));
        obj.set("spacing", Value::Number(0.0));
        obj.set("contentItem", object_value(Object::new("Item")));
        let background = if style.background_color.is_transparent() {
            Rgba::WHITE
        } else {
            style.background_color
        };
        let mut indicator = Object::new("Rectangle");
        indicator.set("width", Value::expr(format!("{id}.width")));
        indicator.set("height", Value::expr(format!("{id}.height")));
        indicator.set("border.width", Value::Number(1.0));
        if radio {
            indicator.set("radius", Value::expr("width / 2"));
            indicator.set("color", color(background));
            indicator.set(
                "border.color",
                Value::expr(format!(
                    "{id}.checked ? {} : {}",
                    color_literal(ACCENT),
                    color_literal(NATIVE_BORDER)
                )),
            );
            let mut dot = Object::new("Rectangle");
            dot.set("anchors.centerIn", Value::expr("parent"));
            dot.set("width", Value::expr("parent.width * 0.6"));
            dot.set("height", Value::expr("width"));
            dot.set("radius", Value::expr("width / 2"));
            dot.set("color", color(ACCENT));
            dot.set("visible", Value::expr(format!("{id}.checked")));
            indicator.push_child(dot);
        } else {
            indicator.set("radius", Value::Number(2.0));
            indicator.set(
                "color",
                Value::expr(format!(
                    "{id}.checked ? {} : {}",
                    color_literal(ACCENT),
                    color_literal(background)
                )),
            );
            indicator.set(
                "border.color",
                Value::expr(format!(
                    "{id}.checked ? {} : {}",
                    color_literal(ACCENT),
                    color_literal(NATIVE_BORDER)
                )),
            );
            let mut mark = Object::new("Text");
            mark.set("anchors.centerIn", Value::expr("parent"));
            mark.set("text", Value::string("\u{2713}"));
            mark.set("color", color(Rgba::WHITE));
            mark.set(
                "font.pixelSize",
                Value::expr(format!("Math.max(1, {id}.height - 2)")),
            );
            mark.set("visible", Value::expr(format!("{id}.checked")));
            indicator.push_child(mark);
        }
        obj.set("indicator", object_value(indicator));
        if radio {
            let name = element.and_then(|e| e.attr("name")).unwrap_or("");
            if name.is_empty() {
                // Radio buttons in one parent would otherwise exclude each
                // other; unnamed ones do not in HTML.
                obj.set("autoExclusive", false);
            } else {
                let group = self.radio_group(form, name);
                obj.set("ButtonGroup.group", Value::expr(group));
            }
        }
        if let Some(form) = form {
            self.forms
                .entry(form)
                .or_default()
                .reset
                .push(format!("{id}.checked = {checked};"));
        }
        obj
    }

    /// Radios of one name in one form exclude each other (HTML §4.10.5.1.15).
    fn radio_group(&mut self, form: Option<NodeId>, name: &str) -> String {
        let key = (form, name.to_owned());
        if let Some(id) = self.radio_groups.get(&key) {
            return id.clone();
        }
        let id = self.ids.claim(&format!("{name} group"));
        self.radio_groups.insert(key, id.clone());
        id
    }

    fn button(&mut self, node: NodeId, style: &ComputedStyle, line: Option<u32>) -> Object {
        let element = self.doc.element(node);
        let mut obj = Object::new("Button");
        let is_input = element.is_some_and(|e| e.local_name() == "input");
        let text = if is_input {
            let ty = element
                .and_then(|e| e.attr("type"))
                .unwrap_or("")
                .to_ascii_lowercase();
            let fallback = match ty.as_str() {
                "submit" => "Submit",
                "reset" => "Reset",
                _ => "",
            };
            element
                .and_then(|e| e.attr("value"))
                .unwrap_or(fallback)
                .to_owned()
        } else {
            if self.doc.element_children(node).next().is_some() {
                self.diags.info(
                    "html",
                    line,
                    format!(
                        "{} has element children; the Button shows only their text",
                        describe(self.doc, node)
                    ),
                );
            }
            h2q_dom::collapse_whitespace(&self.doc.text_content(node))
        };
        obj.set("text", Value::String(text));
        // The Basic style's IconLabel takes its colour from the palette.
        obj.set("palette.buttonText", color(style.color));
        set_font(&mut obj, style);
        let vertical = self.apply_box(&mut obj, style, line);
        self.line_height_implicit(&mut obj, style, vertical);
        obj
    }

    fn combo_box(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        id: &str,
        form: Option<NodeId>,
        line: Option<u32>,
    ) -> Object {
        let mut options = Vec::new();
        let mut selected = None;
        let mut flattened = false;
        for option in self.doc.descendants(node) {
            let Some(e) = self.doc.element(option) else {
                continue;
            };
            flattened |= e.local_name() == "optgroup";
            if e.local_name() != "option" {
                continue;
            }
            let text = h2q_dom::collapse_whitespace(
                e.attr("label")
                    .map(str::to_owned)
                    .unwrap_or_else(|| self.doc.text_content(option))
                    .as_str(),
            );
            let value = e.attr("value").map_or_else(|| text.clone(), str::to_owned);
            if e.has_attr("selected") {
                selected = Some(options.len());
            }
            options.push(format!(
                "{{ text: {}, value: {} }}",
                js_string(&text),
                js_string(&value)
            ));
        }
        let element = self.doc.element(node);
        if element.is_some_and(|e| e.has_attr("multiple") || e.has_attr("size")) {
            self.diags.warning(
                "html",
                line,
                format!(
                    "{} is a list box in a browser; it is a single-choice ComboBox here",
                    describe(self.doc, node)
                ),
            );
        }
        if flattened {
            self.diags.info(
                "html",
                line,
                "`<optgroup>` headings are dropped: ComboBox has flat options".to_owned(),
            );
        }
        let mut obj = Object::new("ComboBox");
        obj.set("model", Value::expr(format!("[{}]", options.join(", "))));
        obj.set("textRole", Value::string("text"));
        obj.set("valueRole", Value::string("value"));
        let index = selected.unwrap_or(0);
        if index != 0 {
            obj.set("currentIndex", Value::Number(index as f64));
        }
        obj.set(
            "implicitContentWidthPolicy",
            Value::expr("ComboBox.WidestText"),
        );
        set_font(&mut obj, style);
        let vertical = self.apply_box(&mut obj, style, line);
        self.line_height_implicit(&mut obj, style, vertical);
        let insets = self.insets(style, line);
        obj.set("rightPadding", number(insets.right + ARROW_ROOM));
        // The Basic style's contentItem is a read-only TextField with its own
        // padding, which clips the text in a box of CSS height.
        let mut content = Object::new("Text");
        content.set("text", Value::expr(format!("{id}.displayText")));
        content.set("font", Value::expr(format!("{id}.font")));
        content.set("color", color(style.color));
        content.set("verticalAlignment", Value::expr("Text.AlignVCenter"));
        content.set("elide", Value::expr("Text.ElideRight"));
        obj.set("contentItem", object_value(content));
        let mut arrow = Object::new("Text");
        arrow.set(
            "x",
            Value::expr(format!("{id}.width - width - {}", insets.right.max(4.0))),
        );
        arrow.set("y", Value::expr(format!("({id}.height - height) / 2")));
        arrow.set("text", Value::string("\u{25be}"));
        arrow.set("color", color(style.color));
        arrow.set("font.pixelSize", Value::Number(12.0));
        obj.set("indicator", object_value(arrow));
        if let Some(form) = form {
            self.forms
                .entry(form)
                .or_default()
                .reset
                .push(format!("{id}.currentIndex = {index};"));
        }
        obj
    }

    fn progress(
        &mut self,
        node: NodeId,
        style: &ComputedStyle,
        control: Control,
        id: &str,
        line: Option<u32>,
    ) -> Object {
        let mut obj = Object::new("ProgressBar");
        let meter = control == Control::Meter;
        let from = if meter {
            attr_f64(self.doc, node, "min").unwrap_or(0.0)
        } else {
            0.0
        };
        let to = attr_f64(self.doc, node, "max")
            .filter(|m| *m > from)
            .unwrap_or(1.0);
        obj.set("from", Value::Number(from));
        obj.set("to", Value::Number(to));
        match attr_f64(self.doc, node, "value") {
            Some(v) => {
                obj.set("value", Value::Number(v));
            }
            None if meter => {
                obj.set("value", Value::Number(from));
            }
            None => {
                obj.set("indeterminate", true);
                self.diags.info(
                    "html",
                    line,
                    "indeterminate `<progress>` shows an empty track, without a browser's animation"
                        .to_owned(),
                );
            }
        }
        let default_width = if meter { 80.0 } else { 160.0 };
        if style.width == Size::Auto {
            obj.set("implicitWidth", Value::Number(default_width));
        }
        obj.set("implicitHeight", Value::Number(16.0));
        let bar = if meter {
            self.meter_color(node, id, from, to)
        } else {
            color(ACCENT)
        };
        let mut track = Object::new("Rectangle");
        let background = if style.background_color.is_transparent() {
            TRACK
        } else {
            style.background_color
        };
        track.set("color", color(background));
        track.set("radius", Value::Number(3.0));
        obj.set("background", object_value(track));
        let mut fill = Object::new("Rectangle");
        fill.set(
            "width",
            Value::expr(format!("{id}.visualPosition * parent.width")),
        );
        fill.set("height", Value::expr("parent.height"));
        fill.set("radius", Value::Number(3.0));
        fill.set("color", bar);
        obj.set(
            "contentItem",
            object_value({
                let mut item = Object::new("Item");
                item.push_child(fill);
                item
            }),
        );
        obj
    }

    /// The regions of HTML §4.10.14: the bar is green where `value` is on
    /// `optimum`'s side, yellow one region away, red two away. A binding,
    /// because scripts set `value`.
    fn meter_color(&self, node: NodeId, id: &str, from: f64, to: f64) -> Value {
        let low = attr_f64(self.doc, node, "low").unwrap_or(from);
        let high = attr_f64(self.doc, node, "high").unwrap_or(to);
        let optimum = attr_f64(self.doc, node, "optimum").unwrap_or((from + to) / 2.0);
        let (good, ok, bad) = (
            color_literal(Rgba::new(0x10, 0x7c, 0x10, 0xff)),
            color_literal(Rgba::new(0xff, 0xb9, 0x00, 0xff)),
            color_literal(Rgba::new(0xd1, 0x34, 0x38, 0xff)),
        );
        let (below, middle, above) = if optimum < low {
            (&good, &ok, &bad)
        } else if optimum > high {
            (&bad, &ok, &good)
        } else {
            (&ok, &good, &ok)
        };
        Value::expr(format!(
            "{id}.value < {low} ? {below} : ({id}.value > {high} ? {above} : {middle})"
        ))
    }

    /// `<fieldset>` with a `<legend>`: a GroupBox, whose label sits on the
    /// top border and whose content starts below the taller of the two.
    pub(super) fn fieldset(
        &mut self,
        b: &LayoutBox<'_>,
        node: NodeId,
        legend: &LayoutBox<'_>,
        rest: &[LayoutBox<'_>],
        decoration: h2q_css::TextDecorationLine,
        line: Option<u32>,
    ) -> Emitted {
        let style = &*b.style;
        let html_id = self
            .doc
            .element(node)
            .and_then(|e| e.id())
            .map(str::to_owned);
        let id = self.ids.claim(html_id.as_deref().unwrap_or("fieldset"));
        let mut obj = Object::new("GroupBox").with_id(id.clone());
        let deco = self.decoration(style, true, line);
        let insets = deco.insets;
        let border_top = super::decoration::border_width(&style.border.top);
        let mut background = self.wrap(style, &deco, true, None, false, line);
        background.members.retain(
            |m| !matches!(m, Member::Property { name, .. } if name.starts_with("implicit")),
        );
        // The border runs through the middle of the legend.
        background.set(
            "y",
            Value::expr(format!(
                "Math.max(0, ({id}.implicitLabelHeight - {}) / 2)",
                units_fmt(border_top)
            )),
        );
        background.set("height", Value::expr(format!("{id}.height - y")));
        obj.set("background", object_value(background));
        let mut label = self.item(legend, decoration).obj;
        label.set("x", number(insets.left));
        obj.set("label", object_value(label));
        obj.set(
            "topPadding",
            Value::expr(format!(
                "Math.max({id}.implicitLabelHeight, {}) + {}",
                units_fmt(border_top),
                units_fmt(insets.top - border_top)
            )),
        );
        obj.set("rightPadding", number(insets.right));
        obj.set("bottomPadding", number(insets.bottom));
        obj.set("leftPadding", number(insets.left));
        if let Some(mut column) = self.column(rest, decoration, false, false).obj {
            column.set("anchors.left", Value::expr("parent.left"));
            column.set("anchors.right", Value::expr("parent.right"));
            obj.push_child(column);
        }
        if self
            .doc
            .element(node)
            .is_some_and(|e| e.has_attr("disabled"))
        {
            // Item.enabled is inherited by every control inside.
            obj.set("enabled", false);
        }
        if self.wanted.contains(&node) {
            if let Some(html_id) = html_id {
                obj.set("objectName", Value::String(html_id));
            }
            self.targets.insert(
                node,
                Target {
                    qml_id: id,
                    kind: TargetKind::Box,
                },
            );
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

fn units_fmt(v: f32) -> String {
    super::units::fmt_number(round(v))
}

fn color_literal(c: Rgba) -> String {
    match color(c) {
        Value::Color(c) => format!("\"{}\"", c.to_qml_string()),
        other => format!("{other:?}"),
    }
}

/// A JavaScript string literal; JSON's escapes are a subset of it.
pub(crate) fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Keeps `Map`'s type out of the `Emitter` field list.
pub(crate) type RadioGroups = BTreeMap<(Option<NodeId>, String), String>;

#[cfg(test)]
mod tests {
    use crate::{Options, convert_str};

    fn qml(body: &str) -> (String, Vec<String>) {
        let html = format!("<!doctype html><html><body>{body}</body></html>");
        let out = convert_str(&html, &Options::default());
        (
            out.main_qml().expect("Main.qml").to_owned(),
            out.diagnostics.iter().map(ToString::to_string).collect(),
        )
    }

    #[test]
    fn text_input_carries_its_attributes() {
        let (q, _) = qml(
            r#"<input type=password id=p name=pw value="v" placeholder="ph" maxlength=8 readonly disabled>"#,
        );
        for want in [
            "TextField {",
            "id: p",
            "text: \"v\"",
            "placeholderText: \"ph\"",
            "echoMode: TextInput.Password",
            "maximumLength: 8",
            "readOnly: true",
            "enabled: false",
            "property string name: \"pw\"",
        ] {
            assert!(q.contains(want), "no `{want}` in\n{q}");
        }
        assert!(!q.contains("not converted"));
    }

    #[test]
    fn input_types_choose_the_control_and_hints() {
        let (q, _) = qml(
            "<input type=email><input type=tel><input type=url><input type=search>\
             <input type=number min=1 max=9 value=3 step=2><input type=range min=0 max=10 value=4 step=2>\
             <input type=checkbox checked><input type=submit value=Go><input type=reset><input type=button value=B>",
        );
        for want in [
            "Qt.ImhEmailCharactersOnly",
            "Qt.ImhDialableCharactersOnly",
            "Qt.ImhUrlCharactersOnly",
            "SpinBox {",
            "from: 1",
            "to: 9",
            "stepSize: 2",
            "Slider {",
            "CheckBox {",
            "checked: true",
            "text: \"Go\"",
            "text: \"Reset\"",
            "text: \"B\"",
        ] {
            assert!(q.contains(want), "no `{want}` in\n{q}");
        }
    }

    #[test]
    fn hidden_inputs_generate_nothing_unless_a_script_wants_them() {
        let (q, _) = qml("<input type=hidden name=csrf value=x><p>p</p>");
        assert!(!q.contains("TextField"), "{q}");
        assert!(!q.contains("csrf"), "{q}");
    }

    #[test]
    fn css_styles_the_control_not_the_basic_style() {
        let html = "<!doctype html><style>input{padding:9px 12px;border:1px solid #cfd6e4;\
            border-radius:8px;background:#fff;color:#123456;font-size:15px;line-height:20px}\
            input::placeholder{color:#abcdef}</style><input placeholder=p>";
        let out = convert_str(html, &Options::default());
        let q = out.main_qml().expect("Main.qml");
        for want in [
            "topPadding: 10",
            "leftPadding: 13",
            "border.width: 1",
            "border.color: \"#cfd6e4\"",
            "radius: 8",
            "color: \"#123456\"",
            "font.pixelSize: 15",
            "implicitHeight: 40",
            "placeholderTextColor: \"#abcdef\"",
        ] {
            assert!(q.contains(want), "no `{want}` in\n{q}");
        }
    }

    #[test]
    fn a_bare_control_gets_the_native_box() {
        let (q, _) = qml("<button>Go</button>");
        assert!(q.contains("Button {"), "{q}");
        assert!(q.contains("text: \"Go\""), "{q}");
        assert!(q.contains("border.color: \"#767676\""), "{q}");
    }

    #[test]
    fn select_keeps_values_and_the_selected_option() {
        let (q, _) = qml(
            r#"<select name=s><option value=a>Alpha</option><option selected>Beta</option></select>"#,
        );
        assert!(q.contains(r#"{ text: "Alpha", value: "a" }"#), "{q}");
        assert!(q.contains(r#"{ text: "Beta", value: "Beta" }"#), "{q}");
        assert!(q.contains("currentIndex: 1"), "{q}");
        assert!(q.contains("valueRole: \"value\""), "{q}");
    }

    #[test]
    fn radios_group_by_name_within_a_form() {
        let (q, _) = qml(
            "<form><input type=radio name=a value=1 checked><input type=radio name=a value=2>\
             <input type=radio name=b></form><input type=radio name=a><input type=radio>",
        );
        assert_eq!(q.matches("ButtonGroup {").count(), 3, "{q}");
        assert_eq!(q.matches("ButtonGroup.group:").count(), 4, "{q}");
        assert!(q.contains("autoExclusive: false"), "{q}");
        assert!(q.contains("property string value: \"2\""), "{q}");
    }

    #[test]
    fn labels_focus_or_toggle_their_control() {
        let (q, _) = qml(
            "<div style='display:flex'><label for=t>Name</label><input id=t></div>\
             <div style='display:flex'><label style='display:flex'><input type=checkbox id=c>Terms</label></div>",
        );
        assert!(q.contains("t.forceActiveFocus();"), "{q}");
        assert!(q.contains("c.click();"), "{q}");
        assert!(q.contains("Accessible.name: \"Name\""), "{q}");
        assert!(q.contains("Accessible.name: \"Terms\""), "{q}");
    }

    #[test]
    fn an_inline_label_is_reported() {
        let (_, d) = qml("<p><label for=t>Name</label></p><input id=t>");
        assert!(d.iter().any(|m| m.contains("is inline")), "{d:?}");
    }

    #[test]
    fn fieldset_with_legend_is_a_group_box() {
        let (q, _) = qml("<fieldset disabled><legend>Plan</legend><p>x</p></fieldset>");
        assert!(q.contains("GroupBox {"), "{q}");
        assert!(q.contains("label: "), "{q}");
        assert!(q.contains("text: \"Plan\""), "{q}");
        assert!(q.contains("enabled: false"), "{q}");
    }

    #[test]
    fn textarea_sizes_from_rows_and_scrolls() {
        let (q, _) = qml("<textarea rows=4 cols=10>hi</textarea>");
        assert!(q.contains("ScrollView {"), "{q}");
        assert!(q.contains("TextArea {"), "{q}");
        assert!(q.contains("text: \"hi\""), "{q}");
    }

    #[test]
    fn progress_and_meter_map_to_a_bar() {
        let (q, _) = qml("<progress value=3 max=10></progress><progress></progress>\
             <meter min=0 max=10 low=2 high=8 optimum=9 value=5></meter>");
        assert_eq!(q.matches("ProgressBar {").count(), 3, "{q}");
        assert!(q.contains("indeterminate: true"), "{q}");
        assert!(q.contains("value < 2"), "{q}");
    }

    #[test]
    fn reset_buttons_restore_initial_state() {
        let (q, _) = qml(
            "<form><input id=t value=v><input type=checkbox id=c checked>\
             <button type=reset id=r>R</button></form>",
        );
        let at = q.find("id: r").expect("reset button");
        let rest = &q[at..];
        assert!(rest.contains("t.text = \"v\";"), "{q}");
        assert!(rest.contains("c.checked = true;"), "{q}");
    }
}
