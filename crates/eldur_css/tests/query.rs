use eldur_css::{AnyAttributeSelector, element_matches, query_selector_all};
use eldur_dom::{Document, NodeId};

fn sample_doc() -> Document {
    Document::parse(
        r#"<!doctype html>
        <html>
          <body>
            <div id="wrap" data-role="container">
              <p id="p1" class="lead x" data-kind="intro">First</p>
              <p id="p2" class="body" data-kind="main">Second <span id="s1" class="x">inner</span></p>
              <p id="p3" class="body x">Third</p>
              <section id="sec">
                <span id="s2" data-flag>Direct</span>
              </section>
            </div>
          </body>
        </html>"#,
    )
}

fn ids(doc: &Document, nodes: &[NodeId]) -> Vec<String> {
    nodes
        .iter()
        .map(|&n| {
            let elem = doc.element(n).expect("matched node should be an element");
            elem.id()
                .map(str::to_owned)
                .unwrap_or_else(|| elem.local_name().to_owned())
        })
        .collect()
}

#[test]
fn query_simple_selectors_tag_id_class_and_attributes() {
    let doc = sample_doc();

    let paragraphs = query_selector_all(&doc, "p").expect("valid tag selector");
    assert_eq!(ids(&doc, &paragraphs), vec!["p1", "p2", "p3"]);

    let by_id = query_selector_all(&doc, "#p2").expect("valid id selector");
    assert_eq!(ids(&doc, &by_id), vec!["p2"]);

    let by_class = query_selector_all(&doc, ".body").expect("valid class selector");
    assert_eq!(ids(&doc, &by_class), vec!["p2", "p3"]);

    let by_attr_pres = query_selector_all(&doc, "[data-flag]").expect("valid [attr] selector");
    assert_eq!(ids(&doc, &by_attr_pres), vec!["s2"]);

    let by_attr_val =
        query_selector_all(&doc, "[data-kind=main]").expect("valid [attr=v] selector");
    assert_eq!(ids(&doc, &by_attr_val), vec!["p2"]);

    let p2 = by_id[0];
    assert_eq!(
        element_matches(&doc, p2, "p.body[data-kind=main]"),
        Some(true)
    );
    assert_eq!(element_matches(&doc, p2, "#p1"), Some(false));
}

#[test]
fn query_descendant_child_and_sibling_combinators() {
    let doc = sample_doc();

    let desc = query_selector_all(&doc, "#wrap span").expect("descendant combinator");
    assert_eq!(ids(&doc, &desc), vec!["s1", "s2"]);

    let child = query_selector_all(&doc, "#sec > span").expect("child combinator");
    assert_eq!(ids(&doc, &child), vec!["s2"]);

    let next_sib = query_selector_all(&doc, "#p1 + p").expect("next-sibling combinator");
    assert_eq!(ids(&doc, &next_sib), vec!["p2"]);

    let subseq_sib = query_selector_all(&doc, "#p1 ~ p").expect("subsequent-sibling combinator");
    assert_eq!(ids(&doc, &subseq_sib), vec!["p2", "p3"]);
}

#[test]
fn query_structural_and_negation_pseudo_classes() {
    let doc = sample_doc();

    let first = query_selector_all(&doc, "#wrap > :first-child").expect(":first-child");
    assert_eq!(ids(&doc, &first), vec!["p1"]);

    let even = query_selector_all(&doc, "#wrap > :nth-child(2n)").expect(":nth-child(2n)");
    assert_eq!(ids(&doc, &even), vec!["p2", "sec"]);

    let not_x = query_selector_all(&doc, "p:not(.x)").expect(":not(.x)");
    assert_eq!(ids(&doc, &not_x), vec!["p2"]);
}

#[test]
fn query_selector_list_preserves_document_order_without_duplicates() {
    let doc = sample_doc();

    let list = query_selector_all(&doc, ".body, #p1, .x, p").expect("selector list");
    assert_eq!(ids(&doc, &list), vec!["p1", "p2", "s1", "p3"]);
}

