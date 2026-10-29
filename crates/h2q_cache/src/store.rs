use std::path::{Path, PathBuf};

use crate::{CacheError, CacheKey, Cached, Input};

pub(crate) fn key_digest(_key: &CacheKey) -> String {
    unimplemented!("h2q_cache worker")
}

pub(crate) fn open(_dir: &Path) -> Result<PathBuf, CacheError> {
    unimplemented!("h2q_cache worker")
}

pub(crate) fn lookup(_dir: &Path, _key: &CacheKey) -> Option<Cached> {
    unimplemented!("h2q_cache worker")
}

pub(crate) fn store(
    _dir: &Path,
    _key: &CacheKey,
    _inputs: &[Input],
    _cached: &Cached,
) -> Result<(), CacheError> {
    unimplemented!("h2q_cache worker")
}

pub(crate) fn gc(_dir: &Path) -> Result<u64, CacheError> {
    unimplemented!("h2q_cache worker")
}
