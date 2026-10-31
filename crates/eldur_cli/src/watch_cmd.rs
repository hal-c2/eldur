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

/// Upper bound on consecutive immediate catch-up conversions when a file keeps
/// changing *while* conversion runs, preventing unbounded spinning on a
/// self-modifying writer.
const MAX_CATCHUP_PASSES: usize = 8;

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

struct WatchLoopState<'a, 'cb> {
    args: &'a ConvertArgs,
    converter: &'a dyn Converter,
    watcher: RecommendedWatcher,
    active_watches: BTreeMap<PathBuf, RecursiveMode>,
    tracked_paths: BTreeSet<PathBuf>,
    fingerprints: eldur_cache::Fingerprints,
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
    on_cycle: Option<&'a mut (dyn FnMut(&ConvertOutcome) + 'cb)>,
}

impl WatchLoopState<'_, '_> {
    fn run_conversion_and_catch_up(&mut self) -> anyhow::Result<()> {
        for _ in 0..MAX_CATCHUP_PASSES {
            match execute_convert(self.args, self.converter, self.stdout, self.stderr) {
                Ok(outcome) => {
                    self.tracked_paths.clear();
                    self.fingerprints = eldur_cache::Fingerprints::new();
                    seed_fingerprints(
                        &outcome.dependencies,
                        &mut self.tracked_paths,
                        &mut self.fingerprints,
                    );
                    sync_watches(
                        &mut self.watcher,
                        &mut self.active_watches,
                        &self.tracked_paths,
                    )?;
                    // Check whether disk diverged from `outcome.dependencies`
                    // *before* invoking `on_cycle`, so observers triggered by
                    // `on_cycle` only interact with the debounced event loop.
                    let changed_during_pass =
                        dependencies_changed(&self.tracked_paths, &self.fingerprints);
                    if let Some(cb) = self.on_cycle.as_deref_mut() {
                        cb(&outcome);
                    }
                    if !changed_during_pass {
                        break;
                    }
                }
                Err(err) => {
                    // Record current disk state so a transient mid-save missing
                    // entry file does not spin; restoring the file changes its
                    // fingerprint again and triggers recovery.
                    refresh_fingerprints_from_disk(&self.tracked_paths, &mut self.fingerprints);
                    writeln!(self.stderr, "error: {err:#}")
                        .context("failed to write watch error")?;
                    break;
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn execute_watch<'cb>(
    args: &ConvertArgs,
    converter: &dyn Converter,
    channel: WatchChannel,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    on_cycle: Option<&mut (dyn FnMut(&ConvertOutcome) + 'cb)>,
) -> anyhow::Result<()> {
    let initial = execute_convert(args, converter, stdout, stderr)?;

    let out_dir = args.resolved_output_dir();
    let cache_dir = args.resolved_cache_dir(&out_dir);
    let ignore_dirs = collect_ignore_dirs(&out_dir, &cache_dir);

    let mut tracked_paths = BTreeSet::new();
    let mut fingerprints = eldur_cache::Fingerprints::new();
    seed_fingerprints(&initial.dependencies, &mut tracked_paths, &mut fingerprints);

    let fs_tx = channel.tx;
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = fs_tx.send(WatchSignal::Fs(res));
    })
    .context("failed to initialize file-system watcher")?;

    let mut active_watches: BTreeMap<PathBuf, RecursiveMode> = BTreeMap::new();
    sync_watches(&mut watcher, &mut active_watches, &tracked_paths)?;

    let mut state = WatchLoopState {
        args,
        converter,
        watcher,
        active_watches,
        tracked_paths,
        fingerprints,
        stdout,
        stderr,
        on_cycle,
    };

    let changed_during_initial = dependencies_changed(&state.tracked_paths, &state.fingerprints);
    if let Some(cb) = state.on_cycle.as_deref_mut() {
        cb(&initial);
    }

    // If an input was edited while the initial conversion was running, disk
    // already differs from `dep.hash`; catch up immediately so the edit is
    // never lost.
    if changed_during_initial {
        state.run_conversion_and_catch_up()?;
    }

    loop {
        let first_signal = match channel.rx.recv() {
            Ok(sig) => sig,
            Err(_) => return Ok(()),
        };

        let mut saw_watch_error = false;
        match first_signal {
            WatchSignal::Stop => return Ok(()),
            WatchSignal::Fs(Err(err)) => {
                writeln!(state.stderr, "warning[watch]: {err}")
                    .context("failed to write watcher warning")?;
                saw_watch_error = true;
            }
            WatchSignal::Fs(Ok(event)) => {
                if !is_relevant_event(&event, &ignore_dirs, &state.tracked_paths) {
                    continue;
                }
            }
        }

        // Trailing 100 ms debounce over incoming relevant events without polling.
        // Skip waiting if we woke solely on a watcher error (e.g. queue overflow)
        // so dropped events are re-checked immediately.
        if !saw_watch_error {
            let mut deadline = Instant::now() + DEBOUNCE_WINDOW;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                match channel.rx.recv_timeout(deadline - now) {
                    Ok(WatchSignal::Stop) => return Ok(()),
                    Ok(WatchSignal::Fs(Ok(ev))) => {
                        if is_relevant_event(&ev, &ignore_dirs, &state.tracked_paths) {
                            deadline = Instant::now() + DEBOUNCE_WINDOW;
                        }
                    }
                    Ok(WatchSignal::Fs(Err(err))) => {
                        writeln!(state.stderr, "warning[watch]: {err}")
                            .context("failed to write watcher warning")?;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                }
            }
        }

        if !dependencies_changed(&state.tracked_paths, &state.fingerprints) {
            continue;
        }

        state.run_conversion_and_catch_up()?;
    }
}

/// Seeds `fingerprints` with the exact `dep.hash` values that conversion read,
/// never re-reading disk post-conversion (which would swallow concurrent edits
/// made while the converter ran).
fn seed_fingerprints(
    dependencies: &[eldur_cache::Input],
    tracked_paths: &mut BTreeSet<PathBuf>,
    fingerprints: &mut eldur_cache::Fingerprints,
) {
    for dep in dependencies {
        tracked_paths.insert(dep.path.clone());
        let key = dep.path.to_string_lossy();
        if dep.is_missing() {
            fingerprints.record_hash(&key, None);
        } else {
            fingerprints.record_hash(&key, Some(dep.hash));
        }
    }
}

fn refresh_fingerprints_from_disk(
    tracked_paths: &BTreeSet<PathBuf>,
    fingerprints: &mut eldur_cache::Fingerprints,
) {
    for path in tracked_paths {
        let key = path.to_string_lossy();
        let current = fs::read(path).ok();
        fingerprints.record_bytes(&key, current.as_deref());
    }
}

fn dependencies_changed(
    tracked_paths: &BTreeSet<PathBuf>,
    fingerprints: &eldur_cache::Fingerprints,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::thread;

    use eldur_convert::{Input, Options, Output, OutputFile, Resources};

    use super::*;
    use crate::ConvertStatus;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn watcher_error_logs_warning_and_rechecks_fingerprints() {
        let pid = std::process::id();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("eldur_watch_err_{pid}_{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create dir");

        let html = dir.join("index.html");
        let unwatched_dep = dir.join("sub_unwatched");
        // Write index.html in `dir`, and mutate it ONLY after moving/simulating
        // a dropped event via WatchSignal::Fs(Err(...)).
        fs::write(&html, b"<h1>v1</h1>").expect("write html");
        let _ = unwatched_dep;

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = Arc::clone(&calls);
        let conv = move |inp: &Input, _opts: &Options, _res: &mut dyn Resources| -> Output {
            calls_c.fetch_add(1, Ordering::SeqCst);
            Output {
                files: vec![OutputFile {
                    path: PathBuf::from("Main.qml"),
                    contents: format!("Item {{ /* {} */ }}", inp.html.trim()).into_bytes(),
                }],
                diagnostics: Vec::new(),
            }
        };

        let mut args = ConvertArgs::new(&html);
        args.output = Some(dir.join("out_qml"));
        args.quiet = true;

        let (channel, stop) = WatchChannel::new();
        let err_tx = channel.tx.clone();
        let (cycle_tx, cycle_rx) = mpsc::channel::<ConvertStatus>();

        let handle = thread::spawn(move || {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut on_cycle = |outcome: &ConvertOutcome| {
                let _ = cycle_tx.send(outcome.status);
            };
            execute_watch(
                &args,
                &conv,
                channel,
                &mut stdout,
                &mut stderr,
                Some(&mut on_cycle),
            )
            .expect("watch ok");
            String::from_utf8_lossy(&stderr).into_owned()
        });

        assert_eq!(
            cycle_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("initial cycle"),
            ConvertStatus::Converted
        );

        // Inject a synthetic notify queue-overflow error even without any file
        // change first: must log warning[watch] without crashing.
        err_tx
            .send(WatchSignal::Fs(Err(notify::Error::generic(
                "inotify queue overflow",
            ))))
            .expect("send watch error");

        // Now modify the file and immediately inject another overflow error so
        // the immediate fingerprint re-check path executes without waiting for
        // debounce.
        fs::write(&html, b"<h1>v2</h1>").expect("write v2");
        err_tx
            .send(WatchSignal::Fs(Err(notify::Error::generic(
                "inotify queue overflow",
            ))))
            .expect("send second watch error");

        assert_eq!(
            cycle_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("reconvert on error recheck"),
            ConvertStatus::Converted
        );
        assert!(calls.load(Ordering::SeqCst) >= 2);

        stop.stop();
        let stderr_text = handle.join().expect("join watch");
        assert!(
            stderr_text.contains("warning[watch]:"),
            "stderr was: {stderr_text}"
        );
        assert!(stderr_text.contains("inotify queue overflow"));

        let _ = fs::remove_dir_all(&dir);
    }
}
