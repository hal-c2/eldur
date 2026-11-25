//! `--message-format`: diagnostics and the outcome as lines for a person
//! (the default), or as JSON objects, one per line on stdout, for a program.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use eldur_cli::{
    Cli, Command as CliCommand, ConvertArgs, ConvertStatus, MessageFormat, WatchChannel,
    run_convert_with_io, run_watch_with_channel,
};
use eldur_convert::{Diagnostic, Input, Options, Output, OutputFile, Resources, Severity};
use serde_json::{Value, json};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let pid = std::process::id();
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("eldur_cli_msgfmt_{label}_{pid}_{seq}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// One diagnostic whose source is a URL (it holds `:`, which makes the human
/// line ambiguous) and one with no line.
fn convert_with_diagnostics(_: &Input, _: &Options, _: &mut dyn Resources) -> Output {
    Output {
        files: vec![
            OutputFile {
                path: PathBuf::from("Main.qml"),
                contents: b"import QtQuick\nItem {}\n".to_vec(),
            },
            OutputFile {
                path: PathBuf::from("assets").join("dot.svg"),
                contents: b"<svg/>".to_vec(),
            },
        ],
        diagnostics: vec![
            Diagnostic {
                severity: Severity::Warning,
                stage: "css",
                source: "https://example.com/site.css".to_owned(),
                line: Some(3),
                message: "approximated box-shadow".to_owned(),
            },
            Diagnostic {
                severity: Severity::Info,
                stage: "html",
                source: "page.html".to_owned(),
                line: None,
                message: "ignored <iframe>".to_owned(),
            },
        ],
    }
}

fn parse_args(argv: &[&str]) -> ConvertArgs {
    let cli = Cli::try_parse_from(argv).expect("parse args");
    match cli.command {
        CliCommand::Convert(args) | CliCommand::Watch(args) => args,
        CliCommand::Check(_) => panic!("expected convert or watch args"),
    }
}

fn json_lines(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}")))
        .collect()
}

fn expected_diagnostics() -> [Value; 2] {
    [
        json!({
            "type": "diagnostic",
            "severity": "warning",
            "stage": "css",
            "source": "https://example.com/site.css",
            "line": 3,
            "message": "approximated box-shadow",
        }),
        // No `line` key at all when the line is unknown.
        json!({
            "type": "diagnostic",
            "severity": "info",
            "stage": "html",
            "source": "page.html",
            "message": "ignored <iframe>",
        }),
    ]
}

fn expected_done(status: &str, out_dir: &std::path::Path) -> Value {
    json!({
        "type": "done",
        "status": status,
        "output": out_dir.to_str().expect("utf8 path"),
        "files": ["Main.qml", "assets/dot.svg"],
        "main": "Main.qml",
    })
}

