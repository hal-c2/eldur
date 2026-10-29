//! Document stylesheet collection (`<style>`, `<link rel="stylesheet">`, `<base href>`,
//! and recursive `@import` resolution).

use h2q_dom::Document;

use crate::media;
use crate::parse::LayerSegment;
use crate::{Diagnostic, Origin, Severity, Stylesheet, StylesheetLoader, Viewport};

const MAX_IMPORT_DEPTH: usize = 16;

/// Collects all active author stylesheets from `doc` in document order, resolving
/// `<link rel="stylesheet">` and `@import` rules through `loader`.
pub(crate) fn collect_stylesheets(
    doc: &Document,
    loader: &mut dyn StylesheetLoader,
    viewport: &Viewport,
) -> (Vec<Stylesheet>, Vec<Diagnostic>) {
    let doc_base = find_document_base_href(doc);
    let mut sheets = Vec::new();
    let mut diagnostics = Vec::new();
    let mut style_counter = 0usize;

    for node in doc.descendants(doc.root()) {
        let Some(elem) = doc.element(node) else {
            continue;
        };
        if !elem.is_html() {
            continue;
        }

        match elem.local_name() {
            "style" => {
                style_counter += 1;
                if !is_css_mime_or_empty(elem.attr("type")) {
                    continue;
                }
                if let Some(m) = elem.attr("media")
                    && !media::matches_str(m, viewport)
                {
                    continue;
                }
                let text = doc.text_content(node);
                let source_name = format!("<style>#{style_counter}");
                let mut ancestors = Vec::new();
                collect_parsed_sheet(
                    &text,
                    &source_name,
                    doc_base.as_deref(),
                    0,
                    &mut ancestors,
                    loader,
                    viewport,
                    &mut sheets,
                    &mut diagnostics,
                );
            }
            "link" => {
                if elem.has_attr("disabled") {
                    continue;
                }
                let Some(rel) = elem.attr("rel") else {
                    continue;
                };
                if !is_active_stylesheet_rel(rel) {
                    continue;
                }
                if !is_css_mime_or_empty(elem.attr("type")) {
                    continue;
                }
                let Some(href) = elem.attr("href").map(str::trim).filter(|h| !h.is_empty()) else {
                    continue;
                };
                if let Some(m) = elem.attr("media")
                    && !media::matches_str(m, viewport)
                {
                    continue;
                }

                let resolved = resolve_href(doc_base.as_deref(), href);
                let Some(loaded) = loader.load(&resolved) else {
                    diagnostics.push(Diagnostic {
                        severity: Severity::Invalid,
                        source: resolved.clone(),
                        line: Some(elem.line as u32),
                        message: format!("failed to load stylesheet `{resolved}`"),
                    });
                    continue;
                };

                let mut ancestors = vec![resolved];
                if !ancestors.contains(&loaded.url) {
                    ancestors.push(loaded.url.clone());
                }
                collect_parsed_sheet(
                    &loaded.text,
                    &loaded.url,
                    Some(&loaded.url),
                    0,
                    &mut ancestors,
                    loader,
                    viewport,
                    &mut sheets,
                    &mut diagnostics,
                );
            }
            _ => {}
        }
    }

    (sheets, diagnostics)
}

