//! `eldur convert <http(s) URL>`: the page and what it references are
//! fetched over HTTP. A server on 127.0.0.1 stands in for the web, so the
//! tests need no network.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eldur_cli::{
    ConvertArgs, ConvertStatus, RealConverter, WatchChannel, run_convert_with_io,
    run_watch_with_channel,
};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("eldur_cli_url_{label}_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Serves `files` (path → content type, body) on 127.0.0.1, 404 for the
/// rest, and records every path asked for.
struct Server {
    origin: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(files: BTreeMap<&'static str, (&'static str, Vec<u8>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
        let origin = format!("http://{}", listener.local_addr().expect("local addr"));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        // The thread ends with the test process; each connection is answered
        // and closed, so a client never waits on it.
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                serve(stream, &files, &seen);
            }
        });
        Self { origin, asked }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("lock asked").clone()
    }
}

fn serve(
    mut stream: TcpStream,
    files: &BTreeMap<&'static str, (&'static str, Vec<u8>)>,
    asked: &Mutex<Vec<String>>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    // The headers are read and dropped: nothing here depends on them.
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header == "\r\n" || header == "\n" => break,
            Ok(_) => {}
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    asked.lock().expect("lock asked").push(path.clone());
    let (status, content_type, body) = match files.get(path.as_str()) {
        Some((content_type, body)) => ("200 OK", *content_type, body.clone()),
        None => ("404 Not Found", "text/plain", b"not found".to_vec()),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}

/// A 1×1 GIF.
const DOT_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

/// [`DOT_GIF`] with its first palette entry black: other bytes, so the
/// converter does not share one file for both images.
fn black_dot() -> Vec<u8> {
    let mut gif = DOT_GIF.to_vec();
    gif[13..16].copy_from_slice(&[0, 0, 0]);
    gif
}

fn convert(url: &str, out: &Path) -> (anyhow::Result<eldur_cli::ConvertOutcome>, String) {
    let mut args = ConvertArgs::new(url);
    args.output = Some(out.to_path_buf());
    args.quiet = true;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let outcome = run_convert_with_io(&args, &RealConverter, &mut stdout, &mut stderr);
    (outcome, String::from_utf8_lossy(&stderr).into_owned())
}

#[test]
fn a_page_served_over_http_is_converted_with_what_it_references() {
    let server = Server::start(BTreeMap::from([
        (
            "/blog/post.html",
            (
                "text/html; charset=utf-8",
                br#"<!doctype html><html><head><title>Fetched</title>
<link rel="stylesheet" href="css/site.css"></head><body>
<h1 id="title">Hello</h1>
<p class="note">Note</p>
<img src="img/dot.gif" width="1" height="1" alt="">
<img src="img/missing.gif" width="1" height="1" alt="">
<script src="js/app.js"></script>
</body></html>"#
                    .to_vec(),
            ),
        ),
        (
            "/blog/css/site.css",
            (
                "text/css",
                b"@import \"theme.css\";\nh1 { color: #123456; }\n".to_vec(),
            ),
        ),
        (
            "/blog/css/theme.css",
            ("text/css", b".note { color: #abcdef; }\n".to_vec()),
        ),
        (
            "/blog/js/app.js",
            (
                "text/javascript",
                b"document.getElementById('title').textContent = 'From the script';\n".to_vec(),
            ),
        ),
        ("/blog/img/dot.gif", ("image/gif", DOT_GIF.to_vec())),
    ]));
    let td = TempDir::new("page");
    let out = td.0.join("out");

    let (outcome, stderr) = convert(&server.url("/blog/post.html"), &out);
    let outcome = outcome.expect("convert the page");
    assert_eq!(outcome.status, ConvertStatus::Converted);

    let qml = fs::read_to_string(out.join("Main.qml")).expect("Main.qml");
    assert!(qml.contains("#123456"), "the linked sheet applies:\n{qml}");
    assert!(
        qml.contains("#abcdef"),
        "the @imported sheet applies:\n{qml}"
    );
    let page_js = fs::read_to_string(out.join("page.js")).expect("page.js");
    assert!(page_js.contains("From the script"), "{page_js}");
    assert_eq!(
        fs::read(out.join("assets/img/dot.gif")).expect("the image is copied"),
        DOT_GIF
    );

    let missing = server.url("/blog/img/missing.gif");
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.starts_with("warning") && d.contains(&missing) && d.contains("404")),
        "the 404 is a warning naming its URL: {:#?}",
        outcome.diagnostics
    );
    assert!(stderr.contains(&missing), "{stderr}");
    assert!(
        !outcome
            .diagnostics
            .iter()
            .any(|d| d.contains("failed to load stylesheet") || d.contains("could not be read")),
        "{:#?}",
        outcome.diagnostics
    );
    assert!(
        server.asked().contains(&"/blog/css/theme.css".to_owned()),
        "{:?}",
        server.asked()
    );
}

