use super::*;

fn names(doc: &Document, ids: impl Iterator<Item = NodeId>) -> Vec<String> {
    ids.filter_map(|n| doc.element(n).map(|e| e.local_name().to_owned()))
        .collect()
}

#[test]
fn implied_structure_is_built() {
    let doc = Document::parse("<p>Hello <b>world</b>");
    let html = doc.html().expect("html");
    assert_eq!(names(&doc, doc.element_children(html)), ["head", "body"]);
    let body = doc.body().expect("body");
    assert_eq!(doc.text_content(body), "Hello world");
}

#[test]
fn misnested_tags_are_fixed() {
    let doc = Document::parse("<b><i>x</b>y</i>");
    let body = doc.body().expect("body");
    assert_eq!(names(&doc, doc.descendants(body)), ["body", "b", "i", "i"]);
}

#[test]
fn attributes_ids_and_classes() {
    let doc = Document::parse(r#"<div id="main" class=" a  b " data-x="1"></div>"#);
    let div = doc.element_by_id("main").expect("div");
    let e = doc.element(div).expect("element");
    assert_eq!(e.classes().collect::<Vec<_>>(), ["a", "b"]);
    assert!(e.has_class("b"));
    assert_eq!(e.attr("data-x"), Some("1"));
    assert_eq!(e.line, 1);
}

#[test]
fn adjacent_text_is_merged() {
    let doc = Document::parse("<p>a&amp;b</p>");
    let p = doc.first_element_by_name("p").expect("p");
    assert_eq!(doc.children(p).count(), 1);
    assert_eq!(doc.text(doc.children(p).next().expect("text")), Some("a&b"));
}

#[test]
fn table_foster_parenting_moves_text_before_the_table() {
    let doc = Document::parse("<table>oops<tr><td>1</td></tr></table>");
    let body = doc.body().expect("body");
    let first = doc.children(body).next().expect("first");
    assert_eq!(doc.text(first), Some("oops"));
}

#[test]
fn template_contents_are_a_fragment() {
    let doc = Document::parse("<template id=t><span>x</span></template>");
    let t = doc.element_by_id("t").expect("template");
    let contents = doc
        .element(t)
        .and_then(|e| e.template_contents)
        .expect("contents");
    assert_eq!(doc.node(contents).data, NodeData::Fragment);
    assert_eq!(names(&doc, doc.children(contents)), ["span"]);
    assert_eq!(doc.children(t).count(), 0);
}

#[test]
fn siblings_and_ancestors() {
    let doc = Document::parse("<ul><li id=a>1</li> <li id=b>2</li><li id=c>3</li></ul>");
    let b = doc.element_by_id("b").expect("b");
    let a = doc.element_by_id("a").expect("a");
    let c = doc.element_by_id("c").expect("c");
    assert_eq!(doc.prev_element_sibling(b), Some(a));
    assert_eq!(doc.next_element_sibling(b), Some(c));
    assert_eq!(doc.prev_element_sibling(a), None);
    assert_eq!(names(&doc, doc.ancestors(b)), ["ul", "body", "html"]);
    assert_eq!(doc.prev_siblings(doc.root()).count(), 0);
}

#[test]
fn svg_is_namespaced() {
    let doc = Document::parse("<svg viewBox='0 0 1 1'><circle r=1 /></svg>");
    let svg = doc
        .descendants(doc.root())
        .find(|&n| doc.element(n).is_some_and(Element::is_svg))
        .expect("svg");
    assert_eq!(
        doc.element(svg).and_then(|e| e.attr("viewBox")),
        Some("0 0 1 1")
    );
}

#[test]
fn fragment_parsing() {
    let doc = Document::parse_fragment("<li>a</li><li>b</li>");
    let html = doc.html().expect("html");
    assert_eq!(names(&doc, doc.element_children(html)), ["li", "li"]);
}

#[test]
fn title_is_collapsed() {
    let doc = Document::parse("<title>  A \n  page </title>");
    assert_eq!(doc.title().as_deref(), Some("A page"));
}
