//! A server on 127.0.0.1 stands in for the web: no test needs the network.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use super::*;

/// The stand-in server is this machine, which [`Reach::Public`] refuses.
fn local_http() -> HttpResources {
    local_http_with(Limits::default())
}

fn local_http_with(limits: Limits) -> HttpResources {
    HttpResources::with_reach(limits, Reach::Any)
}

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
    let mut http = local_http();
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
    let mut http = local_http();
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
    let mut http = local_http();
    let input = http.fetch_page(&server.url("/old")).expect("the page");
    assert_eq!(input.html, "<p>moved</p>");
    assert_eq!(input.url, server.url("/new/index.html"));
}

#[test]
fn a_page_that_fails_is_an_error_naming_its_url() {
    let server = Server::start(BTreeMap::new());
    let mut http = local_http();
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
    let mut http = local_http();
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
    let mut http = local_http();
    let (page, sheet) = (server.url("/index.html"), server.url("/css/site.css"));
    let found = http.read_any("img/bg.png", &[&page, &sheet]);
    assert_eq!(found.expect("beside the sheet").bytes, b"png");
    assert!(messages(&http).is_empty(), "{:?}", messages(&http));

    // A later lookup that needs the URL the first missed still reports it.
    assert!(http.read("img/bg.png", &page).is_none());
    let text = messages(&http).join("\n");
    assert!(text.contains(&server.url("/img/bg.png")), "{text}");

    // One no base serves reports each base's miss.
    let mut none = local_http();
    assert!(none.read_any("gone.png", &[&page, &sheet]).is_none());
    assert_eq!(messages(&none).len(), 2, "{:?}", messages(&none));
}

