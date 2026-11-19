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
    /// What the file says of its size; `None` when it was not read (a
    /// remote or `data:` URL, which Qt loads itself).
    pub natural: Option<Natural>,
}

/// A file's natural dimensions (CSS Images 3 §4.1), each of which it may
/// lack: an SVG with only a `width` has no natural height, one with only a
/// `viewBox` has just a ratio (Chromium then fills the container's width).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Natural {
    pub width: Option<f32>,
    pub height: Option<f32>,
    /// Height over width.
    pub ratio: Option<f32>,
}

impl Natural {
    pub fn size(&self) -> Option<(f32, f32)> {
        self.width.zip(self.height)
    }
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
                    natural: None,
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
        let asset = ImageAsset {
            source: path.to_string_lossy().replace('\\', "/"),
            natural: Some(intrinsic_size(&resource.bytes)),
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

/// A raster image's width and height, or what an SVG's root `width`,
/// `height` and `viewBox` give (SVG 2 §8.2), as Chromium reads them.
pub(crate) fn intrinsic_size(bytes: &[u8]) -> Natural {
    if let Ok(size) = imagesize::blob_size(bytes) {
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (size.width as f32, size.height as f32);
        return Natural {
            width: Some(w),
            height: Some(h),
            ratio: (w > 0.0 && h > 0.0).then(|| h / w),
        };
    }
    std::str::from_utf8(bytes).map_or_else(|_| Natural::default(), svg_natural)
}

/// An SVG's natural dimensions, as Chromium gives them, for a file and an
/// inline `<svg>` alike (inline, its lengths are also its CSS `width` and
/// `height`, as presentation attributes). A negative length is a zero one.
/// With no `viewBox` the lengths are the dimensions, a zero included, and
/// both positive give the ratio. With one, the `viewBox` gives the ratio and a
/// `width` decides the size: the `height` stands only beside a positive
/// width, else it is the width through the ratio (`width="80" height="0"`
/// is 80×40, `width="0" height="40"` is 0×0); a `height` alone gives the
/// width through the ratio.
pub(crate) fn svg_natural(text: &str) -> Natural {
    let Some((width, height, view_box)) = svg_root(text) else {
        return Natural::default();
    };
    // A negative length is a zero one (`-80×-40` is 0×0).
    let (width, height) = (width.map(|w| w.max(0.0)), height.map(|h| h.max(0.0)));
    let positive = |w: f32, h: f32| w > 0.0 && h > 0.0;
    let Some((vw, vh)) = view_box else {
        return Natural {
            width,
            height,
            ratio: width
                .zip(height)
                .filter(|(w, h)| positive(*w, *h))
                .map(|(w, h)| h / w),
        };
    };
    let r = vh / vw;
    let size = match (width, height) {
        (Some(w), Some(h)) if positive(w, h) => Some((w, h)),
        (Some(w), _) => Some((w, w * r)),
        (None, Some(h)) => Some((h / r, h)),
        (None, None) => None,
    };
    Natural {
        width: size.map(|s| s.0),
        height: size.map(|s| s.1),
        ratio: Some(
            size.filter(|(w, h)| positive(*w, *h))
                .map_or(r, |(w, h)| h / w),
        ),
    }
}

/// The root `<svg>`'s absolute `width` and `height` as written (zero and
/// negative included; `svg_natural` reads a negative one as zero) and its
/// `viewBox` size when both of its sides are positive; `None` when the text
/// has no `<svg` tag.
#[allow(clippy::type_complexity)]
fn svg_root(text: &str) -> Option<(Option<f32>, Option<f32>, Option<(f32, f32)>)> {
    let start = text.find("<svg")?;
    let end = text[start..].find('>')?;
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
    // A negative one is kept; `svg_natural` reads it as zero.
    let length = |name: &str| {
        attr(name)
            .map(|v| v.trim().trim_end_matches("px"))
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
    };
    let view_box = attr("viewBox").and_then(|v| {
        let n: Vec<f32> = v
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| (n[2], n[3]))
    });
    Some((length("width"), length("height"), view_box))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_natural_from_attributes_or_view_box() {
        let n = |width, height, ratio| Natural {
            width,
            height,
            ratio,
        };
        assert_eq!(
            svg_natural(r#"<svg xmlns="x" width="320" height="200">"#),
            n(Some(320.0), Some(200.0), Some(0.625))
        );
        assert_eq!(
            svg_natural(r#"<?xml version="1.0"?><svg viewBox="0 0 64 32" width='128px'>"#),
            n(Some(128.0), Some(64.0), Some(0.5))
        );
        // `stroke-width` is not `width`.
        assert_eq!(
            svg_natural(r#"<svg stroke-width="3" viewBox="0 0 10 20" height="40">"#),
            n(Some(20.0), Some(40.0), Some(2.0))
        );
        // Two positive lengths give their own ratio, a viewBox beside them
        // or not (Chromium: 80×40 at `width:200px;height:auto` is 200×100).
        assert_eq!(
            svg_natural(r#"<svg width="80" height="40" viewBox="0 0 20 20">"#),
            n(Some(80.0), Some(40.0), Some(0.5))
        );
        assert_eq!(
            svg_natural(r#"<svg viewBox="0 0 64 32">"#),
            n(None, None, Some(0.5))
        );
    }

    #[test]
    fn svg_files_keep_zero_and_single_dimensions() {
        let natural = |text: &str| intrinsic_size(text.as_bytes());
        let n = |width, height, ratio| Natural {
            width,
            height,
            ratio,
        };
        assert_eq!(
            natural(r#"<svg width="80" height="0">"#),
            n(Some(80.0), Some(0.0), None)
        );
        assert_eq!(
            natural(r#"<svg width="80" height="0" viewBox="0 0 20 10">"#),
            n(Some(80.0), Some(40.0), Some(0.5))
        );
        assert_eq!(
            natural(r#"<svg width="0" height="40" viewBox="0 0 20 10">"#),
            n(Some(0.0), Some(0.0), Some(0.5))
        );
        assert_eq!(
            natural(r#"<svg width="0" height="0" viewBox="0 0 20 10">"#),
            n(Some(0.0), Some(0.0), Some(0.5))
        );
        assert_eq!(
            natural(r#"<svg height="0" viewBox="0 0 20 10">"#),
            n(Some(0.0), Some(0.0), Some(0.5))
        );
        assert_eq!(
            natural(r#"<svg width="80" height="20" viewBox="0 0 20 10">"#),
            n(Some(80.0), Some(20.0), Some(0.25))
        );
        assert_eq!(natural(r#"<svg width="80">"#), n(Some(80.0), None, None));
        assert_eq!(
            natural(r#"<svg width="50%" height="40">"#),
            n(None, Some(40.0), None)
        );
        assert_eq!(
            natural(r#"<svg viewBox="0 0 64 32">"#),
            n(None, None, Some(0.5))
        );
        // A negative length is a zero one (Chromium).
        assert_eq!(
            natural(r#"<svg width="-80" height="40" viewBox="0 0 20 10">"#),
            n(Some(0.0), Some(0.0), Some(0.5))
        );
        assert_eq!(
            natural(r#"<svg width="80" height="-40" viewBox="0 0 20 10">"#),
            n(Some(80.0), Some(40.0), Some(0.5))
        );
        assert_eq!(
            natural(r#"<svg width="-80" height="-40">"#),
            n(Some(0.0), Some(0.0), None)
        );
        // A viewBox with a zero or negative size gives no ratio.
        for view_box in ["0 0 0 10", "0 0 20 -10", "0 0 -20 10", "0 0 20 0"] {
            assert_eq!(
                natural(&format!(r#"<svg width="80" viewBox="{view_box}">"#)),
                n(Some(80.0), None, None),
                "{view_box}"
            );
        }
        assert_eq!(natural("<svg>"), Natural::default());
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
        assert_eq!(
            intrinsic_size(&png),
            Natural {
                width: Some(3.0),
                height: Some(2.0),
                ratio: Some(2.0 / 3.0)
            }
        );
    }
}
