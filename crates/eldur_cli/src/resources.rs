use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::Context;

/// File-system [`eldur_convert::Resources`] implementation that resolves local
/// paths and `file://` URLs and records every successful or missing read for
/// [`eldur_cache`].
#[derive(Debug, Clone)]
pub struct FsResources {
    cwd: PathBuf,
    site_root: Option<PathBuf>,
    recorded: BTreeMap<PathBuf, eldur_cache::Input>,
}

impl FsResources {
    /// Creates a new [`FsResources`] anchored at the current working directory.
    pub fn new() -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("failed to determine current directory")?;
        Ok(Self::with_cwd(cwd))
    }

    /// Creates a new [`FsResources`] anchored at `cwd` for relative base paths.
    #[must_use]
    pub fn with_cwd(cwd: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            site_root: None,
            recorded: BTreeMap::new(),
        }
    }

    /// Resolves root-relative URLs (`/css/site.css`) under `root`, as a web
    /// server serving that directory would, instead of under the file
    /// system's root. A static site generator's output links its assets this
    /// way, and without a root every one of them is missing.
    #[must_use]
    pub fn with_site_root(mut self, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let root = if root.is_absolute() {
            root
        } else {
            self.cwd.join(root)
        };
        self.site_root = Some(root);
        self
    }

    /// Every file dependency read (or found missing) so far, in deterministic
    /// path order.
    #[must_use]
    pub fn inputs(&self) -> Vec<eldur_cache::Input> {
        self.recorded.values().cloned().collect()
    }

    /// Consumes `self`, returning all recorded file dependencies.
    #[must_use]
    pub fn into_inputs(self) -> Vec<eldur_cache::Input> {
        self.recorded.into_values().collect()
    }

    /// Resolves `url` against `base` to a normalized local [`PathBuf`], or
    /// `None` for remote (`http://`, `https://`), `data:`, or non-file URLs.
    #[must_use]
    pub fn resolve_path(&self, url: &str, base: &str) -> Option<PathBuf> {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return None;
        }

        if starts_with_ignore_ascii_case(trimmed, "file:") {
            let file_path = file_url_to_path(trimmed)?;
            return Some(self.normalize_path(&file_path));
        }

        if has_non_file_scheme(trimmed) {
            return None;
        }

        let without_frag = trimmed.split('#').next().unwrap_or(trimmed);
        let path_part = without_frag.split('?').next().unwrap_or(without_frag);
        // `//host/x` is protocol-relative: another host serves it, whatever
        // the site root, so no local file stands for it.
        if path_part.is_empty() || path_part.starts_with("//") {
            return None;
        }

        let decoded = percent_decode(path_part);

        if path_part.starts_with('/') {
            let rooted = |p: &str| match &self.site_root {
                Some(root) => under_root(root, p),
                None => PathBuf::from(p),
            };
            let decoded_norm = self.normalize_path(&rooted(&decoded));
            if decoded != path_part && !decoded_norm.exists() {
                let raw_norm = self.normalize_path(&rooted(path_part));
                if raw_norm.exists() {
                    return Some(raw_norm);
                }
            }
            return Some(decoded_norm);
        }

        let base_dir = self.resolve_base_dir(base);
        let decoded_norm = self.normalize_path(&base_dir.join(&decoded));
        if decoded != path_part && !decoded_norm.exists() {
            let raw_norm = self.normalize_path(&base_dir.join(path_part));
            if raw_norm.exists() {
                return Some(raw_norm);
            }
        }
        Some(decoded_norm)
    }

    fn resolve_base_dir(&self, base: &str) -> PathBuf {
        let trimmed = base.trim();
        if trimmed.is_empty() {
            return self.cwd.clone();
        }

        let without_frag = trimmed.split('#').next().unwrap_or(trimmed);
        let clean_base = without_frag.split('?').next().unwrap_or(without_frag);
        let is_dir_hint = clean_base.ends_with('/');

        let base_path = if starts_with_ignore_ascii_case(clean_base, "file:") {
            file_url_to_path(clean_base).unwrap_or_else(|| self.cwd.clone())
        } else {
            PathBuf::from(clean_base)
        };

        let abs_base = if base_path.is_absolute() {
            base_path
        } else {
            self.cwd.join(base_path)
        };

        if is_dir_hint {
            abs_base
        } else {
            abs_base
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| self.cwd.clone(), Path::to_path_buf)
        }
    }

    fn normalize_path(&self, path: &Path) -> PathBuf {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        };
        let cleaned = clean_lexical(&abs);
        if let Ok(canon) = fs::canonicalize(&cleaned) {
            return canon;
        }

        // Canonicalize the deepest existing ancestor so parent-directory
        // symlinks resolve identically before and after a missing file is
        // created.
        let mut suffix = Vec::new();
        let mut cursor = cleaned.as_path();
        while let Some(parent) = cursor.parent() {
            if let Some(name) = cursor.file_name() {
                suffix.push(name.to_os_string());
            }
            if let Ok(canon_parent) = fs::canonicalize(parent) {
                let mut result = canon_parent;
                for comp in suffix.into_iter().rev() {
                    result.push(comp);
                }
                return result;
            }
            cursor = parent;
        }

        cleaned
    }
}