#[test]
fn query_invalid_selectors_return_none() {
    let doc = sample_doc();
    let p1 = query_selector_all(&doc, "#p1").expect("valid")[0];

    assert_eq!(query_selector_all(&doc, "p["), None);
    assert_eq!(query_selector_all(&doc, "p, p["), None);
    assert_eq!(query_selector_all(&doc, ""), None);
    assert_eq!(element_matches(&doc, p1, "p["), None);
    assert_eq!(element_matches(&doc, p1, "p, p["), None);
}

#[test]
fn query_hover_matches_nothing_in_initial_state() {
    let doc = sample_doc();
    let p1 = query_selector_all(&doc, "#p1").expect("valid")[0];

    let hovered = query_selector_all(&doc, ":hover").expect("valid :hover selector");
    assert!(hovered.is_empty());
    assert_eq!(element_matches(&doc, p1, "p:hover"), Some(false));
}

#[test]
fn query_pseudo_element_selectors_match_no_elements() {
    let doc = Document::parse("<div><p>x</p><span>y</span></div>");
    assert_eq!(
        eldur_css::query_selector_all(&doc, "p::before"),
        Some(vec![])
    );
    let p = doc.first_element_by_name("p").expect("p");
    assert_eq!(eldur_css::element_matches(&doc, p, "p::after"), Some(false));
    assert_eq!(
        eldur_css::query_selector_all(&doc, "p::before, span").map(|v| v.len()),
        Some(1)
    );
}

fn theme(name: &str) -> bool {
    name == "data-theme"
}

fn any_theme(doc: &Document, selector: &str) -> Option<Vec<String>> {
    let relaxed = AnyAttributeSelector::new(selector, &theme)?;
    let all = query_selector_all(doc, "*").expect("valid");
    let hits: Vec<NodeId> = all
        .into_iter()
        .filter(|&n| relaxed.matches(doc, n))
        .collect();
    Some(ids(doc, &hits))
}

#[test]
fn a_written_attribute_is_met_by_some_value() {
    let doc = Document::parse(
        r#"<html data-theme="dark"><body>
        <p id="shy" class="shy">a</p><p id="plain">b</p>
        <ul id="menu"><li id="item">c</li></ul>
        </body></html>"#,
    );
    let shy = |s| any_theme(&doc, s);
    assert_eq!(
        shy(":root[data-theme='light'] .shy"),
        Some(vec!["shy".into()])
    );
    assert_eq!(shy("[data-theme=\"a]b\" i] .shy"), Some(vec!["shy".into()]));
    // Some value of the attribute meets a `:not()` on it.
    assert_eq!(
        shy("html:not([data-theme]) #plain"),
        Some(vec!["plain".into()])
    );
    assert_eq!(
        shy(":is(html[data-theme~=x]) > body > .shy"),
        Some(vec!["shy".into()])
    );
    assert_eq!(
        shy("ul:has(> [data-theme]) > li"),
        Some(vec!["item".into()])
    );
    assert_eq!(
        shy("li:nth-child(1 of [data-theme])"),
        Some(vec!["item".into()])
    );
    // Each branch of a list, and only what the rest of it allows.
    assert_eq!(
        shy("[data-theme] .shy, #menu"),
        Some(vec!["shy".into(), "menu".into()])
    );
    assert_eq!(shy(":root[data-theme] .missing"), Some(vec![]));
}

#[test]
fn a_selector_without_a_written_attribute_is_no_such_selector() {
    let doc = Document::parse("<p class=shy lang=is>a</p>");
    assert!(any_theme(&doc, ":root[lang='is'] .shy").is_none());
    assert!(any_theme(&doc, ".shy:not(.x)").is_none());
    assert!(any_theme(&doc, "[data-theme").is_none());
}
