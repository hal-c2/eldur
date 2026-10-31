//! UA defaults of form controls. Expected values come from Chromium
//! (`tools/visual/computed.mjs`).

use eldur_css::{
    AccentColor, BoxSizing, Length, Rgba, Size, StylesheetLoader, VerticalAlign, Viewport,
    collect_stylesheets, compute_styles,
};
use eldur_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
        None
    }
}

#[test]
fn ua_box_sizing_of_form_controls() {
    let doc = Document::parse(
        "<!DOCTYPE html><select id=select></select><button id=button></button>\
         <textarea id=textarea></textarea><input id=text><input id=number type=number>\
         <input id=checkbox type=checkbox><input id=submit type=submit>\
         <input id=range type=range><input id=color type=color>\
         <progress id=progress></progress><meter id=meter></meter>",
    );
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    for (id, expected) in [
        ("select", BoxSizing::BorderBox),
        ("button", BoxSizing::BorderBox),
        ("textarea", BoxSizing::ContentBox),
        ("text", BoxSizing::ContentBox),
        ("number", BoxSizing::ContentBox),
        ("checkbox", BoxSizing::BorderBox),
        ("submit", BoxSizing::BorderBox),
        ("range", BoxSizing::ContentBox),
        ("color", BoxSizing::BorderBox),
        ("progress", BoxSizing::BorderBox),
        ("meter", BoxSizing::BorderBox),
    ] {
        let style = styles
            .get(doc.element_by_id(id).expect("element"))
            .expect("styled");
        assert_eq!(style.style.box_sizing, expected, "{id}");
    }
}

fn styles(page: &str) -> (Document, eldur_css::StyleMap) {
    let doc = Document::parse(page);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    (doc, styles)
}

#[test]
fn accent_color_inherits_and_keeps_currentcolor() {
    // Chromium: rgb(79, 70, 229) inherited, `auto` by default, a
    // `currentcolor` that inherits as the keyword (the child's blue), and an
    // invalid value dropped.
    let (doc, styles) = styles(
        "<!DOCTYPE html><div style='accent-color:#4f46e5'><progress id=inherit></progress>\
         <p id=bad style='accent-color:bogus'>x</p></div><progress id=auto></progress>\
         <div style='color:red;accent-color:currentcolor'><p id=current style='color:blue'>x</p></div>",
    );
    let get = |id: &str| {
        styles
            .get(doc.element_by_id(id).expect("element"))
            .expect("styled")
            .style
            .clone()
    };
    let indigo = AccentColor::Color(Rgba::new(79, 70, 229, 255));
    assert_eq!(get("inherit").accent_color, indigo);
    assert_eq!(get("bad").accent_color, indigo);
    assert_eq!(get("auto").accent_color, AccentColor::Auto);
    let current = get("current");
    assert_eq!(current.accent_color, AccentColor::CurrentColor);
    assert_eq!(
        current.accent_color.resolve(current.color),
        Some(Rgba::new(0, 0, 255, 255))
    );
}

#[test]
fn ua_size_of_progress_and_meter() {
    // Chromium at font-size 20px: progress 200×20, meter 100×20, both
    // `vertical-align: -4px` (-0.2em).
    let (doc, styles) = styles(
        "<!DOCTYPE html><div style='font-size:20px'><progress id=progress></progress>\
         <meter id=meter></meter></div>",
    );
    for (id, width) in [("progress", 200.0), ("meter", 100.0)] {
        let style = &styles
            .get(doc.element_by_id(id).expect("element"))
            .expect("styled")
            .style;
        assert_eq!(style.width, Size::Length(Length::px(width)), "{id}");
        assert_eq!(style.height, Size::Length(Length::px(20.0)), "{id}");
        assert_eq!(style.vertical_align, VerticalAlign::Length(-4.0), "{id}");
    }
}