impl eldur_convert::Resources for FsResources {
    fn read(&mut self, url: &str, base: &str) -> Option<eldur_convert::Resource> {
        let path = self.resolve_path(url, base)?;

        match fs::read(&path) {
            Ok(bytes) => {
                self.recorded
                    .insert(path.clone(), eldur_cache::Input::new(path.clone(), &bytes));
                Some(eldur_convert::Resource {
                    resolved: path.to_string_lossy().into_owned(),
                    bytes,
                })
            }
            Err(err) => {
                // Record missing files (and inaccessible file paths) so
                // creating or fixing them later invalidates the cache; skip
                // existing directories so `href="."` does not bust every run.
                if err.kind() != io::ErrorKind::IsADirectory && !path.is_dir() {
                    self.recorded
                        .entry(path.clone())
                        .or_insert_with(|| eldur_cache::Input::missing(path));
                }
                None
            }
        }
    }
}

fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn has_non_file_scheme(url: &str) -> bool {
    if let Some((scheme, _rest)) = url.split_once("://") {
        return is_valid_scheme(scheme);
    }
    if let Some((scheme, _rest)) = url.split_once(':')
        && scheme.len() >= 2
        && is_valid_scheme(scheme)
    {
        return true;
    }
    false
}

fn is_valid_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let without_frag = url.split('#').next().unwrap_or(url);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);

    let after_scheme = without_query.get("file:".len()..)?;
    let raw_path = if let Some(after_slashes) = after_scheme.strip_prefix("//") {
        if after_slashes.starts_with('/') {
            after_slashes
        } else if starts_with_ignore_ascii_case(after_slashes, "localhost/") {
            after_slashes.get("localhost".len()..)?
        } else {
            return None;
        }
    } else if after_scheme.starts_with('/') {
        after_scheme
    } else {
        return None;
    };

    Some(PathBuf::from(percent_decode(raw_path)))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `url_path` (root-relative) under `root`. `..` cannot climb above the root,
/// as a server's URL path cannot.
fn under_root(root: &Path, url_path: &str) -> PathBuf {
    let inside: PathBuf = clean_lexical(Path::new(url_path))
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .collect();
    root.join(inside)
}