#[test]
fn base_href_moves_where_a_fetched_page_finds_what_it_references() {
    let server = Server::start(BTreeMap::from([
        (
            "/site/index.html",
            (
                "text/html",
                br#"<!doctype html><html><head><base href="/static/">
<link rel="stylesheet" href="site.css"></head><body>
<h1 id="title">Hello</h1>
<img src="dot.gif" width="1" height="1" alt="">
<script src="app.js"></script>
</body></html>"#
                    .to_vec(),
            ),
        ),
        (
            "/static/site.css",
            ("text/css", b"h1 { color: #123456; }\n".to_vec()),
        ),
        (
            "/static/app.js",
            (
                "text/javascript",
                b"document.getElementById('title').textContent = 'From the base';\n".to_vec(),
            ),
        ),
        ("/static/dot.gif", ("image/gif", DOT_GIF.to_vec())),
    ]));
    let td = TempDir::new("base");
    let out = td.0.join("out");

    let (outcome, _) = convert(&server.url("/site/index.html"), &out);
    let outcome = outcome.expect("convert the page");
    assert!(
        !outcome
            .diagnostics
            .iter()
            .any(|d| d.starts_with("warning") || d.starts_with("error")),
        "everything is found under the base (asked {:?}): {:#?}",
        server.asked(),
        outcome.diagnostics
    );
    let qml = fs::read_to_string(out.join("Main.qml")).expect("Main.qml");
    assert!(qml.contains("#123456"), "{qml}");
    let page_js = fs::read_to_string(out.join("page.js")).expect("page.js");
    assert!(page_js.contains("From the base"), "{page_js}");
    assert_eq!(
        fs::read(out.join("assets/dot.gif")).expect("image"),
        DOT_GIF
    );
}

#[test]
fn a_page_that_cannot_be_fetched_is_an_error() {
    let server = Server::start(BTreeMap::new());
    let td = TempDir::new("missing");
    let url = server.url("/nowhere.html");
    let (outcome, _) = convert(&url, &td.0.join("out"));
    let err = outcome.expect_err("a 404 page is no page");
    let text = format!("{err:#}");
    assert!(text.contains(&url) && text.contains("404"), "{text}");
    assert!(!td.0.join("out/Main.qml").exists());
}

#[test]
fn a_url_input_names_its_output_directory_after_its_last_segment_or_host() {
    let dir = |url: &str| ConvertArgs::new(url).resolved_output_dir();
    assert_eq!(
        dir("https://example.com/blog/post.html"),
        PathBuf::from("post_qml")
    );
    assert_eq!(
        dir("https://example.com/docs/guide/"),
        PathBuf::from("guide_qml")
    );
    assert_eq!(dir("http://example.com"), PathBuf::from("example.com_qml"));
    assert_eq!(
        dir("HTTPS://Example.COM/?q=1"),
        PathBuf::from("example.com_qml")
    );
    // A file name has at most 255 bytes on most file systems.
    let long = dir(&format!("https://example.com/{}.html", "é".repeat(200)));
    let name = long.to_str().expect("UTF-8");
    assert!(name.len() <= 255, "{} bytes", name.len());
    assert!(name.starts_with("éé") && name.ends_with("_qml"), "{name}");
}

#[test]
fn only_an_input_that_spells_out_http_or_https_is_a_url() {
    let url = |input: &str| ConvertArgs::new(input).input_url().map(str::to_owned);
    assert_eq!(
        url("HTTPS://example.com/"),
        Some("HTTPS://example.com/".to_owned())
    );
    assert_eq!(
        url("http://example.com/a.html").as_deref(),
        Some("http://example.com/a.html")
    );
    // A file of that name, for the URL parser a host.
    assert_eq!(url("https:report.html"), None);
    assert_eq!(url("http:/report.html"), None);
}

#[test]
fn the_status_line_and_errors_leave_out_credentials_and_queries() {
    let server = Server::start(BTreeMap::from([(
        "/p.html?token=hunter2",
        (
            "text/html",
            b"<p>hi</p><img src=\"missing.gif\" alt=\"\">".to_vec(),
        ),
    )]));
    let td = TempDir::new("redact");
    let origin = server.url("");
    let with_secrets = |path: &str| origin.replacen("http://", "http://user:sesame@", 1) + path;

    let mut args = ConvertArgs::new(with_secrets("/p.html?token=hunter2#top"));
    args.output = Some(td.0.join("out"));
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    run_convert_with_io(&args, &RealConverter, &mut stdout, &mut stderr).expect("convert");
    let stdout = String::from_utf8_lossy(&stdout);
    assert!(stdout.contains(&server.url("/p.html")), "{stdout}");
    for secret in ["sesame", "hunter2", "#top"] {
        assert!(!stdout.contains(secret), "{secret} in {stdout}");
    }
    // The page's credentials and query are not in the URLs its warnings name.
    let stderr = String::from_utf8_lossy(&stderr);
    assert!(stderr.contains(&server.url("/missing.gif")), "{stderr}");
    for secret in ["sesame", "hunter2"] {
        assert!(!stderr.contains(secret), "{secret} in {stderr}");
    }

    let (outcome, _) = convert(
        &with_secrets("/gone.html?token=hunter2"),
        &td.0.join("gone"),
    );
    let text = format!("{:#}", outcome.expect_err("a 404 page"));
    assert!(
        text.contains(&server.url("/gone.html")) && text.contains("404"),
        "{text}"
    );
    for secret in ["sesame", "hunter2"] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
}

