//! How much the cascade does for a page with state rules (eldur #9): it must
//! grow with the page plus the rules, not with their product.

use eldur_dom::Document;

use crate::work::measure;
use crate::{StyleMap, StylesheetLoader, Viewport, collect_stylesheets};

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<crate::LoadedSheet> {
        None
    }
}

/// The page of eldur #9: `items` cards over 40 classes, each class with a
/// `:hover` and a `:focus-within` rule.
fn cards(items: usize) -> String {
    let rules: Vec<String> = (0..40)
        .map(|i| {
            format!(
                ".c{i}:hover{{color:rgb({},0,0);background:#eee}} \
                 .c{i}:focus-within{{outline:1px solid red}}",
                i % 255
            )
        })
        .collect();
    let items: Vec<String> = (0..items)
        .map(|i| {
            format!(
                "<div class=\"c{}\"><p>Item {i} <a href=\"#a{i}\">link</a> <code>x</code></p></div>",
                i % 40
            )
        })
        .collect();
    format!(
        "<!DOCTYPE html><html><head><style>body{{font:16px sans-serif}} \
         a:hover{{text-decoration:underline}} {}</style></head><body>{}</body></html>",
        rules.join(" "),
        items.join("\n")
    )
}

/// `html`, `head`, `style`, `body` and four elements per card.
fn elements(items: usize) -> usize {
    4 + 4 * items
}

fn styles(page: &str) -> (StyleMap, usize, usize) {
    let doc = Document::parse(page);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    measure(|| crate::cascade::compute_styles(&doc, &sheets, &viewport))
}

#[test]
fn a_state_variant_restyles_only_the_elements_its_state_reaches() {
    let (_, smallest_computes, _) = styles(&cards(20));
    let (small, small_computes, _) = styles(&cards(40));
    let (large, large_computes, _) = styles(&cards(80));
    // Every card has a `:hover` and a `:focus-within` variant on its `div`
    // (four elements each) and an `a:hover` one (one element), on top of the
    // base cascade: about 13 cascades per card.
    assert_eq!(small.variants.len(), 3 * 40);
    assert_eq!(large.variants.len(), 3 * 80);
    assert!(
        large_computes <= 4 * elements(80),
        "{large_computes} element cascades for {} elements",
        elements(80)
    );
    // Each 40 more cards cost twice what 20 more did: the work is linear.
    assert_eq!(
        large_computes - small_computes,
        2 * (small_computes - smallest_computes)
    );
}

#[test]
fn a_declaration_value_is_parsed_once_however_many_elements_it_styles() {
    let (_, _, small_parses) = styles(&cards(40));
    let (_, _, large_parses) = styles(&cards(80));
    // The same 40 classes style twice the cards; the value texts are the same.
    assert_eq!(large_parses, small_parses);
}