fn clean_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    let mut has_root = false;
    let mut normal_depth = 0_usize;

    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => {
                out.push(comp.as_os_str());
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if normal_depth > 0 {
                    out.pop();
                    normal_depth -= 1;
                } else if !has_root {
                    out.push("..");
                }
            }
            Component::Normal(c) => {
                out.push(c);
                normal_depth += 1;
            }
        }
    }

    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use eldur_convert::Resources;

    use super::*;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let pid = std::process::id();
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("eldur_cli_res_{label}_{pid}_{n}"));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_relative_dot_dot_absolute_and_file_urls() {
        let td = TempDir::new("resolve");
        let root = fs::canonicalize(&td.0).expect("canon root");
        let sub = root.join("pages/sub");
        let shared = root.join("shared");
        fs::create_dir_all(&sub).expect("create sub");
        fs::create_dir_all(&shared).expect("create shared");

        let page = sub.join("index.html");
        let local_css = sub.join("local.css");
        let parent_css = shared.join("theme.css");
        let spaced_js = shared.join("my script.js");
        fs::write(&page, b"").expect("write page");
        fs::write(&local_css, b"a{}").expect("write local_css");
        fs::write(&parent_css, b"b{}").expect("write parent_css");
        fs::write(&spaced_js, b"1;").expect("write spaced_js");

        let mut res = FsResources::with_cwd(&root);
        let base = page.to_string_lossy().into_owned();

        // Plain relative and ./ relative.
        let r1 = res.read("local.css", &base).expect("read local.css");
        assert_eq!(r1.bytes, b"a{}");
        assert_eq!(Path::new(&r1.resolved), local_css);

        let r2 = res
            .read("./local.css?v=42#top", &base)
            .expect("read ./local.css with query and fragment");
        assert_eq!(r2.bytes, b"a{}");

        // ../ traversal across directories.
        let r3 = res
            .read("../../shared/theme.css", &base)
            .expect("read ../../shared/theme.css");
        assert_eq!(r3.bytes, b"b{}");
        assert_eq!(Path::new(&r3.resolved), parent_css);

        // Absolute path.
        let r4 = res
            .read(&parent_css.to_string_lossy(), &base)
            .expect("read absolute path");
        assert_eq!(r4.bytes, b"b{}");

        // file:// URL with percent-encoded space and localhost authority.
        let file_url = format!("file://{}/my%20script.js", shared.display());
        let r5 = res.read(&file_url, &base).expect("read file:// url");
        assert_eq!(r5.bytes, b"1;");

        let localhost_url = format!("file://localhost{}", local_css.display());
        let r6 = res
            .read(&localhost_url, &base)
            .expect("read file://localhost url");
        assert_eq!(r6.bytes, b"a{}");

        // Deduplicated recorded inputs: local.css, theme.css, my script.js.
        assert_eq!(res.inputs().len(), 3);
        assert!(res.inputs().iter().all(|i| !i.is_missing()));
    }

    #[test]
    fn rejects_remote_and_data_urls_without_recording_missing() {
        let td = TempDir::new("remote");
        let mut res = FsResources::with_cwd(&td.0);
        let base = td.0.join("index.html").to_string_lossy().into_owned();

        for url in [
            "http://example.com/a.css",
            "https://cdn.example.com/b.js",
            "HTTPS://EXAMPLE.COM/UPPER.CSS",
            "data:image/png;base64,iVBORw0KGgo=",
            "DATA:text/css,body{}",
            "javascript:void(0)",
            "mailto:user@example.com",
            "#section-anchor",
            "",
        ] {
            assert_eq!(res.read(url, &base), None, "expected None for {url}");
        }

        assert!(
            res.inputs().is_empty(),
            "remote/data/fragment URLs must not be recorded as file dependencies"
        );
    }

    #[test]
    fn records_missing_files_with_normalized_paths() {
        let td = TempDir::new("missing");
        let root = fs::canonicalize(&td.0).expect("canon root");
        let sub = root.join("sub");
        fs::create_dir_all(&sub).expect("create sub");
        let base = sub.join("index.html").to_string_lossy().into_owned();

        let mut res = FsResources::with_cwd(&root);
        assert_eq!(res.read("../absent.css", &base), None);

        let inputs = res.into_inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].path, root.join("absent.css"));
        assert!(inputs[0].is_missing());
    }

    #[test]
    fn root_relative_urls_resolve_under_the_site_root_and_stay_inside_it() {
        let td = TempDir::new("site_root");
        let root = fs::canonicalize(&td.0).expect("canon root");
        let site = root.join("dist");
        fs::create_dir_all(site.join("docs/page")).expect("create page dir");
        fs::create_dir_all(site.join("_assets")).expect("create assets dir");
        fs::write(site.join("_assets/site.css"), b"p{}").expect("write css");
        let base = site
            .join("docs/page/index.html")
            .to_string_lossy()
            .into_owned();

        let mut res = FsResources::with_cwd(&root).with_site_root(&site);
        let css = res
            .read("/_assets/site.css?v=1", &base)
            .expect("rooted css");
        assert_eq!(css.bytes, b"p{}");
        assert_eq!(Path::new(&css.resolved), site.join("_assets/site.css"));

        // A server never serves above its root; neither does the converter.
        fs::write(root.join("secret.css"), b"q{}").expect("write outside");
        assert_eq!(res.read("/../secret.css", &base), None);
        // `//host/x` names another host, whatever the root: a server would
        // fetch it from there, so no file in the site stands for it.
        fs::create_dir_all(site.join("fonts.example")).expect("create host-named dir");
        fs::write(site.join("fonts.example/f.css"), b"r{}").expect("write host-named css");
        assert_eq!(res.resolve_path("//fonts.example/f.css", &base), None);
        assert_eq!(res.read("//fonts.example/f.css?v=2", &base), None);
        assert_eq!(
            FsResources::with_cwd(&root).resolve_path("//fonts.example/f.css", &base),
            None
        );

        let missing = res.into_inputs();
        assert!(
            missing
                .iter()
                .any(|i| i.path == site.join("secret.css") && i.is_missing()),
            "the escaped path is looked up, and recorded missing, inside the root: {missing:?}"
        );
        assert!(
            !missing
                .iter()
                .any(|i| i.path.starts_with(site.join("fonts.example"))),
            "a URL on another host reads and records nothing in the site: {missing:?}"
        );

        // Without a root, `/x` is the file system's `/x`, as in a file:// page.
        let mut plain = FsResources::with_cwd(&root);
        assert_eq!(plain.read("/_assets/site.css", &base), None);
    }
}
