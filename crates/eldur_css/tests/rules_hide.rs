//! A script that clears an element's inline display shows the element only
//! when no author rule hides it. The converter asks which of the elements a
//! script reaches would still be hidden without their own display and
//! `hidden` attribute, and whether a `var()` display in a style attribute
//! resolves to `none`.

use std::collections::{BTreeMap, BTreeSet};

use eldur_css::{
    StylesheetLoader, Viewport, collect_stylesheets, rules_hide, sheet_custom_properties,
    style_attribute_custom_names, style_attribute_hides, style_attribute_hides_with,
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
  /* Matches the attribute as written, not once a script clears its display. */
  [style*="display:none"] { display: none; }
  /* With no display to clear the attribute stays as written; with one,
     the rest reads as CSSOM serializes it (Chromium 158). */
  [style="color:red;"] { display: none; }
  [style$="red;"] { display: none; }
</style>
<div id="shy" style="display:none">a</div>
<div id="vary" style="display:none">b</div>
<div id="cls" class="off" style="display:block">c</div>
<div id="plain" style="display:none">d</div>
<div id="attr" hidden>e</div>
<div id="attrshy" class="off" hidden>f</div>
<div id="exact" hidden style="color:red;">g</div>
<div id="tail" style="display: none; color: red;">h</div>
<div id="spaced" style="display:none;color:red!important;margin:0 auto">i</div>
"#;

#[test]
fn rules_hide_names_the_boxes_an_author_rule_keeps_hidden() {
    let doc = Document::parse(PAGE);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let id = |name: &str| doc.element_by_id(name).expect(name);
    let nodes: BTreeSet<_> = [
        "shy", "vary", "cls", "plain", "attr", "attrshy", "exact", "tail", "spaced",
    ]
    .iter()
    .map(|n| id(n))
    .collect();
    let hidden = rules_hide(&doc, &sheets, &viewport, &nodes);
    let want: BTreeSet<_> = ["shy", "vary", "cls", "attrshy", "exact", "tail"]
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

#[test]
fn a_style_attributes_custom_properties_are_named_as_the_cascade_parses_them() {
    let names = |s: &str| {
        style_attribute_custom_names(s)
            .into_iter()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(r#"--a: url("x;--y:1"); --b: "a;--c:1""#),
        ["--a", "--b"]
    );
    assert_eq!(names("--a: url(x;y); color: red"), ["--a"]);
    assert_eq!(names("color: red"), Vec::<String>::new());
}

#[test]
fn sheet_custom_properties_names_what_rules_declare_under_the_attribute() {
    let page = r#"<!DOCTYPE html>
<style>
  #w { --d: block; }
  #in { --d: block; --gone: initial; }
  #over { --d: none; }
</style>
<div id="w"><div id="in"><div id="over" style="--d: block; --own: 1"><p id="leaf">x</p></div></div></div>
"#;
    let doc = Document::parse(page);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = eldur_css::compute_styles(&doc, &sheets, &viewport);
    let id = |name: &str| doc.element_by_id(name).expect(name);
    let nodes: BTreeSet<_> = ["w", "in", "over", "leaf"].iter().map(|n| id(n)).collect();
    let got = sheet_custom_properties(&doc, &sheets, &viewport, &styles, &nodes);
    let some = |v: &str| Some(v.to_owned());
    // Declared on `#in` though `#w` gives the same: it doesn't inherit it.
    assert_eq!(
        got[&id("in")],
        [
            ("--d".to_owned(), some("block")),
            ("--gone".to_owned(), None)
        ]
        .into()
    );
    // The rule's value, not the attribute's, and not the attribute's own one.
    assert_eq!(got[&id("over")], [("--d".to_owned(), some("none"))].into());
    // Only inheriting, it isn't named.
    assert!(!got.contains_key(&id("leaf")));
}
