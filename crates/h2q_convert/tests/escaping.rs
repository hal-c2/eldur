//! Page text reaches QML twice-encoded: as rich text inside a QML string
//! literal. Markup-like text must stay text at both levels.

use h2q_convert::{Options, convert_str};

fn main_qml(html: &str) -> String {
    let out = convert_str(html, &Options::default());
    let file = out
        .files
        .iter()
        .find(|f| f.path.as_os_str() == "Main.qml")
        .expect("Main.qml");
    String::from_utf8(file.contents.clone()).expect("utf-8")
}

#[test]
fn styled_text_escapes_markup_characters_in_page_text() {
    let qml = main_qml(r#"<p>if a &lt; b &amp;&amp; c &gt; "d" then <b>x</b></p>"#);
    assert!(
        qml.contains(r#"text: "if a &lt; b &amp;&amp; c &gt; \"d\" then <b>x</b>""#),
        "{qml}"
    );
}

#[test]
fn plain_text_keeps_markup_characters_literal() {
    let qml = main_qml(r#"<p>a &lt;b&gt; "c" \ d</p>"#);
    assert!(qml.contains(r#"text: "a <b> \"c\" \\ d""#), "{qml}");
    assert!(qml.contains("textFormat: Text.PlainText"), "{qml}");
}

#[test]
fn link_href_is_attribute_escaped() {
    let qml = main_qml(r#"<p>see <a href="/q?a=1&amp;b=&quot;2&quot;">this</a></p>"#);
    assert!(
        qml.contains(r#"<a href=\"/q?a=1&amp;b=&quot;2&quot;\">this</a>"#),
        "{qml}"
    );
}
