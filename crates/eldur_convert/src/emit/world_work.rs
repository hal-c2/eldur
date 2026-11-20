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

/// `items` cards whose `:hover` shows a block inside, each card a world of
/// its own.
fn cards_showing(items: usize) -> String {
    let cards: String = (0..items)
        .map(|i| format!("<div class=c{i}><p>card {i}</p><p class=h>more</p></div>"))
        .collect();
    let rules: String = (0..items)
        .map(|i| format!(".c{i}:hover .h{{display:block}} "))
        .collect();
    format!("<!DOCTYPE html><style>.h{{display:none}} {rules}</style>{cards}")
}

/// `lists` lists of the given tag, each with an item its own `:hover`
/// shows.
fn lists_showing(tag: &str, lists: usize) -> String {
    let body: String = (0..lists)
        .map(|i| format!("<{tag} class=u{i}><li>a</li><li class=h>x</li><li>b</li></{tag}>"))
        .collect();
    let rules: String = (0..lists)
        .map(|i| format!(".u{i}:hover .h{{display:list-item}} "))
        .collect();
    format!("<!DOCTYPE html><style>.h{{display:none}} {rules}</style>{body}")
}

#[test]
fn hidden_items_of_separate_lists_are_not_combined() {
    // Items of different lists cannot change each other's numbers, and a
    // `ul`'s discs number nothing: one world per list, not 2^9.
    for tag in ["ul", "ol"] {
        let n = emissions(&lists_showing(tag, 9));
        assert!(
            n <= 10,
            "{n} emissions of nine {tag}s with a hidden item each"
        );
    }
    // Two hidden items of one `ol` number the items after them together.
    let page = "<!DOCTYPE html><style>.x,.y{display:none} ol:hover .x{display:list-item} \
                .a:hover + .y{display:list-item}</style><ol><li class=a>a</li><li class=y>y</li>\
                <li>b</li><li class=x>x</li><li>c</li></ol>";
    let n = emissions(page);
    assert!(
        n >= 4,
        "{n} emissions: the document's, each item's, and both"
    );
}

#[test]
fn a_property_over_thirteen_conditions_still_binds() {
    // Each `aN` shows the hidden item after it, so the last item's marker
    // depends on all thirteen hovers at once: too many for a joint group,
    // but each hover alone must still renumber it.
    let pairs: String = (0..13)
        .map(|i| format!("<li class=a{i}>a{i}</li><li class=h{i}>h{i}</li>"))
        .collect();
    let rules: String = (0..13)
        .map(|i| format!(".h{i}{{display:none}} .a{i}:hover + .h{i}{{display:list-item}} "))
        .collect();
    let page = format!("<!DOCTYPE html><style>{rules}</style><ol>{pairs}<li>c</li></ol>");
    let out = convert_str(&page, &Options::default());
    let qml = out.main_qml().expect("Main.qml");
    // The last item's marker comes last; the hidden `h12` before it shows
    // "15. " too, over the other twelve hovers.
    let line = qml
        .lines()
        .rfind(|l| l.contains("\"15. \""))
        .unwrap_or_else(|| panic!("the last marker is not bound to the hovers:\n{qml}"));
    let hovers: std::collections::BTreeSet<&str> = line
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.ends_with("Hover"))
        .collect();
    assert_eq!(hovers.len(), 13, "{line}");
    // The list's thirteen hovers are too many to combine, and the page says
    // so: two items shown at once would number "16." only where a world has
    // both.
    let warned = out
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .any(|d| d.contains("13 classes and states show or hide numbered elements"));
    assert!(warned, "no warning: {:?}", out.diagnostics);
}

#[test]
fn a_full_group_leaves_room_for_other_conditions() {
    // Nine hover-shown items in one list make a group of 2^9 worlds, all
    // of MAX_WORLDS. A paragraph after the list, shown by a tenth hover,
    // still needs its own world to show at all.
    let pairs: String = (0..9)
        .map(|i| format!("<li class=a{i}>a{i}</li><li class=h{i}>h{i}</li>"))
        .collect();
    let rules: String = (0..9)
        .map(|i| format!(".h{i}{{display:none}} .a{i}:hover + .h{i}{{display:list-item}} "))
        .collect();
    let page = format!(
        "<!DOCTYPE html><style>{rules} .q{{display:none}} .b:hover + .q{{display:block}}</style>\
         <ol>{pairs}<li>c</li></ol><p class=b>b</p><p class=q>q</p>"
    );
    let out = convert_str(&page, &Options::default());
    let qml = out.main_qml().expect("Main.qml");
    assert!(
        qml.lines()
            .any(|l| l.contains("visible:") && l.contains("bHover.hovered")),
        "q does not show while b is hovered:\n{qml}"
    );
    // The group no longer fits beside the paragraph's world, and says so.
    let warned = out
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .any(|d| d.contains("9 classes and states show or hide numbered elements"));
    assert!(warned, "no warning: {:?}", out.diagnostics);
}

