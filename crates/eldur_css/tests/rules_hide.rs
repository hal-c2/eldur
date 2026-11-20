//! A script that clears an element's inline display shows the element only
//! when no author rule hides it. The converter asks which of the elements a
//! script reaches would still be hidden without their own display and
//! `hidden` attribute, and whether a `var()` display in a style attribute
//! resolves to `none`.

use std::collections::{BTreeMap, BTreeSet};

use eldur_css::{
    StylesheetLoader, Viewport, collect_stylesheets, rules_hide, style_attribute_hides,
    style_attribute_hides_with,
};
use eldur_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
        None
    }
}

const PAGE: &str = r#"<!DOCTYPE html>
<style>
  :root { --gone: none; }
  #shy { display: none; }
  #vary { display: var(--gone); }
  .off { display: none; }
  [hidden] { color: red; }
</style>
<div id="shy" style="display:none">a</div>
<div id="vary" style="display:none">b</div>
<div id="cls" class="off" style="display:block">c</div>
<div id="plain" style="display:none">d</div>
<div id="attr" hidden>e</div>
<div id="attrshy" class="off" hidden>f</div>
"#;

#[test]
fn rules_hide_names_the_boxes_an_author_rule_keeps_hidden() {
    let doc = Document::parse(PAGE);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let id = |name: &str| doc.element_by_id(name).expect(name);
    let nodes: BTreeSet<_> = ["shy", "vary", "cls", "plain", "attr", "attrshy"]
        .iter()
        .map(|n| id(n))
        .collect();
    let hidden = rules_hide(&doc, &sheets, &viewport, &nodes);
    let want: BTreeSet<_> = ["shy", "vary", "cls", "attrshy"]
        .iter()
        .map(|n| id(n))
        .collect();
    assert_eq!(hidden, want);
    // Only the nodes asked about are answered.
    let only: BTreeSet<_> = [id("plain"), id("shy")].into();
    assert_eq!(
        rules_hide(&doc, &sheets, &viewport, &only),
        [id("shy")].into()
    );
}

#[test]
fn a_variable_display_hides_when_it_resolves_to_none() {
    let none: BTreeMap<String, String> = [("--d".to_owned(), "none".to_owned())].into();
    let block: BTreeMap<String, String> = [("--d".to_owned(), "block".to_owned())].into();
    assert!(style_attribute_hides_with("display: var(--d)", &none));
    assert!(!style_attribute_hides_with("display: var(--d)", &block));
    assert!(!style_attribute_hides_with(
        "display: var(--d)",
        &BTreeMap::new()
    ));
    assert!(style_attribute_hides_with(
        "display: var(--e, none)",
        &BTreeMap::new()
    ));
    // The last declaration wins, whichever form it takes.
    assert!(!style_attribute_hides_with(
        "display: var(--d); display: block",
        &none
    ));
    assert!(style_attribute_hides_with(
        "display: block; display: var(--d)",
        &none
    ));
    assert!(style_attribute_hides("display:none"));
    assert!(!style_attribute_hides("display: var(--d)"));
}
