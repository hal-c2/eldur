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

mod mutation {
    use super::*;

    fn attr(local: &str) -> QualName {
        QualName::new(None, ns!(), LocalName::from(local))
    }

    fn kids(doc: &Document, id: NodeId) -> Vec<String> {
        names(doc, doc.children(id))
    }

    // Chromium: createElement("DIV") has localName "div", XHTML namespace.
    #[test]
    fn created_element_is_a_detached_html_element() {
        let mut doc = Document::parse("<p>x</p>");
        let before = doc.len();
        let div = doc.create_html_element("DIV");
        let e = doc.element(div).expect("element");
        assert!(e.is_html());
        assert_eq!(e.local_name(), "div");
        assert_eq!(doc.parent(div), None);
        assert_eq!(doc.len(), before + 1);
    }

    // Chromium: createTextNode("hi") appended to a div gives textContent "hi".
    #[test]
    fn created_text_appends_as_text() {
        let mut doc = Document::parse("<div id=a></div>");
        let div = doc.element_by_id("a").expect("div");
        let text = doc.create_text("hi");
        assert_eq!(doc.parent(text), None);
        doc.append_child(div, text).expect("ok");
        assert_eq!(doc.text(text), Some("hi"));
        assert_eq!(doc.text_content(div), "hi");
    }

    // Chromium: setAttribute("a","3") on a=1 b=2 gives `a="3" b="2"`.
    #[test]
    fn set_attribute_replaces_in_place() {
        let mut doc = Document::parse("<div></div>");
        let div = doc.create_html_element("div");
        doc.set_attribute(div, attr("a"), "1").expect("ok");
        doc.set_attribute(div, attr("b"), "2").expect("ok");
        doc.set_attribute(div, attr("a"), "3").expect("ok");
        let e = doc.element(div).expect("ok");
        let got: Vec<_> = e
            .attrs
            .iter()
            .map(|a| (&*a.name.local, a.value.as_str()))
            .collect();
        assert_eq!(got, [("a", "3"), ("b", "2")]);
    }

    #[test]
    fn set_attribute_on_a_non_element_is_an_error() {
        let mut doc = Document::parse("<p>x</p>");
        let text = doc
            .first_element_by_name("p")
            .and_then(|p| doc.children(p).next())
            .expect("ok");
        assert_eq!(
            doc.set_attribute(text, attr("a"), "1"),
            Err(DomError::InvalidNodeType(text))
        );
    }

    // Chromium: an element with id set while detached is not found by
    // getElementById until it is attached, then it is.
    #[test]
    fn id_set_through_set_attribute_is_found_once_attached() {
        let mut doc = Document::parse("<p>x</p>");
        let div = doc.create_html_element("div");
        doc.set_attribute(div, attr("id"), "zz").expect("ok");
        assert_eq!(doc.element_by_id("zz"), None);
        let body = doc.body().expect("ok");
        assert_eq!(doc.append_child(body, div), Ok(div));
        assert_eq!(doc.element_by_id("zz"), Some(div));
        assert_eq!(doc.parent(div), Some(body));
        assert_eq!(doc.children(body).last(), Some(div));
    }

    // Chromium: c.appendChild(x) removes x from a and appends it to c.
    #[test]
    fn append_child_moves_an_attached_child() {
        let mut doc = Document::parse("<div id=a><i id=x></i><b id=y></b></div><div id=c></div>");
        let (a, c, x, y) = (
            doc.element_by_id("a").expect("ok"),
            doc.element_by_id("c").expect("ok"),
            doc.element_by_id("x").expect("ok"),
            doc.element_by_id("y").expect("ok"),
        );
        doc.append_child(c, x).expect("ok");
        assert_eq!(doc.children(a).collect::<Vec<_>>(), [y]);
        assert_eq!(doc.children(c).collect::<Vec<_>>(), [x]);
        assert_eq!(doc.parent(x), Some(c));
        assert_eq!(doc.element_by_id("x"), Some(x));
    }

    // Chromium: appending a node to its own parent moves it to the end.
    #[test]
    fn append_to_same_parent_moves_to_end() {
        let mut doc = Document::parse("<div id=a><i id=x></i><b id=y></b></div>");
        let (a, x, y) = (
            doc.element_by_id("a").expect("ok"),
            doc.element_by_id("x").expect("ok"),
            doc.element_by_id("y").expect("ok"),
        );
        doc.append_child(a, x).expect("ok");
        assert_eq!(doc.children(a).collect::<Vec<_>>(), [y, x]);
        doc.append_child(a, x).expect("ok");
        assert_eq!(doc.children(a).collect::<Vec<_>>(), [y, x]);
    }

