//! Elements that are, or are inside, `display: none` have no box, but they
//! still have computed values (CSSOM `getComputedStyle` reports them). The
//! converter needs them for elements a script shows later.

use h2q_css::{Display, Rgba, StylesheetLoader, Viewport, collect_stylesheets, compute_styles};
use h2q_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<h2q_css::LoadedSheet> {
        None
    }
}

const PAGE: &str = r#"<!DOCTYPE html>
<style>
  body { color: rgb(0, 0, 255); font-size: 20px; }
  .panel { display: none; color: rgb(255, 0, 0); }
  .panel p { padding-top: 1em; }
</style>
<div id="shown"><p id="shown-p">x</p></div>
<div id="panel" class="panel"><p id="inner">y</p></div>
<p id="attr" hidden>z</p>
"#;

fn styles() -> (Document, h2q_css::StyleMap) {
    let doc = Document::parse(PAGE);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    (doc, styles)
}

#[test]
fn display_none_element_has_no_box_but_has_computed_style() {
    let (doc, styles) = styles();
    let panel = doc.element_by_id("panel").expect("panel");
    assert!(styles.get(panel).is_none());
    let computed = styles
        .computed(panel)
        .expect("computed style of display:none element");
    assert_eq!(computed.style.display, Display::None);
    assert_eq!(computed.style.color, Rgba::new(255, 0, 0, 255));
}

#[test]
fn descendants_of_display_none_inherit_and_resolve_ems() {
    let (doc, styles) = styles();
    let inner = doc.element_by_id("inner").expect("inner");
    assert!(styles.get(inner).is_none());
    let computed = styles
        .computed(inner)
        .expect("computed style inside display:none");
    assert_eq!(computed.style.display, Display::Block);
    assert_eq!(computed.style.color, Rgba::new(255, 0, 0, 255));
    assert_eq!(computed.style.font_size, 20.0);
    assert_eq!(computed.style.padding.top, h2q_css::Length::px(20.0));
}

#[test]
fn hidden_attribute_is_display_none_with_computed_style() {
    let (doc, styles) = styles();
    let p = doc.element_by_id("attr").expect("attr");
    assert!(styles.get(p).is_none());
    assert_eq!(styles.computed(p).unwrap().style.display, Display::None);
    assert_eq!(
        styles.computed(p).unwrap().style.color,
        Rgba::new(0, 0, 255, 255)
    );
}

#[test]
fn computed_equals_get_for_rendered_elements() {
    let (doc, styles) = styles();
    for id in ["shown", "shown-p"] {
        let n = doc.element_by_id(id).expect(id);
        assert_eq!(styles.computed(n), styles.get(n));
        assert!(styles.get(n).is_some());
    }
}
