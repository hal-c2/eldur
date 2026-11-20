//! How many extra cascade passes `sheet_custom_properties` runs: as many as
//! the nodes whose own `style` attribute it takes away nest, not one per
//! node (eldur #85).

use std::collections::BTreeSet;

use eldur_dom::Document;

use super::sheet_custom_properties;
use crate::work::base_passes;
use crate::{StylesheetLoader, Viewport, collect_stylesheets, compute_styles};

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<crate::LoadedSheet> {
        None
    }
}

/// `cards` themed cards, each with its own `--x` over the rule's, and inside
/// it a panel that does the same when `nested`.
fn page(cards: usize, nested: bool) -> String {
    let inner = if nested {
        r#"<div class="card" style="--x: inner"><p>x</p></div>"#
    } else {
        "<p>x</p>"
    };
    let cards: String = (0..cards)
        .map(|i| format!(r#"<div class="card" style="--x: own{i}; --y: v">{inner}</div>"#))
        .collect();
    format!(
        "<!DOCTYPE html><style>.card {{ --x: none; --z: var(--x) }}</style>\
         <div id=\"list\" style=\"--z: a\">{cards}</div>"
    )
}

fn passes(html: &str) -> (usize, Vec<Option<String>>) {
    let doc = Document::parse(html);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    let nodes: BTreeSet<_> = doc
        .descendants(doc.root())
        .filter(|&n| doc.element(n).is_some())
        .collect();
    let (got, passes) =
        base_passes(|| sheet_custom_properties(&doc, &sheets, &viewport, &styles, &nodes));
    let xs = got
        .values()
        .map(|v| v.get("--x").cloned().flatten())
        .collect();
    (passes, xs)
}

#[test]
fn sibling_cards_share_one_pass() {
    let (count, xs) = passes(&page(50, false));
    assert_eq!(count, 1);
    // Every card reads its rule's value, not its attribute's.
    assert_eq!(xs.len(), 50);
    assert!(xs.iter().all(|x| x.as_deref() == Some("none")), "{xs:?}");
}

#[test]
fn a_pass_per_level_of_nesting() {
    let (count, xs) = passes(&page(50, true));
    assert_eq!(count, 2);
    assert_eq!(xs.len(), 100);
    assert!(xs.iter().all(|x| x.as_deref() == Some("none")), "{xs:?}");
}

#[test]
fn no_pass_when_the_attribute_declares_no_name_a_rule_computes() {
    // `#list` declares `--z`, which no rule matching it declares; a card's
    // `--z` is a `var()`, kept as written.
    let html = "<!DOCTYPE html><style>.card { --z: var(--x) } #list { --w: a }</style>\
         <div id=\"list\" style=\"--z: a\"><div class=\"card\" style=\"--z: b\">x</div></div>";
    let (count, _) = passes(html);
    assert_eq!(count, 0);
}
