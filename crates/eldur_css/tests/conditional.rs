//! `conditional_styles`: the cascade under every combination of script-toggled
//! classes and pseudo-states. Expected values come from Chromium: each
//! combination was written as a static page and read with
//! `tools/visual/computed.mjs` (see `conditional/README.md`).

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use eldur_css::{
    ComputedStyle, Condition, ConditionalStyles, Display, LengthAuto, Origin, PseudoState, Rgba,
    Structural, Stylesheet, Viewport, class_candidates, compute_styles, conditional_styles,
    open_class_candidates, structural_pseudos, styled_classes,
};
use eldur_dom::{Document, NodeId};

fn chromium() -> &'static HashMap<String, serde_json::Map<String, serde_json::Value>> {
    static ORACLE: OnceLock<HashMap<String, serde_json::Map<String, serde_json::Value>>> =
        OnceLock::new();
    ORACLE.get_or_init(|| {
        include_str!("conditional/chromium.jsonl")
            .lines()
            .map(|line| {
                let mut obj: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(line).expect("oracle line is JSON");
                let name = obj
                    .remove("name")
                    .expect("name")
                    .as_str()
                    .expect("name is a string")
                    .to_owned();
                (name, obj)
            })
            .collect()
    })
}

fn parse_px(v: &str) -> f32 {
    v.strip_suffix("px")
        .expect("px value")
        .parse()
        .expect("number")
}

fn parse_rgb(v: &str) -> Rgba {
    let inner = v
        .strip_prefix("rgb(")
        .and_then(|s| s.strip_suffix(')'))
        .expect("rgb()");
    let c: Vec<u8> = inner
        .split(',')
        .map(|p| p.trim().parse().expect("channel"))
        .collect();
    Rgba::new(c[0], c[1], c[2], 255)
}

fn margin_px(m: &LengthAuto) -> f32 {
    match m {
        LengthAuto::Length(l) => l.as_px().expect("px margin"),
        LengthAuto::Auto => panic!("auto margin"),
    }
}

/// Asserts every property Chromium reported for `case` against `style`.
fn assert_chromium(style: &ComputedStyle, case: &str) {
    let expected = chromium()
        .get(case)
        .unwrap_or_else(|| panic!("no oracle case {case:?}"));
    for (prop, value) in expected {
        let v = value.as_str().expect("string value");
        let ctx = format!("{case}: {prop}");
        match prop.as_str() {
            "color" => assert_eq!(style.color, parse_rgb(v), "{ctx}"),
            "opacity" => assert_eq!(style.opacity, v.parse::<f32>().expect("opacity"), "{ctx}"),
            "display" => {
                let d = match v {
                    "none" => Display::None,
                    "block" => Display::Block,
                    "inline" => Display::Inline,
                    "list-item" => Display::ListItem,
                    other => panic!("display {other}"),
                };
                assert_eq!(style.display, d, "{ctx}");
            }
            "font-size" => assert_eq!(style.font_size, parse_px(v), "{ctx}"),
            "font-weight" => assert_eq!(
                style.font_weight,
                v.parse::<u16>().expect("weight"),
                "{ctx}"
            ),
            "padding-top" => assert_eq!(style.padding.top.as_px(), Some(parse_px(v)), "{ctx}"),
            "padding-right" => {
                assert_eq!(style.padding.right.as_px(), Some(parse_px(v)), "{ctx}");
            }
            "padding-bottom" => {
                assert_eq!(style.padding.bottom.as_px(), Some(parse_px(v)), "{ctx}");
            }
            "padding-left" => assert_eq!(style.padding.left.as_px(), Some(parse_px(v)), "{ctx}"),
            "margin-top" => assert_eq!(margin_px(&style.margin.top), parse_px(v), "{ctx}"),
            "margin-right" => assert_eq!(margin_px(&style.margin.right), parse_px(v), "{ctx}"),
            "margin-bottom" => assert_eq!(margin_px(&style.margin.bottom), parse_px(v), "{ctx}"),
            "margin-left" => assert_eq!(margin_px(&style.margin.left), parse_px(v), "{ctx}"),
            other => panic!("unchecked property {other}"),
        }
    }
}

struct Page {
    doc: Document,
    sheets: Vec<Stylesheet>,
}