#[test]
fn small_groups_fit_before_a_large_one() {
    // A list of eight hover-shown items (2^8 worlds) first, then 38 lists
    // of three (2^3 each): 522 worlds in all, past MAX_WORLDS. Taken in
    // page order, the large group fits and the last three small ones do
    // not; taken smallest first, only the large one is left out, and 38
    // lists number right instead of 36.
    let list = |prefix: &str, n: usize| -> (String, String) {
        let items: String = (0..n)
            .map(|i| format!("<li class={prefix}a{i}>a</li><li class={prefix}h{i}>h</li>"))
            .collect();
        let rules: String = (0..n)
            .map(|i| {
                format!(
                    ".{prefix}h{i}{{display:none}} .{prefix}a{i}:hover + .{prefix}h{i}{{display:list-item}} "
                )
            })
            .collect();
        (format!("<ol>{items}<li>c</li></ol>"), rules)
    };
    let (mut lists, mut rules) = list("big", 8);
    for k in 0..38 {
        let (l, r) = list(&format!("s{k}"), 3);
        lists.push_str(&l);
        rules.push_str(&r);
    }
    let page = format!("<!DOCTYPE html><style>{rules}</style>{lists}");
    let out = convert_str(&page, &Options::default());
    let left_out: Vec<String> = out
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .filter(|d| d.contains("classes and states show or hide numbered elements"))
        .collect();
    assert!(
        left_out.len() == 1 && left_out[0].contains(" 8 classes"),
        "{left_out:?}"
    );
}

#[test]
fn a_mostly_covered_group_fits_before_costlier_small_ones() {
    // Two lists of four hover-shown items each come first. In each, `t1`
    // is recoloured by hovers 1–3 and `t2` by 0, 2 and 3, so the elements'
    // own worlds already hold every combination of the four hovers except
    // at most the 4 that have both 0 and 1. Then come 80 lists of three,
    // each adding 4 worlds, far more than MAX_WORLDS holds. By condition
    // count the threes go first and fill the budget, and a four is left
    // out. By the worlds each adds, no three goes before a four.
    let list = |prefix: &str, n: usize, covered: bool| -> (String, String) {
        let mut items: String = (0..n)
            .map(|i| format!("<li class={prefix}a{i}>a</li><li class={prefix}h{i}>h</li>"))
            .collect();
        let mut rules: String = (0..n)
            .map(|i| {
                format!(
                    ".{prefix}h{i}{{display:none}} .{prefix}a{i}:hover + .{prefix}h{i}{{display:list-item}} "
                )
            })
            .collect();
        if covered {
            items.push_str(&format!(
                "<li class={prefix}t1>t</li><li class={prefix}t2>t</li>"
            ));
            for (t, hovers) in [("t1", [1, 2, 3]), ("t2", [0, 2, 3])] {
                let selectors: Vec<String> = hovers
                    .iter()
                    .map(|i| format!(".{prefix}a{i}:hover ~ .{prefix}{t}"))
                    .collect();
                rules.push_str(&format!("{}{{color:red}} ", selectors.join(", ")));
            }
        }
        (format!("<ol>{items}<li>c</li></ol>"), rules)
    };
    let (mut lists, mut rules) = (String::new(), String::new());
    for k in 0..2 {
        let (l, r) = list(&format!("w{k}"), 4, true);
        lists.push_str(&l);
        rules.push_str(&r);
    }
    for k in 0..80 {
        let (l, r) = list(&format!("s{k}"), 3, false);
        lists.push_str(&l);
        rules.push_str(&r);
    }
    let page = format!("<!DOCTYPE html><style>{rules}</style>{lists}");
    let out = convert_str(&page, &Options::default());
    let left_out: Vec<String> = out
        .diagnostics
        .iter()
        .map(ToString::to_string)
        .filter(|d| d.contains("classes and states show or hide numbered elements"))
        .collect();
    // The budget binds: some lists of three are left out.
    assert!(
        left_out.iter().any(|d| d.contains(" 3 classes")),
        "{left_out:?}"
    );
    assert!(
        !left_out.iter().any(|d| d.contains(" 4 classes")),
        "{left_out:?}"
    );
}

