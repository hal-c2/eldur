//! The build cache: converting the same inputs with the same options twice
//! does no work the second time.
//!
//! Ideas taken from `~/dev/ljos-ui` (`ljos_dev::fingerprint`, the compiler's
//! `source_hash`): what matters is the *content* of a file, as a BLAKE3
//! hash, never its time stamp. An editor saving without changes, `touch`,
//! or a backup-and-rename are not changes.
//!
//! Layout on disk (`<dir>` is `.h2q-cache/` next to the output by default):
//!
//! ```text
//! <dir>/VERSION                 cache format version; mismatch = wipe
//! <dir>/entries/<key>.json      one per (entry file, options): the manifest
//! <dir>/objects/<aa>/<hash>     output file contents, content-addressed
//! ```
//!
//! A manifest lists every input the conversion read (the HTML, linked CSS,
//! `@import`ed CSS, scripts, images it inspected) with its hash and its
//! size + mtime (a fast path: when both match, the hash is not recomputed),
//! and every output file with its object hash. A lookup re-checks each
//! input; any difference, a missing input, or a new converter version is a
//! miss.
//!
//! The public API in this file is the contract the CLI is written against.
//! Add to it freely; change or remove items only with the lead's agreement.

use std::path::{Path, PathBuf};

pub use fingerprint::Fingerprints;

mod fingerprint;
mod store;

/// Errors returned when opening, writing, or garbage-collecting a [`Cache`].
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// A file-system operation inside the cache directory failed.
    #[error("cache I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A cache manifest could not be parsed.
    #[error("corrupt cache manifest {path}: {message}")]
    Corrupt { path: PathBuf, message: String },
}

/// What identifies a conversion apart from its inputs' contents: the entry
/// file, the converter's version and every option that changes output.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub entry: PathBuf,
    /// `env!("CARGO_PKG_VERSION")` of the converter plus a format number;
    /// bump it whenever output for the same input changes.
    pub converter_version: String,
    /// Stable serialisation of the options (e.g. JSON with sorted keys).
    pub options: String,
}

impl CacheKey {
    /// Hex BLAKE3 of the three fields; the manifest's file name.
    #[must_use]
    pub fn digest(&self) -> String {
        store::key_digest(self)
    }
}

/// An input file the conversion read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// Absolute path (or URL for remote inputs, which are never fresh).
    pub path: PathBuf,
    pub hash: blake3::Hash,
}

impl Input {
    /// Hashes `bytes` as read from `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, bytes: &[u8]) -> Self {
        Self {
            path: path.into(),
            hash: blake3::hash(bytes),
        }
    }

    /// Records a path that did not exist when conversion tried to read it.
    ///
    /// A missing file is still a dependency: creating it later invalidates the
    /// cached entry, while remaining absent counts as fresh.
    #[must_use]
    pub fn missing(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            hash: store::missing_sentinel_hash(),
        }
    }

    /// Whether this input records a missing-file dependency created with
    /// [`Input::missing`].
    #[must_use]
    pub fn is_missing(&self) -> bool {
        self.hash == store::missing_sentinel_hash()
    }
}

/// A file the conversion produced, relative to the output directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputFile {
    pub path: PathBuf,
    pub contents: Vec<u8>,
}

/// A conversion's recorded diagnostics are replayed on a hit, so a cached
/// run prints the same warnings as the first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cached {
    pub outputs: Vec<OutputFile>,
    pub diagnostics: Vec<String>,
}

/// An open cache directory.
#[derive(Debug)]
pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    /// Opens (creating if needed) the cache at `dir`; wipes it when its
    /// format version is not this crate's.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, CacheError> {
        store::open(dir.as_ref()).map(|dir| Self { dir })
    }

    /// Directory on disk holding this cache.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The outputs recorded for `key` when every recorded input still has
    /// the same content; `None` on any miss. Never errors: a corrupt entry
    /// is a miss (and is removed).
    #[must_use]
    pub fn lookup(&self, key: &CacheKey) -> Option<Cached> {
        store::lookup(&self.dir, key)
    }

    /// Like [`Cache::lookup`], also returning the recorded [`Input`]s from the
    /// manifest so callers such as `html2qml watch` know which dependencies to
    /// watch after a cache hit.
    #[must_use]
    pub fn lookup_with_inputs(&self, key: &CacheKey) -> Option<(Cached, Vec<Input>)> {
        store::lookup_with_inputs(&self.dir, key)
    }

    /// Records a conversion. Writes are atomic (temp file + rename), so a
    /// crash or a concurrent run never leaves a half-written entry.
    pub fn store(
        &self,
        key: &CacheKey,
        inputs: &[Input],
        cached: &Cached,
    ) -> Result<(), CacheError> {
        store::store(&self.dir, key, inputs, cached)
    }

    /// Removes objects no manifest refers to. Returns bytes freed.
    pub fn gc(&self) -> Result<u64, CacheError> {
        store::gc(&self.dir)
    }
}
