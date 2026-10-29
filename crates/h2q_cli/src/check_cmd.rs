use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};

/// External Qt executables and timeout used by `html2qml check`.
#[derive(Debug, Clone)]
pub struct QmlTools {
    /// Path or program name for `qmllint`.
    pub qmllint: PathBuf,
    /// Path or program name for `qml6`.
    pub qml6: PathBuf,
    /// Maximum runtime before killing off-screen `qml6`.
    pub qml_timeout: Duration,
}

impl Default for QmlTools {
    fn default() -> Self {
        Self {
            qmllint: PathBuf::from("qmllint"),
            qml6: PathBuf::from("qml6"),
            qml_timeout: Duration::from_secs(3),
        }
    }
}

/// Runs `qmllint` on every `.qml` file under `dir` and off-screen `qml6` on
/// every `Main.qml` under `dir`, writing any diagnostic output to `out`.
///
/// Returns `Ok(true)` when every check succeeded silently, `Ok(false)` when any
/// tool printed output or exited non-zero, and `Err` on I/O or process-spawn
/// failure.
pub fn check_dir_with_tools(
    dir: &Path,
    tools: &QmlTools,
    out: &mut dyn Write,
) -> anyhow::Result<bool> {
    let meta = fs::metadata(dir)
        .with_context(|| format!("failed to access check directory {}", dir.display()))?;
    if !meta.is_dir() {
        bail!("check path {} is not a directory", dir.display());
    }

    let mut qml_files = Vec::new();
    collect_qml_files(dir, &mut qml_files)?;
    qml_files.sort();

    let mut all_clean = true;

    for file in &qml_files {
        let output = Command::new(&tools.qmllint)
            .arg(file)
            .output()
            .with_context(|| {
                format!(
                    "failed to run {} on {}",
                    tools.qmllint.display(),
                    file.display()
                )
            })?;

        let has_output = !output.stdout.is_empty() || !output.stderr.is_empty();
        if !output.status.success() || has_output {
            let combined = combine_streams(&output.stdout, &output.stderr);
            writeln!(out, "qmllint: {}", file.display())
                .context("failed to write qmllint header")?;
            writeln!(out, "{}", trim_trailing_newline(&combined))
                .context("failed to write qmllint output")?;
            all_clean = false;
        }
    }

    for file in &qml_files {
        if file.file_name() != Some(OsStr::new("Main.qml")) {
            continue;
        }
        let (early_status, stdout_bytes, stderr_bytes) = run_qml6_offscreen(file, tools)?;
        let has_output = !stdout_bytes.is_empty() || !stderr_bytes.is_empty();
        let crashed_early = early_status.is_some_and(|st| !st.success());

        if has_output || crashed_early {
            let combined = combine_streams(&stdout_bytes, &stderr_bytes);
            writeln!(out, "qml6: {}", file.display()).context("failed to write qml6 header")?;
            writeln!(out, "{}", trim_trailing_newline(&combined))
                .context("failed to write qml6 output")?;
            all_clean = false;
        }
    }

    Ok(all_clean)
}

fn collect_qml_files(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("failed to read directory {}", dir.display()))?;
    for entry_res in entries {
        let entry = entry_res
            .with_context(|| format!("failed to read directory entry in {}", dir.display()))?;
        let path = entry.path();
        if entry.file_name() == OsStr::new(".h2q-cache") {
            continue;
        }
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_dir() {
            collect_qml_files(&path, out)?;
        } else if path.extension() == Some(OsStr::new("qml")) {
            out.push(path);
        }
    }
    Ok(())
}

fn run_qml6_offscreen(
    main_qml: &Path,
    tools: &QmlTools,
) -> anyhow::Result<(Option<std::process::ExitStatus>, Vec<u8>, Vec<u8>)> {
    let work_dir = main_qml
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = main_qml
        .file_name()
        .unwrap_or_else(|| OsStr::new("Main.qml"));

    let mut child = Command::new(&tools.qml6)
        .current_dir(work_dir)
        .arg(file_name)
        .env("QT_QPA_PLATFORM", "offscreen")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to run {} on {}",
                tools.qml6.display(),
                main_qml.display()
            )
        })?;

    let stdout_handle = child.stdout.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let deadline = Instant::now() + tools.qml_timeout;
    let poll_step = Duration::from_millis(15);
    let early_status = loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed while waiting for {}", tools.qml6.display()))?
        {
            break Some(status);
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep((deadline - now).min(poll_step));
    };

    let stdout_bytes = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr_bytes = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    Ok((early_status, stdout_bytes, stderr_bytes))
}

fn combine_streams(stdout: &[u8], stderr: &[u8]) -> String {
    let mut combined = Vec::with_capacity(stdout.len() + stderr.len() + 1);
    combined.extend_from_slice(stdout);
    if !stdout.is_empty() && !stderr.is_empty() && !stdout.ends_with(b"\n") {
        combined.push(b'\n');
    }
    combined.extend_from_slice(stderr);
    String::from_utf8_lossy(&combined).into_owned()
}

fn trim_trailing_newline(s: &str) -> &str {
    s.strip_suffix("\r\n")
        .or_else(|| s.strip_suffix('\n'))
        .unwrap_or(s)
}