impl Page {
    fn new(css: &str, body: &str) -> Self {
        let doc = Document::parse(&format!("<!DOCTYPE html><body>{body}</body>"));
        let (sheet, diags) = Stylesheet::parse(css, Origin::Author, "test.css", None);
        assert!(diags.is_empty(), "{diags:?}");
        Self {
            doc,
            sheets: vec![sheet],
        }
    }

    fn id(&self, id: &str) -> NodeId {
        self.doc.element_by_id(id).expect(id)
    }

    fn class(&self, id: &str, class: &str) -> Condition {
        Condition::Class {
            node: self.id(id),
            class: class.to_owned(),
        }
    }

    fn state(&self, id: &str, state: PseudoState) -> Condition {
        Condition::State {
            node: self.id(id),
            state,
        }
    }

    fn run(&self, conditions: &[Condition]) -> ConditionalStyles {
        conditional_styles(&self.doc, &self.sheets, &Viewport::default(), conditions)
    }

    /// The style of `id` when exactly the conditions at `held` hold.
    fn style<'a>(&self, cs: &'a ConditionalStyles, id: &str, held: &[usize]) -> &'a ComputedStyle {
        let node = self.id(id);
        let entry = cs
            .elements
            .iter()
            .find(|e| e.node == node)
            .unwrap_or_else(|| panic!("#{id} has no conditional styles"));
        assert_eq!(entry.by_mask.len(), 1 << entry.depends_on.len());
        let mask = entry
            .depends_on
            .iter()
            .enumerate()
            .filter(|(_, c)| held.contains(&usize::from(**c)))
            .fold(0usize, |m, (bit, _)| m | (1 << bit));
        &entry.by_mask[mask]
    }

    fn depends_on(&self, cs: &ConditionalStyles, id: &str) -> Option<Vec<u16>> {
        let node = self.id(id);
        cs.elements
            .iter()
            .find(|e| e.node == node)
            .map(|e| e.depends_on.clone())
    }
}

#[test]
fn descendant_combinator_open_panel() {
    let p = Page::new(
        ".panel{display:none;color:rgb(10,20,30)} .open .panel{display:block;color:rgb(200,0,0)}",
        "<div id=menu><div id=panel class=panel><span id=label>x</span></div></div>",
    );
    let cs = p.run(&[p.class("menu", "open")]);
    assert_eq!(cs.conditions, vec![p.class("menu", "open")]);
    assert!(cs.diagnostics.is_empty(), "{:?}", cs.diagnostics);
    assert_eq!(
        p.depends_on(&cs, "menu"),
        None,
        "the condition element itself is unaffected"
    );
    assert_eq!(p.depends_on(&cs, "panel"), Some(vec![0]));
    assert_eq!(p.depends_on(&cs, "label"), Some(vec![0]));
    for (held, open) in [(&[][..], 0), (&[0][..], 1)] {
        assert_chromium(
            p.style(&cs, "panel", held),
            &format!("desc open={open} #panel"),
        );
        assert_chromium(
            p.style(&cs, "label", held),
            &format!("desc open={open} #label"),
        );
    }
}

#[test]
fn sibling_combinators_next_and_later() {
    let p = Page::new(
        ".on + p{color:rgb(255,0,0)} .on ~ p{font-size:20px}",
        "<div><span id=a class=tab>a</span><p id=b>b</p><p id=c>c</p></div>",
    );
    let cs = p.run(&[p.class("a", "on")]);
    assert!(cs.diagnostics.is_empty(), "{:?}", cs.diagnostics);
    assert_eq!(p.depends_on(&cs, "a"), None);
    assert_eq!(p.depends_on(&cs, "b"), Some(vec![0]));
    assert_eq!(p.depends_on(&cs, "c"), Some(vec![0]));
    for (held, on) in [(&[][..], 0), (&[0][..], 1)] {
        assert_chromium(p.style(&cs, "b", held), &format!("sib on={on} #b"));
        assert_chromium(p.style(&cs, "c", held), &format!("sib on={on} #c"));
    }
}

