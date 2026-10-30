use h2q_css::{Viewport, collect_stylesheets, compute_styles};
use h2q_dom::Document;

use super::*;

struct NoSheets;

impl h2q_css::StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<h2q_css::LoadedSheet> {
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
        "block <html>\n  block <body>\n    flex <div> [\"a\"]\n    grid <div> [\"b\"]\n    replaced <img>\n    table <table>\n      table <tbody>\n        table <tr>\n          table <td> [\"c\"]\n"
    );
}

#[test]
fn list_items_get_markers() {
    assert_eq!(
        outline("<ul><li>a</li></ul><ol start=3><li>b</li><li>c</li></ol>"),
        "block <html>\n  block <body>\n    block <ul>\n      block <li> [\"• a\"]\n    block <ol>\n      block <li> [\"3. b\"]\n      block <li> [\"4. c\"]\n"
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
    assert_eq!(anon.style.color, h2q_css::Rgba::new(255, 0, 0, 255));
    assert_eq!(
        anon.style.margin.top,
        h2q_css::LengthAuto::Length(h2q_css::Length::px(0.0))
    );
}
