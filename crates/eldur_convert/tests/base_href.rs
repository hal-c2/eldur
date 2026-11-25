//! `<base href>` sets the document's base URL (HTML §2.5.1), against which
//! every relative URL in the document resolves: style sheets, and also
//! `<script src>` and `<img src>`.

use std::collections::BTreeMap;
use std::path::Path;

use eldur_convert::{Input, Options, Resource, Resources, Severity};

/// Files by absolute path; a reference is joined to its base's directory.
struct Site {
    files: BTreeMap<String, Vec<u8>>,
    asked: Vec<String>,
}

impl Resources for Site {
    fn read(&mut self, url: &str, base: &str) -> Option<Resource> {
        let path = if url.starts_with('/') {
            url.to_owned()
        } else {
            let dir = Path::new(base).parent().unwrap_or(Path::new("/"));
            dir.join(url).to_string_lossy().into_owned()
        };
        self.asked.push(path.clone());
        self.files.get(&path).map(|bytes| Resource {
            resolved: path,
            bytes: bytes.clone(),
        })
    }
}

/// A 1×1 GIF.
const DOT_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

#[test]
fn scripts_and_images_resolve_against_the_base_href() {
    let mut site = Site {
        files: BTreeMap::from([
            ("/site/static/dot.gif".to_owned(), DOT_GIF.to_vec()),
            (
                "/site/static/app.js".to_owned(),
                b"document.getElementById('t').textContent = 'From the base';".to_vec(),
            ),
            (
                "/site/static/site.css".to_owned(),
                b"h1 { color: #123456; }".to_vec(),
            ),
        ]),
        asked: Vec::new(),
    };
    let html = r#"<!doctype html><html><head><base href="static/">
<link rel="stylesheet" href="site.css"></head><body>
<h1 id="t">Title</h1>
<img src="dot.gif" width="1" height="1" alt="">
<script src="app.js"></script>
</body></html>"#;
    let out = eldur_convert::convert(
        &Input {
            html: html.to_owned(),
            url: "/site/index.html".to_owned(),
        },
        &Options::default(),
        &mut site,
    );

    let warnings: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity >= Severity::Warning)
        .map(ToString::to_string)
        .collect();
    assert!(
        warnings.is_empty(),
        "every reference is found under the base (asked {:?}): {warnings:#?}",
        site.asked
    );
    let qml = out.main_qml().expect("Main.qml");
    assert!(qml.contains("#123456"), "the sheet applies:\n{qml}");
    assert!(
        out.files.iter().any(|f| f.contents == DOT_GIF),
        "the image is copied: {:?}",
        out.files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );
    let page_js = out
        .files
        .iter()
        .find(|f| f.path == Path::new("page.js"))
        .map(|f| String::from_utf8_lossy(&f.contents).into_owned())
        .expect("page.js");
    assert!(page_js.contains("From the base"), "{page_js}");
}

#[test]
fn an_image_an_absolute_base_makes_remote_is_left_for_qt_to_load() {
    // A local resolver serves no web URL: the image is passed to Qt as the
    // base made it, as `<img src="https://…">` is.
    let mut site = Site {
        files: BTreeMap::from([("/site/logo.png".to_owned(), DOT_GIF.to_vec())]),
        asked: Vec::new(),
    };
    let html = r#"<!doctype html><html><head><base href="https://cdn.example.com/img/">
</head><body><img src="logo.png" width="1" height="1" alt=""></body></html>"#;
    let out = eldur_convert::convert(
        &Input {
            html: html.to_owned(),
            url: "/site/index.html".to_owned(),
        },
        &Options::default(),
        &mut site,
    );
    let warnings: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity >= Severity::Warning)
        .map(ToString::to_string)
        .collect();
    assert!(warnings.is_empty(), "{warnings:#?}");
    let qml = out.main_qml().expect("Main.qml");
    assert!(
        qml.contains("https://cdn.example.com/img/logo.png"),
        "the image keeps the URL the base gives it:\n{qml}"
    );
    assert!(
        !out.files.iter().any(|f| f.contents == DOT_GIF),
        "the local file beside the page is not the image"
    );
}
