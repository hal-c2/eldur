//! What runtime metadata the converter emits, and when
//! (`docs/design/dynamic-styles.md` §7, §8.1).

use h2q_convert::{Input, NoResources, Options, Output, RootKind};

fn convert(html: &str, url: &str) -> Output {
    let options = Options {
        root: RootKind::Item,
        source_comments: false,
        ..Options::default()
    };
    h2q_convert::convert(
        &Input {
            html: html.to_owned(),
            url: url.to_owned(),
        },
        &options,
        &mut NoResources,
    )
}

fn main_qml(html: &str, url: &str) -> String {
    convert(html, url).main_qml().expect("Main.qml").to_owned()
}

/// The lines of `qml` that contain `needle`, trimmed.
fn lines_with(qml: &str, needle: &str) -> Vec<String> {
    qml.lines()
        .filter(|l| l.contains(needle))
        .map(|l| l.trim().to_owned())
        .collect()
}

const STYLED: &str = "<style>.a { color: red } .b.c > .d:hover, :not(.e) { color: blue } \
     @media (min-width: 1.5em) { .f { margin: 0 } } p[title='.g'] { margin: 0 }</style>";

#[test]
fn a_page_without_scripts_has_no_metadata() {
    let qml = main_qml(
        &format!("{STYLED}<div class='a'><p class='b'>x</p></div>"),
        "index.html",
    );
    for name in [
        "h2qTag",
        "h2qClasses",
        "h2qStyledClasses",
        "baseUrl",
        "cls_",
    ] {
        assert!(!qml.contains(name), "{name} in\n{qml}");
    }
}

#[test]
fn literal_queries_get_a_base_url_but_no_element_metadata() {
    let qml = main_qml(
        "<div class='a'><p id='p' class='b'>x</p></div>\
         <script>document.getElementById('p').textContent = 'y';</script>",
        "index.html",
    );
    for name in ["h2qTag", "h2qClasses", "h2qStyledClasses", "cls_"] {
        assert!(!qml.contains(name), "{name} in\n{qml}");
    }
    assert_eq!(
        lines_with(&qml, "baseUrl"),
        [r#"property string baseUrl: Qt.resolvedUrl("Main.qml")"#]
    );
}

#[test]
fn runtime_dom_access_tags_every_element_object() {
    let qml = main_qml(
        &format!(
            "{STYLED}<div class='a  b'><p>x</p><section id='s' class='c'><button>Go</button></section></div>\
             <script>var q = '.' + 'a'; document.querySelector(q);</script>"
        ),
        "index.html",
    );
    assert_eq!(
        lines_with(&qml, "h2qTag"),
        [
            r#"property string h2qTag: "html""#,
            r#"property string h2qTag: "body""#,
            r#"property string h2qTag: "div""#,
            r#"property string h2qTag: "p""#,
            r#"property string h2qTag: "section""#,
            r#"property string h2qTag: "button""#,
        ]
    );
    // The attribute as written, so `className` reads back what the page has.
    assert_eq!(
        lines_with(&qml, "h2qStyledClasses"),
        [r#"property var h2qStyledClasses: ["a", "b", "c", "d", "e", "f"]"#]
    );
    assert_eq!(
        lines_with(&qml, "h2qClasses"),
        [
            r#"property string h2qClasses: "a  b""#,
            r#"property string h2qClasses: "c""#,
        ]
    );
    // An element with an id can be found by it at run time.
    assert_eq!(lines_with(&qml, "objectName"), [r#"objectName: "s""#]);
}

#[test]
fn class_writes_declare_their_class_properties() {
    let out = convert(
        "<style>.on { color: red }</style>\
         <p id='p' class='x on'>x</p><p id='q'>y</p>\
         <script>\
           var p = document.getElementById('p');\
           p.classList.toggle('on'); p.classList.add('is-done');\
           document.getElementById('q').classList.add('on', 'a-b', 'a_b');\
         </script>",
        "index.html",
    );
    let qml = out.main_qml().expect("Main.qml");
    assert_eq!(
        lines_with(qml, "property bool cls_"),
        [
            "property bool cls_is_done: false",
            "property bool cls_on: true",
            "property bool cls_a_b: false",
            "property bool cls_on: false",
        ]
    );
    // A styled class the script toggles restyles its element.
    assert_eq!(
        lines_with(qml, ".cls_on ?"),
        [
            r##"color: p.cls_on ? "#ff0000" : "#000000""##,
            r##"color: q.cls_on ? "#ff0000" : "#000000""##,
        ]
    );
    let collisions: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("cls_a_b"))
        .map(ToString::to_string)
        .collect();
    assert_eq!(collisions.len(), 1, "{:?}", out.diagnostics);
    assert!(collisions[0].contains("`a-b`") && collisions[0].contains("`a_b`"));
}

#[test]
fn an_https_page_uses_its_own_url() {
    let qml = main_qml(
        "<script>console.log(location.href);</script>",
        "https://example.com/a/index.html",
    );
    assert_eq!(
        lines_with(&qml, "baseUrl"),
        [r#"property string baseUrl: "https://example.com/a/index.html""#]
    );
}

#[test]
fn base_href_resolves_against_the_page() {
    let page = "<head><base href='../b/'><base href='ignored/'></head>\
                <script>console.log(location.href);</script>";
    assert_eq!(
        lines_with(
            &main_qml(page, "https://example.com/a/c/index.html"),
            "baseUrl"
        ),
        [r#"property string baseUrl: "https://example.com/a/b/""#]
    );
    // Relative to the converted app, as the page's own URL is.
    assert_eq!(
        lines_with(&main_qml(page, "/home/me/site/index.html"), "baseUrl"),
        [r#"property string baseUrl: Qt.resolvedUrl("../b/")"#]
    );
    let absolute = "<base href='http://other.example/x/'><script>location.href</script>";
    assert_eq!(
        lines_with(&main_qml(absolute, "index.html"), "baseUrl"),
        [r#"property string baseUrl: "http://other.example/x/""#]
    );
}
