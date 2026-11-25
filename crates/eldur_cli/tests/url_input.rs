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
