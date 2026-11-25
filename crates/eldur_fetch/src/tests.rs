//! A server on 127.0.0.1 stands in for the web: no test needs the network.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use super::*;

/// What a path answers: a status line and headers after it, and a body.
#[derive(Clone)]
struct Reply {
    status: &'static str,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// The `Content-Length` sent, when it is not the body's: a body that
    /// breaks off.
    length: Option<usize>,
}

fn ok(body: &[u8]) -> Reply {
    Reply {
        status: "200 OK",
        headers: Vec::new(),
        body: body.to_vec(),
        length: None,
    }
}

fn redirect(to: &str) -> Reply {
    Reply {
        status: "302 Found",
        headers: vec![("Location".to_owned(), to.to_owned())],
        body: Vec::new(),
        length: None,
    }
}

struct Server {
    origin: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(replies: BTreeMap<&'static str, Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
        let origin = format!("http://{}", listener.local_addr().expect("local addr"));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        // Lives as long as the test process; every connection is answered
        // and closed.
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                serve(stream, &replies, &seen);
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
    replies: &BTreeMap<&'static str, Reply>,
    asked: &Mutex<Vec<String>>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
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
    let reply = replies.get(path.as_str()).cloned().unwrap_or(Reply {
        status: "404 Not Found",
        headers: Vec::new(),
        body: b"not found".to_vec(),
        length: None,
    });
    let mut head = format!("HTTP/1.1 {}\r\n", reply.status);
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        reply.length.unwrap_or(reply.body.len())
    ));
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&reply.body);
    let _ = stream.flush();
}

