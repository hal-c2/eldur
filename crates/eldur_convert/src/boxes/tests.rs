use eldur_css::{Viewport, collect_stylesheets, compute_styles};
use eldur_dom::Document;

use super::*;

struct NoSheets;

impl eldur_css::StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
        None
    }
}

/// Builds the box tree of `html` and prints it in a compact outline:
/// `block <p>` / `anon` lines with inline content in `[...]`.
fn outline(html: &str) -> String {
    let doc = Document::parse(html);
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    let mut diags = Diagnostics::new("test.html");
    let tree = build(&doc, &styles, &mut diags).expect("root box");
    let mut out = String::new();
    print_box(&doc, &tree, 0, &mut out);
    out
}

fn print_box(doc: &Document, b: &LayoutBox<'_>, depth: usize, out: &mut String) {
    let name = match b.node {
        Some(n) => format!("<{}>", doc.element(n).map_or("?", |e| e.local_name())),
        None => "anon".to_owned(),
    };
    let kind = match b.kind {
        BoxKind::Block => "block",
        BoxKind::Flex => "flex",
        BoxKind::Grid => "grid",
        BoxKind::Table => "table",
        BoxKind::Replaced => "replaced",
    };
    out.push_str(&format!("{}{kind} {name}", "  ".repeat(depth)));
    if let Some(marker) = &b.marker {
        out.push_str(&format!(" {:?}", marker.kind));
    }
    match &b.content {
        Content::Empty => out.push('\n'),
        Content::Inline(items) => {
            out.push(' ');
            print_inline(doc, items, out);
            out.push('\n');
        }
        Content::Blocks(children) => {
            out.push('\n');
            for child in children {
                print_box(doc, child, depth + 1, out);
            }
        }
    }
}

