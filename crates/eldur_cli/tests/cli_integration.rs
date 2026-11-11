use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime};

use clap::Parser;
use eldur_cli::{
    Cli, Command as CliCommand, ConvertArgs, ConvertStatus, Converter, QmlTools, WatchChannel,
    check_dir_with_tools, run_convert_with_io, run_watch_with_channel,
};
use eldur_convert::{
    Diagnostic, Input, Options, Output, OutputFile, Resources, RootKind, Severity,
};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let pid = std::process::id();
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("eldur_cli_it_{label}_{pid}_{seq}"));
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

struct FakeConverter {
    calls: AtomicUsize,
    last_options: Mutex<Option<Options>>,
}

impl FakeConverter {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            last_options: Mutex::new(None),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Converter for FakeConverter {
    fn convert(&self, input: &Input, options: &Options, resources: &mut dyn Resources) -> Output {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last_options.lock().expect("lock options") = Some(options.clone());

        let style = resources
            .read("style.css", &input.url)
            .map(|r| String::from_utf8_lossy(&r.bytes).into_owned())
            .unwrap_or_else(|| "/* no style.css */".to_owned());

        let extra = resources
            .read("extra.css", &input.url)
            .map(|r| String::from_utf8_lossy(&r.bytes).into_owned())
            .unwrap_or_else(|| "/* no extra.css */".to_owned());

        let root_type = match options.root {
            RootKind::Window => "ApplicationWindow",
            RootKind::Item => "Item",
        };

        let qml = format!(
            "import QtQuick\n{root_type} {{\n    // html: {}\n    // css: {}\n    // extra: {}\n    width: {}\n    height: {}\n}}\n",
            input.html.trim(),
            style.trim(),
            extra.trim(),
            options.viewport.width,
            options.viewport.height,
        );

        Output {
            files: vec![
                OutputFile {
                    path: PathBuf::from("Main.qml"),
                    contents: qml.into_bytes(),
                },
                OutputFile {
                    path: PathBuf::from("assets/dot.svg"),
                    contents: b"<svg/>".to_vec(),
                },
            ],
            diagnostics: vec![Diagnostic {
                severity: Severity::Warning,
                stage: "css",
                source: "style.css".to_owned(),
                line: Some(4),
                message: "approximated box-shadow".to_owned(),
            }],
        }
    }
}

fn parse_convert_args(argv: &[&str]) -> ConvertArgs {
    let cli = Cli::try_parse_from(argv).expect("parse convert args");
    match cli.command {
        CliCommand::Convert(args) | CliCommand::Watch(args) => args,
        CliCommand::Check(_) => panic!("expected convert or watch args"),
    }
}

#[test]
fn convert_cache_hit_replays_diagnostics_and_skips_identical_files() {
    let td = TempDir::new("hit_skip");
    let html = td.path.join("page.html");
    let css = td.path.join("style.css");
    fs::write(&html, b"<h1>Hello</h1>").expect("write html");
    fs::write(&css, b"h1 { color: blue; }").expect("write css");

    let converter = FakeConverter::new();
    let args = ConvertArgs::new(&html);

    let mut out1 = Vec::new();
    let mut err1 = Vec::new();
    let res1 = run_convert_with_io(&args, &converter, &mut out1, &mut err1).expect("first convert");

    assert_eq!(res1.status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 1);
    assert_eq!(res1.written_files, 2);
    assert_eq!(res1.skipped_files, 0);
    assert_eq!(res1.output_dir, td.path.join("page_qml"));
    assert!(
        String::from_utf8_lossy(&out1).starts_with("converted "),
        "stdout was: {}",
        String::from_utf8_lossy(&out1)
    );
    assert_eq!(
        String::from_utf8_lossy(&err1).trim(),
        "warning[css]: style.css:4: approximated box-shadow"
    );

    // Backdate Main.qml mtime so we can prove an identical-content cache hit
    // does not rewrite or touch the file on disk.
    let main_qml = res1.output_dir.join("Main.qml");
    let backdated = SystemTime::now() - Duration::from_secs(600);
    fs::File::options()
        .write(true)
        .open(&main_qml)
        .expect("open Main.qml")
        .set_modified(backdated)
        .expect("backdate Main.qml");
    let mtime_before = fs::metadata(&main_qml)
        .expect("stat Main.qml")
        .modified()
        .expect("mtime");

    let mut out2 = Vec::new();
    let mut err2 = Vec::new();
    let res2 =
        run_convert_with_io(&args, &converter, &mut out2, &mut err2).expect("second convert");

    assert_eq!(res2.status, ConvertStatus::Cached);
    assert_eq!(converter.call_count(), 1, "converter must not run on hit");
    assert_eq!(res2.written_files, 0);
    assert_eq!(res2.skipped_files, 2);
    assert!(String::from_utf8_lossy(&out2).starts_with("cached "));
    assert_eq!(
        String::from_utf8_lossy(&err2).trim(),
        "warning[css]: style.css:4: approximated box-shadow"
    );

    let mtime_after = fs::metadata(&main_qml)
        .expect("stat Main.qml after")
        .modified()
        .expect("mtime after");
    assert_eq!(mtime_before, mtime_after);

    // Deleting an output file while the cache remains valid restores it from
    // the cache without re-running the converter.
    fs::remove_file(&main_qml).expect("remove Main.qml");
    let mut out3 = Vec::new();
    let mut err3 = Vec::new();
    let res3 = run_convert_with_io(&args, &converter, &mut out3, &mut err3).expect("third convert");
    assert_eq!(res3.status, ConvertStatus::Cached);
    assert_eq!(converter.call_count(), 1);
    assert_eq!(res3.written_files, 1);
    assert_eq!(res3.skipped_files, 1);
    assert!(main_qml.is_file());
}

#[test]
fn creating_previously_missing_dependency_invalidates_cache() {
    let td = TempDir::new("missing_dep");
    let html = td.path.join("index.html");
    let css = td.path.join("style.css");
    let extra_css = td.path.join("extra.css");
    fs::write(&html, b"<p>Card</p>").expect("write html");
    fs::write(&css, b"p { margin: 8px; }").expect("write css");

    let converter = FakeConverter::new();
    let args = ConvertArgs::new(&html);

    let mut out = Vec::new();
    let mut err = Vec::new();
    let r1 = run_convert_with_io(&args, &converter, &mut out, &mut err).expect("run 1");
    assert_eq!(r1.status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 1);

    // Still missing -> cache hit.
    let r2 = run_convert_with_io(&args, &converter, &mut out, &mut err).expect("run 2");
    assert_eq!(r2.status, ConvertStatus::Cached);
    assert_eq!(converter.call_count(), 1);

    // Create extra.css -> must invalidate cache and re-convert!
    fs::write(&extra_css, b"p { color: green; }").expect("create extra.css");
    let r3 = run_convert_with_io(&args, &converter, &mut out, &mut err).expect("run 3");
    assert_eq!(r3.status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 2);

    let qml_text =
        fs::read_to_string(r3.output_dir.join("Main.qml")).expect("read updated Main.qml");
    assert!(qml_text.contains("p { color: green; }"));

    // Subsequent run with extra.css unchanged -> cache hit again.
    let r4 = run_convert_with_io(&args, &converter, &mut out, &mut err).expect("run 4");
    assert_eq!(r4.status, ConvertStatus::Cached);
    assert_eq!(converter.call_count(), 2);
}

#[test]
fn cli_flags_control_options_cache_dir_no_cache_and_quiet() {
    let td = TempDir::new("flags");
    let html = td.path.join("app.html");
    fs::write(&html, b"<main/>").expect("write html");

    let shared_cache = td.path.join("shared-cache");
    let out_a = td.path.join("out_a");
    let out_b = td.path.join("out_b");

    let converter = FakeConverter::new();

    let args_a = parse_convert_args(&[
        "eldur",
        "convert",
        html.to_str().expect("utf8 path"),
        "-o",
        out_a.to_str().expect("utf8"),
        "--viewport",
        "390x844",
        "--root",
        "item",
        "--no-comments",
        "--no-states",
        "--no-animations",
        "--cache-dir",
        shared_cache.to_str().expect("utf8"),
        "--quiet",
    ]);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let r_a = run_convert_with_io(&args_a, &converter, &mut stdout, &mut stderr).expect("run a");
    assert_eq!(r_a.status, ConvertStatus::Converted);
    assert!(
        stdout.is_empty(),
        "--quiet must suppress status line on stdout"
    );
    assert!(
        !stderr.is_empty(),
        "diagnostics must still be emitted to stderr"
    );

    let recorded_opts = converter
        .last_options
        .lock()
        .expect("lock")
        .clone()
        .expect("options");
    assert_eq!(recorded_opts.viewport.width, 390.0);
    assert_eq!(recorded_opts.viewport.height, 844.0);
    assert_eq!(recorded_opts.root, RootKind::Item);
    assert!(!recorded_opts.source_comments);
    assert!(!recorded_opts.state_variants);
    assert!(!recorded_opts.animations);

    // Same options + shared --cache-dir into a fresh -o out_b hits the cache!
    let mut args_b = args_a.clone();
    args_b.output = Some(out_b.clone());
    let r_b = run_convert_with_io(&args_b, &converter, &mut stdout, &mut stderr).expect("run b");
    assert_eq!(r_b.status, ConvertStatus::Cached);
    assert_eq!(converter.call_count(), 1);
    assert!(out_b.join("Main.qml").is_file());

    // Changing an option (--root window) misses the cache.
    let mut args_window = args_b.clone();
    args_window.root = eldur_cli::RootArg::Window;
    let r_win =
        run_convert_with_io(&args_window, &converter, &mut stdout, &mut stderr).expect("run win");
    assert_eq!(r_win.status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 2);

    // --no-cache always runs the converter and does not create default .eldur-cache.
    let out_nc = td.path.join("out_nocache");
    let args_nc = parse_convert_args(&[
        "eldur",
        "convert",
        html.to_str().expect("utf8"),
        "-o",
        out_nc.to_str().expect("utf8"),
        "--no-cache",
    ]);
    let r_nc =
        run_convert_with_io(&args_nc, &converter, &mut stdout, &mut stderr).expect("run nocache");
    assert_eq!(r_nc.status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 3);
    assert!(!out_nc.join(".eldur-cache").exists());
}

#[test]
fn convert_io_errors_include_offending_path() {
    let td = TempDir::new("io_err");
    let missing = td.path.join("does_not_exist.html");
    let converter = FakeConverter::new();
    let mut out = Vec::new();
    let mut err = Vec::new();

    let missing_err =
        run_convert_with_io(&ConvertArgs::new(&missing), &converter, &mut out, &mut err)
            .expect_err("missing input must error");
    assert!(format!("{missing_err:#}").contains("does_not_exist.html"));

    let dir_err = run_convert_with_io(&ConvertArgs::new(&td.path), &converter, &mut out, &mut err)
        .expect_err("directory input must error");
    assert!(format!("{dir_err:#}").contains("is a directory"));
}

#[test]
fn convert_rejects_output_paths_escaping_output_dir() {
    let td = TempDir::new("escape");
    let html = td.path.join("index.html");
    fs::write(&html, b"<p>hi</p>").expect("write html");

    let parent_target = td.path.join("escaped.qml");
    let abs_target = td.path.join("abs_escaped.qml");

    for bad_path in [PathBuf::from("../escaped.qml"), abs_target.clone()] {
        let out_dir = td.path.join("sub_out");
        let mut args = ConvertArgs::new(&html);
        args.output = Some(out_dir.clone());
        args.no_cache = true;

        let bad_conv = move |_inp: &Input, _opts: &Options, _res: &mut dyn Resources| Output {
            files: vec![OutputFile {
                path: bad_path.clone(),
                contents: b"Item {}".to_vec(),
            }],
            diagnostics: Vec::new(),
        };

        let mut out = Vec::new();
        let mut err = Vec::new();
        let res = run_convert_with_io(&args, &bad_conv, &mut out, &mut err);
        assert!(res.is_err(), "expected escaping output path to fail");
        assert!(!parent_target.exists());
        assert!(!abs_target.exists());
    }
}

#[test]
fn watch_catches_edit_made_mid_conversion() {
    let td = TempDir::new("watch_mid_edit");
    let html = td.path.join("index.html");
    let css = td.path.join("style.css");
    let out_dir = td.path.join("out_qml");
    fs::write(&html, b"<div>card</div>").expect("write html");
    fs::write(&css, b"div { color: red; }").expect("write v1 css");

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_conv = Arc::clone(&calls);
    let css_for_conv = css.clone();

    // Simulate a user saving `style.css` *after* the converter read v1 during
    // conversion #1, before conversion #1 finishes and seeds watch baselines.
    let mid_edit_converter =
        move |input: &Input, _opts: &Options, resources: &mut dyn Resources| -> Output {
            let n = calls_conv.fetch_add(1, Ordering::SeqCst);
            let style = resources
                .read("style.css", &input.url)
                .map(|r| String::from_utf8_lossy(&r.bytes).into_owned())
                .unwrap_or_default();

            if n == 0 {
                fs::write(&css_for_conv, b"div { color: lime; }")
                    .expect("concurrent edit during initial conversion");
            } else if n == 2 {
                fs::write(&css_for_conv, b"div { color: gold; }")
                    .expect("concurrent edit during event-triggered conversion");
            }

            Output {
                files: vec![OutputFile {
                    path: PathBuf::from("Main.qml"),
                    contents: format!("Item {{ /* {} */ }}\n", style.trim()).into_bytes(),
                }],
                diagnostics: Vec::new(),
            }
        };

    let mut args = ConvertArgs::new(&html);
    args.output = Some(out_dir.clone());
    args.quiet = true;

    let (channel, stop) = WatchChannel::new();
    let (cycle_tx, cycle_rx) = mpsc::channel::<ConvertStatus>();

    let handle = thread::spawn(move || {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut on_cycle = |outcome: &eldur_cli::ConvertOutcome| {
            let _ = cycle_tx.send(outcome.status);
        };
        run_watch_with_channel(
            &args,
            &mid_edit_converter,
            channel,
            &mut stdout,
            &mut stderr,
            Some(&mut on_cycle),
        )
    });

    // Cycle 1 (initial) and Cycle 2 (immediate catch-up for the mid-convert
    // edit) must both execute without requiring any further edits!
    assert_eq!(
        cycle_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("initial cycle"),
        ConvertStatus::Converted
    );
    assert_eq!(
        cycle_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("catch-up cycle for mid-conversion edit"),
        ConvertStatus::Converted
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let qml_after_initial_catchup =
        fs::read_to_string(out_dir.join("Main.qml")).expect("read Main.qml");
    assert!(
        qml_after_initial_catchup.contains("div { color: lime; }"),
        "Main.qml remained stale after initial catch-up: {qml_after_initial_catchup}"
    );

    // Trigger an event-driven conversion (n == 2), during which the converter
    // performs another concurrent edit (`gold`); Cycle 3 and Cycle 4 must both run.
    fs::write(&css, b"div { color: navy; }").expect("write v3 css");
    assert_eq!(
        cycle_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("event-triggered cycle"),
        ConvertStatus::Converted
    );
    assert_eq!(
        cycle_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("catch-up cycle after mid-conversion edit in watch loop"),
        ConvertStatus::Converted
    );
    assert_eq!(calls.load(Ordering::SeqCst), 4);

    let final_qml = fs::read_to_string(out_dir.join("Main.qml")).expect("read Main.qml");
    assert!(
        final_qml.contains("div { color: gold; }"),
        "Main.qml remained stale after watch-loop catch-up: {final_qml}"
    );

    // Ensure no stray atomic-write temp files remain in out_dir.
    for entry in fs::read_dir(&out_dir).expect("read out_dir") {
        let name = entry.expect("dir entry").file_name();
        assert!(
            !name.to_string_lossy().contains(".eldur-tmp."),
            "stray temp file left behind: {name:?}"
        );
    }

    stop.stop();
    handle.join().expect("join").expect("watch ok");
}

#[test]
fn watch_ignores_same_content_writes_and_reconverts_on_real_changes() {
    let td = TempDir::new("watch");
    let html = td.path.join("index.html");
    let css = td.path.join("style.css");
    let extra = td.path.join("extra.css");
    let out_dir = td.path.join("out_qml");
    fs::write(&html, b"<div>v1</div>").expect("write html");
    fs::write(&css, b"div { color: red; }").expect("write css");

    let converter = Arc::new(FakeConverter::new());
    let mut args = ConvertArgs::new(&html);
    args.output = Some(out_dir.clone());
    args.quiet = true;

    let (channel, stop) = WatchChannel::new();
    let (cycle_tx, cycle_rx) = mpsc::channel::<ConvertStatus>();

    let converter_bg = Arc::clone(&converter);
    let handle = thread::spawn(move || {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut on_cycle = |outcome: &eldur_cli::ConvertOutcome| {
            let _ = cycle_tx.send(outcome.status);
        };
        run_watch_with_channel(
            &args,
            converter_bg.as_ref(),
            channel,
            &mut stdout,
            &mut stderr,
            Some(&mut on_cycle),
        )
    });

    // Wait for initial conversion + watch registration.
    let first_status = cycle_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("initial watch cycle");
    assert_eq!(first_status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 1);

    // Re-save style.css with identical content: Fingerprints must suppress reconversion.
    fs::write(&css, b"div { color: red; }").expect("rewrite identical css");
    assert!(
        cycle_rx.recv_timeout(Duration::from_millis(260)).is_err(),
        "identical save must not trigger a re-conversion"
    );
    assert_eq!(converter.call_count(), 1);

    // Modify style.css with new content: must debounce and re-convert.
    fs::write(&css, b"div { color: purple; }").expect("edit css");
    let second_status = cycle_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("second watch cycle after real edit");
    assert_eq!(second_status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 2);
    let qml_after_edit = fs::read_to_string(out_dir.join("Main.qml")).expect("read Main.qml");
    assert!(qml_after_edit.contains("div { color: purple; }"));

    // Create previously missing extra.css: must also trigger re-conversion.
    fs::write(&extra, b"div { padding: 4px; }").expect("create extra.css");
    let third_status = cycle_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("third watch cycle after creating missing dependency");
    assert_eq!(third_status, ConvertStatus::Converted);
    assert_eq!(converter.call_count(), 3);
    let qml_after_extra = fs::read_to_string(out_dir.join("Main.qml")).expect("read Main.qml");
    assert!(qml_after_extra.contains("div { padding: 4px; }"));

    stop.stop();
    handle
        .join()
        .expect("join watch thread")
        .expect("watch result");
}

#[test]
fn watch_started_on_warm_cache_still_watches_nested_dependencies() {
    let td = TempDir::new("watch_warm");
    let pages_dir = td.path.join("pages");
    let styles_dir = td.path.join("styles");
    fs::create_dir_all(&pages_dir).expect("create pages");
    fs::create_dir_all(&styles_dir).expect("create styles");

    let html = pages_dir.join("index.html");
    let nested_css = styles_dir.join("tokens.css");
    let out_dir = td.path.join("dist_qml");
    fs::write(&html, b"<section/>").expect("write html");
    fs::write(&nested_css, b":root { color: black; }").expect("write tokens.css");

    // Chained converter reading ../styles/tokens.css relative to pages/index.html.
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_conv = Arc::clone(&calls);
    let chained = move |input: &Input, _opts: &Options, res: &mut dyn Resources| -> Output {
        calls_conv.fetch_add(1, Ordering::SeqCst);
        let tokens = res
            .read("../styles/tokens.css", &input.url)
            .map(|r| String::from_utf8_lossy(&r.bytes).into_owned())
            .unwrap_or_default();
        Output {
            files: vec![OutputFile {
                path: PathBuf::from("Main.qml"),
                contents: format!("Item {{ /* {tokens} */ }}\n").into_bytes(),
            }],
            diagnostics: Vec::new(),
        }
    };

    let mut args = ConvertArgs::new(&html);
    args.output = Some(out_dir.clone());
    args.quiet = true;

    // Warm the cache before starting `watch`.
    let mut out = Vec::new();
    let mut err = Vec::new();
    let warm = run_convert_with_io(&args, &chained, &mut out, &mut err).expect("warm convert");
    assert_eq!(warm.status, ConvertStatus::Converted);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let (channel, stop) = WatchChannel::new();
    let (cycle_tx, cycle_rx) = mpsc::channel::<ConvertStatus>();
    let handle = thread::spawn(move || {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut on_cycle = |outcome: &eldur_cli::ConvertOutcome| {
            let _ = cycle_tx.send(outcome.status);
        };
        run_watch_with_channel(
            &args,
            &chained,
            channel,
            &mut stdout,
            &mut stderr,
            Some(&mut on_cycle),
        )
    });

    // Initial watch run hits the warm cache.
    assert_eq!(
        cycle_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("warm initial cycle"),
        ConvertStatus::Cached
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Editing ../styles/tokens.css in the sibling directory must still wake watch!
    fs::write(&nested_css, b":root { color: white; }").expect("update tokens.css");
    assert_eq!(
        cycle_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reconvert after sibling dir edit"),
        ConvertStatus::Converted
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        fs::read_to_string(out_dir.join("Main.qml"))
            .expect("read Main.qml")
            .contains("white")
    );

    stop.stop();
    handle.join().expect("join").expect("watch ok");
}

fn write_executable(path: &Path, script: &str) {
    // Write to a temporary sibling and rename so the file descriptor is closed
    // before `execve` runs (avoids ETXTBSY under parallel test execution).
    let tmp = path.with_extension("sh.tmp");
    fs::write(&tmp, script).expect("write script");
    let mut perms = fs::metadata(&tmp).expect("stat script").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&tmp, perms).expect("chmod +x");
    fs::rename(&tmp, path).expect("rename script");
}

#[test]
fn check_runs_qmllint_and_offscreen_qml6_failing_on_any_output() {
    let td = TempDir::new("check");
    let qml_dir = td.path.join("app_qml");
    let bin_dir = td.path.join("mock_bin");
    fs::create_dir_all(&qml_dir).expect("create qml_dir");
    fs::create_dir_all(&bin_dir).expect("create bin_dir");

    fs::write(qml_dir.join("Main.qml"), b"import QtQuick\nItem {}\n").expect("write Main.qml");
    fs::write(qml_dir.join("Card.qml"), b"import QtQuick\nRectangle {}\n").expect("write Card.qml");

    let mock_qmllint = bin_dir.join("mock_qmllint");
    let mock_qml6 = bin_dir.join("mock_qml6");

    // Clean run: qmllint silent, qml6 verifies QT_QPA_PLATFORM=offscreen and
    // sleeps until killed by timeout without printing anything.
    write_executable(&mock_qmllint, "#!/usr/bin/env bash\nexit 0\n");
    write_executable(
        &mock_qml6,
        "#!/usr/bin/env bash\nif [[ \"${QT_QPA_PLATFORM:-}\" != \"offscreen\" ]]; then\n  echo \"missing offscreen\"\n  exit 1\nfi\nif [[ \"$1\" != \"Main.qml\" || ! -f \"Main.qml\" ]]; then\n  echo \"wrong cwd or arg: $1\"\n  exit 1\nfi\nexec sleep 5\n",
    );

    let tools = QmlTools {
        qmllint: mock_qmllint.clone(),
        qml6: mock_qml6.clone(),
        qml_timeout: Duration::from_millis(80),
    };

    let mut report = Vec::new();
    let ok = check_dir_with_tools(&qml_dir, &tools, &mut report).expect("clean check");
    assert!(ok, "report was: {}", String::from_utf8_lossy(&report));
    assert!(report.is_empty());

    // Any warning printed by qmllint or qml6 fails the check.
    write_executable(
        &mock_qmllint,
        "#!/usr/bin/env bash\nif [[ \"$1\" == *Card.qml ]]; then\n  echo \"Warning: unqualified access\"\nfi\nexit 0\n",
    );
    write_executable(
        &mock_qml6,
        "#!/usr/bin/env bash\necho \"Main.qml:2: ReferenceError: foo is not defined\" >&2\nexec sleep 5\n",
    );

    let mut fail_report = Vec::new();
    let ok_fail =
        check_dir_with_tools(&qml_dir, &tools, &mut fail_report).expect("check with warnings");
    assert!(!ok_fail);
    let text = String::from_utf8_lossy(&fail_report);
    assert!(text.contains("qmllint:"));
    assert!(text.contains("Warning: unqualified access"));
    assert!(text.contains("qml6:"));
    assert!(text.contains("ReferenceError: foo is not defined"));
}

/// Qt sends `console.*` and uncaught script errors to the systemd journal when
/// stderr is not a terminal, so a page that throws at load prints nothing to a
/// piped qml6 unless the check asks for stderr logging.
#[test]
fn check_asks_qml6_for_stderr_logging_so_script_errors_reach_the_report() {
    let td = TempDir::new("check-stderr");
    let qml_dir = td.path.join("app_qml");
    let bin_dir = td.path.join("mock_bin");
    fs::create_dir_all(&qml_dir).expect("create qml_dir");
    fs::create_dir_all(&bin_dir).expect("create bin_dir");
    fs::write(qml_dir.join("Main.qml"), b"import QtQuick\nItem {}\n").expect("write Main.qml");

    let mock_qmllint = bin_dir.join("mock_qmllint");
    let mock_qml6 = bin_dir.join("mock_qml6");
    write_executable(&mock_qmllint, "#!/usr/bin/env bash\nexit 0\n");
    // Behaves as Qt does with a piped stderr: the error goes to the journal
    // unless QT_FORCE_STDERR_LOGGING is set.
    write_executable(
        &mock_qml6,
        "#!/usr/bin/env bash\nif [[ \"${QT_FORCE_STDERR_LOGGING:-}\" == \"1\" ]]; then\n  echo \"page.js:3: ReferenceError: HTMLElement is not defined\" >&2\nfi\nexec sleep 5\n",
    );

    let tools = QmlTools {
        qmllint: mock_qmllint,
        qml6: mock_qml6,
        qml_timeout: Duration::from_millis(80),
    };

    let mut report = Vec::new();
    let ok = check_dir_with_tools(&qml_dir, &tools, &mut report).expect("check");
    let text = String::from_utf8_lossy(&report);
    assert!(!ok, "a page that throws at load must fail the check");
    assert!(
        text.contains("ReferenceError: HTMLElement is not defined"),
        "report was: {text}"
    );

    // A page's own console.log is not a fault: Qt prints it in the `qml`
    // category at debug level, which the check turns off.
    write_executable(
        &tools.qml6,
        "#!/usr/bin/env bash\nif [[ \"${QT_LOGGING_RULES:-}\" != *qml.debug=false* ]]; then\n  echo \"qml: page ready\" >&2\nfi\nexec sleep 5\n",
    );
    let mut quiet = Vec::new();
    let ok = check_dir_with_tools(&qml_dir, &tools, &mut quiet).expect("check");
    assert!(ok, "report was: {}", String::from_utf8_lossy(&quiet));

    // Nor is the host's accessibility bus: where its socket refuses
    // connections, GLib's at-spi bridge warns unless it is turned off.
    write_executable(
        &tools.qml6,
        "#!/usr/bin/env bash\nif [[ \"${NO_AT_BRIDGE:-}\" != \"1\" ]]; then\n  echo \"atspi-WARNING **: Couldn't connect to accessibility bus\" >&2\nfi\nexec sleep 5\n",
    );
    let mut no_bus = Vec::new();
    let ok = check_dir_with_tools(&qml_dir, &tools, &mut no_bus).expect("check");
    assert!(ok, "report was: {}", String::from_utf8_lossy(&no_bus));
}

#[test]
fn binary_help_and_argument_errors() {
    let exe = env!("CARGO_BIN_EXE_eldur");

    for sub in [
        &["--help"][..],
        &["convert", "--help"],
        &["watch", "--help"],
        &["check", "--help"],
    ] {
        let out = Command::new(exe).args(sub).output().expect("run --help");
        assert!(out.status.success(), "failed for {sub:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(!stdout.is_empty());
    }

    let convert_help = Command::new(exe)
        .args(["convert", "--help"])
        .output()
        .expect("convert --help");
    let help_text = String::from_utf8_lossy(&convert_help.stdout);
    for expected_flag in [
        "--viewport",
        "--root",
        "--no-comments",
        "--no-states",
        "--no-animations",
        "--no-cache",
        "--cache-dir",
        "--quiet",
    ] {
        assert!(
            help_text.contains(expected_flag),
            "convert --help missing {expected_flag}"
        );
    }

    for bad_args in [
        &[][..],
        &["convert"],
        &["convert", "page.html", "--viewport", "not_a_size"],
        &["convert", "page.html", "--viewport", "0x800"],
        &["convert", "page.html", "--root", "dialog"],
        &["check"],
        &["unknown-subcommand"],
    ] {
        let out = Command::new(exe)
            .args(bad_args)
            .output()
            .expect("run invalid args");
        assert!(
            !out.status.success(),
            "expected non-zero exit for {bad_args:?}"
        );
    }
}

/// A conversion reads no machine fonts: what fontconfig answers changes nothing.
#[test]
fn output_does_not_depend_on_the_machines_fonts() {
    let dir = TempDir::new("fontconfig");
    let page = dir.path.join("page.html");
    fs::write(
        &page,
        "<!DOCTYPE html><style>h1{font-family:Helvetica,Arial}p{font:14px Georgia,serif}\
         code{font-family:monospace}button{font:13px system-ui}</style>\
         <h1>Title</h1><p>Some <code>code</code> and <b>bold</b>.</p><button>Go</button>",
    )
    .expect("write page");
    // No fonts at all, against what the machine has.
    let config = dir.path.join("fonts.conf");
    fs::write(
        &config,
        "<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"fonts.dtd\"><fontconfig>\
         <dir>/nonexistent</dir><cachedir>/nonexistent</cachedir></fontconfig>",
    )
    .expect("write config");
    let run = |out: &Path, fontconfig: Option<&Path>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_eldur"));
        command
            .args(["convert", "-q"])
            .arg(&page)
            .arg("-o")
            .arg(out);
        if let Some(file) = fontconfig {
            command.env("FONTCONFIG_FILE", file);
        }
        let status = command.status().expect("run eldur");
        assert!(status.success());
    };
    let (usual, none) = (dir.path.join("usual"), dir.path.join("none"));
    run(&usual, None);
    run(&none, Some(&config));
    let names: Vec<_> = fs::read_dir(&usual)
        .expect("output")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert!(!names.is_empty());
    for name in names {
        let (a, b) = (usual.join(&name), none.join(&name));
        if a.is_file() {
            assert!(
                fs::read(&a).expect("read") == fs::read(&b).expect("read"),
                "{name:?} differs"
            );
        }
    }
}
