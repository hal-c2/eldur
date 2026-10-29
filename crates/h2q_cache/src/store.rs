use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{CacheError, CacheKey, Cached, Input, OutputFile};

/// On-disk cache format version. Bump whenever the manifest schema or object
/// layout changes so `open` wipes incompatible caches cleanly.
const CACHE_FORMAT_VERSION: &str = "1";

/// Stray temporary files older than one hour are removed during `gc`; newer
/// ones are left alone because a concurrent writer may still be flushing them.
const STRAY_TEMP_MAX_AGE: Duration = Duration::from_secs(3600);

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    inputs: Vec<ManifestInput>,
    outputs: Vec<ManifestOutput>,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestInput {
    path: PathBuf,
    #[serde(default)]
    missing: bool,
    #[serde(default)]
    hash: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    mtime_ns: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestOutput {
    path: PathBuf,
    object_hash: String,
}

/// Domain-separated BLAKE3 value marking a missing-file dependency in
/// [`Input`]. Because BLAKE3 uses a distinct compression flag for key
/// derivation, this value cannot collide with `blake3::hash(bytes)`.
pub(crate) fn missing_sentinel_hash() -> blake3::Hash {
    blake3::Hasher::new_derive_key("h2q_cache 2026-10 missing input sentinel").finalize()
}

pub(crate) fn key_digest(key: &CacheKey) -> String {
    let mut hasher = blake3::Hasher::new();
    update_len_prefixed(&mut hasher, key.entry.as_os_str().as_encoded_bytes());
    update_len_prefixed(&mut hasher, key.converter_version.as_bytes());
    update_len_prefixed(&mut hasher, key.options.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn update_len_prefixed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    let len = bytes.len() as u64;
    hasher.update(&len.to_le_bytes());
    hasher.update(bytes);
}

pub(crate) fn open(dir: &Path) -> Result<PathBuf, CacheError> {
    create_dir_all_io(dir)?;

    let version_path = dir.join("VERSION");
    let entries_dir = dir.join("entries");
    let objects_dir = dir.join("objects");

    let version_matches = fs::read_to_string(&version_path)
        .map(|s| s.trim() == CACHE_FORMAT_VERSION)
        .unwrap_or(false);

    if !version_matches {
        remove_tree_if_exists(&entries_dir)?;
        remove_tree_if_exists(&objects_dir)?;
        create_dir_all_io(&entries_dir)?;
        create_dir_all_io(&objects_dir)?;
        write_atomic(
            &version_path,
            format!("{CACHE_FORMAT_VERSION}\n").as_bytes(),
        )?;
    } else {
        create_dir_all_io(&entries_dir)?;
        create_dir_all_io(&objects_dir)?;
    }

    Ok(dir.to_path_buf())
}

pub(crate) fn lookup(dir: &Path, key: &CacheKey) -> Option<Cached> {
    lookup_with_inputs(dir, key).map(|(cached, _inputs)| cached)
}

pub(crate) fn lookup_with_inputs(dir: &Path, key: &CacheKey) -> Option<(Cached, Vec<Input>)> {
    let manifest_path = manifest_path(dir, key);
    let raw = fs::read(&manifest_path).ok()?;

    let Ok(mut manifest) = serde_json::from_slice::<Manifest>(&raw) else {
        let _ = fs::remove_file(&manifest_path);
        return None;
    };

    if !validate_manifest_hashes(&manifest) {
        let _ = fs::remove_file(&manifest_path);
        return None;
    }

    let mut refresh_manifest = false;
    let mut resolved_inputs = Vec::with_capacity(manifest.inputs.len());

    for inp in &mut manifest.inputs {
        // Remote or URL inputs cannot be stat'd or trusted locally; they are
        // never fresh.
        if is_url(&inp.path) {
            return None;
        }

        if inp.missing {
            match fs::metadata(&inp.path) {
                Err(e)
                    if e.kind() == io::ErrorKind::NotFound
                        || e.kind() == io::ErrorKind::NotADirectory =>
                {
                    resolved_inputs.push(Input::missing(inp.path.clone()));
                    continue;
                }
                _ => return None,
            }
        }

        let Ok(expected_hash) = blake3::Hash::from_hex(&inp.hash) else {
            let _ = fs::remove_file(&manifest_path);
            return None;
        };

        let meta = fs::metadata(&inp.path).ok()?;
        if !meta.is_file() {
            return None;
        }

        let actual_size = meta.len();
        let actual_mtime = mtime_nanos(&meta);

        let fast_match =
            inp.size == Some(actual_size) && inp.mtime_ns.is_some() && inp.mtime_ns == actual_mtime;

        if !fast_match {
            let bytes = fs::read(&inp.path).ok()?;
            if blake3::hash(&bytes) != expected_hash {
                return None;
            }
            // Content is unchanged after a touch; refresh cached metadata so
            // subsequent lookups return to the fast path.
            let refreshed_meta = fs::metadata(&inp.path).ok();
            inp.size = Some(bytes.len() as u64);
            inp.mtime_ns = refreshed_meta.as_ref().and_then(mtime_nanos);
            refresh_manifest = true;
        }

        resolved_inputs.push(Input {
            path: inp.path.clone(),
            hash: expected_hash,
        });
    }

    let mut outputs = Vec::with_capacity(manifest.outputs.len());
    for out in &manifest.outputs {
        let Ok(obj_hash) = blake3::Hash::from_hex(&out.object_hash) else {
            let _ = fs::remove_file(&manifest_path);
            return None;
        };
        let obj_path = object_path(dir, &obj_hash);
        let contents = fs::read(&obj_path).ok()?;
        if blake3::hash(&contents) != obj_hash {
            let _ = fs::remove_file(&obj_path);
            return None;
        }
        outputs.push(OutputFile {
            path: out.path.clone(),
            contents,
        });
    }

    if refresh_manifest && let Ok(bytes) = serde_json::to_vec_pretty(&manifest) {
        let _ = write_atomic(&manifest_path, &bytes);
    }

    Some((
        Cached {
            outputs,
            diagnostics: manifest.diagnostics,
        },
        resolved_inputs,
    ))
}

pub(crate) fn store(
    dir: &Path,
    key: &CacheKey,
    inputs: &[Input],
    cached: &Cached,
) -> Result<(), CacheError> {
    create_dir_all_io(&dir.join("entries"))?;
    create_dir_all_io(&dir.join("objects"))?;

    // Write content-addressed objects before publishing the manifest so any
    // concurrent reader observing the renamed manifest finds all its objects.
    let mut manifest_outputs = Vec::with_capacity(cached.outputs.len());
    for out in &cached.outputs {
        let obj_hash = blake3::hash(&out.contents);
        let obj_path = object_path(dir, &obj_hash);
        write_atomic(&obj_path, &out.contents)?;
        manifest_outputs.push(ManifestOutput {
            path: out.path.clone(),
            object_hash: obj_hash.to_hex().to_string(),
        });
    }

    let mut manifest_inputs = Vec::with_capacity(inputs.len());
    for inp in inputs {
        if inp.is_missing() {
            manifest_inputs.push(ManifestInput {
                path: inp.path.clone(),
                missing: true,
                hash: String::new(),
                size: None,
                mtime_ns: None,
            });
        } else if is_url(&inp.path) {
            manifest_inputs.push(ManifestInput {
                path: inp.path.clone(),
                missing: false,
                hash: inp.hash.to_hex().to_string(),
                size: None,
                mtime_ns: None,
            });
        } else {
            let meta = fs::metadata(&inp.path).ok().filter(fs::Metadata::is_file);
            let size = meta.as_ref().map(fs::Metadata::len);
            let mtime_ns = meta.as_ref().and_then(mtime_nanos);
            manifest_inputs.push(ManifestInput {
                path: inp.path.clone(),
                missing: false,
                hash: inp.hash.to_hex().to_string(),
                size,
                mtime_ns,
            });
        }
    }

    let manifest = Manifest {
        inputs: manifest_inputs,
        outputs: manifest_outputs,
        diagnostics: cached.diagnostics.clone(),
    };

    let target = manifest_path(dir, key);
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| CacheError::Corrupt {
        path: target.clone(),
        message: e.to_string(),
    })?;
    write_atomic(&target, &json)
}

pub(crate) fn gc(dir: &Path) -> Result<u64, CacheError> {
    let now = SystemTime::now();
    let mut freed = 0_u64;

    freed += sweep_stray_temps_in_dir(dir, now)?;

    let entries_dir = dir.join("entries");
    let mut referenced = HashSet::new();

    if let Some(read_dir) = read_dir_optional(&entries_dir)? {
        for entry_res in read_dir {
            let entry = entry_res.map_err(|source| CacheError::Io {
                path: entries_dir.clone(),
                source,
            })?;
            let path = entry.path();
            let file_name = entry.file_name();

            if is_temp_file_name(&file_name) {
                freed += remove_if_stale_temp(&path, now)?;
                continue;
            }

            if path.extension() != Some(OsStr::new("json")) {
                continue;
            }

            let raw = match fs::read(&path) {
                Ok(b) => b,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(source) => return Err(CacheError::Io { path, source }),
            };

            if let Ok(manifest) = serde_json::from_slice::<Manifest>(&raw)
                && validate_manifest_hashes(&manifest)
            {
                for out in manifest.outputs {
                    referenced.insert(out.object_hash.to_ascii_lowercase());
                }
            }
        }
    }

    let objects_dir = dir.join("objects");
    if let Some(shards) = read_dir_optional(&objects_dir)? {
        for shard_res in shards {
            let shard = shard_res.map_err(|source| CacheError::Io {
                path: objects_dir.clone(),
                source,
            })?;
            let shard_path = shard.path();
            let shard_name = shard.file_name();

            if is_temp_file_name(&shard_name) {
                freed += remove_if_stale_temp(&shard_path, now)?;
                continue;
            }

            let file_type = match shard.file_type() {
                Ok(ft) => ft,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(source) => {
                    return Err(CacheError::Io {
                        path: shard_path,
                        source,
                    });
                }
            };
            if !file_type.is_dir() {
                continue;
            }

            if let Some(objs) = read_dir_optional(&shard_path)? {
                for obj_res in objs {
                    let obj = obj_res.map_err(|source| CacheError::Io {
                        path: shard_path.clone(),
                        source,
                    })?;
                    let obj_path = obj.path();
                    let obj_name = obj.file_name();

                    if is_temp_file_name(&obj_name) {
                        freed += remove_if_stale_temp(&obj_path, now)?;
                        continue;
                    }

                    let keep = obj_name
                        .to_str()
                        .is_some_and(|s| referenced.contains(&s.to_ascii_lowercase()));

                    if !keep {
                        freed += remove_file_counting_bytes(&obj_path)?;
                    }
                }
            }

            // Tidy up empty shard directories on a best-effort basis; fails
            // harmlessly if still non-empty or concurrently populated.
            let _ = fs::remove_dir(&shard_path);
        }
    }

    Ok(freed)
}

fn manifest_path(dir: &Path, key: &CacheKey) -> PathBuf {
    dir.join("entries")
        .join(format!("{}.json", key_digest(key)))
}

fn object_path(dir: &Path, hash: &blake3::Hash) -> PathBuf {
    let hex = hash.to_hex();
    let hex_str = hex.as_str();
    dir.join("objects").join(&hex_str[..2]).join(hex_str)
}

fn validate_manifest_hashes(manifest: &Manifest) -> bool {
    for inp in &manifest.inputs {
        if inp.missing {
            if !inp.hash.is_empty() {
                return false;
            }
        } else if blake3::Hash::from_hex(&inp.hash).is_err() {
            return false;
        }
    }
    for out in &manifest.outputs {
        if blake3::Hash::from_hex(&out.object_hash).is_err() {
            return false;
        }
    }
    true
}

fn mtime_nanos(meta: &fs::Metadata) -> Option<i64> {
    let modified = meta.modified().ok()?;
    match modified.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).ok(),
        Err(e) => i64::try_from(e.duration().as_nanos())
            .ok()
            .and_then(i64::checked_neg),
    }
}

