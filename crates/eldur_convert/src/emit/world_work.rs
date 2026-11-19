//! How many times a page with state rules is emitted (eldur #9): worlds
//! whose styles change only paint in subtrees apart share one emission, so
//! the count does not grow with the page. Counted, not timed, so it is the
//! same on every machine.

use std::path::{Path, PathBuf};

use super::dynamic::{ONE_WORLD_AT_A_TIME, PAGE_EMISSIONS};
use crate::{Input, Options, Output, Resource, Resources, convert, convert_str};

/// The page of eldur #9: `items` cards over 40 classes, each class with a
/// `:hover` and a `:focus-within` rule.
fn cards(items: usize) -> String {
    let rules: Vec<String> = (0..40)
        .map(|i| {
            format!(
                ".c{i}:hover{{color:rgb({},0,0);background:#eee}} \
                 .c{i}:focus-within{{outline:1px solid red}}",
                i % 255
            )
        })
        .collect();
    let items: Vec<String> = (0..items)
        .map(|i| {
            format!(
                "<div class=\"c{}\"><p>Item {i} <a href=\"#a{i}\">link</a> <code>x</code></p></div>",
                i % 40
            )
        })
        .collect();
    format!(
        "<!DOCTYPE html><html><head><style>body{{font:16px sans-serif}} \
         a:hover{{text-decoration:underline}} {}</style></head><body>{}</body></html>",
        rules.join(" "),
        items.join("\n")
    )
}

fn emissions(page: &str) -> usize {
    let before = PAGE_EMISSIONS.with(std::cell::Cell::get);
    let _ = convert_str(page, &Options::default());
    PAGE_EMISSIONS.with(std::cell::Cell::get) - before
}

#[test]
fn worlds_that_repaint_subtrees_apart_share_one_emission() {
    let small = emissions(&cards(40));
    let large = emissions(&cards(80));
    // The document's world, then one for every card's `:hover`: each
    // repaints its own card, so all of them are emitted together.
    assert_eq!(large, small, "twice the cards, twice the emissions");
    assert!(
        small <= 3,
        "{small} emissions of a page with one kind of state"
    );
}

/// Pages whose worlds can and cannot share an emission.
const BATCHING: &[&str] = &[
    // Repainted siblings, a nested hover and colour inherited by inline text.
    "<style>.a:hover{background:#eee;color:red} .b:hover{border-color:blue}\
     .b{border:1px solid red} li:hover{text-decoration:underline red}</style>\
     <div class=a><p>x <b>y</b></p></div><div class=a><p class=b>z</p></div>\
     <ul><li>1<li>2<li>3</ul>",
    // A hover that changes size, and one that changes paint beside it.
    "<style>.w:hover{width:200px} .p:hover{color:green}</style>\
     <div class=w>w</div><div class=p>p</div><div class=w>w2</div>",
    // A sibling's state repaints the next one.
    "<style>.a:hover + .b{color:red} .a:hover{color:blue}</style>\
     <p class=a>0</p><p class=b>1</p><p class=a>2</p><p class=b>3</p>",
    // The body's own state repaints the window; a child's repaints itself.
    "<style>body:hover{background:#ddd} p:hover{color:red}</style><p>x</p><p>y</p>",
    // Paint that appears and disappears, and links inside one paragraph.
    "<style>.t:hover{background:transparent} .t{background:red} .o:hover{color:transparent}\
     a:hover{color:red}</style><div class=t>t</div><div class=o>o</div>\
     <p>a <a href=#1>one</a> and <a href=#2>two</a></p>",
    // Focus and checked states with an inherited colour.
    "<style>div:focus-within{background:#eef} input:checked + label{color:red}</style>\
     <div><input></div><div><input type=checkbox><label>l</label></div>\
     <div><input type=checkbox checked><label>m</label></div>",
];

/// Files next to a corpus page, as the corpus snapshot test reads them.
struct CaseFiles {
    dir: PathBuf,
}

impl Resources for CaseFiles {
    fn read(&mut self, url: &str, base: &str) -> Option<Resource> {
        if url.contains("://") || url.starts_with("data:") {
            return None;
        }
        let base_dir = Path::new(base).parent().unwrap_or(Path::new(""));
        let relative = base_dir.join(url);
        let bytes = std::fs::read(self.dir.join(&relative)).ok()?;
        Some(Resource {
            resolved: relative.to_string_lossy().into_owned(),
            bytes,
        })
    }
}

fn everything(out: &Output) -> Vec<String> {
    let mut all: Vec<String> = out
        .files
        .iter()
        .map(|f| {
            format!(
                "{}\n{}",
                f.path.display(),
                String::from_utf8_lossy(&f.contents)
            )
        })
        .collect();
    all.sort();
    all.extend(out.diagnostics.iter().map(ToString::to_string));
    all
}

fn both_ways(html: &str, dir: Option<&Path>) -> (Vec<String>, Vec<String>) {
    let run = |one: bool| {
        ONE_WORLD_AT_A_TIME.with(|c| c.set(one));
        let input = Input {
            html: html.to_owned(),
            url: "index.html".to_owned(),
        };
        let out = match dir {
            Some(dir) => convert(
                &input,
                &Options::default(),
                &mut CaseFiles {
                    dir: dir.to_owned(),
                },
            ),
            None => convert_str(html, &Options::default()),
        };
        ONE_WORLD_AT_A_TIME.with(|c| c.set(false));
        everything(&out)
    };
    (run(false), run(true))
}

#[test]
fn shared_emissions_convert_as_one_world_at_a_time_does() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("tests/corpus exists")
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.join("index.html").is_file())
        .collect();
    cases.sort();
    assert!(cases.len() > 50, "the corpus is there");
    for case in &cases {
        let html = std::fs::read_to_string(case.join("index.html")).expect("readable");
        let (shared, alone) = both_ways(&html, Some(case));
        assert_eq!(shared, alone, "{}", case.display());
    }
    for (i, page) in BATCHING.iter().enumerate() {
        let page = format!("<!DOCTYPE html>{page}");
        let (shared, alone) = both_ways(&page, None);
        assert_eq!(shared, alone, "BATCHING[{i}]");
        // The crafted pages do have worlds, so the comparison is not empty.
        assert!(
            alone
                .iter()
                .any(|f| f.starts_with("Main.qml\n") && f.contains(" ? ")),
            "BATCHING[{i}] binds a state"
        );
    }
    let (shared, alone) = both_ways(&cards(40), None);
    assert_eq!(shared, alone, "cards");
}
