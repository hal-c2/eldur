use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};

use crate::{ConvertArgs, Converter, FsResources};

/// Format constant mixed into [`eldur_cache::CacheKey::converter_version`].
/// Bump whenever output generation changes without a crate version bump.
pub const CONVERTER_FORMAT_VERSION: u32 = 1;

static OUT_TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Whether a conversion run was served from [`eldur_cache::Cache`] or executed
/// through [`Converter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertStatus {
    /// Outputs and diagnostics were replayed from a valid cache entry.
    Cached,
    /// The converter executed and produced fresh outputs.
    Converted,
}

/// Summary of a completed `convert` invocation.
#[derive(Debug, Clone)]
pub struct ConvertOutcome {
    /// Whether the cache hit or the converter ran.
    pub status: ConvertStatus,
    /// Directory where output files live.
    pub output_dir: PathBuf,
    /// Number of output files created or updated on disk.
    pub written_files: usize,
    /// Number of output files skipped because identical bytes were already on disk.
    pub skipped_files: usize,
    /// Formatted diagnostics printed to stderr.
    pub diagnostics: Vec<String>,
    /// Every input dependency (entry HTML plus sub-resources, including missing ones).
    pub dependencies: Vec<eldur_cache::Input>,
    /// Wall-clock duration of the run.
    pub elapsed: Duration,
}

/// Computes the default output directory (`<input stem>_qml/`) next to `input`.
#[must_use]
pub fn default_output_dir(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("out");
    let dir_name = format!("{stem}_qml");
    match input.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(dir_name),
        _ => PathBuf::from(dir_name),
    }
}

/// Stable JSON serialisation of [`eldur_convert::Options`] for [`eldur_cache::CacheKey`].
#[must_use]
pub fn serialize_options(options: &eldur_convert::Options) -> String {
    let root = match options.root {
        eldur_convert::RootKind::Window => "window",
        eldur_convert::RootKind::Item => "item",
    };
    // `serde_json::Map` without `preserve_order` is backed by `BTreeMap`, so
    // object keys are always emitted in deterministic lexicographical order.
    serde_json::json!({
        "animations": options.animations,
        "root": root,
        "source_comments": options.source_comments,
        "state_variants": options.state_variants,
        "viewport": {
            "height": options.viewport.height,
            "prefers_dark": options.viewport.prefers_dark,
            "prefers_reduced_motion": options.viewport.prefers_reduced_motion,
            "width": options.viewport.width,
        }
    })
    .to_string()
}

pub(crate) fn execute_convert(
    args: &ConvertArgs,
    converter: &dyn Converter,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> anyhow::Result<ConvertOutcome> {
    let started = Instant::now();

    let canonical_input = fs::canonicalize(&args.input)
        .with_context(|| format!("failed to resolve input file {}", args.input.display()))?;
    let input_meta = fs::metadata(&canonical_input)
        .with_context(|| format!("failed to stat input file {}", canonical_input.display()))?;
    if input_meta.is_dir() {
        bail!(
            "input path {} is a directory, expected an HTML file",
            canonical_input.display()
        );
    }
    // Verify read permission up front so an unreadable file always fails with
    // an I/O error even if a prior cache entry has matching size + mtime.
    fs::File::open(&canonical_input)
        .with_context(|| format!("failed to open input file {}", canonical_input.display()))?;

    let out_dir = args.resolved_output_dir();
    let options = args.to_convert_options();

    let key = eldur_cache::CacheKey {
        entry: canonical_input.clone(),
        converter_version: converter_version().to_owned(),
        options: serialize_options(&options),
    };

    let cache =
        if args.no_cache {
            None
        } else {
            let cache_dir = args.resolved_cache_dir(&out_dir);
            Some(eldur_cache::Cache::open(&cache_dir).with_context(|| {
                format!("failed to open build cache at {}", cache_dir.display())
            })?)
        };

    if let Some(cache) = &cache
        && let Some((cached, dependencies)) = cache.lookup_with_inputs(&key)
    {
        let (written_files, skipped_files) = write_cached_outputs(&out_dir, &cached.outputs)?;
        for diag in &cached.diagnostics {
            writeln!(stderr, "{diag}").context("failed to write diagnostic to stderr")?;
        }
        let elapsed = started.elapsed();
        if !args.quiet {
            writeln!(
                stdout,
                "cached {} -> {} ({elapsed:.2?})",
                args.input.display(),
                out_dir.display()
            )
            .context("failed to write status to stdout")?;
        }
        return Ok(ConvertOutcome {
            status: ConvertStatus::Cached,
            output_dir: out_dir,
            written_files,
            skipped_files,
            diagnostics: cached.diagnostics,
            dependencies,
            elapsed,
        });
    }

    let raw_html = fs::read(&canonical_input)
        .with_context(|| format!("failed to read input file {}", canonical_input.display()))?;
    // Non-UTF-8 bytes are decoded with U+FFFD replacement per WHATWG stream
    // decoding fallback while the cache hashes the exact raw bytes on disk.
    let conv_input = eldur_convert::Input {
        html: String::from_utf8_lossy(&raw_html).into_owned(),
        url: canonical_input.to_string_lossy().into_owned(),
    };

    let mut resources = FsResources::new()?;
    let output = converter.convert(&conv_input, &options, &mut resources);

    let diagnostics: Vec<String> = output.diagnostics.iter().map(ToString::to_string).collect();
    let (written_files, skipped_files) = write_convert_outputs(&out_dir, &output.files)?;

    let mut dep_map = BTreeMap::new();
    for dep in resources.into_inputs() {
        dep_map.insert(dep.path.clone(), dep);
    }
    dep_map.insert(
        canonical_input.clone(),
        eldur_cache::Input::new(canonical_input, &raw_html),
    );
    let dependencies: Vec<eldur_cache::Input> = dep_map.into_values().collect();

    if let Some(cache) = &cache {
        let cached = eldur_cache::Cached {
            outputs: output
                .files
                .into_iter()
                .map(|f| eldur_cache::OutputFile {
                    path: f.path,
                    contents: f.contents,
                })
                .collect(),
            diagnostics: diagnostics.clone(),
        };
        cache
            .store(&key, &dependencies, &cached)
            .with_context(|| format!("failed to write build cache at {}", cache.dir().display()))?;
    }

    for diag in &diagnostics {
        writeln!(stderr, "{diag}").context("failed to write diagnostic to stderr")?;
    }
    let elapsed = started.elapsed();
    if !args.quiet {
        writeln!(
            stdout,
            "converted {} -> {} ({elapsed:.2?})",
            args.input.display(),
            out_dir.display()
        )
        .context("failed to write status to stdout")?;
    }

    Ok(ConvertOutcome {
        status: ConvertStatus::Converted,
        output_dir: out_dir,
        written_files,
        skipped_files,
        diagnostics,
        dependencies,
        elapsed,
    })
}

fn validate_relative_output_path(rel: &Path) -> anyhow::Result<()> {
    if rel.as_os_str().is_empty() {
        bail!("refusing to write empty output file path");
    }
    let mut has_normal = false;
    for comp in rel.components() {
        match comp {
            Component::Normal(_) => has_normal = true,
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                bail!(
                    "refusing output path '{}' that escapes output directory",
                    rel.display()
                );
            }
        }
    }
    if !has_normal {
        bail!(
            "refusing output path '{}' without a file name",
            rel.display()
        );
    }
    Ok(())
}