fn print_inline(doc: &Document, items: &[Inline<'_>], out: &mut String) {
    out.push('[');
    for item in items {
        match item {
            Inline::Text(t) => out.push_str(&format!("{t:?}")),
            Inline::LineBreak => out.push_str("BR"),
            Inline::Span(span) => {
                let name = span
                    .node
                    .and_then(|n| doc.element(n))
                    .map_or("pseudo", |e| e.local_name());
                out.push_str(name);
                print_inline(doc, &span.children, out);
            }
            Inline::Atomic(b) => {
                let mut inner = String::new();
                print_box(doc, b, 0, &mut inner);
                out.push_str(&format!("atomic:{}", inner.trim_end().replace(" <", "<")));
            }
        }
    }
    out.push(']');
}

#[test]
fn inline_run_next_to_block_gets_anonymous_box() {
    assert_eq!(
        outline("<body>Hello <b>world</b><p>para</p>tail</body>"),
        "block <html>\n  block <body>\n    block anon [\"Hello \"b[\"world\"]]\n    block <p> [\"para\"]\n    block anon [\"tail\"]\n"
    );
}

#[test]
fn out_of_flow_boxes_with_no_line_before_them_stay_in_tree_order() {
    let out = outline(
        "<body><section style='position:absolute'></section>\n\
         <input style='position:absolute'>\n\
         <article style='position:absolute'></article><p>x</p></body>",
    );
    let (a, b, c) = (
        out.find("<section"),
        out.find("<input"),
        out.find("<article"),
    );
    assert!(a.is_some() && a < b && b < c, "{out}");
}

#[test]
fn whitespace_between_blocks_makes_no_box() {
    assert_eq!(
        outline("<body>\n  <p>a</p>\n  \n  <p>b</p>\n</body>"),
        "block <html>\n  block <body>\n    block <p> [\"a\"]\n    block <p> [\"b\"]\n"
    );
}

#[test]
fn whitespace_collapses_across_inline_boxes() {
    assert_eq!(
        outline("<p>  one \n\t two <em> three </em> four  </p>"),
        "block <html>\n  block <body>\n    block <p> [\"one two \"em[\"three \"]\"four\"]\n"
    );
}

#[test]
fn display_none_generates_nothing() {
    assert_eq!(
        outline(
            "<style>.gone{display:none}</style><p>a<span class=gone>hidden</span>b</p>\
             <div class=gone><p>x</p></div><p hidden>h</p>"
        ),
        "block <html>\n  block <body>\n    block <p> [\"ab\"]\n"
    );
}

#[test]
fn display_contents_hoists_children() {
    assert_eq!(
        outline("<style>div{display:contents}</style><body><div><p>a</p>b</div></body>"),
        "block <html>\n  block <body>\n    block <p> [\"a\"]\n    block anon [\"b\"]\n"
    );
}

#[test]
fn line_break_removes_spaces_around_it() {
    assert_eq!(
        outline("<p>one <br> two</p>"),
        "block <html>\n  block <body>\n    block <p> [\"one\"BR\"two\"]\n"
    );
}

#[test]
fn pre_keeps_spaces_and_newlines() {
    assert_eq!(
        outline("<pre>a  b\n\tc</pre>"),
        "block <html>\n  block <body>\n    block <pre> [\"a  b\"BR\"        c\"]\n"
    );
}

#[test]
fn pre_drops_leading_newline_like_the_parser() {
    // The HTML parser drops the newline right after `<pre>`.
    assert_eq!(
        outline("<pre>\nx</pre>"),
        "block <html>\n  block <body>\n    block <pre> [\"x\"]\n"
    );
}

#[test]
fn inline_with_block_child_is_blockified() {
    assert_eq!(
        outline("<body><a href=x><div>card</div></a></body>"),
        "block <html>\n  block <body>\n    block <a>\n      block <div> [\"card\"]\n"
    );
}

#[test]
fn layout_kinds_are_kept() {
    assert_eq!(
        outline(
            "<style>.f{display:flex}.g{display:grid}</style><body>\
             <div class=f>a</div><div class=g>b</div><img src=x.png><table><tr><td>c</td></tr></table></body>"
        ),
        "block <html>\n  block <body>\n    flex <div> [\"a\"]\n    grid <div> [\"b\"]\n    block anon [atomic:replaced<img>]\n    table <table>\n      table <tbody>\n        table <tr>\n          table <td> [\"c\"]\n"
    );
}

#[test]
fn list_items_get_markers() {
    assert_eq!(
        outline("<ul><li>a</li></ul><ol start=3><li>b</li><li>c</li></ol>"),
        "block <html>\n  block <body>\n    block <ul>\n      block <li> Disc [\"a\"]\n    block <ol>\n      block <li> Text(\"3. \") [\"b\"]\n      block <li> Text(\"4. \") [\"c\"]\n"
    );
}

/// The `li` lines of `outline(html)`, without their indentation.
fn items(html: &str) -> Vec<String> {
    outline(html)
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("block <li>"))
        .map(|l| l.trim_start_matches("block <li> ").to_owned())
        .collect()
}

#[test]
fn inside_markers_are_inline_content() {
    assert_eq!(
        items("<style>ul,ol{list-style-position:inside}</style><ul><li>a</ul><ol><li>b</ol>"),
        ["[\"•\\u{2003}a\"]", "[\"1. b\"]"]
    );
}

#[test]
fn ordinals_follow_start_reversed_and_value() {
    assert_eq!(
        items("<ol reversed><li>a<li>b<li>c</ol>"),
        [
            "Text(\"3. \") [\"a\"]",
            "Text(\"2. \") [\"b\"]",
            "Text(\"1. \") [\"c\"]"
        ]
    );
    assert_eq!(
        items("<ol start=10 reversed><li>a<li>b</ol>"),
        ["Text(\"10. \") [\"a\"]", "Text(\"9. \") [\"b\"]"]
    );
    assert_eq!(
        items("<ol><li>a<li value=7>b<li>c</ol>"),
        [
            "Text(\"1. \") [\"a\"]",
            "Text(\"7. \") [\"b\"]",
            "Text(\"8. \") [\"c\"]"
        ]
    );
}

#[test]
fn hidden_and_non_list_item_siblings_do_not_count() {
    assert_eq!(
        items("<ol><li hidden>a<li>b<li style='display:block'>c<li>d</ol>"),
        ["Text(\"1. \") [\"b\"]", "[\"c\"]", "Text(\"2. \") [\"d\"]"].map(str::to_owned)
    );
}

