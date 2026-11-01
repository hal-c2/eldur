//! Chromium's sizes for the generic `monospace`, recorded in
//! `monospace_size/chromium.jsonl`.

use eldur_css::{StylesheetLoader, Viewport, collect_stylesheets, compute_styles};
use eldur_dom::Document;

struct NoSheets;

impl StylesheetLoader for NoSheets {
    fn load(&mut self, _href: &str) -> Option<eldur_css::LoadedSheet> {
        None
    }
}

#[test]
fn monospace_sizes_are_chromiums() {
    let mut wrong = Vec::new();
    for line in include_str!("monospace_size/chromium.jsonl").lines() {
        let case: serde_json::Value = serde_json::from_str(line).expect("line is JSON");
        let html = format!("<!DOCTYPE html>{}", case["html"].as_str().expect("html"));
        let want: f32 = case["font-size"]
            .as_str()
            .and_then(|v| v.strip_suffix("px"))
            .and_then(|v| v.parse().ok())
            .expect("px");
        let id = case["select"]
            .as_str()
            .expect("select")
            .trim_start_matches('#');
        let doc = Document::parse(&html);
        let viewport = Viewport::default();
        let (sheets, _) = collect_stylesheets(&doc, &mut NoSheets, &viewport);
        let styles = compute_styles(&doc, &sheets, &viewport);
        let node = doc.element_by_id(id).expect("element");
        let got = styles.get(node).expect("style").style.font_size;
        // Chromium prints four decimals.
        if (got - want).abs() > 1e-3 {
            wrong.push(format!("{}: {got} want {want}", case["name"]));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
