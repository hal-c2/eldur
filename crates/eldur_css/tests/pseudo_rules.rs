//! What the runtime is told about `::before` and `::after`, to follow a class a
//! script sets: the text, and how the text is drawn when it is not as the
//! element's own.

use eldur_css::{
    CounterChange, ListStyleType, PseudoPart, StylesheetLoader, Viewport, collect_stylesheets,
    counter_rules, pseudo_rules,
};
use eldur_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
        None
    }
}

#[test]
fn generated_text_carries_its_own_font_and_colour() {
    let doc = Document::parse(
        r#"<!DOCTYPE html><style>
        th.asc::after { content: " \25B2"; font-size: 10px; color: #a00; }
        th::before { font-weight: bold; }
        th.mark::before { content: "*"; }
        th.box::after { content: "x"; display: block; }
        </style><table><tr><th>a</th></tr></table>"#,
    );
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let rules = pseudo_rules(&sheets, &viewport);
    let found: Vec<_> = rules
        .iter()
        .map(|r| {
            (
                r.selector.as_str(),
                r.after,
                r.sets_content,
                r.content.clone(),
                r.declarations.clone(),
            )
        })
        .collect();
    let decl = |p: &str, v: &str| (p.to_owned(), v.to_owned(), false);
    assert_eq!(
        found,
        vec![
            (
                "th.asc",
                true,
                true,
                vec![PseudoPart::Text(" \u{25B2}".to_owned())],
                vec![decl("font-size", "10px"), decl("color", "#a00")],
            ),
            // Only how it is drawn: the text comes from another rule.
            (
                "th",
                false,
                false,
                vec![],
                vec![decl("font-weight", "bold")]
            ),
            (
                "th.mark",
                false,
                true,
                vec![PseudoPart::Text("*".to_owned())],
                vec![],
            ),
        ]
    );
}

#[test]
fn counters_in_generated_text_and_the_rules_that_move_them() {
    let doc = Document::parse(
        r#"<!DOCTYPE html><style>
        ol { counter-reset: n 2; }
        li { counter-increment: n; color: red; }
        li.skip { counter-increment: n 2 !important; counter-set: m 5; }
        li::before { content: counter(n) ". " counters(n, ".", lower-roman); }
        li::after { content: url(x.png); color: blue; }
        </style><ol><li>a</li></ol>"#,
    );
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let rules = pseudo_rules(&sheets, &viewport);
    // Text no rule can give stays as the converter wrote it.
    assert_eq!(rules.len(), 1);
    assert_eq!(
        rules[0].content,
        vec![
            PseudoPart::Counter("n".to_owned(), ListStyleType::Decimal),
            PseudoPart::Text(". ".to_owned()),
            PseudoPart::Counters("n".to_owned(), ".".to_owned(), ListStyleType::LowerRoman),
        ]
    );
    let found: Vec<_> = counter_rules(&sheets, &viewport)
        .into_iter()
        .map(|r| (r.selector, r.specificity, r.changes))
        .collect();
    let change = |name: &str, value: i32| CounterChange {
        name: name.to_owned(),
        value,
    };
    assert_eq!(
        found,
        vec![
            (
                "ol".to_owned(),
                (0, 0, 1),
                vec![("counter-reset".to_owned(), vec![change("n", 2)], false)]
            ),
            (
                "li".to_owned(),
                (0, 0, 1),
                vec![("counter-increment".to_owned(), vec![change("n", 1)], false)]
            ),
            (
                "li.skip".to_owned(),
                (0, 1, 1),
                vec![
                    ("counter-increment".to_owned(), vec![change("n", 2)], true),
                    ("counter-set".to_owned(), vec![change("m", 5)], false),
                ]
            ),
        ]
    );
}
