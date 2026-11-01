//! The page's `<img>` files: copied under `assets/` so that the QML refers
//! to them relatively, with their intrinsic sizes read from the file
//! headers (the converter lays out before Qt loads anything).

use std::collections::BTreeMap;
use std::path::PathBuf;

use eldur_dom::{Document, NodeId};

use crate::diag::Diagnostics;
use crate::{OutputFile, Resources};

/// What an `<img>` shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageAsset {
    /// The `Image.source`: a copied file under `assets/`, or the URL as
    /// written when it is not a local file.
    pub source: String,
    /// Intrinsic width and height in CSS px, when the file says.
    pub size: Option<(f32, f32)>,
    /// Height over width, when the file has a ratio but no size (an SVG
    /// with only a `viewBox`): Chromium then fills the container's width.
    pub ratio: Option<f32>,
}

#[derive(Debug, Default)]
pub(crate) struct Images {
    pub by_node: BTreeMap<NodeId, ImageAsset>,
    /// A background's `url()` as written → its `Image.source`.
    pub backgrounds: BTreeMap<String, String>,
    pub files: Vec<OutputFile>,
}

pub(crate) fn collect(
    doc: &Document,
    base: &str,
    resources: &mut dyn Resources,
    diags: &mut Diagnostics,
) -> Images {
    let mut images = Images::default();
    // Resolved file → asset path, so a file used twice is copied once.
    let mut copied: BTreeMap<String, ImageAsset> = BTreeMap::new();
    for node in doc.descendants(doc.root()) {
        let Some(element) = doc.element(node) else {
            continue;
        };
        if !element.is_html() || element.local_name() != "img" {
            continue;
        }
        let Some(src) = element.attr("src").map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        if src.contains("://") || src.starts_with("data:") {
            // Qt's Image loads these itself; the layout uses the
            // element's width and height attributes.
            images.by_node.insert(
                node,
                ImageAsset {
                    source: src.to_owned(),
                    size: None,
                    ratio: None,
                },
            );
            continue;
        }
        let Some(resource) = resources.read(src, base) else {
            diags.warning(
                "html",
                crate::boxes::node_line(doc, node),
                format!("image `{src}` was not found; an empty placeholder keeps its space"),
            );
            continue;
        };
        if let Some(asset) = copied.get(&resource.resolved) {
            images.by_node.insert(node, asset.clone());
            continue;
        }
        let path = unique_path(asset_path(src), &images.files);
        let (size, ratio) = intrinsic_size(&resource.bytes);
        let asset = ImageAsset {
            source: path.to_string_lossy().replace('\\', "/"),
            size,
            ratio,
        };
        images.files.push(OutputFile {
            path,
            contents: resource.bytes,
        });
        copied.insert(resource.resolved, asset.clone());
        images.by_node.insert(node, asset);
    }
    images
}

/// The files of every `background-image` an element has or takes in a state.
/// A computed style no longer says which sheet its `url()` came from, so each
/// sheet's base is tried after the document's.
pub(crate) fn collect_backgrounds(
    images: &mut Images,
    doc: &Document,
    styles: &eldur_css::StyleMap,
    sheets: &[eldur_css::Stylesheet],
    base: &str,
    resources: &mut dyn Resources,
    diags: &mut Diagnostics,
) {
    let mut urls = std::collections::BTreeSet::new();
    let mut add = |style: &eldur_css::ComputedStyle| {
        if let Some(url) = &style.list_style_image {
            urls.insert(url.clone());
        }
        for layer in &style.background_layers {
            if let Some(eldur_css::Image::Url(url)) = &layer.image {
                urls.insert(url.clone());
            }
        }
    };
    for node in doc.descendants(doc.root()) {
        if let Some(style) = styles.style(node) {
            add(style);
        }
    }
    for variant in &styles.variants {
        for (_, style) in &variant.changed {
            add(style);
        }
    }
    add_background_files(images, urls, sheets, base, resources, diags);
}

/// Copies the files of `urls`, each once, under `assets/`.
pub(crate) fn add_background_files(
    images: &mut Images,
    urls: std::collections::BTreeSet<String>,
    sheets: &[eldur_css::Stylesheet],
    base: &str,
    resources: &mut dyn Resources,
    diags: &mut Diagnostics,
) {
    for url in urls {
        if images.backgrounds.contains_key(&url) {
            continue;
        }
        if url.contains("://") || url.starts_with("data:") {
            images.backgrounds.insert(url.clone(), url);
            continue;
        }
        let bases =
            std::iter::once(base).chain(sheets.iter().filter_map(|s| s.base_url.as_deref()));
        let found = bases.into_iter().find_map(|b| resources.read(&url, b));
        let Some(resource) = found else {
            diags.warning(
                "css",
                None,
                format!("background image `{url}` was not found; it paints nothing"),
            );
            continue;
        };
        let known = images
            .by_node
            .values()
            .map(|a| a.source.clone())
            .chain(images.backgrounds.values().cloned())
            .find(|source| {
                images.files.iter().any(|f| {
                    f.path.to_string_lossy().replace('\\', "/") == *source
                        && f.contents == resource.bytes
                })
            });
        let source = known.unwrap_or_else(|| {
            let path = unique_path(asset_path(&url), &images.files);
            let source = path.to_string_lossy().replace('\\', "/");
            images.files.push(OutputFile {
                path,
                contents: resource.bytes,
            });
            source
        });
        images.backgrounds.insert(url, source);
    }
}

