//! Robustness tests for broken, messy, or adversarial CSS inputs (CSS Syntax 3
//! error recovery and CSS 2.1 §4.2 rules for handling parsing errors).

use std::time::{Duration, Instant};

use h2q_css::{
    Length, LengthAuto, Rgba, Size, StylesheetLoader, Viewport, collect_stylesheets, compute_styles,
};
use h2q_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<h2q_css::LoadedSheet> {
        None
    }
}

fn style_for(css: &str, html: &str, id: &str) -> h2q_css::ComputedStyle {
    let page = format!("<!DOCTYPE html><style>{css}</style>{html}");
    let doc = Document::parse(&page);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    let node = doc.element_by_id(id).expect(id);
    styles.computed(node).expect("computed style").style.clone()
}

fn p_style(css: &str) -> h2q_css::ComputedStyle {
    style_for(css, r#"<p id="t">hello</p>"#, "t")
}

/// 1. A missing semicolon: `p { color: red background: blue; margin-top: 5px }`.
/// The `color: red background: blue` declaration is invalid and dropped; a
/// preceding declaration and `margin-top: 5px` after it still apply.
#[test]
fn case1_missing_semicolon() {
    let s = p_style("p { padding-top: 4px; color: red background: blue; margin-top: 5px }");
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::BLACK);
    assert_eq!(s.background_color, Rgba::TRANSPARENT);
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(5.0)));
}

/// 2. An unclosed block at EOF: `p { color: red` applies red.
#[test]
fn case2_unclosed_block_at_eof() {
    let s = p_style("p { padding-top: 4px; color: rgb(255, 0, 0)");
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::new(255, 0, 0, 255));
}

/// 3. An unclosed string, `content: "abc` followed by a newline and then
/// `color: red;`.
///
/// Per CSS Syntax 3 §4.3.4 ("Consume a string token"), an unescaped newline
/// inside a string ends the string token immediately as `<bad-string-token>`
/// and reconsumes the newline as whitespace (so later quotes on following
/// lines are not swallowed). However, per CSS Syntax 3 §5.5.2 ("Consume a
/// declaration"), a newline is whitespace rather than a declaration
/// terminator: without a `;` before the newline, `content: <bad-string> color: red`
/// is consumed as a single declaration up to the `;` after `red`, and that
/// entire declaration is dropped because it contains `<bad-string-token>`,
/// while declarations after that `;` still apply. If `;` follows the bad
/// string on the next token, `color: red;` is a separate declaration and
/// applies.
#[test]
fn case3_unclosed_string_newline() {
    // Without `;` after `"abc`: the bad string ends at `\n`, the declaration
    // continues until `;` after `rgb(255, 0, 0)`, so both `content` and `color`
    // are part of the dropped bad declaration, and `margin-top: 6px;` after `;`
    // still applies. A subsequent quoted string (`font-family: "Custom"`) is
    // not corrupted by the earlier unclosed quote.
    let s = p_style(
        "p {\n  padding-top: 4px;\n  content: \"abc\n  color: rgb(255, 0, 0);\n  margin-top: 6px;\n  font-family: \"Custom\";\n}",
    );
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::BLACK);
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(6.0)));
    assert_eq!(s.font_family, vec!["Custom".to_owned()]);
}

/// 4. A stray `}` at top level before a valid rule: the next rule still applies.
#[test]
fn case4_stray_closing_brace_at_top_level() {
    let s = p_style("p { padding-top: 4px; } } p { color: rgb(255, 0, 0); }");
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::new(255, 0, 0, 255));
}

/// 5. An unknown at-rule with a block, `@foo bar { p { color: red } }`, is
/// skipped. A following rule applies, and the inner rule does NOT apply.
#[test]
fn case5_unknown_at_rule_with_block() {
    let s = p_style(
        "p { padding-top: 4px; } @foo bar { p { color: rgb(255, 0, 0); } } p { margin-top: 7px; }",
    );
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::BLACK);
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(7.0)));
}

/// 6. A garbage selector, `p[[ { color: red } p { margin-top: 3px }`. The
/// first rule is dropped and the second applies.
#[test]
fn case6_garbage_selector() {
    let s = p_style("p { padding-top: 4px } p[[ { color: rgb(255, 0, 0) } p { margin-top: 3px }");
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::BLACK);
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(3.0)));
}

/// 7. `!important` misspelt as `! importantt` drops that declaration.
#[test]
fn case7_misspelt_important() {
    let s = p_style(
        "p { padding-top: 4px; color: rgb(255, 0, 0) ! importantt; margin-top: 8px; }",
    );
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::BLACK);
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(8.0)));
}

