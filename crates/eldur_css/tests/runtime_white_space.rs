//! What the runtime is told about `white-space`, to keep or collapse the spaces
//! of text a script puts on a page: the one keyword the two longhands make.

use eldur_css::{StylesheetLoader, Viewport, collect_stylesheets, runtime_rules};
use eldur_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
        None
    }
}

fn white_space_of(css: &str) -> Vec<(String, String, bool)> {
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
                .filter(|d| d.0 == "white-space")
                .map(move |d| (selector.clone(), d.1, d.2))
        })
        .collect()
}

fn found(selector: &str, keyword: &str, important: bool) -> (String, String, bool) {
    (selector.to_owned(), keyword.to_owned(), important)
}

#[test]
fn every_shorthand_keyword_comes_back_as_written() {
    assert_eq!(
        white_space_of(
            ".n{white-space:normal} .p{white-space:pre} .w{white-space:pre-wrap} \
             .l{white-space:pre-line} .x{white-space:nowrap} .b{white-space:break-spaces}"
        ),
        vec![
            found(".n", "normal", false),
            found(".p", "pre", false),
            found(".w", "pre-wrap", false),
            found(".l", "pre-line", false),
            found(".x", "nowrap", false),
            found(".b", "break-spaces", false),
        ]
    );
}

#[test]
fn a_lone_longhand_takes_the_other_at_its_initial_value() {
    assert_eq!(
        white_space_of(
            ".c{white-space-collapse:preserve} .k{white-space-collapse:preserve-breaks} \
             .m{text-wrap-mode:nowrap}"
        ),
        vec![
            found(".c", "pre-wrap", false),
            found(".k", "pre-line", false),
            found(".m", "nowrap", false),
        ]
    );
}

#[test]
fn longhands_that_name_no_keyword_keep_their_spaces_as_written() {
    // `preserve-breaks` with `nowrap` has no keyword of its own: the breaks
    // are kept, as `pre-line` keeps them, and the spaces still collapse.
    assert_eq!(
        white_space_of(
            ".k{white-space-collapse:preserve-breaks;text-wrap-mode:nowrap} \
             .s{white-space-collapse:break-spaces;text-wrap-mode:nowrap}"
        ),
        vec![found(".k", "pre-line", false), found(".s", "pre", false)]
    );
}

#[test]
fn css_wide_keywords_and_importance_pass_through() {
    assert_eq!(
        white_space_of(".i{white-space:inherit} .t{white-space:pre !important}"),
        vec![found(".i", "inherit", false), found(".t", "pre", true)]
    );
}
