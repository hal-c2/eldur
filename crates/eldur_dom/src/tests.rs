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
fn svg_markup_is_a_standalone_svg_document() {
    let doc = Document::parse(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='currentColor'>\
         <style>path{stroke:currentColor}</style><linearGradient id=g><stop offset=0 /></linearGradient>\
         <use xlink:href='#p' /><text>a &lt; b</text><path d='M0 0' title='\"' /></svg>",
    );
    let svg = doc
        .descendants(doc.root())
        .find(|&n| doc.element(n).is_some_and(Element::is_svg))
        .expect("svg");
    let upper = |name: &str, v: &str, _: Option<&str>| {
        if name == "fill" || name == "#style" {
            v.replace("currentColor", "#123456")
        } else {
            v.to_owned()
        }
    };
    assert_eq!(
        doc.svg_markup(svg, None, &upper).as_deref(),
        Some(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             viewBox=\"0 0 24 24\" fill=\"#123456\"><style>path{stroke:#123456}</style>\
             <linearGradient id=\"g\"><stop offset=\"0\"/></linearGradient>\
             <use xlink:href=\"#p\"/><text>a &lt; b</text><path d=\"M0 0\" title=\"&quot;\"/></svg>"
        )
    );
    // Not an svg element.
    let body = doc.body().expect("body");
    assert_eq!(doc.svg_markup(body, None, &upper), None);
}

#[test]
fn svg_markup_carries_what_it_references_from_outside() {
    // A sprite: the `<symbol>` lives in another svg, and the gradient it
    // fills with in a third. The standalone file must hold both, or it
    // draws nothing.
    let doc = Document::parse(
        "<svg style='display:none'><symbol id=s viewBox='0 0 24 24'>\
         <rect fill='url(#g)' /></symbol><path id=unused d='M0 0'/></svg>\
         <svg><linearGradient id=g><stop offset=0 stop-color='currentColor'/></linearGradient></svg>\
         <svg id=icon><use href='#s'/><use xlink:href='#missing'/><circle id=own /><use href='#own'/></svg>",
    );
    let icon = doc
        .descendants(doc.root())
        .find(|&n| doc.element(n).and_then(Element::id) == Some("icon"))
        .expect("icon");
    let red = |_: &str, v: &str, _: Option<&str>| v.replace("currentColor", "#c00");
    assert_eq!(
        doc.svg_markup(icon, None, &red).as_deref(),
        Some(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             id=\"icon\"><use href=\"#s\"/><use xlink:href=\"#missing\"/><circle id=\"own\"/>\
             <use href=\"#own\"/><defs><symbol id=\"s\" viewBox=\"0 0 24 24\"><rect fill=\"url(#g)\"/>\
             </symbol><linearGradient id=\"g\"><stop offset=\"0\" stop-color=\"#c00\"/>\
             </linearGradient></defs></svg>"
        )
    );
}

#[test]
fn svg_markup_drops_attributes_with_an_unbound_prefix() {
    // HTML parsing binds only `xlink:`, `xml:` and `xmlns:`; an editor's
    // `inkscape:label` would make the file namespace-ill-formed, and an XML
    // loader then refuses all of it. A framework's `@click` is no XML name
    // at all; `x.y` is one, and stays.
    let doc = Document::parse(
        "<svg xmlns:inkscape='http://www.inkscape.org/namespaces/inkscape' \
         inkscape:label=x sodipodi:docname=y xml:space=preserve width=4 @click=go x.y=1>\
         <g inkscape:groupmode=layer/></svg>",
    );
    let svg = doc
        .descendants(doc.root())
        .find(|&n| doc.element(n).is_some_and(Element::is_svg))
        .expect("svg");
    let same = |_: &str, v: &str, _: Option<&str>| v.to_owned();
    assert_eq!(
        doc.svg_markup(svg, None, &same).as_deref(),
        Some(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xml:space=\"preserve\" width=\"4\" x.y=\"1\"><g/></svg>"
        )
    );
}