/// 8. Vendor hacks: `*zoom: 1`, `_height: 1px`,
/// `filter: progid:DXImageTransform.Microsoft.gradient(...)`,
/// `-webkit-box-shadow: ...`, `expression()`. None of these may break the
/// neighbouring declarations.
#[test]
fn case8_vendor_hacks() {
    let s = p_style(
        "p {
            padding-top: 4px;
            *zoom: 1;
            _height: 1px;
            filter: progid:DXImageTransform.Microsoft.gradient(startColorstr='#ff0000', endColorstr='#0000ff');
            -webkit-box-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
            width: expression(document.body.clientWidth > 600 ? \"600px\" : \"auto\");
            color: rgb(0, 128, 0);
            margin-top: 9px;
        }",
    );
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.height, Size::Auto);
    assert_eq!(s.width, Size::Auto);
    assert_eq!(s.color, Rgba::new(0, 128, 0, 255));
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(9.0)));
}

/// 9. IE hacks: `color: red\9` and `width: 100px\0/`. Assert what the computed
/// `color` and `width` are.
///
/// Per CSS Syntax 3 §4.3.7, `\9` is not a hex digit so it unescapes to `U+0039`
/// (`'9'`), producing identifier `red9` (not a valid color), and `\0` followed
/// by `/` unescapes `\0` to `U+FFFD` in unit `px\u{FFFD}` followed by delim `/`.
/// Both declarations are invalid in standards-compliant browsers and dropped,
/// leaving `color` and `width` at their prior/initial values while surrounding
/// declarations apply.
#[test]
fn case9_ie_backslash_hacks() {
    let s = p_style(
        "p { padding-top: 4px; color: red\\9; width: 100px\\0/; margin-top: 10px; }",
    );
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::BLACK);
    assert_eq!(s.width, Size::Auto);
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(10.0)));
}

/// 10. Unbalanced brackets inside a value: `width: calc(100% - (10px)` with
/// the closing paren missing, inside a block that is then closed. Also
/// `background: url(foo.png` with no closing paren.
#[test]
fn case10_unbalanced_brackets_in_value() {
    let s1 = p_style(
        "p { padding-top: 4px; width: calc(100% - (10px); color: rgb(255, 0, 0); } p { margin-top: 11px; }",
    );
    assert_eq!(s1.padding.top, Length::px(4.0));
    assert_eq!(s1.width, Size::Auto);
    assert_eq!(s1.margin.top, LengthAuto::Length(Length::px(11.0)));

    // Unquoted `url(foo.png` followed by `;`: per CSS Syntax 3 §4.3.6 & §4.3.14,
    // an unquoted URL with no `)` or whitespace consumes code points up to `)`
    // or EOF (or if whitespace/semicolon triggers `<bad-url-token>`, consumes
    // remnants up to `)` or EOF). With recovery before `}` / `;`, the invalid
    // `background` declaration is dropped and surrounding valid rules/declarations
    // still apply.
    let s2 = p_style(
        "p { padding-top: 4px; background: url(foo.png; } p { margin-top: 12px; }",
    );
    assert_eq!(s2.padding.top, Length::px(4.0));
    assert!(s2.background_layers.is_empty());
    assert_eq!(s2.margin.top, LengthAuto::Length(Length::px(12.0)));
}

/// 11. Null bytes and the BOM (`\u{FEFF}` at the start of the sheet), CRLF
/// line endings, and form feeds.
#[test]
fn case11_null_bytes_bom_crlf_form_feeds() {
    // BOM at start of stylesheet, CRLF (`\r\n`) and form feed (`\u{000C}`) whitespace,
    // and embedded NUL (`\0`) inside an invalid declaration and followed by valid rules.
    let css = "\u{FEFF}p {\r\n\tpadding-top:\u{000C}4px;\r\n\tfont-family: \"a\0b\";\r\n\tcolor: rgb(0, 0, 255);\r\n\tmargin-top: 13px;\r\n}";
    let s = p_style(css);
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::new(0, 0, 255, 255));
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(13.0)));
    // Per CSS Syntax 3 §3.3 preprocessing, U+0000 NULL is replaced with U+FFFD REPLACEMENT CHARACTER.
    assert_eq!(s.font_family, vec!["a\u{FFFD}b".to_owned()]);
}