fn is_url(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return false;
    };
    if let Some((scheme, _rest)) = s.split_once("://") {
        return is_valid_scheme(scheme);
    }
    if let Some((scheme, _rest)) = s.split_once(':')
        && scheme.len() >= 2
        && is_valid_scheme(scheme)
    {
        return matches!(
            scheme.to_ascii_lowercase().as_str(),
            "data" | "blob" | "about" | "javascript" | "mailto"
        );
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

fn is_temp_file_name(name: &OsStr) -> bool {
    let s = name.to_string_lossy();
    let Some((_base, tail)) = s.rsplit_once(".tmp.") else {
        return false;
    };
    let Some((pid_str, seq_str)) = tail.split_once('.') else {
        return false;
    };
    !pid_str.is_empty()
        && pid_str.bytes().all(|b| b.is_ascii_digit())
        && !seq_str.is_empty()
        && seq_str.bytes().all(|b| b.is_ascii_digit())
}

fn write_atomic(target: &Path, contents: &[u8]) -> Result<(), CacheError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    create_dir_all_io(parent)?;

    let pid = std::process::id();
    let base_name = target
        .file_name()
        .map_or_else(|| OsString::from("cache"), OsStr::to_os_string);

    let (mut file, tmp_path) = loop {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let mut tmp_name = base_name.clone();
        tmp_name.push(format!(".tmp.{pid}.{seq}"));
        let candidate = parent.join(tmp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(f) => break (f, candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(CacheError::Io {
                    path: candidate,
                    source,
                });
            }
        }
    };

    if let Err(source) = file.write_all(contents).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&tmp_path);
        return Err(CacheError::Io {
            path: tmp_path,
            source,
        });
    }
    drop(file);

    if let Err(source) = fs::rename(&tmp_path, target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(CacheError::Io {
            path: target.to_path_buf(),
            source,
        });
    }

    Ok(())
}

