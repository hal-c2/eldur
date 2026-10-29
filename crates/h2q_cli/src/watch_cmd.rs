use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Context;
use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::convert_cmd::execute_convert;
use crate::{ConvertArgs, ConvertOutcome, Converter};

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(100);

#[derive(Debug)]
enum WatchSignal {
    Fs(notify::Result<Event>),
    Stop,
}

/// Event channel pairing [`notify`] events with an external [`WatchStop`]
/// handle on a single blocking queue (0 % CPU when idle, no polling).
#[derive(Debug)]
pub struct WatchChannel {
    tx: mpsc::Sender<WatchSignal>,
    rx: mpsc::Receiver<WatchSignal>,
}

impl WatchChannel {
    /// Creates a connected `(WatchChannel, WatchStop)` pair.
    #[must_use]
    pub fn new() -> (Self, WatchStop) {
        let (tx, rx) = mpsc::channel();
        let stop = WatchStop { tx: tx.clone() };
        (Self { tx, rx }, stop)
    }
}

/// Handle that wakes and terminates a running watch loop immediately.
#[derive(Debug, Clone)]
pub struct WatchStop {
    tx: mpsc::Sender<WatchSignal>,
}

impl WatchStop {
    /// Signals the associated watch loop to exit cleanly.
    pub fn stop(&self) {
        let _ = self.tx.send(WatchSignal::Stop);
    }
}

pub(crate) fn execute_watch(
    args: &ConvertArgs,
    converter: &dyn Converter,
    channel: WatchChannel,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    mut on_cycle: Option<&mut dyn FnMut(&ConvertOutcome)>,
) -> anyhow::Result<()> {
    let initial = execute_convert(args, converter, stdout, stderr)?;

    let out_dir = args.resolved_output_dir();
    let cache_dir = args.resolved_cache_dir(&out_dir);
    let ignore_dirs = collect_ignore_dirs(&out_dir, &cache_dir);

    let mut tracked_paths = BTreeSet::new();
    let mut fingerprints = h2q_cache::Fingerprints::new();
    seed_fingerprints(&initial.dependencies, &mut tracked_paths, &mut fingerprints);

    let fs_tx = channel.tx;
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = fs_tx.send(WatchSignal::Fs(res));
    })
    .context("failed to initialize file-system watcher")?;

    let mut active_watches: BTreeMap<PathBuf, RecursiveMode> = BTreeMap::new();
    sync_watches(&mut watcher, &mut active_watches, &tracked_paths)?;

    if let Some(cb) = on_cycle.as_deref_mut() {
        cb(&initial);
    }

    loop {
        let first_signal = match channel.rx.recv() {
            Ok(sig) => sig,
            Err(_) => return Ok(()),
        };

        match first_signal {
            WatchSignal::Stop => return Ok(()),
            WatchSignal::Fs(Err(_)) => continue,
            WatchSignal::Fs(Ok(event)) => {
                if !is_relevant_event(&event, &ignore_dirs, &tracked_paths) {
                    continue;
                }
            }
        }

        // Trailing 100 ms debounce over incoming relevant events without polling.
        let mut deadline = Instant::now() + DEBOUNCE_WINDOW;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match channel.rx.recv_timeout(deadline - now) {
                Ok(WatchSignal::Stop) => return Ok(()),
                Ok(WatchSignal::Fs(Ok(ev))) => {
                    if is_relevant_event(&ev, &ignore_dirs, &tracked_paths) {
                        deadline = Instant::now() + DEBOUNCE_WINDOW;
                    }
                }
                Ok(WatchSignal::Fs(Err(_))) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }

        if !dependencies_changed(&tracked_paths, &fingerprints) {
            continue;
        }

        match execute_convert(args, converter, stdout, stderr) {
            Ok(outcome) => {
                tracked_paths.clear();
                fingerprints = h2q_cache::Fingerprints::new();
                seed_fingerprints(&outcome.dependencies, &mut tracked_paths, &mut fingerprints);
                sync_watches(&mut watcher, &mut active_watches, &tracked_paths)?;
                if let Some(cb) = on_cycle.as_deref_mut() {
                    cb(&outcome);
                }
            }
            Err(err) => {
                // Record current disk state so a transient mid-save missing
                // entry file does not spin; restoring the file changes its
                // fingerprint again and triggers recovery.
                refresh_fingerprints_from_disk(&tracked_paths, &mut fingerprints);
                writeln!(stderr, "error: {err:#}").context("failed to write watch error")?;
            }
        }
    }
}