#[allow(clippy::too_many_arguments)]
fn collect_parsed_sheet(
    text: &str,
    source_name: &str,
    base_url: Option<&str>,
    depth: usize,
    ancestors: &mut Vec<String>,
    loader: &mut dyn StylesheetLoader,
    viewport: &Viewport,
    out_sheets: &mut Vec<Stylesheet>,
    out_diagnostics: &mut Vec<Diagnostic>,
) {
    let (mut sheet, sheet_diags) = Stylesheet::parse(text, Origin::Author, source_name, base_url);
    out_diagnostics.extend(sheet_diags);

    let imports = std::mem::take(&mut sheet.inner.imports);
    let mut sheet_layer_regs = std::mem::take(&mut sheet.inner.layer_registrations).into_iter();
    let mut consumed_regs = 0usize;
    let mut pending_regs = Vec::new();

    for imp in imports {
        while consumed_regs < imp.preceding_layer_registrations {
            if let Some(reg) = sheet_layer_regs.next() {
                pending_regs.push(reg);
            }
            consumed_regs += 1;
        }
        if !imp.supports_matches {
            continue;
        }
        if let Some(mql) = &imp.media
            && !mql.matches(viewport)
        {
            continue;
        }
        if depth >= MAX_IMPORT_DEPTH {
            out_diagnostics.push(Diagnostic {
                severity: Severity::Invalid,
                source: source_name.to_owned(),
                line: Some(imp.line),
                message: format!(
                    "`@import` of `{}` exceeds maximum depth of {MAX_IMPORT_DEPTH}",
                    imp.href
                ),
            });
            continue;
        }

        let resolved = resolve_href(base_url, &imp.href);
        if ancestors.contains(&resolved) {
            out_diagnostics.push(Diagnostic {
                severity: Severity::Invalid,
                source: source_name.to_owned(),
                line: Some(imp.line),
                message: format!("`@import` cycle detected for `{resolved}`"),
            });
            continue;
        }

        let Some(loaded) = loader.load(&resolved) else {
            out_diagnostics.push(Diagnostic {
                severity: Severity::Invalid,
                source: source_name.to_owned(),
                line: Some(imp.line),
                message: format!("failed to load imported stylesheet `{resolved}`"),
            });
            continue;
        };

        if ancestors.contains(&loaded.url) {
            out_diagnostics.push(Diagnostic {
                severity: Severity::Invalid,
                source: source_name.to_owned(),
                line: Some(imp.line),
                message: format!("`@import` cycle detected for `{}`", loaded.url),
            });
            continue;
        }

        let prev_len = ancestors.len();
        ancestors.push(resolved);
        if !ancestors.contains(&loaded.url) {
            ancestors.push(loaded.url.clone());
        }

        let mut imported_sheets = Vec::new();
        collect_parsed_sheet(
            &loaded.text,
            &loaded.url,
            Some(&loaded.url),
            depth + 1,
            ancestors,
            loader,
            viewport,
            &mut imported_sheets,
            out_diagnostics,
        );
        ancestors.truncate(prev_len);

        let import_layer_segs: Option<Vec<LayerSegment>> = imp.layer.map(|l| l.into_segments());
        for s in &mut imported_sheets {
            s.inner
                .wrap_imported(imp.media.as_ref(), import_layer_segs.as_deref());
        }
        if !pending_regs.is_empty()
            && let Some(first_imported) = imported_sheets.first_mut()
        {
            let mut combined = std::mem::take(&mut pending_regs);
            combined.append(&mut first_imported.inner.layer_registrations);
            first_imported.inner.layer_registrations = combined;
        }
        out_sheets.extend(imported_sheets);
    }

    pending_regs.extend(sheet_layer_regs);
    sheet.inner.layer_registrations = pending_regs;
    out_sheets.push(sheet);
}

fn find_document_base_href(doc: &Document) -> Option<String> {
    for node in doc.descendants(doc.root()) {
        let Some(elem) = doc.element(node) else {
            continue;
        };
        if elem.is_html()
            && elem.local_name() == "base"
            && let Some(href) = elem.attr("href").map(str::trim).filter(|h| !h.is_empty())
        {
            return Some(href.to_owned());
        }
    }
    None
}

fn is_css_mime_or_empty(attr: Option<&str>) -> bool {
    match attr {
        None => true,
        Some(raw) => {
            let trimmed = raw.trim();
            trimmed.is_empty() || trimmed.eq_ignore_ascii_case("text/css")
        }
    }
}

fn is_active_stylesheet_rel(rel: &str) -> bool {
    let mut has_stylesheet = false;
    for tok in rel.split_ascii_whitespace() {
        if tok.eq_ignore_ascii_case("alternate") {
            return false;
        }
        if tok.eq_ignore_ascii_case("stylesheet") {
            has_stylesheet = true;
        }
    }
    has_stylesheet
}

/// Resolves a relative CSS/HTML URL or filesystem path against an optional `base`.
pub(crate) fn resolve_href(base: Option<&str>, href: &str) -> String {
    let href = href.trim();
    if href.is_empty() || has_uri_scheme(href) {
        return href.to_owned();
    }

    let Some(base_raw) = base.map(str::trim).filter(|b| !b.is_empty()) else {
        return href.to_owned();
    };

    let (href_path, href_suffix) = split_query_or_fragment(href);

    if let Some((origin_prefix, base_path)) = split_url_origin_and_path(base_raw) {
        if href.starts_with("//") {
            if let Some(colon) = origin_prefix.find(':') {
                return format!("{}:{href}", &origin_prefix[..colon]);
            }
            return href.to_owned();
        }
        if href_path.starts_with('/') {
            let norm = normalize_segments(href_path, true);
            return format!("{origin_prefix}{norm}{href_suffix}");
        }
        let dir = parent_dir_with_trailing_slash(base_path, true);
        let joined = format!("{dir}{href_path}");
        let norm = normalize_segments(&joined, true);
        return format!("{origin_prefix}{norm}{href_suffix}");
    }

    // Filesystem or relative path base without `scheme://`.
    let (base_path, _) = split_query_or_fragment(base_raw);
    if href_path.starts_with('/') {
        let norm = normalize_segments(href_path, true);
        return format!("{norm}{href_suffix}");
    }

    let rooted = base_path.starts_with('/');
    let dir = parent_dir_with_trailing_slash(base_path, rooted);
    let joined = format!("{dir}{href_path}");
    let norm = normalize_segments(&joined, rooted);
    format!("{norm}{href_suffix}")
}