#[test]
fn only_the_preceding_sibling_condition_reaches_a_next_sibling_rule() {
    // `.on + p`: the `on` of a sibling two places back cannot match.
    let p = Page::new(
        ".on + p{color:rgb(255,0,0)}",
        "<div><span id=a>a</span><span id=b>b</span><p id=c>c</p></div>",
    );
    let cs = p.run(&[p.class("a", "on"), p.class("b", "on")]);
    assert_eq!(p.depends_on(&cs, "c"), Some(vec![1]));
    assert_eq!(p.depends_on(&cs, "b"), None);
}

#[test]
fn hover_together_with_a_class() {
    // Chromium was given `.hov` in place of `:hover` (same specificity).
    let p = Page::new(
        ".item{opacity:1} .item:hover{opacity:0.9} .item.done{opacity:0.6} \
         .item.done:hover{color:rgb(0,128,0)}",
        "<ul><li id=i class=item>x</li></ul>",
    );
    let cs = p.run(&[p.class("i", "done"), p.state("i", PseudoState::Hover)]);
    assert!(cs.diagnostics.is_empty(), "{:?}", cs.diagnostics);
    assert_eq!(p.depends_on(&cs, "i"), Some(vec![0, 1]));
    for (held, done, hover) in [
        (&[][..], 0, 0),
        (&[1][..], 0, 1),
        (&[0][..], 1, 0),
        (&[0, 1][..], 1, 1),
    ] {
        assert_chromium(
            p.style(&cs, "i", held),
            &format!("hover done={done} hover={hover}"),
        );
    }
}

#[test]
fn not_pseudo_class_over_a_toggled_class() {
    let p = Page::new(
        ".msg:not(.read){font-weight:700}",
        "<p id=p class=msg>x</p>",
    );
    let cs = p.run(&[p.class("p", "read")]);
    assert_eq!(p.depends_on(&cs, "p"), Some(vec![0]));
    assert_chromium(p.style(&cs, "p", &[]), "not read=0");
    assert_chromium(p.style(&cs, "p", &[0]), "not read=1");
}

#[test]
fn class_present_in_the_document_and_removed() {
    let p = Page::new(
        ".note{color:rgb(0,0,255)} .warn{color:rgb(255,165,0)}",
        "<p id=p class=\"note warn\">x</p>",
    );
    let cs = p.run(&[p.class("p", "warn")]);
    assert_eq!(p.depends_on(&cs, "p"), Some(vec![0]));
    // Bits mean "holds": index 0 is the style *without* the document's class.
    assert_chromium(&cs.elements[0].by_mask[0], "present warn=0");
    assert_chromium(&cs.elements[0].by_mask[1], "present warn=1");
    let base = compute_styles(&p.doc, &p.sheets, &Viewport::default());
    assert_eq!(base.style(p.id("p")), Some(&*cs.elements[0].by_mask[1]));
}

#[test]
fn specificity_and_source_order_between_two_class_rules() {
    let p = Page::new(
        ".b.b{color:rgb(0,128,0)} .a{color:rgb(255,0,0)} \
         .b{color:rgb(0,0,255);margin-top:3px} .a{margin-top:5px}",
        "<p id=p>x</p>",
    );
    let cs = p.run(&[p.class("p", "a"), p.class("p", "b")]);
    assert_eq!(p.depends_on(&cs, "p"), Some(vec![0, 1]));
    for (held, a, b) in [
        (&[][..], 0, 0),
        (&[0][..], 1, 0),
        (&[1][..], 0, 1),
        (&[0, 1][..], 1, 1),
    ] {
        assert_chromium(p.style(&cs, "p", held), &format!("spec a={a} b={b}"));
    }
}

#[test]
fn inherited_properties_flow_into_the_subtree() {
    let p = Page::new(
        ".big{font-size:20px;color:rgb(1,2,3)} p{padding-left:1em;margin-top:0}",
        "<div id=box><p id=t><span id=s>x</span></p></div>",
    );
    let cs = p.run(&[p.class("box", "big")]);
    // No selector mentions #t or #s with `.big`; they change through inheritance.
    assert_eq!(p.depends_on(&cs, "t"), Some(vec![0]));
    assert_eq!(p.depends_on(&cs, "s"), Some(vec![0]));
    for (held, big) in [(&[][..], 0), (&[0][..], 1)] {
        assert_chromium(p.style(&cs, "t", held), &format!("inherit big={big} #t"));
        assert_chromium(p.style(&cs, "s", held), &format!("inherit big={big} #s"));
    }
}

