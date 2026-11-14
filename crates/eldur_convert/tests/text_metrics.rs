//! A binding that measures text with a `FontMetrics` reads that metrics'
//! `font`. `advanceWidth()` is a method, so a binding calling it alone does
//! not depend on the font: run before the font is set, it keeps the
//! default font's widths. Qt 6.12 runs it then, and a table cell came out
//! wider than its word (absolute_e2e's `table_cell`, 33.28 for Chromium's
//! 30.02).

use eldur_convert::{Options, convert_str};

/// Every line of `qml` that calls `<id>.advanceWidth(` also reads `<id>.font`.
fn measures_with_its_font(qml: &str) -> usize {
    let mut found = 0;
    for line in qml.lines() {
        let mut rest = line;
        while let Some(at) = rest.find(".advanceWidth(") {
            let id: String = rest[..at]
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            assert!(
                line.contains(&format!("{id}.font")),
                "measures with {id} and does not read {id}.font: {}",
                line.trim()
            );
            found += 1;
            rest = &rest[at + 1..];
        }
    }
    found
}

fn main_qml(html: &str) -> String {
    let out = convert_str(html, &Options::default());
    let main = out
        .files
        .iter()
        .find(|f| f.path.to_str() == Some("Main.qml"))
        .expect("Main.qml");
    String::from_utf8(main.contents.clone()).expect("utf-8")
}

#[test]
fn a_table_cell_measures_its_words_with_its_font() {
    let qml = main_qml("<table><tr><td>cell</td><td>two words</td></tr></table>");
    assert!(
        measures_with_its_font(&qml) > 0,
        "no cell is measured:\n{qml}"
    );
}

#[test]
fn a_select_measures_its_options_with_its_font() {
    let qml = main_qml("<select><option>one</option><option>a longer one</option></select>");
    assert!(
        measures_with_its_font(&qml) > 0,
        "no select is measured:\n{qml}"
    );
}

#[test]
fn a_wrapping_button_measures_its_words_with_its_font() {
    let qml = main_qml("<div style=\"width:60px\"><button>a label that has to wrap</button></div>");
    assert!(
        measures_with_its_font(&qml) > 0,
        "no button label is measured:\n{qml}"
    );
}