#[test]
fn json_reports_each_diagnostic_and_the_outcome_and_a_cache_hit_replays_the_same_fields() {
    let td = TempDir::new("json_hit");
    let html = td.path.join("page.html");
    fs::write(&html, b"<h1>Hi</h1>").expect("write html");
    let out_dir = td.path.join("out");
    let args = parse_args(&[
        "eldur",
        "convert",
        html.to_str().expect("utf8"),
        "-o",
        out_dir.to_str().expect("utf8"),
        "--message-format",
        "json",
    ]);
    assert_eq!(args.message_format, MessageFormat::Json);

    for status in ["converted", "cached"] {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let outcome = run_convert_with_io(&args, &convert_with_diagnostics, &mut out, &mut err)
            .expect("convert");
        assert_eq!(
            outcome.status,
            if status == "cached" {
                ConvertStatus::Cached
            } else {
                ConvertStatus::Converted
            }
        );
        // stdout is the whole report: nothing a program would have to skip.
        assert!(
            err.is_empty(),
            "stderr was: {}",
            String::from_utf8_lossy(&err)
        );
        let [first, second] = expected_diagnostics();
        assert_eq!(
            json_lines(&out),
            vec![first, second, expected_done(status, &out_dir)],
            "{status} run"
        );
        // A reader can branch on the first key it sees.
        for line in String::from_utf8_lossy(&out).lines() {
            assert!(line.starts_with(r#"{"type":""#), "line was: {line}");
        }
    }
}

#[test]
fn human_is_the_default_and_keeps_its_lines_on_a_cache_hit() {
    let td = TempDir::new("human");
    let html = td.path.join("page.html");
    fs::write(&html, b"<h1>Hi</h1>").expect("write html");
    let args = parse_args(&["eldur", "convert", html.to_str().expect("utf8")]);
    assert_eq!(args.message_format, MessageFormat::Human);
    assert_eq!(ConvertArgs::new(&html).message_format, MessageFormat::Human);

    for prefix in ["converted ", "cached "] {
        let mut out = Vec::new();
        let mut err = Vec::new();
        run_convert_with_io(&args, &convert_with_diagnostics, &mut out, &mut err).expect("convert");
        assert_eq!(
            String::from_utf8_lossy(&err),
            "warning[css]: https://example.com/site.css:3: approximated box-shadow\n\
             info[html]: page.html: ignored <iframe>\n"
        );
        let stdout = String::from_utf8_lossy(&out);
        assert!(stdout.starts_with(prefix), "stdout was: {stdout}");
        assert_eq!(stdout.lines().count(), 1);
    }
}

#[test]
fn json_quiet_still_reports_the_outcome() {
    let td = TempDir::new("quiet");
    let html = td.path.join("page.html");
    fs::write(&html, b"<p/>").expect("write html");
    let out_dir = td.path.join("out");
    let args = parse_args(&[
        "eldur",
        "convert",
        html.to_str().expect("utf8"),
        "-o",
        out_dir.to_str().expect("utf8"),
        "-q",
        "--message-format=json",
    ]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    run_convert_with_io(&args, &convert_with_diagnostics, &mut out, &mut err).expect("convert");
    let lines = json_lines(&out);
    assert_eq!(lines.last(), Some(&expected_done("converted", &out_dir)));
}

#[test]
fn json_failure_is_an_error_object_on_stdout() {
    let td = TempDir::new("json_err");
    let missing = td.path.join("missing.html");
    let mut args = ConvertArgs::new(&missing);
    args.message_format = MessageFormat::Json;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let result = run_convert_with_io(&args, &convert_with_diagnostics, &mut out, &mut err);
    assert!(result.is_err());
    assert!(
        err.is_empty(),
        "stderr was: {}",
        String::from_utf8_lossy(&err)
    );
    let lines = json_lines(&out);
    assert_eq!(
        lines.len(),
        1,
        "stdout was: {}",
        String::from_utf8_lossy(&out)
    );
    assert_eq!(lines[0]["type"], "error");
    let message = lines[0]["message"].as_str().expect("message is a string");
    assert!(
        message.contains("failed to resolve input file") && message.contains("missing.html"),
        "message was: {message}"
    );
    assert_eq!(
        lines[0].as_object().map(serde_json::Map::len),
        Some(2),
        "only type and message: {}",
        lines[0]
    );
}

#[test]
fn binary_takes_message_format_and_exits_non_zero_with_a_json_error() {
    let exe = env!("CARGO_BIN_EXE_eldur");
    let td = TempDir::new("bin");

    for sub in ["convert", "watch"] {
        let help = Command::new(exe)
            .args([sub, "--help"])
            .output()
            .expect("help");
        assert!(
            String::from_utf8_lossy(&help.stdout).contains("--message-format"),
            "{sub} --help lacks --message-format"
        );
    }

    let bad = Command::new(exe)
        .args(["convert", "page.html", "--message-format", "xml"])
        .output()
        .expect("bad format");
    assert!(!bad.status.success());

    for sub in ["convert", "watch"] {
        let missing = td.path.join("missing.html");
        let run = Command::new(exe)
            .args([sub, missing.to_str().expect("utf8")])
            .args(["--message-format", "json"])
            .output()
            .expect("run");
        assert!(!run.status.success(), "{sub} should fail");
        assert!(
            run.stderr.is_empty(),
            "{sub} stderr was: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let lines = json_lines(&run.stdout);
        assert_eq!(lines.len(), 1, "{sub} stdout: {lines:?}");
        assert_eq!(lines[0]["type"], "error", "{sub}");
    }
}

/// A `Write` the test reads while the watch loop, on its own thread, writes.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl Shared {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("lock")).into_owned()
    }
}

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn watch_reports_every_cycle_and_a_failed_cycle_as_json() {
    let td = TempDir::new("watch");
    let html = td.path.join("index.html");
    fs::write(&html, b"<h1>v1</h1>").expect("write html");
    let out_dir = td.path.join("out");
    let mut args = ConvertArgs::new(&html);
    args.output = Some(out_dir.clone());
    args.message_format = MessageFormat::Json;

    let (channel, stop) = WatchChannel::new();
    let (cycle_tx, cycle_rx) = mpsc::channel::<ConvertStatus>();
    let stdout = Shared::default();
    let stderr = Shared::default();
    let (mut out_w, mut err_w) = (stdout.clone(), stderr.clone());
    let handle = thread::spawn(move || {
        let mut on_cycle = |outcome: &eldur_cli::ConvertOutcome| {
            let _ = cycle_tx.send(outcome.status);
        };
        run_watch_with_channel(
            &args,
            &convert_with_diagnostics,
            channel,
            &mut out_w,
            &mut err_w,
            Some(&mut on_cycle),
        )
    });

    cycle_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("initial cycle");
    let [first, second] = expected_diagnostics();
    assert_eq!(
        json_lines(stdout.text().as_bytes()),
        vec![first, second, expected_done("converted", &out_dir)]
    );

    // A cycle that fails reports it and the watch goes on.
    fs::remove_file(&html).expect("remove input");
    let deadline = Instant::now() + Duration::from_secs(5);
    let error = loop {
        let lines = json_lines(stdout.text().as_bytes());
        if let Some(error) = lines.iter().find(|l| l["type"] == "error") {
            break error.clone();
        }
        assert!(
            Instant::now() < deadline,
            "no error line; stdout was: {}",
            stdout.text()
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|m| m.contains("index.html")),
        "error was: {error}"
    );

    stop.stop();
    handle.join().expect("join").expect("watch ends cleanly");
    assert!(
        !stderr.text().contains("error"),
        "stderr was: {}",
        stderr.text()
    );
}