#[test]
fn marker_text_for_each_list_style_type() {
    let first = |ty: &str| {
        items(&format!(
            "<ol start=4 style='list-style-type:{ty}'><li>x</ol>"
        ))
        .remove(0)
    };
    assert_eq!(first("decimal-leading-zero"), "Text(\"04. \") [\"x\"]");
    assert_eq!(first("lower-alpha"), "Text(\"d. \") [\"x\"]");
    assert_eq!(first("upper-alpha"), "Text(\"D. \") [\"x\"]");
    assert_eq!(first("lower-roman"), "Text(\"iv. \") [\"x\"]");
    assert_eq!(first("upper-roman"), "Text(\"IV. \") [\"x\"]");
    assert_eq!(first("circle"), "Circle [\"x\"]");
    assert_eq!(first("square"), "Square [\"x\"]");
    assert_eq!(first("none"), "[\"x\"]");
}

#[test]
fn stray_table_parts_get_anonymous_table_boxes() {
    assert_eq!(
        outline(
            "<style>.c{display:table-cell}.r{display:table-row}.t{display:table}</style>\
             <body><div class=c>a</div> <div class=c>b</div><p>d</p></body>"
        ),
        "block <html>\n  block <body>\n    table anon\n      table anon\n        table <div> [\"a\"]\n        table <div> [\"b\"]\n    block <p> [\"d\"]\n"
    );
    assert_eq!(
        outline(
            "<style>.r{display:table-row}.t{display:table}</style>\
             <body><div class=t>x<div class=r>y</div>  </div></body>"
        ),
        "block <html>\n  block <body>\n    table <div>\n      table anon\n        table anon [\"x\"]\n      table <div>\n        table anon [\"y\"]\n"
    );
}

#[test]
fn whitespace_between_table_parts_generates_nothing() {
    assert_eq!(
        outline("<table> <colgroup> <col> </colgroup> <tr> <td>a</td> </tr> </table>"),
        "block <html>\n  block <body>\n    table <table>\n      table <colgroup>\n        table <col>\n      table <tbody>\n        table <tr>\n          table <td> [\"a\"]\n"
    );
}

#[test]
fn before_and_after_become_inline_boxes() {
    assert_eq!(
        outline("<style>p::before{content:'[' attr(id)}p::after{content:']'}</style><p id=x>y</p>"),
        "block <html>\n  block <body>\n    block <p> [pseudo[\"[x\"]\"y\"pseudo[\"]\"]]\n"
    );
}

#[test]
fn empty_block_has_empty_content() {
    assert_eq!(
        outline("<body><div>  </div></body>"),
        "block <html>\n  block <body>\n    block <div>\n"
    );
}

#[test]
fn anonymous_box_inherits_text_style() {
    let doc = Document::parse("<body style='color:red;font-size:20px;margin:3px'>x<p>y</p></body>");
    let viewport = Viewport::default();
    let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
    let styles = compute_styles(&doc, &sheets, &viewport);
    let mut diags = Diagnostics::new("test.html");
    let tree = build(&doc, &styles, &mut diags).expect("root box");
    let Content::Blocks(html) = &tree.content else {
        panic!("html has blocks");
    };
    let Content::Blocks(body) = &html[0].content else {
        panic!("body has blocks");
    };
    let anon = &body[0];
    assert!(anon.is_anonymous());
    assert_eq!(anon.style.font_size, 20.0);
    assert_eq!(anon.style.color, eldur_css::Rgba::new(255, 0, 0, 255));
    assert_eq!(
        anon.style.margin.top,
        eldur_css::LengthAuto::Length(eldur_css::Length::px(0.0))
    );
}

#[test]
fn an_inline_block_alone_in_its_parent_is_atomic_in_a_line() {
    let tree = outline("<div><a style=\"display:inline-block\" href=\"#\">x</a> </div>");
    assert!(tree.contains("atomic:block<a>"), "{tree}");
}