fn write_cached_outputs(
    out_dir: &Path,
    files: &[eldur_cache::OutputFile],
) -> anyhow::Result<(usize, usize)> {
    for file in files {
        validate_relative_output_path(&file.path)?;
    }
    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create output directory {}", out_dir.display()))?;
    let mut written = 0_usize;
    let mut skipped = 0_usize;
    for file in files {
        if write_if_changed(&out_dir.join(&file.path), &file.contents)? {
            written += 1;
        } else {
            skipped += 1;
        }
    }
    Ok((written, skipped))
}

fn write_convert_outputs(
    out_dir: &Path,
    files: &[eldur_convert::OutputFile],
) -> anyhow::Result<(usize, usize)> {
    for file in files {
        validate_relative_output_path(&file.path)?;
    }
    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create output directory {}", out_dir.display()))?;
    let mut written = 0_usize;
    let mut skipped = 0_usize;
    for file in files {
        if write_if_changed(&out_dir.join(&file.path), &file.contents)? {
            written += 1;
        } else {
            skipped += 1;
        }
    }
    Ok((written, skipped))
}

/// Atomically writes `contents` to `target` (temp file + `fsync` + `rename`)
/// unless `target` already exists with identical bytes.
///
/// Skipping identical files avoids unnecessary reloads in `qml6` / `qmlscene`,
/// and atomic rename ensures live-reloaders never observe a half-written file.
fn write_if_changed(target: &Path, contents: &[u8]) -> anyhow::Result<bool> {
    if let Ok(existing) = fs::read(target)
        && existing == contents
    {
        return Ok(false);
    }

    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))?;

    let pid = std::process::id();
    let base_name = target
        .file_name()
        .map_or_else(|| OsString::from("out"), OsStr::to_os_string);

    let (mut file, tmp_path) = loop {
        let seq = OUT_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let mut tmp_name = OsString::from(".");
        tmp_name.push(&base_name);
        tmp_name.push(format!(".eldur-tmp.{pid}.{seq}"));
        let candidate = parent.join(tmp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(f) => break (f, candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to create temporary output file {}",
                        candidate.display()
                    )
                });
            }
        }
    };

    if let Err(err) = file.write_all(contents).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "failed to write temporary output file {}",
                tmp_path.display()
            )
        });
    }
    drop(file);

    if let Err(err) = fs::rename(&tmp_path, target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err)
            .with_context(|| format!("failed to publish output file {}", target.display()));
    }

    Ok(true)
}

// Mix the running binary's digest into the cache key so rebuilds during
// development invalidate cached outputs without requiring a version bump.
fn converter_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let base = format!(
            "{}+fmt{CONVERTER_FORMAT_VERSION}",
            env!("CARGO_PKG_VERSION")
        );
        match std::env::current_exe().and_then(fs::read) {
            Ok(bytes) => {
                let hex = blake3::hash(&bytes).to_hex();
                format!("{base}+{}", &hex[..16])
            }
            Err(_) => base,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converter_version_includes_stable_non_empty_build_id() {
        let first = converter_version();
        let second = converter_version();
        assert_eq!(first, second);
        let prefix = format!(
            "{}+fmt{CONVERTER_FORMAT_VERSION}+",
            env!("CARGO_PKG_VERSION")
        );
        let build_id = first
            .strip_prefix(&prefix)
            .expect("converter_version should include build id suffix");
        assert!(!build_id.is_empty());
        assert_eq!(build_id.len(), 16);
        assert!(build_id.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