fn seed_fingerprints(
    dependencies: &[h2q_cache::Input],
    tracked_paths: &mut BTreeSet<PathBuf>,
    fingerprints: &mut h2q_cache::Fingerprints,
) {
    for dep in dependencies {
        tracked_paths.insert(dep.path.clone());
        let key = dep.path.to_string_lossy();
        if dep.is_missing() {
            fingerprints.record_bytes(&key, None);
        } else {
            let current = fs::read(&dep.path).ok();
            fingerprints.record_bytes(&key, current.as_deref());
        }
    }
}

fn refresh_fingerprints_from_disk(
    tracked_paths: &BTreeSet<PathBuf>,
    fingerprints: &mut h2q_cache::Fingerprints,
) {
    for path in tracked_paths {
        let key = path.to_string_lossy();
        let current = fs::read(path).ok();
        fingerprints.record_bytes(&key, current.as_deref());
    }
}

fn dependencies_changed(
    tracked_paths: &BTreeSet<PathBuf>,
    fingerprints: &h2q_cache::Fingerprints,
) -> bool {
    for path in tracked_paths {
        let key = path.to_string_lossy();
        let current = fs::read(path).ok();
        if fingerprints.differs_bytes(&key, current.as_deref()) {
            return true;
        }
    }
    false
}

fn collect_ignore_dirs(out_dir: &Path, cache_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for d in [out_dir, cache_dir] {
        dirs.push(d.to_path_buf());
        if let Ok(canon) = fs::canonicalize(d)
            && !dirs.contains(&canon)
        {
            dirs.push(canon);
        }
    }
    dirs
}

fn is_relevant_event(
    event: &Event,
    ignore_dirs: &[PathBuf],
    tracked_paths: &BTreeSet<PathBuf>,
) -> bool {
    // Reading files on Linux emits inotify OPEN / CLOSE_NOWRITE access events.
    // Ignoring non-write access events avoids waking up from our own reads.
    if matches!(
        event.kind,
        EventKind::Access(AccessKind::Open(_))
            | EventKind::Access(AccessKind::Read)
            | EventKind::Access(AccessKind::Close(AccessMode::Read))
    ) {
        return false;
    }

    if event.paths.is_empty() {
        return true;
    }

    // Ignore events strictly inside our output or cache directories unless a
    // tracked input dependency happens to live there.
    event
        .paths
        .iter()
        .any(|p| tracked_paths.contains(p) || !ignore_dirs.iter().any(|dir| p.starts_with(dir)))
}

fn desired_watch_dirs(tracked_paths: &BTreeSet<PathBuf>) -> BTreeMap<PathBuf, RecursiveMode> {
    let mut desired = BTreeMap::new();
    for path in tracked_paths {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        if parent.is_dir() {
            desired
                .entry(parent.to_path_buf())
                .or_insert(RecursiveMode::NonRecursive);
        } else {
            // Missing dependency inside a not-yet-created subdirectory: watch
            // the nearest existing ancestor recursively so `mkdir` + file
            // creation is caught.
            for ancestor in parent.ancestors() {
                let candidate = if ancestor.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    ancestor
                };
                if candidate.is_dir() {
                    desired.insert(candidate.to_path_buf(), RecursiveMode::Recursive);
                    break;
                }
            }
        }
    }
    desired
}

fn sync_watches(
    watcher: &mut RecommendedWatcher,
    active: &mut BTreeMap<PathBuf, RecursiveMode>,
    tracked_paths: &BTreeSet<PathBuf>,
) -> anyhow::Result<()> {
    let desired = desired_watch_dirs(tracked_paths);

    let stale: Vec<PathBuf> = active
        .keys()
        .filter(|dir| !desired.contains_key(*dir))
        .cloned()
        .collect();
    for dir in stale {
        let _ = watcher.unwatch(&dir);
        active.remove(&dir);
    }

    for (dir, mode) in desired {
        if active.get(&dir) == Some(&mode) {
            continue;
        }
        if active.contains_key(&dir) {
            let _ = watcher.unwatch(&dir);
        }
        watcher
            .watch(&dir, mode)
            .with_context(|| format!("failed to watch directory {}", dir.display()))?;
        active.insert(dir, mode);
    }

    Ok(())
}