/// 12. Huge input:
/// - A 2 MB sheet of 20 000 rules.
/// - Selectors nested 2000 levels deep, `div div div ...`.
/// - `calc()` nested 500 levels deep.
/// - A 100 000-character identifier.
/// Each must finish in under 2 s in release-independent terms (< 10 s in debug)
/// and must not overflow a 2 MB thread stack.
#[test]
fn case12_huge_inputs() {
    fn run_on_2mb_stack<F: FnOnce() + Send + 'static>(name: &str, f: F) {
        let start = Instant::now();
        let handle = std::thread::Builder::new()
            .name(name.to_owned())
            .stack_size(2 * 1024 * 1024)
            .spawn(f)
            .expect("spawn thread");
        handle.join().unwrap_or_else(|e| panic!("{name} panicked: {e:?}"));
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(10),
            "{name} took {elapsed:?} (>= 10s limit)"
        );
    }

    // (a) 2 MB sheet of 20 000 rules.
    run_on_2mb_stack("20k_rules", || {
        let mut css = String::with_capacity(2_100_000);
        for i in 0..19_999 {
            // ~105 bytes per rule -> ~2.1 MB total.
            use std::fmt::Write;
            let _ = writeln!(
                css,
                ".unused-rule-number-{i:05} {{ padding-left: 1px; padding-right: 2px; padding-bottom: 3px; }}"
            );
        }
        css.push_str("p { padding-top: 4px; color: rgb(12, 34, 56); margin-top: 14px; }\n");
        assert!(css.len() >= 2_000_000, "sheet len was {}", css.len());
        let s = p_style(&css);
        assert_eq!(s.padding.top, Length::px(4.0));
        assert_eq!(s.color, Rgba::new(12, 34, 56, 255));
        assert_eq!(s.margin.top, LengthAuto::Length(Length::px(14.0)));
    });

    // (b) Selectors nested 2000 levels deep (`div div div ...`).
    run_on_2mb_stack("nested_selectors_2000", || {
        let deep_selector = std::iter::repeat_n("div", 2000)
            .collect::<Vec<_>>()
            .join(" ");
        let css = format!(
            "p {{ padding-top: 4px; }} {deep_selector} {{ color: rgb(255, 0, 0); }} p {{ margin-top: 15px; }}"
        );
        let s = style_for(
            &css,
            r#"<div><div><p id="t">hello</p></div></div>"#,
            "t",
        );
        assert_eq!(s.padding.top, Length::px(4.0));
        assert_eq!(s.color, Rgba::BLACK);
        assert_eq!(s.margin.top, LengthAuto::Length(Length::px(15.0)));
    });

    // (c) `calc()` nested 500 levels deep.
    run_on_2mb_stack("nested_calc_500", || {
        let open = "calc(".repeat(500);
        let close = ")".repeat(500);
        let css = format!(
            "p {{ padding-top: 4px; width: {open}10px{close}; margin-top: 16px; }}"
        );
        let s = p_style(&css);
        assert_eq!(s.padding.top, Length::px(4.0));
        assert_eq!(s.width, Size::Auto);
        assert_eq!(s.margin.top, LengthAuto::Length(Length::px(16.0)));
    });

    // (d) A 100 000-character identifier.
    run_on_2mb_stack("ident_100k", || {
        let long_ident = "a".repeat(100_000);
        let css = format!(
            "p {{ padding-top: 4px; color: {long_ident}; margin-top: 17px; }}"
        );
        let s = p_style(&css);
        assert_eq!(s.padding.top, Length::px(4.0));
        assert_eq!(s.color, Rgba::BLACK);
        assert_eq!(s.margin.top, LengthAuto::Length(Length::px(17.0)));
    });
}

/// 13. `@media` with garbage, `@media screen and (min-width: ) { p { color: red } }`.
/// Per Media Queries 4 §3.2, an invalid media query evaluates to `not all`,
/// so the rule inside does not apply while surrounding rules do.
#[test]
fn case13_media_with_garbage() {
    let s = p_style(
        "p { padding-top: 4px; } @media screen and (min-width: ) { p { color: rgb(255, 0, 0); } } p { margin-top: 18px; }",
    );
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::BLACK);
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(18.0)));
}

/// 14. Comments in odd places: `p/**/{color/**/:/**/red}`, and an unclosed `/*` at EOF.
#[test]
fn case14_comments_in_odd_places_and_unclosed_at_eof() {
    let s = p_style(
        "p/**/{padding-top/**/:/**/4px;color/**/:/**/rgb(255, 0, 0);margin-top/**/:/**/19px}/* unclosed comment at eof p { color: blue }",
    );
    assert_eq!(s.padding.top, Length::px(4.0));
    assert_eq!(s.color, Rgba::new(255, 0, 0, 255));
    assert_eq!(s.margin.top, LengthAuto::Length(Length::px(19.0)));
}