/// `assets/` plus the path as written, without its query, fragment and
/// any step outside the page's directory.
fn asset_path(src: &str) -> PathBuf {
    let path = src.split(['?', '#']).next().unwrap_or(src);
    let mut out = PathBuf::from("assets");
    for part in path.split('/') {
        if !matches!(part, "" | "." | "..") {
            out.push(part);
        }
    }
    out
}

/// `path`, or `name-2.ext`, `name-3.ext`, … when another file has it.
fn unique_path(path: PathBuf, files: &[OutputFile]) -> PathBuf {
    let taken = |p: &PathBuf| files.iter().any(|f| &f.path == p);
    if !taken(&path) {
        return path;
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem}-{n}{ext}")))
        .find(|p| !taken(p))
        .expect("an unused name")
}

/// Width and height of a raster image, or of an SVG's root `width` and
/// `height` (one of them with the `viewBox` ratio); else the `viewBox`
/// ratio alone.
pub(crate) fn intrinsic_size(bytes: &[u8]) -> (Option<(f32, f32)>, Option<f32>) {
    if let Ok(size) = imagesize::blob_size(bytes) {
        #[allow(clippy::cast_precision_loss)]
        return (Some((size.width as f32, size.height as f32)), None);
    }
    std::str::from_utf8(bytes).map_or((None, None), svg_size)
}

fn svg_size(text: &str) -> (Option<(f32, f32)>, Option<f32>) {
    let Some(start) = text.find("<svg") else {
        return (None, None);
    };
    let Some(end) = text[start..].find('>') else {
        return (None, None);
    };
    let tag = &text[start..start + end];
    let attr = |name: &str| {
        let mut rest = tag;
        loop {
            let at = rest.find(name)?;
            let before = rest[..at].chars().next_back();
            rest = &rest[at + name.len()..];
            if !before.is_some_and(char::is_whitespace) {
                continue;
            }
            let value = rest.trim_start().strip_prefix('=')?.trim_start();
            let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let value = &value[1..];
            return Some(&value[..value.find(quote)?]);
        }
    };
    // Only absolute lengths size the image; `100%` defers to the viewBox.
    let length = |name: &str| {
        attr(name)
            .map(|v| v.trim().trim_end_matches("px"))
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| *v > 0.0)
    };
    let view_box = attr("viewBox").and_then(|v| {
        let n: Vec<f32> = v
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| (n[2], n[3]))
    });
    match (length("width"), length("height"), view_box) {
        (Some(w), Some(h), _) => (Some((w, h)), None),
        (Some(w), None, Some((vw, vh))) => (Some((w, w * vh / vw)), None),
        (None, Some(h), Some((vw, vh))) => (Some((h * vw / vh, h)), None),
        (None, None, Some((vw, vh))) => (None, Some(vh / vw)),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_size_from_attributes_or_view_box() {
        assert_eq!(
            svg_size(r#"<svg xmlns="x" width="320" height="220">"#),
            (Some((320.0, 220.0)), None)
        );
        assert_eq!(
            svg_size(r#"<?xml version="1.0"?><svg viewBox="0 0 64 32" width='128px'>"#),
            (Some((128.0, 64.0)), None)
        );
        // `stroke-width` is not `width`.
        assert_eq!(
            svg_size(r#"<svg stroke-width="3" viewBox="0 0 10 20" height="40">"#),
            (Some((20.0, 40.0)), None)
        );
        assert_eq!(svg_size(r#"<svg viewBox="0 0 64 32">"#), (None, Some(0.5)));
    }

    #[test]
    fn asset_paths_stay_inside_assets() {
        assert_eq!(asset_path("a.png"), PathBuf::from("assets/a.png"));
        assert_eq!(
            asset_path("../img/./b.svg?v=2"),
            PathBuf::from("assets/img/b.svg")
        );
        let files = vec![OutputFile {
            path: PathBuf::from("assets/a.png"),
            contents: Vec::new(),
        }];
        assert_eq!(
            unique_path(PathBuf::from("assets/a.png"), &files),
            PathBuf::from("assets/a-2.png")
        );
    }

    #[test]
    fn png_size_from_header() {
        // A 3×2 PNG's signature and IHDR chunk.
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend([0, 0, 0, 13]);
        png.extend(b"IHDR");
        png.extend([0, 0, 0, 3, 0, 0, 0, 2, 8, 6, 0, 0, 0]);
        png.extend([0, 0, 0, 0]);
        assert_eq!(intrinsic_size(&png), (Some((3.0, 2.0)), None));
    }
}