/// A table of `cells` cells, four to a row, each recoloured by its own
/// `:hover`, with borders `collapse` or `separate`.
fn table_hovers(cells: usize, borders: &str) -> String {
    let rows: String = (0..cells.div_ceil(4))
        .map(|r| {
            let row: String = (r * 4..(r * 4 + 4).min(cells))
                .map(|i| format!("<td class=c{i}>{i}</td>"))
                .collect();
            format!("<tr>{row}</tr>")
        })
        .collect();
    let rules: String = (0..cells)
        .map(|i| format!(".c{i}:hover{{color:red}} "))
        .collect();
    format!(
        "<!DOCTYPE html><style>table{{border-collapse:{borders}}} \
         td{{border:1px solid black}} {rules}</style><table>{rows}</table>"
    )
}

#[test]
fn a_world_that_shows_or_hides_an_element_is_emitted_alone() {
    // Showing an element moves what follows it, which is not a repaint:
    // the document's world, then each card's world on its own.
    for items in [4, 8] {
        assert_eq!(emissions(&cards_showing(items)), 1 + items, "{items} cards");
    }
}

#[test]
fn a_collapsed_table_s_cells_repaint_the_table() {
    // A collapsed table draws half of each cell's border in its neighbour,
    // so its cells' worlds are never emitted together.
    for cells in [8, 16] {
        assert_eq!(
            emissions(&table_hovers(cells, "collapse")),
            1 + cells,
            "{cells} collapsed cells"
        );
    }
    // Apart, each cell draws its own borders and they share an emission.
    assert_eq!(
        emissions(&table_hovers(16, "separate")),
        emissions(&table_hovers(8, "separate")),
        "separate cells"
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
    // A state that hides an element moves what follows it, beside one that
    // repaints.
    "<style>.a:hover .x{display:none} p{margin:10px} .b:hover{color:red}</style>\
     <div class=a><p class=x>x</p><p>y</p></div><div class=b><p>z</p></div>\
     <div class=a><p class=x>v</p><p>w</p></div>",
    // A collapsed table draws half of a cell's border in its neighbour.
    "<style>table{border-collapse:collapse} td{border:1px solid black}\
     .a{border-right:2px solid black} .a:hover{border-right-color:red}\
     .b:hover{background:yellow}</style>\
     <table><tr><td class=a>A<td class=b>B<tr><td>C<td>D</table>",
    // Showing a block moves its batch-mate: the next paragraph's top margin
    // reads whether the block is shown. Parity only: the shown block's id
    // makes that margin a binding, which reshapes and takes the fallback even
    // without the hide guard. The guard itself is pinned by
    // `a_world_that_shows_or_hides_an_element_is_emitted_alone`.
    "<style>.x{display:none;margin:30px} .a:hover + .x{display:block} p{margin:10px}\
     .b:hover{color:red}</style><p>0</p><p class=a>a</p><p class=x id=x>x</p><p class=b>b</p>",
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

/// Every file, byte for byte, and every diagnostic.
type Everything = (Vec<(PathBuf, Vec<u8>)>, Vec<String>);

fn everything(out: &Output) -> Everything {
    let mut files: Vec<(PathBuf, Vec<u8>)> = out
        .files
        .iter()
        .map(|f| (f.path.clone(), f.contents.clone()))
        .collect();
    files.sort();
    let diagnostics = out.diagnostics.iter().map(ToString::to_string).collect();
    (files, diagnostics)
}

fn binds_a_state((files, _): &Everything) -> bool {
    files.iter().any(|(path, bytes)| {
        path == Path::new("Main.qml") && String::from_utf8_lossy(bytes).contains(" ? ")
    })
}

fn both_ways(html: &str, dir: Option<&Path>) -> (Everything, Everything) {
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
        assert!(binds_a_state(&alone), "BATCHING[{i}] binds a state");
    }
    let (shared, alone) = both_ways(&cards(40), None);
    assert_eq!(shared, alone, "cards");
}