    // Chromium: existing ids stay valid; traversal order of untouched nodes is unchanged.
    #[test]
    fn existing_ids_and_order_survive() {
        let mut doc = Document::parse("<ul id=l><li id=a></li><li id=b></li></ul>");
        let l = doc.element_by_id("l").expect("ok");
        let before: Vec<_> = doc.descendants(doc.root()).collect();
        let li = doc.create_html_element("li");
        doc.append_child(l, li).expect("ok");
        let after: Vec<_> = doc.descendants(doc.root()).filter(|&n| n != li).collect();
        assert_eq!(before, after);
        assert_eq!(kids(&doc, l), ["li", "li", "li"]);
    }

    // Chromium: HierarchyRequestError for self, own descendant, non-parent kinds.
    #[test]
    fn hierarchy_errors() {
        let mut doc = Document::parse("<div id=a><p id=p></p></div>");
        let (a, p) = (
            doc.element_by_id("a").expect("ok"),
            doc.element_by_id("p").expect("ok"),
        );
        let err = |parent, child| Err(DomError::HierarchyRequest { parent, child });
        assert_eq!(doc.append_child(a, a), err(a, a));
        assert_eq!(doc.append_child(p, a), err(p, a));
        let root = doc.root();
        assert_eq!(doc.append_child(a, root), err(a, root));
        // The tree is untouched after a refusal.
        assert_eq!(doc.parent(p), Some(a));
        assert!(
            doc.parent(a)
                .is_some_and(|b| doc.node(b).children.contains(&a))
        );
    }

    #[test]
    fn non_container_parents_are_refused() {
        let mut doc = Document::parse("<!-- c --><p>t</p>");
        let p = doc.first_element_by_name("p").expect("ok");
        let text = doc.children(p).next().expect("ok");
        let comment = doc
            .children(doc.root())
            .find(|&n| matches!(doc.node(n).data, NodeData::Comment(_)))
            .expect("ok");
        let div = doc.create_html_element("div");
        assert_eq!(
            doc.append_child(text, div),
            Err(DomError::HierarchyRequest {
                parent: text,
                child: div
            })
        );
        assert_eq!(
            doc.append_child(comment, div),
            Err(DomError::HierarchyRequest {
                parent: comment,
                child: div
            })
        );
    }

    // Chromium: text under a document, a doctype under an element and a
    // second root element all throw HierarchyRequestError.
    #[test]
    fn document_level_constraints() {
        let mut doc = Document::parse("<!DOCTYPE html><div id=a></div>");
        let a = doc.element_by_id("a").expect("ok");
        let root = doc.root();
        let doctype = doc
            .children(root)
            .find(|&n| matches!(doc.node(n).data, NodeData::Doctype { .. }))
            .expect("ok");
        let extra = doc.create_html_element("p");
        assert!(doc.append_child(root, extra).is_err());
        assert!(doc.append_child(a, doctype).is_err());
        let html = doc.html().expect("ok");
        assert_eq!(doc.parent(html), Some(root));
    }

    #[test]
    fn ids_outside_the_arena_are_stale() {
        let mut doc = Document::parse("<p></p>");
        let other = Document::parse("<p></p><p></p><p></p><p></p>");
        let far = other.descendants(other.root()).last().expect("ok");
        let body = doc.body().expect("ok");
        assert_eq!(doc.append_child(body, far), Err(DomError::StaleNode(far)));
    }

    // Chromium: appending a fragment inserts its children and empties it.
    #[test]
    fn appending_a_fragment_moves_its_children() {
        let mut doc = Document::parse("<template><u></u><s></s></template><div id=c></div>");
        let tpl = doc.first_element_by_name("template").expect("ok");
        let frag = doc.element(tpl).expect("ok").template_contents.expect("ok");
        let c = doc.element_by_id("c").expect("ok");
        assert_eq!(doc.append_child(c, frag), Ok(frag));
        assert_eq!(kids(&doc, c), ["u", "s"]);
        assert_eq!(doc.children(frag).count(), 0);
    }
}