fn has_uri_scheme(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let scheme = s.as_bytes();
    scheme[0].is_ascii_alphabetic()
        && scheme[1..colon]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

fn split_query_or_fragment(s: &str) -> (&str, &str) {
    match s.find(['?', '#']) {
        Some(idx) => (&s[..idx], &s[idx..]),
        None => (s, ""),
    }
}

fn split_url_origin_and_path(url: &str) -> Option<(&str, &str)> {
    let scheme_end = url.find("://")?;
    if !has_uri_scheme(&url[..=scheme_end]) {
        return None;
    }
    let after_slashes = scheme_end + 3;
    let scheme = &url[..scheme_end];
    if scheme.eq_ignore_ascii_case("file") && url[after_slashes..].starts_with('/') {
        let (path, _) = split_query_or_fragment(&url[after_slashes..]);
        return Some((&url[..after_slashes], path));
    }
    match url[after_slashes..].find('/') {
        Some(rel_slash) => {
            let split_at = after_slashes + rel_slash;
            let (path, _) = split_query_or_fragment(&url[split_at..]);
            Some((&url[..split_at], path))
        }
        None => {
            let (origin, _) = split_query_or_fragment(url);
            Some((origin, "/"))
        }
    }
}

fn parent_dir_with_trailing_slash(path: &str, rooted: bool) -> &str {
    if path.ends_with('/') {
        return path;
    }
    match path.rfind('/') {
        Some(idx) => &path[..=idx],
        None => {
            if rooted {
                "/"
            } else {
                ""
            }
        }
    }
}

fn normalize_segments(path: &str, rooted: bool) -> String {
    let trailing_slash = path.ends_with('/') && path.len() > 1;
    let mut stack: Vec<&str> = Vec::new();

    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if let Some(&last) = stack.last() {
                    if last != ".." {
                        stack.pop();
                    } else if !rooted {
                        stack.push("..");
                    }
                } else if !rooted {
                    stack.push("..");
                }
            }
            normal => stack.push(normal),
        }
    }

    let mut out = String::new();
    if rooted {
        out.push('/');
    }
    out.push_str(&stack.join("/"));
    if trailing_slash && !out.ends_with('/') {
        out.push('/');
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::LoadedSheet;

    struct MapLoader {
        files: HashMap<String, String>,
    }

    impl StylesheetLoader for MapLoader {
        fn load(&mut self, href: &str) -> Option<LoadedSheet> {
            self.files.get(href).map(|text| LoadedSheet {
                text: text.clone(),
                url: href.to_owned(),
            })
        }
    }

    #[test]
    fn url_resolution_handles_urls_and_relative_paths() {
        assert_eq!(resolve_href(None, "main.css"), "main.css");
        assert_eq!(resolve_href(Some("main.css"), "sub.css"), "sub.css");
        assert_eq!(
            resolve_href(Some("css/main.css"), "reset/norm.css"),
            "css/reset/norm.css"
        );
        assert_eq!(
            resolve_href(Some("css/sub/main.css"), "../tokens.css"),
            "css/tokens.css"
        );
        assert_eq!(
            resolve_href(Some("https://example.com/app/page.html"), "styles/site.css"),
            "https://example.com/app/styles/site.css"
        );
        assert_eq!(
            resolve_href(
                Some("https://example.com/app/styles/site.css"),
                "../base.css"
            ),
            "https://example.com/app/base.css"
        );
        assert_eq!(
            resolve_href(Some("https://example.com/app/styles/site.css"), "/root.css"),
            "https://example.com/root.css"
        );
    }

    #[test]
    fn collects_in_order_respects_base_media_and_breaks_import_cycles() {
        let doc = Document::parse(
            r#"<!doctype html>
            <html>
              <head>
                <base href="assets/page.html">
                <link rel="stylesheet" href="css/a.css">
                <link rel="alternate stylesheet" href="css/alt.css">
                <link rel="stylesheet" href="css/disabled.css" disabled>
                <link rel="stylesheet" href="css/print.css" media="print">
                <link rel="stylesheet" href="css/missing.css">
                <style type="text/plain">body { color: yellow; }</style>
                <style media="(min-width: 800px)">@import "css/from-style.css"; p { color: green; }</style>
              </head>
              <body><p>Hello</p></body>
            </html>"#,
        );

        let mut files = HashMap::new();
        files.insert(
            "assets/css/a.css".to_owned(),
            r#"@import "b.css"; .a { display: block; }"#.to_owned(),
        );
        files.insert(
            "assets/css/b.css".to_owned(),
            r#"@import "a.css"; .b { display: flex; }"#.to_owned(),
        );
        files.insert(
            "assets/css/from-style.css".to_owned(),
            r#".imported-from-style { color: blue; }"#.to_owned(),
        );

        let mut loader = MapLoader { files };
        let vp = Viewport::default();
        let (sheets, diags) = collect_stylesheets(&doc, &mut loader, &vp);

        // Expected order: b.css (imported by a.css) -> a.css -> from-style.css -> <style>#2
        let sources: Vec<&str> = sheets.iter().map(|s| s.source_name.as_str()).collect();
        assert_eq!(
            sources,
            vec![
                "assets/css/b.css",
                "assets/css/a.css",
                "assets/css/from-style.css",
                "<style>#2",
            ]
        );

        // 1 diagnostic for `b.css -> a.css` cycle + 1 diagnostic for `missing.css`.
        assert_eq!(diags.len(), 2, "unexpected diagnostics: {diags:?}");
    }
}