#[test]
fn at_most_six_conditions_per_element() {
    let p = Page::new(
        ".c1{margin-top:1px} .c2{margin-right:2px} .c3{margin-bottom:3px} \
         .c4{margin-left:4px} .c5{padding-top:5px} .c6{padding-right:6px} \
         .c7{padding-bottom:7px}",
        "<p id=p>x</p>",
    );
    let conditions: Vec<Condition> = (1..=7).map(|i| p.class("p", &format!("c{i}"))).collect();
    let cs = p.run(&conditions);
    assert_eq!(p.depends_on(&cs, "p"), Some(vec![0, 1, 2, 3, 4, 5]));
    assert_eq!(cs.elements[0].by_mask.len(), 64);
    assert!(
        cs.diagnostics
            .iter()
            .any(|d| d.message.contains("dynamic-combinations")),
        "{:?}",
        cs.diagnostics
    );
    assert_chromium(p.style(&cs, "p", &[]), "cap ");
    assert_chromium(p.style(&cs, "p", &[0, 1, 2, 3, 4, 5]), "cap 1,2,3,4,5,6");
    assert_chromium(p.style(&cs, "p", &[1, 4]), "cap 2,5");
    // The seventh condition is dropped: Chromium's "cap 1,2,3,4,5,6,7" adds
    // padding-bottom 7px, which no combination here can express.
    assert_ne!(
        chromium()["cap 1,2,3,4,5,6,7"],
        chromium()["cap 1,2,3,4,5,6"]
    );
}

#[test]
fn checked_state_condition_over_an_unchecked_checkbox() {
    // Chromium read the page with and without the `checked` attribute.
    let p = Page::new(
        "input:checked + label{color:rgb(0,128,0)}",
        "<input id=cb type=checkbox><label id=lb>l</label>",
    );
    let cs = p.run(&[p.state("cb", PseudoState::Checked)]);
    assert_eq!(p.depends_on(&cs, "lb"), Some(vec![0]));
    assert_chromium(p.style(&cs, "lb", &[]), "checked on=0");
    assert_chromium(p.style(&cs, "lb", &[0]), "checked on=1");
}

#[test]
fn custom_property_set_by_a_toggled_class() {
    let p = Page::new(
        ".dark{--fg:rgb(9,9,9)} p{color:var(--fg, rgb(1,1,1))}",
        "<div id=box><p id=t>x</p></div>",
    );
    let cs = p.run(&[p.class("box", "dark")]);
    assert_eq!(p.depends_on(&cs, "t"), Some(vec![0]));
    assert_chromium(p.style(&cs, "t", &[]), "var dark=0");
    assert_chromium(p.style(&cs, "t", &[0]), "var dark=1");
}

#[test]
fn conditions_no_selector_reads_produce_nothing() {
    let p = Page::new(
        ".x{color:rgb(1,1,1)}",
        "<div id=a><p id=b>b</p></div><p id=c>c</p>",
    );
    let cs = p.run(&[p.class("a", "unstyled"), p.state("a", PseudoState::Focus)]);
    assert!(cs.elements.is_empty(), "{:?}", cs.elements);
    assert!(cs.diagnostics.is_empty(), "{:?}", cs.diagnostics);
}

#[test]
fn has_over_a_descendant_condition_is_reported() {
    let p = Page::new(
        ".card:has(.on){color:rgb(255,0,0)}",
        "<div id=card class=card><span id=inner>x</span></div>",
    );
    let cs = p.run(&[p.class("inner", "on")]);
    assert_eq!(p.depends_on(&cs, "card"), None);
    assert!(
        cs.diagnostics
            .iter()
            .any(|d| d.message.contains("dynamic-unsupported-selector")),
        "{:?}",
        cs.diagnostics
    );
}

#[test]
fn state_variants_are_unchanged_by_conditions() {
    let p = Page::new(
        ".card:hover .title{color:rgb(255,0,0)}",
        "<div id=card class=card><p id=title class=title>x</p></div>",
    );
    let map = compute_styles(&p.doc, &p.sheets, &Viewport::default());
    let variants: Vec<_> = map.variants_of(p.id("card")).collect();
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].changed[0].0, p.id("title"));
    let cs = p.run(&[p.state("card", PseudoState::Hover)]);
    assert_eq!(p.style(&cs, "title", &[0]), &*variants[0].changed[0].1);
}