#[test]
fn svg_markup_tells_the_colour_the_svg_sets_itself() {
    // `currentColor` under `<g color=blue>` is blue in the page, whatever
    // colour holds the svg; a `style` colour beats the attribute, and
    // `currentColor` as a colour sets nothing new.
    let doc = Document::parse(
        "<svg fill=currentColor><g color=blue><path d='M0 0' fill=currentColor/>\
         <g style='stroke:red; color: #0f0 !important' color=red><path d='M1 1'/></g>\
         <g color=red style='color:inherit'><path d='M3 3'/></g>\
         <g color=red style='color: #00f !important; color: inherit'><path d='M4 4'/></g></g>\
         <g color=currentColor><path d='M2 2'/></g></svg>",
    );
    let svg = doc
        .descendants(doc.root())
        .find(|&n| doc.element(n).is_some_and(Element::is_svg))
        .expect("svg");
    let tag = |name: &str, v: &str, c: Option<&str>| {
        if name == "d" {
            format!("{v} {}", c.unwrap_or("holder"))
        } else {
            v.to_owned()
        }
    };
    let markup = doc.svg_markup(svg, None, &tag).expect("markup");
    assert!(markup.contains("d=\"M0 0 blue\""), "{markup}");
    assert!(markup.contains("d=\"M1 1 #0f0\""), "{markup}");
    assert!(markup.contains("d=\"M2 2 holder\""), "{markup}");
    // A declared `inherit` beats the presentation attribute: Chromium
    // draws M3 3 blue.
    assert!(markup.contains("d=\"M3 3 blue\""), "{markup}");
    // An important colour beats a later normal `inherit`.
    assert!(markup.contains("d=\"M4 4 #00f\""), "{markup}");
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

#[test]
fn a_used_symbol_is_sized_where_it_is_drawn() {
    // A symbol with no size of its own fills the viewport its `<use>` makes
    // (SVG 2 §5.6.2), 100 % of the svg holding it; some loaders draw it at
    // its viewBox's size instead. Written in units it is the same picture
    // in a browser, as a use's own size still wins.
    let doc = Document::parse(
        "<svg style='display:none'><symbol id=s viewBox='0 0 24 24'><rect /></symbol>\
         <symbol id=fixed viewBox='0 0 24 24' width=10 height=auto><rect /></symbol></svg>\
         <svg id=px width=48px height=48><use href='#s'/></svg>\
         <svg id=box viewBox='0 0 32 30' width=64><use href='#s'/></svg>\
         <svg id=same width=48 height=48><use href='#s' width=20 height=20 /><use href='#s' width=20 height=20 /></svg>\
         <svg id=mixed width=48 height=48><use href='#s' width=20 height=20 /><use href='#s'/></svg>\
         <svg id=own width=48 height=48><use href='#fixed'/></svg>\
         <svg id=inside width=40 height=40><symbol id=i viewBox='0 0 8 8'/><use href='#i'/></svg>\
         <svg id=unsized><use href='#s'/></svg>\
         <svg id=css class=icon><use href='#s'/></svg>\
         <svg id=wide width=100% viewBox='0 0 24 24'><use href='#s'/></svg>\
         <svg id=over width=24 height=24><rect width=12 height=12 /></svg>",
    );
    let same = |_: &str, v: &str, _: Option<&str>| v.to_owned();
    // What the layout gives a CSS-sized icon (`.icon { width: 48px }`).
    let used = |id: &str| match id {
        "css" => Some((48.0, 47.5)),
        "wide" => Some((300.0, 300.0)),
        "over" => Some((48.0, 48.0)),
        _ => None,
    };
    let symbol = |id: &str| {
        let markup = doc
            .svg_markup(doc.element_by_id(id).expect(id), used(id), &same)
            .expect("markup");
        let at = markup.find("<symbol").expect("symbol");
        markup[at..markup[at..].find('>').map_or(markup.len(), |e| at + e + 1)].to_owned()
    };
    let s = "<symbol id=\"s\" viewBox=\"0 0 24 24\"";
    assert_eq!(symbol("px"), format!("{s} width=\"48\" height=\"48\">"));
    assert_eq!(symbol("box"), format!("{s} width=\"32\" height=\"30\">"));
    assert_eq!(symbol("same"), format!("{s} width=\"20\" height=\"20\">"));
    assert_eq!(symbol("mixed"), format!("{s} width=\"48\" height=\"48\">"));
    assert_eq!(
        symbol("own"),
        "<symbol id=\"fixed\" viewBox=\"0 0 24 24\" width=\"10\" height=\"48\">"
    );
    assert_eq!(
        symbol("inside"),
        "<symbol id=\"i\" viewBox=\"0 0 8 8\" width=\"40\" height=\"40\"/>"
    );
    assert_eq!(symbol("unsized"), format!("{s}>"));
    assert_eq!(symbol("css"), format!("{s} width=\"48\" height=\"47.5\">"));
    assert_eq!(symbol("wide"), format!("{s} width=\"24\" height=\"24\">"));
    // The file has no CSS box: the root takes the size it is laid out at,
    // in place of a length that needs one.
    let root = |id: &str| {
        let markup = doc
            .svg_markup(doc.element_by_id(id).expect(id), used(id), &same)
            .expect("markup");
        markup[..markup.find('>').expect("tag") + 1].to_owned()
    };
    let ns = "<svg xmlns=\"http://www.w3.org/2000/svg\"";
    assert_eq!(
        root("css"),
        format!("{ns} id=\"css\" class=\"icon\" width=\"48\" height=\"47.5\">")
    );
    assert_eq!(
        root("wide"),
        format!("{ns} id=\"wide\" viewBox=\"0 0 24 24\" width=\"300\" height=\"300\">")
    );
    // CSS overrides the attributes: Chromium draws the 12-unit rect of a
    // 48px box with `width=24` and no viewBox at 12px, not scaled to 24.
    assert_eq!(
        root("over"),
        format!("{ns} id=\"over\" width=\"48\" height=\"48\">")
    );
    assert_eq!(
        root("px"),
        format!("{ns} id=\"px\" width=\"48px\" height=\"48\">")
    );
    assert_eq!(root("unsized"), format!("{ns} id=\"unsized\">"));
}