fn create_dir_all_io(dir: &Path) -> Result<(), CacheError> {
    fs::create_dir_all(dir).map_err(|source| CacheError::Io {
        path: dir.to_path_buf(),
        source,
    })
}

fn remove_tree_if_exists(path: &Path) -> Result<(), CacheError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path).map_err(|source| CacheError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Ok(_) => fs::remove_file(path).map_err(|source| CacheError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(CacheError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_dir_optional(dir: &Path) -> Result<Option<fs::ReadDir>, CacheError> {
    match fs::read_dir(dir) {
        Ok(rd) => Ok(Some(rd)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CacheError::Io {
            path: dir.to_path_buf(),
            source,
        }),
    }
}

fn sweep_stray_temps_in_dir(dir: &Path, now: SystemTime) -> Result<u64, CacheError> {
    let mut freed = 0_u64;
    let Some(entries) = read_dir_optional(dir)? else {
        return Ok(0);
    };
    for entry_res in entries {
        let entry = entry_res.map_err(|source| CacheError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        if is_temp_file_name(&entry.file_name()) {
            freed += remove_if_stale_temp(&entry.path(), now)?;
        }
    }
    Ok(freed)
}

fn remove_if_stale_temp(path: &Path, now: SystemTime) -> Result<u64, CacheError> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(source) => {
            return Err(CacheError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let is_stale = meta
        .modified()
        .ok()
        .and_then(|mtime| now.duration_since(mtime).ok())
        .is_some_and(|age| age > STRAY_TEMP_MAX_AGE);

    if is_stale {
        remove_file_with_known_len(path, meta.len())
    } else {
        Ok(0)
    }
}

fn remove_file_counting_bytes(path: &Path) -> Result<u64, CacheError> {
    let len = match fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(source) => {
            return Err(CacheError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    remove_file_with_known_len(path, len)
}

fn remove_file_with_known_len(path: &Path, len: u64) -> Result<u64, CacheError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(len),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(source) => Err(CacheError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;
    use crate::Cache;

    static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let pid = std::process::id();
            let seq = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("h2q_cache_test_{label}_{pid}_{seq}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create test temp dir");
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn sample_key(entry: &Path) -> CacheKey {
        CacheKey {
            entry: entry.to_path_buf(),
            converter_version: "0.1.0+fmt1".to_owned(),
            options: r#"{"root":"window","viewport":"1280x800"}"#.to_owned(),
        }
    }

    fn sample_cached(qml: &str) -> Cached {
        Cached {
            outputs: vec![
                OutputFile {
                    path: PathBuf::from("Main.qml"),
                    contents: qml.as_bytes().to_vec(),
                },
                OutputFile {
                    path: PathBuf::from("assets/icon.svg"),
                    contents: b"<svg/>".to_vec(),
                },
            ],
            diagnostics: vec!["warning[css]: style.css:3: unsupported filter".to_owned()],
        }
    }

    #[test]
    fn key_digest_separates_field_boundaries() {
        let k1 = CacheKey {
            entry: PathBuf::from("ab"),
            converter_version: "c".to_owned(),
            options: "d".to_owned(),
        };
        let k2 = CacheKey {
            entry: PathBuf::from("a"),
            converter_version: "bc".to_owned(),
            options: "d".to_owned(),
        };
        let k3 = CacheKey {
            entry: PathBuf::from("a"),
            converter_version: "b".to_owned(),
            options: "cd".to_owned(),
        };
        assert_ne!(k1.digest(), k2.digest());
        assert_ne!(k2.digest(), k3.digest());
        assert_ne!(k1.digest(), k3.digest());
        assert_eq!(k1.digest().len(), 64);
    }

    #[test]
    fn store_and_lookup_hit_uses_size_mtime_fast_path() {
        let td = TestDir::new("hit");
        let html = td.path.join("index.html");
        let css = td.path.join("style.css");
        fs::write(&html, b"<h1>Hello</h1>").expect("write html");
        fs::write(&css, b"h1 { color: red; }").expect("write css");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key = sample_key(&html);
        let cached = sample_cached("ApplicationWindow {}");
        let inputs = vec![
            Input::new(&html, b"<h1>Hello</h1>"),
            Input::new(&css, b"h1 { color: red; }"),
        ];

        assert_eq!(cache.lookup(&key), None);
        cache.store(&key, &inputs, &cached).expect("store");
        assert_eq!(cache.lookup(&key), Some(cached.clone()));

        // Verify objects live at objects/<first 2 hex>/<hash>.
        let main_hash = blake3::hash(b"ApplicationWindow {}").to_hex().to_string();
        assert!(
            cache
                .dir()
                .join("objects")
                .join(&main_hash[..2])
                .join(&main_hash)
                .is_file()
        );

        // White-box check for the size+mtime fast path: recording a synthetic
        // hash for an unchanged file hits without re-hashing until mtime/size
        // changes.
        let synthetic_key = CacheKey {
            options: "synthetic".to_owned(),
            ..key
        };
        let synthetic_input = Input {
            path: html.clone(),
            hash: blake3::hash(b"different bytes of different size"),
        };
        cache
            .store(&synthetic_key, &[synthetic_input], &cached)
            .expect("store synthetic");
        assert_eq!(cache.lookup(&synthetic_key), Some(cached));
    }

    #[test]
    fn miss_on_content_change_different_or_same_size() {
        let td = TestDir::new("content_change");
        let html = td.path.join("index.html");
        fs::write(&html, b"<h1>Hello</h1>").expect("write html");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key = sample_key(&html);
        let cached = sample_cached("Item {}");

        cache
            .store(&key, &[Input::new(&html, b"<h1>Hello</h1>")], &cached)
            .expect("store");

        // Different size change.
        fs::write(&html, b"<h1>Hello, QML!</h1>").expect("rewrite html");
        assert_eq!(cache.lookup(&key), None);

        // Re-store and then mutate keeping exact same byte length with newer mtime.
        cache
            .store(&key, &[Input::new(&html, b"<h1>Hello, QML!</h1>")], &cached)
            .expect("re-store");
        assert_eq!(cache.lookup(&key), Some(cached));

        fs::write(&html, b"<h1>Goodbye!!!!</h1>").expect("same-len rewrite");
        let bumped = SystemTime::now() + Duration::from_secs(10);
        fs::File::options()
            .write(true)
            .open(&html)
            .expect("open html")
            .set_modified(bumped)
            .expect("set mtime");
        assert_eq!(cache.lookup(&key), None);
    }

    #[test]
    fn hit_after_touch_with_same_content_refreshes_manifest_mtime() {
        let td = TestDir::new("touch");
        let html = td.path.join("index.html");
        fs::write(&html, b"<p>unchanged</p>").expect("write html");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key = sample_key(&html);
        let cached = sample_cached("Item {}");
        cache
            .store(&key, &[Input::new(&html, b"<p>unchanged</p>")], &cached)
            .expect("store");

        let manifest_file = cache
            .dir()
            .join("entries")
            .join(format!("{}.json", key.digest()));
        let before_manifest = fs::read_to_string(&manifest_file).expect("read manifest before");

        let future = SystemTime::now() + Duration::from_secs(120);
        fs::File::options()
            .write(true)
            .open(&html)
            .expect("open html")
            .set_modified(future)
            .expect("touch html");

        assert_eq!(cache.lookup(&key), Some(cached));

        let after_manifest = fs::read_to_string(&manifest_file).expect("read manifest after");
        assert_ne!(before_manifest, after_manifest);
    }

    #[test]
    fn miss_on_removed_input_and_missing_dependency_invalidation() {
        let td = TestDir::new("removed_and_missing");
        let html = td.path.join("index.html");
        let css = td.path.join("style.css");
        let optional_css = td.path.join("optional.css");
        fs::write(&html, b"<link rel=stylesheet href=style.css>").expect("write html");
        fs::write(&css, b"body {}").expect("write css");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key = sample_key(&html);
        let cached = sample_cached("Item {}");

        let inputs = vec![
            Input::new(&html, b"<link rel=stylesheet href=style.css>"),
            Input::new(&css, b"body {}"),
            Input::missing(&optional_css),
        ];
        cache.store(&key, &inputs, &cached).expect("store");

        // Still missing -> hit, and lookup_with_inputs preserves the missing marker.
        let (hit, recorded) = cache
            .lookup_with_inputs(&key)
            .expect("hit while still missing");
        assert_eq!(hit, cached);
        assert_eq!(recorded.len(), 3);
        assert!(recorded[2].is_missing());

        // Creating the previously missing file invalidates the entry even if empty.
        fs::write(&optional_css, b"").expect("create optional.css");
        assert_eq!(cache.lookup(&key), None);

        // Removing a previously present file also invalidates.
        fs::remove_file(&optional_css).expect("remove optional.css again");
        assert_eq!(cache.lookup(&key), Some(cached));
        fs::remove_file(&css).expect("remove required css");
        assert_eq!(cache.lookup(&key), None);
    }

    #[test]
    fn miss_on_version_or_options_change() {
        let td = TestDir::new("version_options");
        let html = td.path.join("index.html");
        fs::write(&html, b"<div/>").expect("write html");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key = sample_key(&html);
        let cached = sample_cached("Item {}");
        cache
            .store(&key, &[Input::new(&html, b"<div/>")], &cached)
            .expect("store");

        let new_version_key = CacheKey {
            converter_version: "0.2.0+fmt1".to_owned(),
            ..key.clone()
        };
        let new_options_key = CacheKey {
            options: r#"{"root":"item","viewport":"800x600"}"#.to_owned(),
            ..key.clone()
        };

        assert_eq!(cache.lookup(&new_version_key), None);
        assert_eq!(cache.lookup(&new_options_key), None);
        assert_eq!(cache.lookup(&key), Some(cached));
    }

    #[test]
    fn url_inputs_are_never_fresh() {
        let td = TestDir::new("urls");
        let html = td.path.join("index.html");
        fs::write(&html, b"<div/>").expect("write html");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let cached = sample_cached("Item {}");

        for url in [
            "https://example.com/app.css",
            "http://localhost:8080/app.js",
            "data:text/css,body{}",
            "file:///tmp/some.css",
        ] {
            let key = CacheKey {
                options: url.to_owned(),
                ..sample_key(&html)
            };
            cache
                .store(
                    &key,
                    &[Input::new(&html, b"<div/>"), Input::new(url, b"body {}")],
                    &cached,
                )
                .expect("store with url input");
            assert_eq!(cache.lookup(&key), None, "URL input {url} must miss");
        }
    }

    #[test]
    fn corrupt_manifest_is_removed_and_misses() {
        let td = TestDir::new("corrupt_manifest");
        let html = td.path.join("index.html");
        fs::write(&html, b"<div/>").expect("write html");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key = sample_key(&html);
        let manifest_file = cache
            .dir()
            .join("entries")
            .join(format!("{}.json", key.digest()));

        // Case 1: malformed JSON.
        fs::write(&manifest_file, b"{not valid json").expect("write bad json");
        assert_eq!(cache.lookup(&key), None);
        assert!(!manifest_file.exists());

        // Case 2: valid JSON syntax with malformed BLAKE3 hex digest.
        fs::write(
            &manifest_file,
            br#"{"inputs":[{"path":"/tmp/x","hash":"zzzz"}],"outputs":[],"diagnostics":[]}"#,
        )
        .expect("write bad hash manifest");
        assert_eq!(cache.lookup(&key), None);
        assert!(!manifest_file.exists());
    }

    #[test]
    fn missing_or_corrupt_object_is_a_miss() {
        let td = TestDir::new("missing_object");
        let html = td.path.join("index.html");
        fs::write(&html, b"<div/>").expect("write html");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key = sample_key(&html);
        let cached = sample_cached("ApplicationWindow {}");
        cache
            .store(&key, &[Input::new(&html, b"<div/>")], &cached)
            .expect("store");

        let obj_hash = blake3::hash(b"ApplicationWindow {}");
        let obj_file = object_path(cache.dir(), &obj_hash);
        assert!(obj_file.is_file());

        // Corrupted object bytes -> miss and bad object removed.
        fs::write(&obj_file, b"tampered").expect("tamper object");
        assert_eq!(cache.lookup(&key), None);
        assert!(!obj_file.exists());

        // Re-store heals the missing object; deleting the object -> miss.
        cache
            .store(&key, &[Input::new(&html, b"<div/>")], &cached)
            .expect("re-store");
        assert_eq!(cache.lookup(&key), Some(cached));
        fs::remove_file(&obj_file).expect("delete object");
        assert_eq!(cache.lookup(&key), None);
    }

    #[test]
    fn gc_removes_unreferenced_objects_and_stale_temps_only() {
        let td = TestDir::new("gc");
        let html = td.path.join("index.html");
        fs::write(&html, b"<div/>").expect("write html");

        let cache = Cache::open(td.path.join(".h2q-cache")).expect("open cache");
        let key1 = sample_key(&html);
        let key2 = CacheKey {
            options: "second".to_owned(),
            ..key1.clone()
        };

        let cached1 = Cached {
            outputs: vec![OutputFile {
                path: PathBuf::from("Main.qml"),
                contents: b"Item { id: first }".to_vec(),
            }],
            diagnostics: Vec::new(),
        };
        let cached2 = Cached {
            outputs: vec![OutputFile {
                path: PathBuf::from("Main.qml"),
                contents: b"Item { id: second_longer }".to_vec(),
            }],
            diagnostics: Vec::new(),
        };

        cache
            .store(&key1, &[Input::new(&html, b"<div/>")], &cached1)
            .expect("store 1");
        cache
            .store(&key2, &[Input::new(&html, b"<div/>")], &cached2)
            .expect("store 2");

        // Remove key2's manifest so its object becomes unreferenced.
        let manifest2 = cache
            .dir()
            .join("entries")
            .join(format!("{}.json", key2.digest()));
        fs::remove_file(&manifest2).expect("remove manifest 2");

        // Plant one stale (>1h) temp file and one fresh temp file.
        let stale_tmp = cache.dir().join("entries").join("abc.json.tmp.4242.0");
        let fresh_tmp = cache.dir().join("entries").join("abc.json.tmp.4242.1");
        let stale_payload = b"stale partial manifest";
        fs::write(&stale_tmp, stale_payload).expect("write stale tmp");
        fs::write(&fresh_tmp, b"in-flight manifest").expect("write fresh tmp");

        let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
        fs::File::options()
            .write(true)
            .open(&stale_tmp)
            .expect("open stale tmp")
            .set_modified(two_hours_ago)
            .expect("backdate stale tmp");

        let freed = cache.gc().expect("run gc");
        let expected_freed = (b"Item { id: second_longer }".len() + stale_payload.len()) as u64;
        assert_eq!(freed, expected_freed);
        assert!(!stale_tmp.exists());
        assert!(fresh_tmp.exists());
        assert_eq!(cache.lookup(&key1), Some(cached1));
    }

    #[test]
    fn concurrent_stores_from_threads_never_tear_entries() {
        let td = TestDir::new("concurrent");
        let html = td.path.join("index.html");
        fs::write(&html, b"<main/>").expect("write html");

        let cache = Arc::new(Cache::open(td.path.join(".h2q-cache")).expect("open cache"));
        let key = Arc::new(sample_key(&html));

        let mut handles = Vec::new();
        for tid in 0..8_usize {
            let cache = Arc::clone(&cache);
            let key = Arc::clone(&key);
            let html = html.clone();
            handles.push(thread::spawn(move || {
                for iter in 0..15_usize {
                    let payload = format!(
                        "ApplicationWindow {{ title: \"worker-{tid}-{iter}-{}\" }}",
                        "x".repeat(256)
                    );
                    let cached = Cached {
                        outputs: vec![OutputFile {
                            path: PathBuf::from("Main.qml"),
                            contents: payload.into_bytes(),
                        }],
                        diagnostics: vec![format!("info[qml]: worker {tid} iter {iter}")],
                    };
                    cache
                        .store(&key, &[Input::new(&html, b"<main/>")], &cached)
                        .expect("concurrent store");
                    // Interleave concurrent lookups while writers are active.
                    let hit = cache.lookup(&key).expect("concurrent lookup must hit");
                    assert_eq!(hit.outputs.len(), 1);
                    assert_eq!(hit.diagnostics.len(), 1);
                }
            }));
        }

        for h in handles {
            h.join().expect("join worker thread");
        }

        let final_hit = cache.lookup(&key).expect("final lookup");
        assert_eq!(final_hit.outputs.len(), 1);
    }

    #[test]
    fn version_mismatch_or_missing_wipes_entries_and_objects() {
        let td = TestDir::new("version_wipe");
        let html = td.path.join("index.html");
        fs::write(&html, b"<div/>").expect("write html");

        let cache_dir = td.path.join(".h2q-cache");
        let cache = Cache::open(&cache_dir).expect("open cache");
        let key = sample_key(&html);
        let cached = sample_cached("Item {}");
        cache
            .store(&key, &[Input::new(&html, b"<div/>")], &cached)
            .expect("store");
        assert_eq!(cache.lookup(&key), Some(cached.clone()));

        // Mismatched VERSION wipes entries/ and objects/.
        fs::write(cache_dir.join("VERSION"), b"999\n").expect("write wrong version");
        let reopened = Cache::open(&cache_dir).expect("reopen mismatched cache");
        assert_eq!(reopened.lookup(&key), None);
        assert_eq!(
            fs::read_to_string(cache_dir.join("VERSION"))
                .expect("read version")
                .trim(),
            CACHE_FORMAT_VERSION
        );
        assert_eq!(
            fs::read_dir(cache_dir.join("objects"))
                .expect("read objects")
                .count(),
            0
        );

        // Missing VERSION also wipes entries/ and objects/.
        reopened
            .store(&key, &[Input::new(&html, b"<div/>")], &cached)
            .expect("store again");
        fs::remove_file(cache_dir.join("VERSION")).expect("delete VERSION");
        let reopened_missing = Cache::open(&cache_dir).expect("reopen missing VERSION");
        assert_eq!(reopened_missing.lookup(&key), None);
    }
}
