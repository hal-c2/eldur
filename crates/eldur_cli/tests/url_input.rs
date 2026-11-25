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

/// Each path asked for, with the `Authorization` it came with.
type Asked = Vec<(String, Option<String>)>;

/// Serves `files` (path → content type, body) on 127.0.0.1, 404 for the
/// rest, and records every path asked for with the `Authorization` it came
/// with. A `redirect` content type answers 302 to the body.
struct Server {
    origin: String,
    asked: Arc<Mutex<Asked>>,
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
        self.asked
            .lock()
            .expect("lock asked")
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }

    fn authorization(&self) -> Asked {
        self.asked.lock().expect("lock asked").clone()
    }
}

fn serve(
    mut stream: TcpStream,
    files: &BTreeMap<&'static str, (&'static str, Vec<u8>)>,
    asked: &Mutex<Asked>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut authorization = None;
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header == "\r\n" || header == "\n" => break,
            Ok(_) => {
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("authorization")
                {
                    authorization = Some(value.trim().to_owned());
                }
            }
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    asked
        .lock()
        .expect("lock asked")
        .push((path.clone(), authorization));
    let head = |status: &str, content_type: &str, extra: &str, len: usize| {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{extra}Content-Length: {len}\r\nConnection: close\r\n\r\n"
        )
    };
    let (head, body) = match files.get(path.as_str()) {
        Some(("redirect", to)) => {
            let to = String::from_utf8_lossy(to);
            (
                head("302 Found", "text/plain", &format!("Location: {to}\r\n"), 0),
                Vec::new(),
            )
        }
        Some((content_type, body)) => (head("200 OK", content_type, "", body.len()), body.clone()),
        None => (
            head("404 Not Found", "text/plain", "", 9),
            b"not found".to_vec(),
        ),
    };
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
    // The stand-in server is this machine.
    args.allow_private_hosts = true;
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
fn a_url_input_names_its_output_directory_after_its_host_and_last_segment() {
    let dir = |url: &str| ConvertArgs::new(url).resolved_output_dir();
    assert_eq!(
        dir("https://example.com/blog/post.html"),
        PathBuf::from("example.com-post_qml")
    );
    assert_eq!(
        dir("https://example.com/docs/guide/"),
        PathBuf::from("example.com-guide_qml")
    );
    // Two sites' `index.html`, and a local `index.html`, each their own.
    assert_ne!(
        dir("https://a.example/index.html"),
        dir("https://b.example/index.html")
    );
    assert_ne!(
        dir("https://a.example/index.html"),
        ConvertArgs::new("index.html").resolved_output_dir()
    );
    assert_eq!(
        dir("http://localhost:8080/index.html"),
        PathBuf::from("localhost_8080-index_qml")
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
    assert!(
        name.starts_with("example.com-éé") && name.ends_with("_qml"),
        "{name}"
    );
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
    args.allow_private_hosts = true;
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
    args.allow_private_hosts = true;
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
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
    let gone = format!(
        "http://{}/gone.js?sig=hunter2",
        listener.local_addr().expect("local addr")
    );
    drop(listener);
    let page =
        format!("<p>hi</p><img src=\"missing.gif\" alt=\"\"><script src=\"{gone}\"></script>");
    let server = Server::start(BTreeMap::from([(
        "/p.html?token=hunter2",
        ("text/html", page.into_bytes()),
    )]));
    let td = TempDir::new("json_url");
    let origin = server.url("");
    let url = origin.replacen("http://", "http://user:sesame@", 1) + "/p.html?token=hunter2";

    let mut args = ConvertArgs::new(url);
    args.allow_private_hosts = true;
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
    // A script that could not be fetched is named without its signed query.
    assert!(
        lines.iter().any(|l| l["message"]
            .as_str()
            .is_some_and(|m| m.contains(gone.split('?').next().unwrap_or_default()))),
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

#[test]
fn a_reference_that_is_not_found_is_named_without_its_secrets_under_any_base() {
    let server = Server::start(BTreeMap::new());
    let host = server.url("").replacen("http://", "", 1);
    for base in ["sub/".to_owned(), server.url("/cdn/")] {
        let page = format!(
            concat!(
                r#"<html><head><base href="{base}">"#,
                r#"<link rel="stylesheet" href="s.css?sig=hunter5">"#,
                r#"<script src=" app.js?sig=hunter3 "></script>"#,
                r#"<script src="//user:sesame@{host}/x.js?next=ftp://o.invalid&sig=hunter6"></script>"#,
                r#"</head><body><img src="pic.gif?sig=hunter4#t=hunter7" alt="">"#,
                r#"<div style="width: 2px; height: 2px; background-image: url(bg.gif?sig=hunter8)"></div>"#,
                r#"</body></html>"#,
            ),
            base = base,
            host = host,
        );
        let server = Server::start(BTreeMap::from([(
            "/index.html",
            ("text/html", page.into_bytes()),
        )]));
        let td = TempDir::new("secrets");
        let mut args = ConvertArgs::new(server.url("/index.html"));
        args.allow_private_hosts = true;
        args.output = Some(td.0.join("out"));
        args.message_format = eldur_cli::MessageFormat::Json;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        run_convert_with_io(&args, &RealConverter, &mut stdout, &mut stderr).expect("convert");
        let shown = format!(
            "{}{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        for named in ["app.js`", "pic.gif", "s.css`", "x.js"] {
            assert!(shown.contains(named), "{base}: {named} in {shown}");
        }
        for secret in [
            "hunter3", "hunter4", "hunter5", "hunter6", "hunter7", "hunter8", "sesame",
        ] {
            assert!(!shown.contains(secret), "{base}: {secret} in {shown}");
        }
    }
}

#[test]
fn the_page_s_credentials_go_to_its_own_origin_only() {
    let other = Server::start(BTreeMap::from([
        ("/b.gif", ("image/gif", DOT_GIF.to_vec())),
        ("/c.gif", ("image/gif", black_dot())),
    ]));
    let hop = other.url("/c.gif").into_bytes();
    let html = format!(
        r#"<!doctype html><html><body>
<img src="/a.gif" width="1" height="1" alt="">
<img src="{b}" width="1" height="1" alt="">
<img src="/hop.gif" width="1" height="1" alt="">
</body></html>"#,
        b = other.url("/b.gif"),
    );
    let mut a = DOT_GIF.to_vec();
    a[13..16].copy_from_slice(&[0, 0, 0xff]);
    let page = Server::start(BTreeMap::from([
        ("/index.html", ("text/html", html.into_bytes())),
        ("/a.gif", ("image/gif", a)),
        ("/hop.gif", ("redirect", hop)),
    ]));
    let td = TempDir::new("credentials");
    let url = page
        .url("/index.html")
        .replacen("http://", "http://user:sesame@", 1);
    let (outcome, stderr) = convert(&url, &td.0.join("out"));
    outcome.expect("convert the page");

    // `user:sesame`, as Basic sends it.
    let basic = Some("Basic dXNlcjpzZXNhbWU=".to_owned());
    let mut own = page.authorization();
    own.sort();
    assert_eq!(
        own,
        [
            ("/a.gif".to_owned(), basic.clone()),
            ("/hop.gif".to_owned(), basic.clone()),
            ("/index.html".to_owned(), basic),
        ],
        "{stderr}"
    );
    // Another origin gets none, asked directly or by the page's redirect.
    let mut elsewhere = other.authorization();
    elsewhere.sort();
    assert_eq!(
        elsewhere,
        [("/b.gif".to_owned(), None), ("/c.gif".to_owned(), None)],
        "{stderr}"
    );
}

#[test]
fn a_remote_image_s_file_is_named_as_its_url_decodes() {
    let server = Server::start(BTreeMap::from([(
        "/my%20logo.gif",
        ("image/gif", DOT_GIF.to_vec()),
    )]));
    let html = format!(
        r#"<!doctype html><html><body><img src="{img}" width="1" height="1" alt=""></body></html>"#,
        img = server.url("/my%20logo.gif"),
    );
    let page = Server::start(BTreeMap::from([(
        "/index.html",
        ("text/html", html.into_bytes()),
    )]));
    let td = TempDir::new("decoded");
    let out = td.0.join("out");
    let (outcome, stderr) = convert(&page.url("/index.html"), &out);
    outcome.expect("convert the page");
    // Qt reads a relative `source` as a URL: `my logo.gif` is the file it opens.
    assert_eq!(
        fs::read(out.join("assets/my logo.gif")).expect("the image, decoded"),
        DOT_GIF,
        "{stderr}"
    );
    let qml = fs::read_to_string(out.join("Main.qml")).expect("Main.qml");
    assert!(qml.contains("\"assets/my logo.gif\""), "{qml}");
}

#[test]
fn a_remote_image_s_name_holds_no_byte_a_file_or_qt_cannot() {
    let images = Server::start(BTreeMap::from([
        ("/a%00b.gif", ("image/gif", DOT_GIF.to_vec())),
        ("/%5C..%5Cup.gif", ("image/gif", DOT_GIF.to_vec())),
        ("/%2541.gif", ("image/gif", DOT_GIF.to_vec())),
        ("/x", ("image/gif", DOT_GIF.to_vec())),
        ("/x%2Fy.gif", ("image/gif", DOT_GIF.to_vec())),
    ]));
    // Relative to the page and with no `<base>`: named as the URL it was
    // read from decodes, as the absolute ones are.
    let html = format!(
        concat!(
            r#"<!doctype html><html><body>"#,
            r#"<div style="width: 4px; height: 4px; background-image: url(bg%20one.gif)"></div>"#,
            r#"<img src="near%20by.gif" width="1" height="1" alt="">"#,
            r#"<img src="{nul}" width="1" height="1" alt=""><img src="{slash}" width="1" height="1" alt="">"#,
            r#"<img src="{percent}" width="1" height="1" alt="">"#,
            r#"<img src="{x}" width="1" height="1" alt=""><img src="{xy}" width="1" height="1" alt=""></body></html>"#,
        ),
        x = images.url("/x"),
        xy = images.url("/x%2Fy.gif"),
        nul = images.url("/a%00b.gif"),
        slash = images.url("/%5C..%5Cup.gif"),
        percent = images.url("/%2541.gif"),
    );
    let page = Server::start(BTreeMap::from([
        ("/index.html", ("text/html", html.into_bytes())),
        ("/near%20by.gif", ("image/gif", DOT_GIF.to_vec())),
        // Its own bytes: a background the same as an image takes its file.
        ("/bg%20one.gif", ("image/gif", black_dot())),
    ]));
    let td = TempDir::new("named");
    let out = td.0.join("out");
    let (outcome, stderr) = convert(&page.url("/index.html"), &out);
    outcome.expect("one odd image name does not fail the page");
    let qml = fs::read_to_string(out.join("Main.qml")).expect("Main.qml");
    for name in [
        "near by.gif",
        "bg one.gif",
        "a_b.gif",
        "_.._up.gif",
        "_41.gif",
        // An encoded `/` is part of the name: no folder where `x` is a file.
        "x",
        "x_y.gif",
    ] {
        assert!(
            out.join("assets").join(name).is_file(),
            "assets/{name}: {stderr}"
        );
        assert!(qml.contains(&format!("\"assets/{name}\"")), "{name}: {qml}");
    }
}

#[test]
fn a_url_on_this_machine_is_refused_unless_private_hosts_are_allowed() {
    let server = Server::start(BTreeMap::from([(
        "/index.html",
        ("text/html", b"<p>local</p>".to_vec()),
    )]));
    let td = TempDir::new("private");
    let url = server.url("/index.html");
    let mut args = ConvertArgs::new(&url);
    args.output = Some(td.0.join("out"));
    args.quiet = true;
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let err = run_convert_with_io(&args, &RealConverter, &mut stdout, &mut stderr)
        .expect_err("127.0.0.1 is not on the public internet");
    assert!(
        format!("{err:#}").contains("not on the public internet"),
        "{err:#}"
    );
    assert!(server.asked().is_empty(), "{:?}", server.asked());
    assert!(!td.0.join("out/Main.qml").exists());
    let (outcome, _) = convert(&url, &td.0.join("out"));
    outcome.expect("allowed, the page converts");
    assert!(td.0.join("out/Main.qml").exists());
}