#[test]
fn an_inline_block_beside_text_stays_in_the_line() {
    let tree = outline("<div>before <a style=\"display:inline-block\" href=\"#\">x</a></div>");
    assert!(!tree.contains("block <a>"), "{tree}");
}

#[test]
fn an_inline_block_beside_text_is_an_atomic_box_in_the_line() {
    let tree = outline(
        "<div>before <span style=\"display:inline-block\">x</span> after <span>y</span></div>",
    );
    assert!(
        tree.contains(r#"block <div> ["before "atomic:block<span> ["x"]" after "span["y"]]"#),
        "{tree}"
    );
}

#[test]
fn an_atomic_inline_keeps_its_block_children() {
    let tree =
        outline("<div>a <span style=\"display:inline-flex\"><div>b</div><div>c</div></span></div>");
    assert!(
        tree.contains(r#"block <div> ["a "atomic:flex<span>"#),
        "{tree}"
    );
    assert!(tree.contains(r#"block<div> ["b"]"#), "{tree}");
}

#[test]
fn a_space_after_an_atomic_inline_at_the_line_end_is_removed() {
    let tree = outline("<div>a <span style=\"display:inline-block\">x</span> </div>");
    assert!(
        tree.contains(r#"block <div> ["a "atomic:block<span> ["x"]]"#),
        "{tree}"
    );
}

/// Chromium's layout text (DOMSnapshot) for the same page: a reset's scope
/// takes in the following siblings, `counters()` joins the nested ones,
/// `counter-set` applies after `counter-increment`, and a counter nothing
/// reset is created where it is first incremented.
#[test]
fn counters_number_generated_content() {
    let tree = outline(concat!(
        "<style>ol{list-style:none;counter-reset:s}li{counter-increment:s}",
        "li::before{content:counters(s, '.') ') '}",
        ".x::before{content:counter(s, lower-alpha) '|' counter(t) '|' counter(s, upper-roman) ' '}",
        ".r{counter-reset:s 4}.inc2{counter-increment:s 2}.set{counter-set:s 10}",
        "p{counter-increment:t}</style>",
        "<ol><li>a</li><li class=inc2>b<ol><li>c</li><li class=set>d</li></ol></li><li>e</li></ol>",
        "<div><p class=x>f</p><p class='x r'>g</p><p class=x>h</p></div>"
    ));
    for line in [
        r#"block <li> [pseudo["1) "]"a"]"#,
        r#"anon [pseudo["3) "]"b"]"#,
        r#"block <li> [pseudo["3.1) "]"c"]"#,
        r#"block <li> [pseudo["3.10) "]"d"]"#,
        r#"block <li> [pseudo["4) "]"e"]"#,
        r#"block <p> [pseudo["d|1|IV "]"f"]"#,
        r#"block <p> [pseudo["d|2|IV "]"g"]"#,
        r#"block <p> [pseudo["d|3|IV "]"h"]"#,
    ] {
        assert!(tree.contains(line), "{line} in\n{tree}");
    }
}

/// Chromium (created_rules/rule_counters_own.html): an `<ol start>` resets
/// `list-item` to one below it, and a nested `<ol>` restarts it though its own
/// `counter-reset` names another counter.
#[test]
fn lists_reset_list_item() {
    let tree = outline(concat!(
        "<style>ol{list-style:none}li::before{content:counter(list-item, upper-roman) '. '}",
        ".own{counter-reset:n}</style>",
        "<ol start=5><li>a</li><li>b</li></ol>",
        "<ol><li>c<ol class=own><li>d</li><li>e</li></ol></li><li>f</li></ol>"
    ));
    for line in [
        r#"[pseudo["V. "]"a"]"#,
        r#"[pseudo["VI. "]"b"]"#,
        r#"[pseudo["I. "]"c"]"#,
        r#"[pseudo["I. "]"d"]"#,
        r#"[pseudo["II. "]"e"]"#,
        r#"[pseudo["II. "]"f"]"#,
    ] {
        assert!(tree.contains(line), "{line} in\n{tree}");
    }
}
