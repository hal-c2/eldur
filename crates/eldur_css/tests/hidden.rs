//! Elements that are, or are inside, `display: none` have no box, but they
//! still have computed values (CSSOM `getComputedStyle` reports them). The
//! converter needs them for elements a script shows later.

use eldur_css::{Display, Rgba, StylesheetLoader, Viewport, collect_stylesheets, compute_styles};
use eldur_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
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

fn styles() -> (Document, eldur_css::StyleMap) {
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
    assert_eq!(computed.style.padding.top, eldur_css::Length::px(20.0));
}

#[test]
fn hidden_attribute_is_display_none_with_computed_style() {
    let (doc, styles) = styles();
    let p = doc.element_by_id("attr").expect("attr");
    assert!(styles.get(p).is_none());
    let computed = styles.computed(p).expect("computed style of [hidden]");
    assert_eq!(computed.style.display, Display::None);
    assert_eq!(computed.style.color, Rgba::new(0, 0, 255, 255));
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

#[test]
fn with_styles_moves_subtrees_in_and_out_of_the_boxes() {
    let (doc, styles) = styles();
    let panel = doc.element_by_id("panel").expect("panel");
    let inner = doc.element_by_id("inner").expect("inner");
    let shown = doc.element_by_id("shown").expect("shown");
    let shown_p = doc.element_by_id("shown-p").expect("shown-p");

    let mut block = styles.computed(panel).expect("panel").style.clone();
    block.display = Display::Block;
    let mut none = styles.style(shown).expect("shown").clone();
    none.display = Display::None;
    let swapped = styles.with_styles([(panel, block), (shown, none)]);

    assert_eq!(
        swapped.style(panel).map(|s| s.color),
        Some(Rgba::new(255, 0, 0, 255))
    );
    assert!(swapped.get(inner).is_some(), "the panel's child is shown");
    assert!(swapped.get(shown).is_none());
    assert!(
        swapped.get(shown_p).is_none(),
        "and the hidden one's is not"
    );
    assert!(swapped.computed(shown_p).is_some());
    // The original is unchanged.
    assert!(styles.get(panel).is_none() && styles.get(shown_p).is_some());
}