fn messages(resources: &HttpResources) -> Vec<String> {
    resources
        .diagnostics()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn http_urls_are_told_from_paths() {
    assert!(is_http_url("https://example.com/"));
    assert!(is_http_url("HTTP://Example.com/page.html"));
    assert!(!is_http_url("page.html"));
    assert!(!is_http_url("/var/www/page.html"));
    assert!(!is_http_url("file:///var/www/page.html"));
    assert!(!is_http_url("C:\\www\\page.html"));
    // A file of that name; the URL parser takes it for a host.
    assert!(!is_http_url("https:report.html"));
}

#[test]
fn a_reference_resolves_against_the_document_or_sheet_holding_it() {
    let server = Server::start(BTreeMap::from([
        ("/a/css/site.css", ok(b"site")),
        ("/a/css/theme.css", ok(b"theme")),
        ("/root.css", ok(b"root")),
    ]));
    let mut http = HttpResources::new();
    let page = server.url("/a/index.html#top");
    let sheet = http.read("css/site.css", &page).expect("beside the page");
    assert_eq!(sheet.bytes, b"site");
    assert_eq!(sheet.resolved, server.url("/a/css/site.css"));
    let theme = http
        .read("theme.css", &sheet.resolved)
        .expect("beside the sheet");
    assert_eq!(theme.bytes, b"theme");
    let root = http
        .read("/root.css", &sheet.resolved)
        .expect("from the root");
    assert_eq!(root.bytes, b"root");
    assert!(messages(&http).is_empty(), "{:?}", messages(&http));
}

#[test]
fn a_url_is_fetched_once() {
    let server = Server::start(BTreeMap::from([("/x.css", ok(b"x"))]));
    let mut http = HttpResources::new();
    let base = server.url("/index.html");
    assert!(http.read("x.css", &base).is_some());
    assert!(http.read("/x.css#again", &base).is_some());
    assert_eq!(server.asked(), ["/x.css"]);
    assert_eq!(http.fetch_count(), 1);
}

#[test]
fn a_page_is_where_its_redirects_end() {
    let server = Server::start(BTreeMap::from([
        ("/old", redirect("/new/index.html")),
        ("/new/index.html", ok(b"<p>moved</p>")),
    ]));
    let mut http = HttpResources::new();
    let input = http.fetch_page(&server.url("/old")).expect("the page");
    assert_eq!(input.html, "<p>moved</p>");
    assert_eq!(input.url, server.url("/new/index.html"));
}

#[test]
fn a_page_that_fails_is_an_error_naming_its_url() {
    let server = Server::start(BTreeMap::new());
    let mut http = HttpResources::new();
    let url = server.url("/gone.html");
    let err = http.fetch_page(&url).expect_err("404");
    assert_eq!(
        err,
        FetchError::Failed {
            url: url.clone(),
            reason: "HTTP status 404".to_owned()
        }
    );
    assert!(err.to_string().contains(&url), "{err}");
    assert!(matches!(
        http.fetch_page("page.html"),
        Err(FetchError::NotHttp { .. })
    ));
    assert!(matches!(
        http.fetch_page("file:///etc/hostname"),
        Err(FetchError::NotHttp { .. })
    ));
}

#[test]
fn a_reference_that_fails_is_a_warning_naming_its_url() {
    let server = Server::start(BTreeMap::new());
    let mut http = HttpResources::new();
    assert!(
        http.read("missing.png", &server.url("/index.html"))
            .is_none()
    );
    let diagnostics = http.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert_eq!(diagnostics[0].stage, STAGE);
    assert_eq!(diagnostics[0].source, server.url("/missing.png"));
    assert!(
        diagnostics[0].message.contains("404"),
        "{:?}",
        diagnostics[0]
    );
}

#[test]
fn a_lookup_another_base_serves_is_not_reported() {
    // As the converter tries a background image: against the page, then
    // against each sheet.
    let server = Server::start(BTreeMap::from([("/css/img/bg.png", ok(b"png"))]));
    let mut http = HttpResources::new();
    let (page, sheet) = (server.url("/index.html"), server.url("/css/site.css"));
    let found = http.read_any("img/bg.png", &[&page, &sheet]);
    assert_eq!(found.expect("beside the sheet").bytes, b"png");
    assert!(messages(&http).is_empty(), "{:?}", messages(&http));

    // A later lookup that needs the URL the first missed still reports it.
    assert!(http.read("img/bg.png", &page).is_none());
    let text = messages(&http).join("\n");
    assert!(text.contains(&server.url("/img/bg.png")), "{text}");

    // One no base serves reports each base's miss.
    let mut none = HttpResources::new();
    assert!(none.read_any("gone.png", &[&page, &sheet]).is_none());
    assert_eq!(messages(&none).len(), 2, "{:?}", messages(&none));
}

#[test]
fn only_http_https_and_data_urls_are_read() {
    let mut http = HttpResources::new();
    assert!(
        http.read("file:///etc/hostname", "https://example.com/")
            .is_none()
    );
    assert!(
        http.read("ftp://example.com/x.css", "https://example.com/")
            .is_none()
    );
    let text = messages(&http).join("\n");
    assert!(
        text.contains("file:///etc/hostname") && text.contains("refused"),
        "{text}"
    );
    assert!(text.contains("ftp://example.com/x.css"), "{text}");
    assert_eq!(http.fetch_count(), 0);
}

#[test]
fn a_data_url_is_decoded_without_a_fetch() {
    let mut http = HttpResources::new();
    let plain = http
        .read(
            "data:text/css,h1%20%7B%20color%3A%20red%20%7D",
            "https://example.com/",
        )
        .expect("percent-encoded");
    assert_eq!(plain.bytes, b"h1 { color: red }");
    let b64 = http
        .read("data:image/gif;base64,R0lGODlh", "https://example.com/")
        .expect("base64");
    assert_eq!(b64.bytes, b"GIF89a");
    assert_eq!(http.fetch_count(), 0);
    assert!(messages(&http).is_empty(), "{:?}", messages(&http));
}

#[test]
fn a_relative_reference_needs_a_url_to_resolve_against() {
    let mut http = HttpResources::new();
    assert!(http.read("site.css", "index.html").is_none());
    let text = messages(&http).join("\n");
    assert!(
        text.contains("site.css") && text.contains("not a URL"),
        "{text}"
    );
}

#[test]
fn the_number_of_fetches_is_capped() {
    let server = Server::start(BTreeMap::from([
        ("/a.css", ok(b"a")),
        ("/b.css", ok(b"b")),
        ("/c.css", ok(b"c")),
    ]));
    let mut http = HttpResources::with_limits(Limits {
        max_fetches: 2,
        ..Limits::default()
    });
    let base = server.url("/");
    assert!(http.read("a.css", &base).is_some());
    assert!(http.read("b.css", &base).is_some());
    assert!(http.read("c.css", &base).is_none());
    assert_eq!(server.asked(), ["/a.css", "/b.css"]);
    let text = messages(&http).join("\n");
    assert!(
        text.contains(&server.url("/c.css")) && text.contains("refused"),
        "{text}"
    );
}

#[test]
fn a_resource_larger_than_its_cap_is_refused() {
    let server = Server::start(BTreeMap::from([
        ("/small.css", ok(&[b'a'; 8])),
        ("/big.css", ok(&[b'a'; 9])),
    ]));
    let mut http = HttpResources::with_limits(Limits {
        max_resource_bytes: 8,
        ..Limits::default()
    });
    let base = server.url("/");
    assert!(http.read("small.css", &base).is_some());
    assert!(http.read("big.css", &base).is_none());
    assert!(
        http.read("data:text/plain,123456789", &base).is_none(),
        "a data: URL is capped too"
    );
    let text = messages(&http).join("\n");
    assert!(
        text.contains(&server.url("/big.css")) && text.contains("larger than 8"),
        "{text}"
    );
}

#[test]
fn all_fetches_together_are_capped() {
    let server = Server::start(BTreeMap::from([
        ("/a.css", ok(&[b'a'; 6])),
        ("/b.css", ok(&[b'b'; 6])),
        ("/c.css", ok(&[b'c'; 1])),
    ]));
    let mut http = HttpResources::with_limits(Limits {
        max_total_bytes: 10,
        ..Limits::default()
    });
    let base = server.url("/");
    assert!(http.read("a.css", &base).is_some());
    assert!(http.read("b.css", &base).is_none(), "6 + 6 > 10");
    assert!(http.read("c.css", &base).is_some(), "6 + 1 <= 10");
    assert_eq!(http.total_bytes(), 7);
    let text = messages(&http).join("\n");
    assert!(
        text.contains(&server.url("/b.css")) && text.contains("in all"),
        "{text}"
    );
}

#[test]
fn a_request_that_takes_too_long_is_a_warning() {
    // Never accepted: the kernel takes the connection and the request, and
    // no answer comes while the listener is held.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/slow.css", listener.local_addr().expect("addr"));
    let mut http = HttpResources::with_limits(Limits {
        timeout: Duration::from_millis(200),
        ..Limits::default()
    });
    assert!(http.read(&url, "https://example.com/").is_none());
    let text = messages(&http).join("\n");
    assert!(text.contains(&url) && text.contains("timed out"), "{text}");
    drop(listener);
}

/// Local files by name, for [`MixedResources`].
#[derive(Default)]
struct Local(Vec<String>);

impl Resources for Local {
    fn read(&mut self, url: &str, _base: &str) -> Option<Resource> {
        self.0.push(url.to_owned());
        Some(Resource {
            resolved: url.to_owned(),
            bytes: b"local".to_vec(),
        })
    }
}

#[test]
fn mixed_resources_send_web_references_to_the_web_and_the_rest_to_disk() {
    let server = Server::start(BTreeMap::from([("/lib.js", ok(b"web"))]));
    let mut mixed = MixedResources::new(Local::default());
    let page = "/home/me/site/index.html";
    assert_eq!(mixed.read("app.js", page).expect("local").bytes, b"local");
    assert_eq!(
        mixed.read(&server.url("/lib.js"), page).expect("web").bytes,
        b"web"
    );
    assert_eq!(
        mixed.read("data:,inline", page).expect("data").bytes,
        b"inline"
    );
    // Relative to a sheet that was itself fetched: the web.
    let remote = mixed.read("lib.js", &server.url("/css/../site.css"));
    assert_eq!(remote.expect("beside the remote sheet").bytes, b"web");
    assert_eq!(mixed.local.0, ["app.js"]);
}

#[test]
fn every_redirect_hop_is_a_fetch() {
    let server = Server::start(BTreeMap::from([
        ("/old", redirect("/new.html")),
        ("/new.html", ok(b"<p>moved</p>")),
    ]));
    let with_fetches = |max_fetches| {
        HttpResources::with_limits(Limits {
            max_fetches,
            ..Limits::default()
        })
    };
    let mut one = with_fetches(1);
    let err = one
        .fetch_page(&server.url("/old"))
        .expect_err("the redirect needs a second request");
    assert!(err.to_string().contains("requests"), "{err}");
    assert_eq!(one.fetch_count(), 1);

    let mut two = with_fetches(2);
    let input = two.fetch_page(&server.url("/old")).expect("both fit");
    assert_eq!(input.html, "<p>moved</p>");
    assert_eq!(input.url, server.url("/new.html"));
    assert_eq!(two.fetch_count(), 2);
}

#[test]
fn a_body_that_breaks_off_still_counts_what_it_brought() {
    let server = Server::start(BTreeMap::from([(
        "/cut.css",
        Reply {
            length: Some(10),
            ..ok(b"abc")
        },
    )]));
    let mut http = HttpResources::new();
    assert!(http.read("cut.css", &server.url("/")).is_none());
    assert_eq!(http.total_bytes(), 3, "{:?}", messages(&http));
}

#[test]
fn data_urls_count_toward_the_total() {
    let server = Server::start(BTreeMap::from([("/a.css", ok(&[b'a'; 6]))]));
    let mut http = HttpResources::with_limits(Limits {
        max_total_bytes: 10,
        max_resource_bytes: 8,
        ..Limits::default()
    });
    let base = server.url("/");
    assert!(http.read("a.css", &base).is_some());
    assert!(http.read("data:,12345", &base).is_none(), "6 + 5 > 10");
    assert!(http.read("data:,1234", &base).is_some(), "6 + 4 <= 10");
    assert_eq!(http.total_bytes(), 10);
    let text = messages(&http).join("\n");
    assert!(
        text.contains("data:,12345") && text.contains("in all"),
        "{text}"
    );
}

#[test]
fn a_reference_with_the_base_scheme_and_no_slashes_is_relative() {
    // WHATWG URL: a special scheme matching the base's, without `//`, is a
    // relative reference.
    let joined = resolve("http:assets/x.css", "http://example.com/blog/index.html")
        .expect("a reference")
        .expect("resolves");
    assert_eq!(joined.as_str(), "http://example.com/blog/assets/x.css");
    let other = resolve("https:x.css", "http://example.com/blog/index.html")
        .expect("a reference")
        .expect("resolves");
    assert_eq!(other.as_str(), "https://x.css/");
}

#[test]
fn a_lookup_that_fails_is_reported_though_another_finds_its_spelling() {
    // Two sheets each import `theme.css`; only the second's exists.
    let server = Server::start(BTreeMap::from([("/b/theme.css", ok(b"theme"))]));
    let mut http = HttpResources::new();
    assert!(http.read("theme.css", &server.url("/a/site.css")).is_none());
    assert!(http.read("theme.css", &server.url("/b/site.css")).is_some());
    let text = messages(&http).join("\n");
    assert!(text.contains(&server.url("/a/theme.css")), "{text}");
}

#[test]
fn mixed_resources_do_not_let_a_fetched_sheet_read_local_files() {
    let server = Server::start(BTreeMap::new());
    let mut mixed = MixedResources::new(Local::default());
    let sheet = server.url("/css/site.css");
    assert!(mixed.read("file:///etc/hostname", &sheet).is_none());
    assert!(mixed.read("/etc/hostname", &sheet).is_none());
    assert!(mixed.local.0.is_empty(), "{:?}", mixed.local.0);
    let text = messages(&mixed.remote).join("\n");
    assert!(text.contains("file:"), "{text}");
}

#[test]
fn a_message_s_urls_are_shown_without_secrets_wherever_they_end() {
    let cases = [
        // At the end of the text, with nothing after it.
        (
            "failed http://h.invalid/a.js?sig=s1",
            "failed http://h.invalid/a.js",
        ),
        // Two in one message, `https` and `http`, the scheme in any case.
        (
            "`HTTPS://u:p@h.invalid/a?sig=s2` then `http://h.invalid/b#t=s3`",
            "`https://h.invalid/a` then `http://h.invalid/b`",
        ),
        // Closed by a bracket or a quote.
        (
            "error (Http://h.invalid/c?sig=s4) and \"https://h.invalid/d?sig=s5\"",
            "error (http://h.invalid/c) and \"https://h.invalid/d\"",
        ),
        // A reference relative to the page, and one without a scheme.
        (
            "script `missing.js?sig=s6` and `//u:p@h.invalid/e.js?sig=s7`",
            "script `missing.js` and `//h.invalid/e.js`",
        ),
        // A local diagnostic is shown as it is.
        (
            "image `img/a b.png` was not found",
            "image `img/a b.png` was not found",
        ),
    ];
    for (text, shown) in cases {
        assert_eq!(redacted_in(text), shown, "{text}");
    }
}