#[test]
fn only_http_https_and_data_urls_are_read() {
    let mut http = local_http();
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
    let mut http = local_http();
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
    let mut http = local_http();
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
    let mut http = local_http_with(Limits {
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
    let mut http = local_http_with(Limits {
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
    let mut http = local_http_with(Limits {
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
    let mut http = local_http_with(Limits {
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
    let mut mixed = MixedResources {
        local: Local::default(),
        remote: local_http(),
    };
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
        local_http_with(Limits {
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
    let mut http = local_http();
    assert!(http.read("cut.css", &server.url("/")).is_none());
    assert_eq!(http.total_bytes(), 3, "{:?}", messages(&http));
}

#[test]
fn data_urls_count_toward_the_total() {
    let server = Server::start(BTreeMap::from([("/a.css", ok(&[b'a'; 6]))]));
    let mut http = local_http_with(Limits {
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
    let mut http = local_http();
    assert!(http.read("theme.css", &server.url("/a/site.css")).is_none());
    assert!(http.read("theme.css", &server.url("/b/site.css")).is_some());
    let text = messages(&http).join("\n");
    assert!(text.contains(&server.url("/a/theme.css")), "{text}");
}

#[test]
fn mixed_resources_do_not_let_a_fetched_sheet_read_local_files() {
    let server = Server::start(BTreeMap::new());
    let mut mixed = MixedResources {
        local: Local::default(),
        remote: local_http(),
    };
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
        // Quoted text that is no URL keeps its `#` and `?`, a backtick
        // inside it too.
        (
            "invalid value `#zzz` for property `color`",
            "invalid value `#zzz` for property `color`",
        ),
        (
            "selector `a#x:::` does not parse in `q(`a`)?b#c`",
            "selector `a#x:::` does not parse in `q(`a`)?b#c`",
        ),
    ];
    for (text, shown) in cases {
        assert_eq!(redacted_in(text), shown, "{text}");
    }
}

#[test]
fn a_reference_is_quoted_without_its_secrets() {
    for (written, shown) in [
        ("missing.js?sig=s6", "missing.js"),
        (" app.js?sig=s ", "app.js"),
        ("//u:p@h.invalid/e.js?sig=s7", "//h.invalid/e.js"),
        (
            "//u:p@h.invalid/a.js?next=ftp://o.invalid&sig=s8",
            "//h.invalid/a.js",
        ),
        ("HTTPS://u:p@h.invalid/a?sig=s2", "HTTPS://h.invalid/a"),
        ("img/a b.png#t=s9", "img/a b.png"),
        ("img/a b.png", "img/a b.png"),
        ("mailto:a@b.invalid", "mailto:a@b.invalid"),
    ] {
        assert_eq!(eldur_convert::quoted_reference(written), shown, "{written}");
    }
    let long = format!("data:image/gif;base64,{}", "A".repeat(500));
    assert!(eldur_convert::quoted_reference(&long).chars().count() < 60);
}

#[test]
fn a_second_page_takes_no_credentials_or_answers_from_the_first() {
    let server = Server::start(BTreeMap::from([
        ("/one.html", ok(b"<p>one</p>")),
        ("/two.html", ok(b"<p>two</p>")),
        ("/a.gif", ok(b"GIF89a")),
    ]));
    let mut resources = local_http();
    let with_user = server.url("/one.html").replace("http://", "http://u:p@");
    resources.fetch_page(&with_user).expect("the first page");
    let a = Url::parse(&server.url("/a.gif")).expect("a URL");
    assert!(resources.request_url(&a).password().is_some());
    resources
        .read(&server.url("/a.gif"), "")
        .expect("its image");

    resources
        .fetch_page(&server.url("/two.html"))
        .expect("the second page");
    assert_eq!(
        resources.request_url(&a).as_str(),
        a.as_str(),
        "the second page's requests carry no credentials"
    );
    resources
        .read(&server.url("/a.gif"), "")
        .expect("its image");
    assert_eq!(
        server.asked().iter().filter(|p| *p == "/a.gif").count(),
        2,
        "the image is asked for again, not answered from the first page"
    );
    assert_eq!(resources.fetch_count(), 2, "the page and its image");
    assert_eq!(
        resources.fetched().len(),
        2,
        "only the second page's requests"
    );
}

#[test]
fn a_second_page_has_its_own_limits_and_failures() {
    let server = Server::start(BTreeMap::from([
        ("/one.html", ok(b"<p>one</p>")),
        ("/two.html", ok(b"<p>two</p>")),
    ]));
    let mut resources = local_http_with(Limits {
        max_fetches: 1,
        ..Limits::default()
    });
    resources
        .fetch_page(&server.url("/one.html"))
        .expect("the first page");
    assert!(resources.read(&server.url("/gone.gif"), "").is_none());
    assert_eq!(resources.diagnostics().len(), 1, "the first page's cap");

    resources
        .fetch_page(&server.url("/two.html"))
        .expect("the second page, under its own cap");
    assert!(
        resources.diagnostics().is_empty(),
        "no failure of the first page is reported against the second"
    );
    assert_eq!(resources.fetch_count(), 1);
}

#[test]
fn a_page_on_this_machine_or_a_private_network_is_refused_unless_reach_is_any() {
    let server = Server::start(BTreeMap::from([("/page.html", ok(b"<p>local</p>"))]));
    let mut public = HttpResources::new();
    assert_eq!(public.reach(), Reach::Public);
    for url in [
        server.url("/page.html"),
        server.url("/page.html").replace("127.0.0.1", "localhost"),
        "http://app.localhost/".to_owned(),
        "http://10.0.0.1/".to_owned(),
        "http://192.168.1.1/".to_owned(),
        "http://169.254.169.254/latest/meta-data/".to_owned(),
        "http://100.64.0.1/".to_owned(),
        "http://0.0.0.0/".to_owned(),
        "http://[::1]/".to_owned(),
        "http://[fd00::1]/".to_owned(),
        "http://[fe80::1]/".to_owned(),
        "http://[::ffff:127.0.0.1]/".to_owned(),
        "http://[64:ff9b::a00:1]/".to_owned(),
    ] {
        let err = public.fetch_page(&url).expect_err(&url);
        let FetchError::Failed { reason, .. } = err else {
            panic!("{url}: {err:?}");
        };
        assert!(
            reason.contains("not on the public internet"),
            "{url}: {reason}"
        );
    }
    assert!(server.asked().is_empty(), "nothing reached the server");
    let mut any = local_http();
    assert!(any.fetch_page(&server.url("/page.html")).is_ok());
}

#[test]
fn a_public_page_cannot_redirect_or_refer_to_this_machine() {
    let server = Server::start(BTreeMap::from([("/secret", ok(b"secret"))]));
    let mut public = HttpResources::new();
    // A reference read as a public page's: refused before any request.
    assert!(
        public
            .read(&server.url("/secret"), "https://example.com/")
            .is_none()
    );
    assert!(
        public
            .read("http://[::1]/x.png", "https://example.com/")
            .is_none()
    );
    let diags = public.diagnostics();
    assert_eq!(diags.len(), 2, "{diags:?}");
    assert!(
        diags
            .iter()
            .all(|d| d.message.contains("not on the public internet")),
        "{diags:?}"
    );
    assert_eq!(public.fetch_count(), 0);
    assert!(server.asked().is_empty());
}

/// Plays the public internet in the tests below: 127.0.0.1, where the
/// local server is, and nothing else.
fn only_the_server_is_public(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

/// Resolves `public.test` to the server and every other name to
/// 127.0.0.2, which [`only_the_server_is_public`] counts as private.
#[derive(Debug)]
struct Names;

impl ureq::unversioned::resolver::Resolver for Names {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        _: &ureq::config::Config,
        _: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let ip = if uri.host() == Some("public.test") {
            Ipv4Addr::LOCALHOST
        } else {
            Ipv4Addr::new(127, 0, 0, 2)
        };
        let port = uri.port_u16().unwrap_or(80);
        let mut addrs = self.empty();
        addrs.push((ip, port).into());
        Ok(addrs)
    }
}

#[test]
fn a_redirect_or_a_name_that_leads_off_the_public_internet_is_refused() {
    // Refused before any connection, so no server needs to listen there.
    let to_private_ip = "http://127.0.0.2:9/secret";
    let to_private_name = "http://private.test:9/secret";
    let server = Server::start(BTreeMap::from([
        ("/page", ok(b"<p>hi</p>")),
        ("/to-ip", redirect(to_private_ip)),
        ("/to-name", redirect(to_private_name)),
    ]));
    let port = server.origin.rsplit(':').next().expect("a port").to_owned();
    let mut http = HttpResources::with_public(Limits::default(), only_the_server_is_public, Names);
    let at = |path: &str| format!("http://public.test:{port}{path}");
    // A name that resolves to the public stand-in is fetched: the refusals
    // below are the resolver's and the hop check's, not a broken agent.
    assert!(http.read(&at("/page"), "https://example.com/").is_some());
    // A redirect to a private address is refused at the hop, and one to a
    // name resolving to it by the resolver; neither reaches the server.
    assert!(http.read(&at("/to-ip"), "https://example.com/").is_none());
    assert!(http.read(&at("/to-name"), "https://example.com/").is_none());
    // A page given by a name that resolves only to a private address.
    assert!(
        http.read(
            &format!("http://private.test:{port}/page"),
            "https://example.com/"
        )
        .is_none()
    );
    let diags = http.diagnostics();
    assert_eq!(diags.len(), 3, "{diags:?}");
    for (diag, host) in diags
        .iter()
        .zip(["127.0.0.2", "private.test", "private.test"])
    {
        assert!(
            diag.message
                .contains(&format!("refused: {host} is not on the public internet")),
            "{diags:?}"
        );
    }
    assert_eq!(server.asked(), ["/page", "/to-ip", "/to-name"]);
}

#[test]
fn a_name_that_resolves_only_to_this_machine_is_refused_by_the_resolver() {
    use ureq::unversioned::resolver::Resolver as _;
    let config = ureq::config::Config::default();
    let resolve = |uri: &str| {
        PublicOnly::default().resolve(
            &uri.parse().expect("a URI"),
            &config,
            ureq::unversioned::transport::NextTimeout::default(),
        )
    };
    // `localhost` is a name, not a literal: what refuses it here is the
    // address it resolves to, as for any name a page writes.
    let refused = resolve("http://localhost:9/secret").expect_err("localhost is this machine");
    assert!(
        matches!(&refused, ureq::Error::Other(e) if e.to_string().contains("not on the public internet")),
        "{refused:?}"
    );
    let kept = resolve("http://93.184.215.14:80/").expect("a public address");
    assert!(kept.iter().all(|addr| is_public(addr.ip())), "{kept:?}");
}

#[test]
fn public_addresses_are_told_from_the_rest() {
    for ip in [
        "93.184.215.14",
        "1.1.1.1",
        "2606:4700::1111",
        "::ffff:8.8.8.8",
    ] {
        assert!(is_public(ip.parse().expect("an address")), "{ip}");
    }
    for ip in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "192.168.0.1",
        "169.254.0.1",
        "100.100.0.1",
        "192.0.0.1",
        "198.18.0.1",
        "192.0.2.1",
        "240.0.0.1",
        "255.255.255.255",
        "224.0.0.1",
        "::1",
        "::",
        "fc00::1",
        "fe80::1",
        "fec0::1",
        "2001:db8::1",
        "ff02::1",
        "::ffff:10.0.0.1",
        "64:ff9b::7f00:1",
        "::127.0.0.1",
        "2002:7f00:1::1",
        "2002:a00:1::1",
        "2001:0:4136:e378:8000:63bf:3fff:fdd2",
        "100::1",
    ] {
        assert!(!is_public(ip.parse().expect("an address")), "{ip}");
    }
}

#[test]
fn a_background_whose_sheet_is_unknown_never_reaches_local_when_a_sheet_was_fetched() {
    let server = Server::start(BTreeMap::new());
    let mut mixed = MixedResources {
        local: Local::default(),
        remote: local_http(),
    };
    let sheet = server.url("/css/site.css");
    // The page is local, one of its sheets fetched: `secret.png` may be the
    // fetched sheet's, so the local page's base is not tried.
    assert!(
        mixed
            .read_any("secret.png", &["/home/me/page.html", &sheet])
            .is_none()
    );
    assert!(mixed.local.0.is_empty(), "{:?}", mixed.local.0);
    // With no fetched sheet among the bases, the local page reads its own.
    assert!(
        mixed
            .read_any("logo.png", &["/home/me/page.html"])
            .is_some()
    );
    assert_eq!(mixed.local.0, ["logo.png"]);
}

#[test]
fn a_remote_image_a_fetching_resolver_refused_is_not_handed_to_qt() {
    assert!(HttpResources::new().fetches_web());
    assert!(MixedResources::new(Local::default()).fetches_web());
    let input = Input {
        html: r#"<img src="http://10.0.0.1/admin.png" width="10" height="10">
            <div style="width:10px;height:10px;background-image:url(http://169.254.169.254/x.png)"></div>"#
            .to_owned(),
        url: "https://example.com/page.html".to_owned(),
    };
    let mut resources = HttpResources::new();
    let output = eldur_convert::convert(&input, &Options::default(), &mut resources);
    let qml = output.main_qml().expect("Main.qml");
    assert!(
        !qml.contains("10.0.0.1") && !qml.contains("169.254.169.254"),
        "{qml}"
    );
    let text = output
        .diagnostics
        .iter()
        .chain(&resources.diagnostics())
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("not on the public internet"), "{text}");
    assert_eq!(resources.fetch_count(), 0);
}