#[test]
fn a_fetched_page_s_absolute_images_are_fetched_and_copied_too() {
    let server = Server::start(BTreeMap::from([
        ("/img/a.gif", ("image/gif", DOT_GIF.to_vec())),
        ("/img/bg.gif", ("image/gif", black_dot())),
    ]));
    let html = format!(
        r#"<!doctype html><html><body>
<img src="{img}" width="1" height="1" alt="">
<div style="width: 4px; height: 4px; background-image: url({bg})"></div>
</body></html>"#,
        img = server.url("/img/a.gif"),
        bg = server.url("/img/bg.gif"),
    );
    let page = Server::start(BTreeMap::from([(
        "/index.html",
        ("text/html", html.into_bytes()),
    )]));
    let td = TempDir::new("absolute");
    let out = td.0.join("out");
    let (outcome, _) = convert(&page.url("/index.html"), &out);
    let outcome = outcome.expect("convert the page");
    assert!(
        !outcome.diagnostics.iter().any(|d| d.starts_with("warning")),
        "{:#?}",
        outcome.diagnostics
    );
    assert_eq!(
        fs::read(out.join("assets/img/a.gif")).expect("the image"),
        DOT_GIF
    );
    assert_eq!(
        fs::read(out.join("assets/img/bg.gif")).expect("the background"),
        black_dot()
    );
    let qml = fs::read_to_string(out.join("Main.qml")).expect("Main.qml");
    assert!(
        !qml.contains(&server.url("")),
        "Qt loads nothing remote:\n{qml}"
    );
    let mut asked = server.asked();
    asked.sort();
    assert_eq!(asked, ["/img/a.gif", "/img/bg.gif"]);
}

#[test]
fn a_url_is_neither_watched_nor_given_a_site_root() {
    let server = Server::start(BTreeMap::new());
    let url = server.url("/page.html");
    let td = TempDir::new("refused");

    let mut args = ConvertArgs::new(url.as_str());
    args.output = Some(td.0.join("out"));
    let (channel, _stop) = WatchChannel::new();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let err = run_watch_with_channel(
        &args,
        &RealConverter,
        channel,
        &mut stdout,
        &mut stderr,
        None,
    )
    .expect_err("watch refuses a URL");
    assert!(format!("{err:#}").contains("local file"), "{err:#}");

    args.site_root = Some(td.0.clone());
    let err = run_convert_with_io(&args, &RealConverter, &mut stdout, &mut stderr)
        .expect_err("--site-root is for a local page");
    assert!(format!("{err:#}").contains("--site-root"), "{err:#}");
    assert!(
        server.asked().is_empty(),
        "nothing is fetched: {:?}",
        server.asked()
    );
}

#[test]
fn json_output_for_a_url_names_its_files_and_leaves_out_credentials() {
    let server = Server::start(BTreeMap::from([(
        "/p.html?token=hunter2",
        (
            "text/html",
            b"<p>hi</p><img src=\"missing.gif\" alt=\"\">".to_vec(),
        ),
    )]));
    let td = TempDir::new("json_url");
    let origin = server.url("");
    let url = origin.replacen("http://", "http://user:sesame@", 1) + "/p.html?token=hunter2";

    let mut args = ConvertArgs::new(url);
    args.output = Some(td.0.join("out"));
    args.message_format = eldur_cli::MessageFormat::Json;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    run_convert_with_io(&args, &RealConverter, &mut stdout, &mut stderr).expect("convert");
    let stdout = String::from_utf8_lossy(&stdout);
    let lines: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}")))
        .collect();
    // The image that could not be fetched is a diagnostic, and the page's
    // credentials are not in the URL it names.
    assert!(
        lines
            .iter()
            .any(|l| l["type"] == "diagnostic" && l["source"] == server.url("/missing.gif")),
        "{stdout}"
    );
    let done = lines.last().expect("a done line");
    assert_eq!(done["type"], "done", "{stdout}");
    assert_eq!(done["status"], "converted", "{stdout}");
    assert!(
        done["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|f| f == "Main.qml")),
        "{stdout}"
    );
    for secret in ["sesame", "hunter2"] {
        assert!(!stdout.contains(secret), "{secret} in {stdout}");
    }
    assert!(
        stderr.is_empty(),
        "stderr was: {}",
        String::from_utf8_lossy(&stderr)
    );
}
