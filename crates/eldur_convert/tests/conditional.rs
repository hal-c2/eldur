//! Conditional styles of static elements become bindings
//! (`docs/design/dynamic-styles.md` §4, §13 item 4).

use eldur_convert::{Input, NoResources, Options, RootKind};

fn main_qml_with(html: &str, options: &Options) -> String {
    eldur_convert::convert(
        &Input {
            html: html.to_owned(),
            url: "index.html".to_owned(),
        },
        options,
        &mut NoResources,
    )
    .main_qml()
    .expect("Main.qml")
    .to_owned()
}

fn main_qml(html: &str) -> String {
    main_qml_with(
        html,
        &Options {
            root: RootKind::Item,
            source_comments: false,
            ..Options::default()
        },
    )
}

/// The lines of `qml` that contain `needle`, trimmed.
fn lines_with(qml: &str, needle: &str) -> Vec<String> {
    qml.lines()
        .filter(|l| l.contains(needle))
        .map(|l| l.trim().to_owned())
        .collect()
}

const SIBLINGS: &str = "<style>.on { color: red } .on + p { color: blue }</style>\
     <p id='a'>a</p><p id='b'>b</p><p id='c'>c</p>\
     <script>\
       document.getElementById('a').classList.toggle('on');\
       document.getElementById('b').classList.toggle('on');\
       document.getElementById('c').classList.toggle('on');\
     </script>";

#[test]
fn a_binding_reads_only_the_conditions_that_change_it() {
    let qml = main_qml(SIBLINGS);
    // `b` is blue after an `.on` sibling, whatever its own class; `c`'s
    // class does not reach it.
    assert_eq!(
        lines_with(&qml, "color: b"),
        [
            r##"color: b.cls_on ? (a.cls_on ? "#0000ff" : "#ff0000") : (a.cls_on ? "#0000ff" : "#000000")"##
        ]
    );
    assert_eq!(
        lines_with(&qml, "color: a"),
        [r##"color: a.cls_on ? "#ff0000" : "#000000""##]
    );
}

#[test]
fn hover_reads_a_handler_or_the_control() {
    let qml = main_qml(
        "<style>div:hover { background: red } button { background: white } \
         button:hover { background: blue }</style>\
         <div>x</div><button>b</button>",
    );
    assert_eq!(
        lines_with(&qml, "Hover.hovered ?"),
        [r##"color: divHover.hovered ? "#ff0000" : "#00000000""##]
    );
    assert_eq!(lines_with(&qml, "HoverHandler"), ["HoverHandler {"]);
    assert_eq!(
        lines_with(&qml, "Control.hovered ?"),
        [r##"color: buttonControl.hovered ? "#0000ff" : "#ffffff""##]
    );
    assert_eq!(lines_with(&qml, "hoverEnabled"), ["hoverEnabled: true"]);
}

#[test]
fn transitions_become_behaviors_that_wait_for_the_scripts() {
    let page = "<style>p { opacity: 1; transition: opacity 0.25s linear } \
         p.dim { opacity: 0.5 }</style><p id='p'>x</p>";
    let static_qml = main_qml(&format!("<style>p:hover {{ opacity: 0.5 }}</style>{page}"));
    assert_eq!(lines_with(&static_qml, "duration"), ["duration: 250"]);
    assert!(!static_qml.contains("eldurAnimate"), "{static_qml}");

    let qml = main_qml(&format!(
        "{page}<script>document.getElementById('p').classList.add('dim');</script>"
    ));
    assert_eq!(
        lines_with(&qml, "opacity: p"),
        ["opacity: p.cls_dim ? 0.5 : 1"]
    );
    assert_eq!(lines_with(&qml, "Behavior on"), ["Behavior on opacity {"]);
    // The init function sets the document's classes; that must not animate.
    assert_eq!(
        lines_with(&qml, "eldurAnimate"),
        [
            "property bool eldurAnimate: false",
            "root.eldurAnimate = true;",
            "duration: root.eldurAnimate ? 250 : 0",
        ]
    );
}

#[test]
fn display_none_under_a_class_is_visible() {
    let qml = main_qml(
        "<style>.gone { display: none }</style><section id='s'><p>x</p></section>\
         <script>document.getElementById('s').classList.toggle('gone');</script>",
    );
    assert_eq!(lines_with(&qml, "visible: "), ["visible: !s.cls_gone"]);
}

#[test]
fn without_state_variants_the_document_is_all_there_is() {
    let qml = main_qml_with(
        SIBLINGS,
        &Options {
            root: RootKind::Item,
            source_comments: false,
            state_variants: false,
            ..Options::default()
        },
    );
    assert!(!qml.contains(" ? "), "{qml}");
}

fn convert(html: &str) -> eldur_convert::Output {
    eldur_convert::convert(
        &Input {
            html: html.to_owned(),
            url: "index.html".to_owned(),
        },
        &Options {
            root: RootKind::Item,
            source_comments: false,
            ..Options::default()
        },
        &mut NoResources,
    )
}

/// A list of `n` items and a list scripts never change, each styled by
/// position, with a script that removes the first item of `#list`.
fn positions_page(n: usize) -> String {
    let items: String = (0..n).map(|i| format!("<li>{i}</li>")).collect();
    format!(
        "<style>li {{ background: white }} li:first-child {{ color: red }} \
         li:nth-child(even) {{ background: silver }}</style>\
         <ul id='list'>{items}</ul><ul id='fixed'><li>a</li><li>b</li></ul>\
         <script>var list = document.getElementById('list');\
         list.removeChild(list.firstElementChild);</script>"
    )
}

#[test]
fn positions_in_a_changing_list_are_bindings() {
    let out = convert(&positions_page(3));
    let qml = out.main_qml().expect("Main.qml");
    // Only `#list`'s items read their place; `#fixed` keeps the document's.
    assert_eq!(lines_with(qml, "property int eldurIndex").len(), 3, "{qml}");
    assert_eq!(lines_with(qml, "property int eldurCount").len(), 3, "{qml}");
    assert_eq!(
        lines_with(qml, "% 2 === 0 ?").len(),
        3,
        "each item's background reads its place: {qml}"
    );
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.to_string().contains("dynamic-structural")),
        "{:?}",
        out.diagnostics
    );
}

#[test]
fn a_changing_list_with_too_many_items_keeps_the_document_places() {
    let out = convert(&positions_page(33));
    let qml = out.main_qml().expect("Main.qml");
    assert!(lines_with(qml, "eldurIndex").is_empty(), "{qml}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.to_string().contains("dynamic-structural")),
        "{:?}",
        out.diagnostics
    );
}
