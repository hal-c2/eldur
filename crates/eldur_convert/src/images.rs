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
    /// The `Image.source`: a copied file under `assets/`, or the URL when
    /// it is a `data:` URL or a remote one the resolver does not serve.
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
    // A `src` resolves against `<base href>` as a style sheet's does.
    let base_href = eldur_css::document_base_href(doc);
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
        let resolved = eldur_css::resolve_url(base_href.as_deref(), src);
        // Qt's Image loads these itself; the layout uses the element's
        // width and height attributes. A remote one is copied when the
        // resolver serves it (a fetched page's does).
        let pass_through = |images: &mut Images| {
            images.by_node.insert(
                node,
                ImageAsset {
                    source: resolved.clone(),
                    natural: None,
                },
            );
        };
        if resolved.starts_with("data:") {
            pass_through(&mut images);
            continue;
        }
        let remote = resolved.contains("://");
        let Some(resource) = resources.read(&resolved, base) else {
            if remote {
                pass_through(&mut images);
                continue;
            }
            diags.warning(
                "html",
                crate::boxes::node_line(doc, node),
                format!(
                    "image `{}` was not found; an empty placeholder keeps its space",
                    eldur_css::quoted_reference(src)
                ),
            );
            continue;
        };
        if let Some(asset) = copied.get(&resource.resolved) {
            images.by_node.insert(node, asset.clone());
            continue;
        }
        let path = unique_path(asset_path(&asset_name(src, &resolved)), &images.files);
        // The size is read from the file as written, before the copy drops
        // what Chromium ignores: how Chromium sizes it depends on those.
        let asset = ImageAsset {
            source: path.to_string_lossy().replace('\\', "/"),
            natural: Some(intrinsic_size(&resource.bytes)),
        };
        images.files.push(OutputFile {
            path,
            contents: drawable(resource.bytes),
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
        if url.starts_with("data:") {
            images.backgrounds.insert(url.clone(), url);
            continue;
        }
        // An absolute URL needs no base; a remote one Qt loads itself when
        // the resolver does not serve it.
        let remote = url.contains("://");
        let bases: Vec<&str> = if remote {
            vec![base]
        } else {
            std::iter::once(base)
                .chain(sheets.iter().filter_map(|s| s.base_url.as_deref()))
                .collect()
        };
        let Some(mut resource) = resources.read_any(&url, &bases) else {
            if remote {
                images.backgrounds.insert(url.clone(), url);
                continue;
            }
            diags.warning(
                "css",
                None,
                format!(
                    "background image `{}` was not found; it paints nothing",
                    eldur_css::quoted_reference(&url)
                ),
            );
            continue;
        };
        resource.bytes = drawable(resource.bytes);
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
            let path = unique_path(asset_path(&asset_name(&url, &url)), &images.files);
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

/// What names a copied file: the path as written, or a remote URL's path
/// (its scheme and host are no file name); percent-decoded, relative or
/// not, since Qt reads a relative `source` as a URL and would look for
/// `my logo.gif` where `my%20logo.gif` was written.
/// What decoding lets through that no file name, or no `source` Qt reads
/// back, can hold is `_`: control bytes, `\` and what Windows refuses in a
/// name, and `%`, `?` and `#`, which Qt would read as URL syntax.
fn asset_name(written: &str, resolved: &str) -> String {
    let path = match resolved.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("", |slash| &rest[slash..]),
        None => written,
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    // Each segment decoded alone: an encoded `/` stays in its name, so no
    // name becomes a folder another file's name already is.
    path.split('/')
        .map(|segment| {
            percent_decoded(segment)
                .chars()
                .map(|c| {
                    let bad = c.is_control()
                        || matches!(
                            c,
                            '/' | '\\' | '%' | '?' | '#' | ':' | '*' | '<' | '>' | '|' | '"'
                        );
                    if bad { '_' } else { c }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `text` with each `%XX` read as the byte it stands for; what is not
/// UTF-8 afterwards is replaced, so a name always comes out.
#[must_use]
pub fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .filter(|_| bytes[i] == b'%')
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let Some(byte) = hex {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
    // A URL with no path (`https://example.com`) still names a file.
    if out.as_os_str() == "assets" {
        out.push("image");
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

/// `bytes` as Qt can draw them: an SVG whose root has an attribute Chromium
/// ignores as invalid, without it. Qt's SVG reader refuses the whole file
/// over a negative `width` or `height` or a broken `viewBox`, where Chromium
/// draws as if the attribute were not there. Anything else is unchanged.
pub(crate) fn drawable(bytes: Vec<u8>) -> Vec<u8> {
    if imagesize::blob_size(&bytes).is_ok() {
        return bytes;
    }
    match std::str::from_utf8(&bytes)
        .ok()
        .and_then(without_invalid_root)
    {
        Some(text) => text.into_bytes(),
        None => bytes,
    }
}

/// `text` without the root `<svg>` attributes Chromium ignores: a negative
/// `width` or `height` (SVG 2 §8.3, a negative length is an error) and a
/// `viewBox` that is not four numbers or has a negative side (§8.6). A
/// `viewBox` with a zero side stays: it disables rendering, in Chromium as
/// in Qt. `None` when there is nothing to drop.
pub(crate) fn without_invalid_root(text: &str) -> Option<String> {
    let start = root_start(text)?;
    let negative = |value: &str| {
        // The number is the longest start that reads as one: `-8em` is -8,
        // `1e-3px` is 0.001.
        let v = value.trim();
        (1..=v.len())
            .rev()
            .filter(|&end| v.is_char_boundary(end))
            .find_map(|end| v[..end].parse::<f64>().ok())
            .is_some_and(|n| n < 0.0)
    };
    let broken_view_box = |value: &str| {
        let n: Vec<Option<f64>> = value
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().ok().filter(|n: &f64| n.is_finite()))
            .collect();
        !matches!(n.as_slice(), [Some(_), Some(_), Some(w), Some(h)] if *w >= 0.0 && *h >= 0.0)
    };
    let drop: Vec<_> = root_attributes(text, start)?
        .into_iter()
        .filter(|a| match a.name {
            "width" | "height" => negative(&unescape(a.value)),
            "viewBox" => broken_view_box(&unescape(a.value)),
            _ => false,
        })
        .map(|a| a.span)
        .collect();
    if drop.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for range in drop {
        out.push_str(&text[from..range.start]);
        from = range.end;
    }
    out.push_str(&text[from..]);
    Some(out)
}

/// An attribute of the root start tag: its name, its value as written
/// (references not yet decoded) and its span, the space before it included.
struct RootAttribute<'a> {
    name: &'a str,
    value: &'a str,
    span: std::ops::Range<usize>,
}

/// The attributes of the `<svg` start tag at `start`, read as XML reads
/// them: a `>` inside a quoted value is the value's, not the tag's end.
/// `None` when a quoted value is never closed.
fn root_attributes(text: &str, start: usize) -> Option<Vec<RootAttribute<'_>>> {
    let mut attributes = Vec::new();
    let mut at = start + 4;
    loop {
        let rest = &text[at..];
        let ws = rest.len() - rest.trim_start().len();
        let attr_start = at;
        at += ws;
        let rest = &text[at..];
        if rest.is_empty() || rest.starts_with('>') || rest.starts_with("/>") {
            return Some(attributes);
        }
        let name_len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(rest.len());
        if name_len == 0 {
            // A stray `/` or `=` inside the tag; step over it.
            at += 1;
            continue;
        }
        let name = &rest[..name_len];
        at += name_len;
        let after = &text[at..];
        let eq = after.len() - after.trim_start().len();
        let mut value = "";
        if after.trim_start().starts_with('=') {
            at += eq + 1;
            let after = &text[at..];
            at += after.len() - after.trim_start().len();
            let after = &text[at..];
            match after.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let end = after[1..].find(q)?;
                    value = &after[1..=end];
                    at += end + 2;
                }
                _ => {
                    let end = after
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(after.len());
                    value = &after[..end];
                    at += end;
                }
            }
        }
        attributes.push(RootAttribute {
            name,
            value,
            span: attr_start..at,
        });
    }
}

/// Where the root `<svg` start tag begins: the first `<svg` element tag
/// outside a comment, a CDATA section or a `<!DOCTYPE …>` (whose internal
/// subset may name one too).
fn root_start(text: &str) -> Option<usize> {
    let mut at = 0;
    while let Some(lt) = text[at..].find('<') {
        let tag = at + lt;
        let rest = &text[tag..];
        let skip = |end: &str| rest.find(end).map(|e| e + end.len());
        at = tag
            + if rest.starts_with("<!--") {
                skip("-->")?
            } else if rest.starts_with("<![CDATA[") {
                skip("]]>")?
            } else if rest.starts_with("<!") {
                match (rest.find('['), rest.find('>')) {
                    (Some(open), Some(close)) if open < close => skip("]>")?,
                    (_, close) => close? + 1,
                }
            } else if rest.starts_with("<svg")
                && rest[4..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/')
            {
                return Some(tag);
            } else {
                1
            };
    }
    None
}

/// An attribute value with its character and predefined entity references
/// replaced, as an XML reader gives it (XML 1.0 §4.6, §4.1). An unknown or
/// malformed reference stays as written.
fn unescape(value: &str) -> std::borrow::Cow<'_, str> {
    if !value.contains('&') {
        return value.into();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').and_then(|semi| {
            let name = &rest[1..semi];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => name
                    .strip_prefix("#x")
                    .map(|h| u32::from_str_radix(h, 16))
                    .or_else(|| name.strip_prefix('#').map(str::parse))
                    .and_then(Result::ok)
                    .and_then(char::from_u32),
            }?;
            Some((c, semi + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.into()
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
    let attributes = root_attributes(text, root_start(text)?)?;
    let attr = |name: &str| {
        attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| unescape(a.value))
    };
    // Only absolute lengths size the image; `100%` defers to the viewBox.
    // A negative one is kept; `svg_natural` reads it as zero.
    let length = |name: &str| {
        attr(name)
            .as_deref()
            .map(|v| v.trim().trim_end_matches("px"))
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
    };
    let view_box = attr("viewBox").as_deref().and_then(|v| {
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
    fn invalid_root_attributes_are_dropped() {
        let drop = |t: &str| without_invalid_root(t);
        assert_eq!(
            drop(r#"<svg xmlns="x" width="-80" height="40"><rect/></svg>"#).as_deref(),
            Some(r#"<svg xmlns="x" height="40"><rect/></svg>"#)
        );
        assert_eq!(
            drop(r#"<?xml version="1.0"?><svg width='-8em' height = "-4px" viewBox="0 0 20 10">"#)
                .as_deref(),
            Some(r#"<?xml version="1.0"?><svg viewBox="0 0 20 10">"#)
        );
        for broken in ["0 0 -20 10", "0,0,20,-10", "0 0 20", "a b c d", ""] {
            assert_eq!(
                drop(&format!(r#"<svg width="80" viewBox="{broken}">"#)).as_deref(),
                Some(r#"<svg width="80">"#),
                "{broken}"
            );
        }
        // What Chromium keeps stays as written: a zero length or viewBox
        // side, `-0`, a percentage, and `stroke-width` or a child's width.
        for kept in [
            r#"<svg width="0" height="-0" viewBox="0 0 0 10">"#,
            r#"<svg width="100%" stroke-width="-1"><rect width="-5"/></svg>"#,
            r#"<svgx width="-1"><svg/>"#,
            "no svg here",
        ] {
            assert_eq!(drop(kept), None, "{kept}");
        }
        // The root is the first `<svg` tag outside a comment, a CDATA section
        // or a doctype, and a reference in a value counts as what it reads.
        assert_eq!(
            drop(r#"<!-- an <svg> --><!DOCTYPE svg [<!ENTITY e "<svg x">]><svg width="&#45;8">"#)
                .as_deref(),
            Some(r#"<!-- an <svg> --><!DOCTYPE svg [<!ENTITY e "<svg x">]><svg>"#)
        );
        for kept in [
            r#"<svg viewBox="0 0 &#48; 10">"#,
            r#"<svg viewBox="0&#x20;0&#32;20 10" width="&amp;-1">"#,
        ] {
            assert_eq!(drop(kept), None, "{kept}");
        }
        assert_eq!(
            svg_natural(r#"<!-- <svg width="9"> --><svg width="&#56;0" height="40">"#),
            Natural {
                width: Some(80.0),
                height: Some(40.0),
                ratio: Some(0.5)
            }
        );
        // A `>` or an attribute's name inside a quoted value is the value's.
        assert_eq!(
            svg_natural(r#"<svg data-note="a > b" viewBox="0 0 20 10" height="40">"#),
            Natural {
                width: Some(80.0),
                height: Some(40.0),
                ratio: Some(0.5)
            }
        );
        assert_eq!(
            intrinsic_size(
                br#"<svg aria-label='w > 0 width="9"' height="40" viewBox="0 0 20 10">"#
            ),
            Natural {
                width: Some(80.0),
                height: Some(40.0),
                ratio: Some(0.5)
            }
        );
        assert_eq!(
            drop(r#"<svg data-note="a > b" width="-8">"#).as_deref(),
            Some(r#"<svg data-note="a > b">"#)
        );
        // A raster file is never touched.
        let png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        assert_eq!(drawable(png.clone()), png);
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
