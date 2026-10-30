//! Table and list properties. Expected values come from Chromium
//! (`tools/visual/computed.mjs`).

use h2q_css::{
    BorderCollapse, CaptionSide, ListStyleType, Rgba, StylesheetLoader, TableLayout, Viewport,
    collect_stylesheets, compute_styles,
};
use h2q_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<h2q_css::LoadedSheet> {
        None
    }
}

fn styles(page: &str) -> (Document, h2q_css::StyleMap) {
    let doc = Document::parse(page);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    (doc, styles)
}

#[test]
fn table_properties_inherit_except_table_layout() {
    let (doc, styles) = styles(
        "<!DOCTYPE html><style>table{border-collapse:collapse;border-spacing:3px 5px;\
         caption-side:bottom;table-layout:fixed}</style>\
         <table id=table><tr><td id=t>x</table>",
    );
    let table = styles
        .get(doc.element_by_id("table").expect("styled"))
        .expect("styled");
    assert_eq!(table.style.table_layout, TableLayout::Fixed);
    let cell = styles
        .get(doc.element_by_id("t").expect("styled"))
        .expect("styled");
    assert_eq!(cell.style.border_collapse, BorderCollapse::Collapse);
    assert_eq!(cell.style.border_spacing, (3.0, 5.0));
    assert_eq!(cell.style.caption_side, CaptionSide::Bottom);
    assert_eq!(cell.style.table_layout, TableLayout::Auto);
}

#[test]
fn border_spacing_with_one_length_applies_to_both_axes() {
    let (doc, styles) = styles(
        "<!DOCTYPE html><style>table{border-spacing:4px}</style>\
         <table id=t><tr><td>x</table>",
    );
    let table = styles
        .get(doc.element_by_id("t").expect("styled"))
        .expect("styled");
    assert_eq!(table.style.border_spacing, (4.0, 4.0));
}

#[test]
fn ua_sheet_gives_tables_two_pixel_spacing() {
    let (doc, styles) = styles("<!DOCTYPE html><table id=t><tr><td>x</table>");
    let table = styles
        .get(doc.element_by_id("t").expect("styled"))
        .expect("styled");
    assert_eq!(table.style.border_spacing, (2.0, 2.0));
    assert_eq!(table.style.border_collapse, BorderCollapse::Separate);
}

#[test]
fn ol_type_attribute_is_case_sensitive() {
    let (doc, styles) = styles(
        "<!DOCTYPE html><ol id=l type=a><li id=li>x</ol><ol id=u type=A><li>x</ol>\
         <ol id=i type=i><li>x</ol><ol id=r type=I><li>x</ol>",
    );
    let ty = |id| {
        styles
            .get(doc.element_by_id(id).expect("styled"))
            .expect("styled")
            .style
            .list_style_type
    };
    assert_eq!(ty("l"), ListStyleType::LowerAlpha);
    assert_eq!(ty("li"), ListStyleType::LowerAlpha);
    assert_eq!(ty("u"), ListStyleType::UpperAlpha);
    assert_eq!(ty("i"), ListStyleType::LowerRoman);
    assert_eq!(ty("r"), ListStyleType::UpperRoman);
}

#[test]
fn marker_rules_apply_to_list_items_only() {
    let (doc, styles) = styles(
        "<!DOCTYPE html><style>.m::marker{color:rgb(255,0,0);font-size:30px}</style>\
         <ul><li id=li class=m>x</ul><div id=div class=m>y</div><ul><li id=plain>z</ul>",
    );
    let li = styles
        .get(doc.element_by_id("li").expect("styled"))
        .expect("styled");
    let marker = li.marker.as_ref().expect("marker style");
    assert_eq!(marker.color, Rgba::new(255, 0, 0, 255));
    assert_eq!(marker.font_size, 30.0);
    assert!(
        styles
            .get(doc.element_by_id("div").expect("styled"))
            .expect("styled")
            .marker
            .is_none()
    );
    assert!(
        styles
            .get(doc.element_by_id("plain").expect("styled"))
            .expect("styled")
            .marker
            .is_none()
    );
}
