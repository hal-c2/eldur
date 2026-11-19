//! Qt 6's `qmltestrunner` for the end-to-end tests: where it is, how to start
//! it off-screen, and what to report when it fails before any test ran.
//!
//! Every end-to-end test file has `mod support;`; none uses every item.
#![allow(dead_code)]

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// What a test prints when it cannot run, and what `mise run test` says once.
pub const MISSING: &str = "Qt 6's qmltestrunner not found (set QMLTESTRUNNER, or install \
     qt6-declarative on Arch, qt6-declarative-dev-tools on Debian/Ubuntu, qt on Homebrew)";

/// Qt 6's `qmltestrunner`, found once per test binary.
pub fn runner() -> Option<&'static Path> {
    static RUNNER: OnceLock<Option<PathBuf>> = OnceLock::new();
    RUNNER.get_or_init(find).as_deref()
}

/// True, after printing why, when Qt 6 is not installed: the test returns
/// early, as it has nothing to run its QML in. A skipped test passes, so
/// `mise run test` says up front when these are skipped.
pub fn no_qt() -> bool {
    if runner().is_some() {
        return false;
    }
    eprintln!("skipped: {MISSING}");
    true
}

/// True, after printing why, when a command the test starts besides
/// `qmltestrunner` is not on PATH, as for [`no_qt`].
pub fn missing(commands: &[&str]) -> bool {
    let absent: Vec<&str> = commands
        .iter()
        .copied()
        .filter(|name| on_path(name).is_none())
        .collect();
    if absent.is_empty() {
        return false;
    }
    eprintln!("skipped: not on PATH: {}", absent.join(", "));
    true
}

/// `qmltestrunner`, ready for the caller's arguments. Callers check
/// [`no_qt`] first.
pub fn qmltestrunner() -> Command {
    let mut command = Command::new(runner().expect("no_qt() is checked before running"));
    // A desktop's platform theme (QT_QPA_PLATFORMTHEME=gtk3 is common) loads
    // GTK, which exits with "cannot open display" when there is none, before
    // any test runs; the off-screen platform needs no theme.
    command.env_remove("QT_QPA_PLATFORMTHEME");
    command
}

/// The lines of a run's output that `keep` picks. When the runner never
/// printed its `Totals:` line it stopped before or during the tests (a crash,
/// a missing module, a plugin that would not load), and its whole output is
/// the only account of why, so that is returned too.
pub fn problem_lines(text: &str, keep: impl Fn(&str) -> bool) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| keep(l))
        .map(str::to_owned)
        .collect();
    if !text.lines().any(|l| l.starts_with("Totals:")) {
        lines.push(format!(
            "qmltestrunner stopped before reporting; its output:\n{}",
            text.trim()
        ));
    }
    lines
}

/// Qt 6's `qmllint`: `QMLLINT` when set, else the one beside the
/// `qmltestrunner` found (Arch's plain `qmllint` is Qt 5's, which cannot
/// judge Qt 6 QML).
pub fn qmllint() -> Option<PathBuf> {
    if let Some(set) = env::var_os("QMLLINT").filter(|s| !s.is_empty()) {
        return Some(named(&set, env::var_os("PATH").as_deref()));
    }
    // Without a qmltestrunner, a `qmllint6` on PATH still serves.
    runner()
        .map(|r| r.with_file_name("qmllint"))
        .filter(|beside| beside.is_file())
        .or_else(|| on_path("qmllint6"))
}

fn find() -> Option<PathBuf> {
    if let Some(set) = env::var_os("QMLTESTRUNNER").filter(|s| !s.is_empty()) {
        let path = named(&set, env::var_os("PATH").as_deref());
        assert!(
            path.is_file(),
            "QMLTESTRUNNER is {}, which is not a file",
            path.display()
        );
        return Some(path);
    }
    if let Some(path) = on_path("qmltestrunner6") {
        return Some(path);
    }
    for (tool, args) in [
        ("qtpaths6", ["--query", "QT_INSTALL_BINS"]),
        ("qmake6", ["-query", "QT_INSTALL_BINS"]),
    ] {
        if let Some(bins) = query(tool, &args) {
            let path = Path::new(&bins).join("qmltestrunner");
            if path.is_file() {
                return Some(path);
            }
        }
    }
    // Where distributions and Homebrew put Qt 6's tools when nothing above is
    // on PATH. Arch's /usr/bin/qmltestrunner is Qt 5's, so these come first.
    for dir in [
        "/usr/lib/qt6/bin",
        "/usr/lib64/qt6/bin",
        "/usr/libexec/qt6",
        "/opt/homebrew/opt/qt/bin",
        "/usr/local/opt/qt/bin",
    ] {
        let path = Path::new(dir).join("qmltestrunner");
        if path.is_file() {
            return Some(path);
        }
    }
    // Last, and only when it is known to be Qt 6's: on macOS and in Qt's own
    // installers the plain name is. Qt 5's qtpaths has no --query, but both
    // answer --qt-version; with no answer the runner may be Qt 5's, which
    // fails on Qt 6 imports instead of skipping.
    let plain = on_path("qmltestrunner")?;
    let version = query("qtpaths", &["--qt-version"])?;
    version.starts_with("6.").then_some(plain)
}

/// The tool a variable (QMLTESTRUNNER, QMLLINT) names, as setup finds it
/// (`qt_tool_runs` in mise-tasks/_lib/common.sh): a path as written, a bare
/// name as an executable file in one of `search`'s (PATH's) absolute
/// directories, so never in "." nor a file that cannot run. Absolute, as the tests
/// start it from their own working directories. A bare name PATH lacks stays
/// as written, so the caller's is-a-file check names it.
pub fn named(set: &OsStr, search: Option<&OsStr>) -> PathBuf {
    let written = Path::new(set);
    if written.components().count() == 1
        && !written.has_root()
        && let Some(found) = search.and_then(|dirs| in_dirs(dirs, set))
    {
        return found;
    }
    std::path::absolute(written).expect("the working directory")
}

fn on_path(name: &str) -> Option<PathBuf> {
    in_dirs(&env::var_os("PATH")?, OsStr::new(name))
}

fn in_dirs(dirs: &OsStr, name: &OsStr) -> Option<PathBuf> {
    env::split_paths(dirs)
        // An empty entry means "." to a shell; setup never looks there.
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        // A shell passes over a file it cannot run and goes on down PATH.
        .find(|path| runs(path))
}

#[cfg(unix)]
fn runs(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runs(path: &Path) -> bool {
    path.is_file()
}

fn query(tool: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(tool).args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !text.is_empty()).then_some(text)
}
