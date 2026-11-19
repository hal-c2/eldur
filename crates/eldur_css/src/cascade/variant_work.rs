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
    // (four elements each); its link's `:hover` cascades the link alone and
    // changes nothing, as links are underlined already. With the base
    // cascade that is 13 cascades per card.
    assert_eq!(small.variants.len(), 2 * 40);
    assert_eq!(large.variants.len(), 2 * 80);
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

fn variants_both_ways(page: &str) -> (Vec<crate::StateVariant>, Vec<crate::StateVariant>) {
    use crate::cascade::{VariantWalk, compute_styles_walking};
    let doc = Document::parse(page);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let reach = compute_styles_walking(&doc, &sheets, &viewport, VariantWalk::Reach);
    let whole = compute_styles_walking(&doc, &sheets, &viewport, VariantWalk::Document);
    (reach.variants, whole.variants)
}

/// Selectors that carry a state up, sideways and down: what restyling only
/// the reach must find just as restyling the whole document does.
const REACHING: &[&str] = &[
    // Descendants, inheritance through `display: contents`, and `rem`.
    "<style>.a:hover .b{color:red} .a:hover{font-size:30px} .c{display:contents}\
     .a:hover em{width:2em;height:1rem}</style>\
     <div class=c><div class=a><p class=b>x <em>y</em></p></div></div>",
    // The root's own state changes `rem` everywhere.
    "<style>html:hover{font-size:20px} p{width:2rem}</style><p>x</p><div><p>y</p></div>",
    // Following siblings and their subtrees.
    "<style>.a:hover + .b{color:red} .a:focus ~ .c span{color:blue}</style>\
     <p class=b>0</p><p class=a>1</p><p class=b>2</p><p class=c><span>3</span></p>",
    // `:has()` reads a descendant's state; `:nth-child(of)` a sibling's.
    "<style>div:has(.a:hover){background:red} li:nth-child(2 of .x:hover){color:red}\
     .a:hover{color:green}</style>\
     <div><p><span class=a>x</span></p></div><ul><li class=x>1<li class=x>2<li class=x>3</ul>",
    // States read under other names, and in `:is()`/`:not()`.
    "<style>input:enabled{color:red} a:link{color:blue} :is(.a:hover) b{color:red}\
     p:not(.a:hover) i{color:green}</style>\
     <form><input disabled><input></form><a href=#x>l</a><p class=a><b>b</b><i>i</i></p>",
    // Shown and hidden by a state, and hidden around it.
    "<style>.a:hover .h{display:block} .h{display:none} .n{display:none}\
     .a:hover{display:none}</style><div class=a><p class=h>x</p></div>\
     <div class=n><p class=a>y</p></div>",
    // A state that starts from the document.
    "<style>input:checked + label{color:red} :disabled{opacity:.5}</style>\
     <input type=checkbox checked><label>x</label><fieldset disabled><input></fieldset>",
];

#[test]
fn restyling_a_state_variant_s_reach_agrees_with_restyling_the_document() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut pages: Vec<(String, String)> = std::fs::read_dir(&corpus)
        .expect("tests/corpus exists")
        .filter_map(|e| {
            let path = e.ok()?.path().join("index.html");
            Some((
                path.display().to_string(),
                std::fs::read_to_string(path).ok()?,
            ))
        })
        .collect();
    assert!(pages.len() > 50, "the corpus is there");
    pages.extend(
        REACHING
            .iter()
            .enumerate()
            .map(|(i, page)| (format!("REACHING[{i}]"), format!("<!DOCTYPE html>{page}"))),
    );
    pages.push(("cards".to_owned(), cards(40)));
    for (name, page) in &pages {
        let (reach, whole) = variants_both_ways(page);
        assert_eq!(reach, whole, "{name}");
    }
    // The crafted pages do have variants, so the comparison is not empty.
    for page in REACHING {
        let (reach, _) = variants_both_ways(&format!("<!DOCTYPE html>{page}"));
        assert!(reach.iter().any(|v| !v.changed.is_empty()), "{page}");
    }
}
