//! UA defaults of form controls. Expected values come from Chromium
//! (`tools/visual/computed.mjs`).

use h2q_css::{BoxSizing, StylesheetLoader, Viewport, collect_stylesheets, compute_styles};
use h2q_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<h2q_css::LoadedSheet> {
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