fn sheets(css: &str) -> Vec<Stylesheet> {
    vec![Stylesheet::parse(css, Origin::Author, "test.css", None).0]
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn styled_classes_include_nested_and_pseudo_element_selectors() {
    let s = sheets(
        ".a .b:not(.c) { color: red } :is(.d, p) { color: red } \
         .card:has(.e) { color: red } p.f::before { content: 'x' } \
         @media (min-width: 1px) { .g { color: red } } #id, div { color: red }",
    );
    assert_eq!(
        styled_classes(&s),
        set(&["a", "b", "c", "card", "d", "e", "f", "g"])
    );
}

#[test]
fn class_candidates_are_elements_the_rest_of_a_selector_matches() {
    let doc = Document::parse(
        "<!DOCTYPE html><ul id=l><li id=i1 class=tab>1</li><li id=i2>2</li>\
         <li id=i3 class=tab>3</li></ul><div id=d class=tab></div><p id=p>p</p>",
    );
    let id = |s| doc.element_by_id(s).expect("id");
    let s = sheets("li.tab.active { color: red } .active + p { color: blue }");
    assert_eq!(
        class_candidates(&doc, &s, "active"),
        vec![id("i1"), id("i3"), id("d")]
    );
    assert_eq!(class_candidates(&doc, &s, "missing"), Vec::<NodeId>::new());
    let s = sheets("p:not(.read) { font-weight: bold }");
    assert_eq!(class_candidates(&doc, &s, "read"), vec![id("p")]);
}

#[test]
fn open_class_candidates_co_occur_with_known_classes_or_the_tag() {
    let s = sheets(
        ".chat-message.user { color: red } .chat-message.bot { color: blue } \
         .toast.error { color: red } li.selected { color: red } div.other { color: red } \
         :is(.chat-message.system) { color: gray }",
    );
    assert_eq!(
        open_class_candidates(&s, "div", &["chat-message".to_owned()]),
        set(&["bot", "other", "system", "user"])
    );
    assert_eq!(open_class_candidates(&s, "li", &[]), set(&["selected"]));
    assert_eq!(open_class_candidates(&s, "span", &[]), BTreeSet::new());
}

/// `#x`'s 1-based place and sibling count in `layout` (`"li,x,li"`), all
/// siblings and same-tag siblings.
fn place(layout: &str) -> ((u32, u32), (u32, u32)) {
    let tags: Vec<&str> = layout
        .split(',')
        .map(|t| if t == "x" { "li" } else { t })
        .collect();
    let at = layout.split(',').position(|t| t == "x").expect("x");
    let same: Vec<usize> = (0..tags.len()).filter(|&i| tags[i] == "li").collect();
    let in_type = same.iter().position(|&i| i == at).expect("x is an li");
    let n = |v: usize| u32::try_from(v).expect("small");
    ((n(at + 1), n(tags.len())), (n(in_type + 1), n(same.len())))
}

#[test]
fn structural_pseudo_classes_as_position_conditions() {
    let css = "li{color:rgb(1,1,1);margin-bottom:4px} li:first-child{color:rgb(200,0,0)} \
               li:last-child{margin-bottom:0} li:nth-child(even){padding-left:10px} \
               li:nth-last-child(2){font-size:20px} li:only-child{font-weight:700} \
               li:first-of-type{padding-top:3px} li:nth-of-type(3n){padding-right:6px} \
               li:first-child span{opacity:0.5}";
    // The document's own place does not matter: every place is a combination.
    let p = Page::new(
        css,
        "<div><li>a</li><li id=x><span id=s>x</span></li><li>b</li></div>",
    );
    let positions: Vec<Structural> = structural_pseudos(&p.sheets).into_iter().collect();
    assert_eq!(
        positions,
        vec![
            Structural::FirstChild,
            Structural::LastChild,
            Structural::OnlyChild,
            Structural::NthChild { a: 2, b: 0 },
            Structural::NthLastChild { a: 0, b: 2 },
            Structural::FirstOfType,
            Structural::NthOfType { a: 3, b: 0 },
        ]
    );
    let conditions: Vec<Condition> = positions
        .iter()
        .map(|&test| Condition::Structural {
            node: p.id("x"),
            test,
        })
        .collect();
    let cs = p.run(&conditions);
    // Seven conditions on #x: one past the cap of six.
    assert!(
        cs.diagnostics
            .iter()
            .all(|d| d.message.contains("dynamic-combinations")),
        "{:?}",
        cs.diagnostics
    );
    let combined = p.depends_on(&cs, "x").expect("#x depends on its place");
    for layout in [
        "x",
        "x,li,li",
        "li,x,li",
        "li,li,x",
        "li,li,x,li,li",
        "p,x,li",
        "p,li,x",
        "li,li,p,x",
    ] {
        let ((index, count), (type_index, type_count)) = place(layout);
        let held: Vec<usize> = positions
            .iter()
            .enumerate()
            .filter(|(_, pos)| {
                if pos.of_type() {
                    pos.matches(type_index, type_count)
                } else {
                    pos.matches(index, count)
                }
            })
            .map(|(i, _)| i)
            .collect();
        // A condition past the cap keeps the document's value (#x is the
        // second of three li, by either count), so only layouts that agree
        // on it are exact.
        let agrees = (0..positions.len())
            .filter(|c| !combined.contains(&u16::try_from(*c).expect("small")))
            .all(|c| held.contains(&c) == positions[c].matches(2, 3));
        if !agrees {
            continue;
        }
        assert_chromium(p.style(&cs, "x", &held), &format!("pos {layout} #x"));
        assert_chromium(p.style(&cs, "s", &held), &format!("pos {layout} #s"));
    }
}

#[test]
fn position_conditions_without_a_cap() {
    let css = "li{color:rgb(1,1,1);margin-bottom:4px} li:first-child{color:rgb(200,0,0)} \
               li:last-child{margin-bottom:0} li:nth-child(even){padding-left:10px} \
               li:first-child span{opacity:0.5}";
    let p = Page::new(
        css,
        "<div><li>a</li><li id=x><span id=s>x</span></li><li>b</li></div>",
    );
    let positions = [
        Structural::FirstChild,
        Structural::LastChild,
        Structural::NthChild { a: 2, b: 0 },
    ];
    let conditions: Vec<Condition> = positions
        .iter()
        .map(|&test| Condition::Structural {
            node: p.id("x"),
            test,
        })
        .collect();
    let cs = p.run(&conditions);
    assert!(cs.diagnostics.is_empty(), "{:?}", cs.diagnostics);
    assert_eq!(p.depends_on(&cs, "x"), Some(vec![0, 1, 2]));
    // The span reads its parent's `:first-child`, and inherits its colour;
    // its parent's margin and padding do not reach it.
    assert_eq!(p.depends_on(&cs, "s"), Some(vec![0]));
    let mut seen = BTreeSet::new();
    for layout in ["x", "x,li,li", "li,x,li", "li,li,x", "li,li,x,li,li"] {
        let ((index, count), _) = place(layout);
        let held: Vec<usize> = (0..positions.len())
            .filter(|&i| positions[i].matches(index, count))
            .collect();
        seen.insert(held.clone());
        // The page's other rules (`:only-child`, `:nth-last-child(2)`, …)
        // are in Chromium's sheet only where they do not apply here.
        let x = p.style(&cs, "x", &held);
        let s = p.style(&cs, "s", &held);
        let want = &chromium()[&format!("pos {layout} #x")];
        assert_eq!(
            x.color,
            parse_rgb(want["color"].as_str().expect("color")),
            "{layout}"
        );
        assert_eq!(
            margin_px(&x.margin.bottom),
            parse_px(want["margin-bottom"].as_str().expect("margin")),
            "{layout}"
        );
        assert_eq!(
            x.padding.left.as_px(),
            Some(parse_px(want["padding-left"].as_str().expect("padding"))),
            "{layout}"
        );
        let want = &chromium()[&format!("pos {layout} #s")];
        assert_eq!(
            s.color,
            parse_rgb(want["color"].as_str().expect("color")),
            "{layout}"
        );
        assert_eq!(
            s.opacity,
            want["opacity"]
                .as_str()
                .expect("opacity")
                .parse::<f32>()
                .expect("number"),
            "{layout}"
        );
    }
    assert_eq!(seen.len(), 5, "every layout is a different combination");
}
