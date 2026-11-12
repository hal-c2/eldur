//! What the runtime is told about `white-space`, to keep or collapse the spaces
//! of text a script puts on a page: its two longhands as written, which the
//! runtime inherits one by one, as the static cascade does.

use eldur_css::{StylesheetLoader, Viewport, collect_stylesheets, runtime_rules};
use eldur_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
        None
    }
}

fn white_space_of(css: &str) -> Vec<(String, String, String, bool)> {
    let doc = Document::parse(&format!("<!DOCTYPE html><style>{css}</style><p>a</p>"));
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    runtime_rules(&sheets, &viewport)
        .into_iter()
        .filter(|r| !r.user_agent)
        .flat_map(|r| {
            let selector = r.selector.clone();
            r.declarations
                .into_iter()
                .filter(|d| {
                    matches!(
                        d.0.as_str(),
                        "white-space" | "white-space-collapse" | "text-wrap-mode"
                    )
                })
                .map(move |d| (selector.clone(), d.0, d.1, d.2))
        })
        .collect()
}

fn found(
    selector: &str,
    property: &str,
    value: &str,
    important: bool,
) -> (String, String, String, bool) {
    (
        selector.to_owned(),
        property.to_owned(),
        value.to_owned(),
        important,
    )
}

#[test]
fn the_shorthand_comes_back_as_both_longhands() {
    assert_eq!(
        white_space_of(".p{white-space:pre} .l{white-space:pre-line}"),
        vec![
            found(".p", "white-space-collapse", "preserve", false),
            found(".p", "text-wrap-mode", "nowrap", false),
            found(".l", "white-space-collapse", "preserve-breaks", false),
            found(".l", "text-wrap-mode", "wrap", false),
        ]
    );
}

#[test]
fn a_rule_that_sets_only_how_it_wraps_says_nothing_of_spaces() {
    // `text-wrap: balance` sets `text-wrap-mode` alone: the spaces a parent's
    // `pre-wrap` keeps stay kept.
    assert_eq!(
        white_space_of(
            "p{text-wrap:balance} .x{text-wrap-mode:nowrap} .c{white-space-collapse:preserve}"
        ),
        vec![
            found("p", "text-wrap-mode", "wrap", false),
            found(".x", "text-wrap-mode", "nowrap", false),
            found(".c", "white-space-collapse", "preserve", false),
        ]
    );
}

#[test]
fn css_wide_keywords_and_importance_pass_through() {
    assert_eq!(
        white_space_of(".i{white-space:inherit} .t{white-space:pre-wrap !important}"),
        vec![
            found(".i", "white-space-collapse", "inherit", false),
            found(".i", "text-wrap-mode", "inherit", false),
            found(".t", "white-space-collapse", "preserve", true),
            found(".t", "text-wrap-mode", "wrap", true),
        ]
    );
}
