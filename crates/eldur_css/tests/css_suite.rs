//! End-to-end integration and performance test suite for `eldur_css` Part A:
//! CSS parsing, Selectors 4, Media Queries 4, CSS Cascade 5, custom properties,
//! dynamic state variants, stylesheet/@import collection, and the WHATWG §15 UA sheet.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eldur_css::{
    Diagnostic, Display, Easing, Length, LengthAuto, LoadedSheet, Origin, PseudoState, Rgba,
    Severity, Stylesheet, StylesheetLoader, Viewport, collect_stylesheets, compute_styles,
};
use eldur_dom::Document;

fn px(v: f32) -> LengthAuto {
    LengthAuto::Length(Length::px(v))
}

fn parse_author(css: &str) -> (Stylesheet, Vec<Diagnostic>) {
    Stylesheet::parse(css, Origin::Author, "test.css", None)
}

#[derive(Default)]
struct MemoryLoader {
    files: HashMap<String, String>,
}

impl MemoryLoader {
    fn with_file(mut self, url: &str, css: &str) -> Self {
        self.files.insert(url.to_owned(), css.to_owned());
        self
    }
}

impl StylesheetLoader for MemoryLoader {
    fn load(&mut self, href: &str) -> Option<LoadedSheet> {
        self.files.get(href).map(|text| LoadedSheet {
            text: text.clone(),
            url: href.to_owned(),
        })
    }
}

#[test]
fn selector_matching_combinators_attributes_and_nth_formulas() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html>
          <body>
            <div id="sib-wrap">
              <div class="uncle"></div>
              <div class="parent"><span id="sib-child" class="child">x</span></div>
            </div>
            <ul id="list">
              <li id="li1" class="item special" data-tag="alpha beta" lang-code="en-US" data-url="https://example.com/app.qml">1</li>
              <li id="li2" class="item" data-tag="beta" lang-code="fr">2</li>
              <li id="li3" class="item special" data-tag="gamma">3</li>
              <li id="li4" class="item">4</li>
              <li id="li5" class="item special">5</li>
            </ul>
            <div id="empty-box"><!-- comment only --></div>
            <div id="ws-box"> </div>
            <div id="types">
              <em>a</em>
              <span id="s1">1</span>
              <b>b</b>
              <span id="s2">2</span>
              <i id="only-i">only</i>
            </div>
            <input id="inp-upper" TYPE="TEXT">
          </body>
        </html>"#,
    );

    let css = r#"
        /* Sibling followed by child combinator: `.uncle` is NOT an ancestor of `.child` */
        .uncle + .parent > .child { color: rgb(12, 34, 56); }

        /* Attribute operators & flags */
        [data-tag~="alpha"][lang-code|="en"][data-url^="https://"][data-url$=".qml"][data-url*="example"] {
            margin-top: 11px;
        }
        input[type="text"] { margin-top: 7px; }
        input[type="text" s] { margin-top: 99px; }
        [data-tag="BETA" i] { margin-top: 22px; }

        /* :nth-child formulas: odd, even, -n+3, An+B of S */
        #list > li:nth-child(even) { font-size: 14px; }
        #list > li:nth-child(-n+3) { color: rgb(10, 20, 30); }
        #list > li:nth-child(2n+1 of .special) { color: rgb(200, 100, 50); }
        #list > li:nth-last-child(1 of .special) { margin-top: 55px; }

        /* :empty and *-of-type */
        #empty-box:empty { margin-top: 9px; }
        #ws-box:empty { margin-top: 9px; }
        #types > span:first-of-type { margin-top: 3px; }
        #types > span:last-of-type { margin-top: 4px; }
        #types > i:only-of-type { margin-top: 5px; }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    let sib_child = doc.element_by_id("sib-child").expect("sib-child");
    assert_eq!(
        map.style(sib_child).expect("style").color,
        Rgba::new(12, 34, 56, 255)
    );

    let li1 = doc.element_by_id("li1").expect("li1");
    let li2 = doc.element_by_id("li2").expect("li2");
    let li3 = doc.element_by_id("li3").expect("li3");
    let li4 = doc.element_by_id("li4").expect("li4");
    let li5 = doc.element_by_id("li5").expect("li5");

    assert_eq!(map.style(li1).expect("li1").margin.top, px(11.0));
    // li1 is 1st of .special -> matches 2n+1 of .special
    assert_eq!(
        map.style(li1).expect("li1").color,
        Rgba::new(200, 100, 50, 255)
    );
    // li2 is even & [data-tag="BETA" i]
    assert_eq!(map.style(li2).expect("li2").font_size, 14.0);
    assert_eq!(map.style(li2).expect("li2").margin.top, px(22.0));
    assert_eq!(
        map.style(li2).expect("li2").color,
        Rgba::new(10, 20, 30, 255)
    );
    // li3 is in -n+3, and 2nd of .special (does not match 2n+1 of .special)
    assert_eq!(
        map.style(li3).expect("li3").color,
        Rgba::new(10, 20, 30, 255)
    );
    // li4 is even
    assert_eq!(map.style(li4).expect("li4").font_size, 14.0);
    // li5 is 3rd of .special (matches 2n+1 of .special) and 1st from end of .special
    assert_eq!(
        map.style(li5).expect("li5").color,
        Rgba::new(200, 100, 50, 255)
    );
    assert_eq!(map.style(li5).expect("li5").margin.top, px(55.0));

    let inp = doc.element_by_id("inp-upper").expect("inp-upper");
    assert_eq!(map.style(inp).expect("inp").margin.top, px(7.0));

    let empty_box = doc.element_by_id("empty-box").expect("empty-box");
    let ws_box = doc.element_by_id("ws-box").expect("ws-box");
    assert_eq!(map.style(empty_box).expect("empty").margin.top, px(9.0));
    assert_eq!(map.style(ws_box).expect("ws").margin.top, px(0.0));

    let s1 = doc.element_by_id("s1").expect("s1");
    let s2 = doc.element_by_id("s2").expect("s2");
    let only_i = doc.element_by_id("only-i").expect("only-i");
    assert_eq!(map.style(s1).expect("s1").margin.top, px(3.0));
    assert_eq!(map.style(s2).expect("s2").margin.top, px(4.0));
    assert_eq!(map.style(only_i).expect("only-i").margin.top, px(5.0));
}

#[test]
fn selector_logical_relational_has_and_form_link_lang_dir_pseudos() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html lang="en-US" dir="rtl">
          <body>
            <section id="sec-desc"><div><span class="badge">!</span></div></section>
            <section id="sec-child"><p class="lead">Lead</p></section>
            <div>
              <h2 id="h-before-note">Heading</h2>
              <p class="note">Note</p>
            </div>
            <div>
              <h2 id="h-before-later">Heading 2</h2>
              <div>gap</div>
              <p class="later">Later</p>
            </div>

            <a id="a-link" href="/home">Home</a>
            <a id="a-nolink">Anchor</a>

            <form>
              <fieldset disabled>
                <legend><input id="in-legend" type="text" required></legend>
                <input id="in-disabled-fs" type="text">
              </fieldset>
              <input id="chk" type="checkbox" checked>
              <input id="ph" type="text" placeholder="Search...">
              <input id="ro" type="text" readonly value="fixed">
              <button id="default-submit" type="submit">Send</button>
              <progress id="indet-prog"></progress>
            </form>

            <div id="ar-auto" dir="auto">مرحبا</div>
            <div id="fr-sub" lang="fr-CA"><span id="fr-span">Bonjour</span></div>
          </body>
        </html>"#,
    );

    let css = r#"
        /* :has() with all 4 relative combinators */
        section:has(.badge) { margin-top: 10px; }
        section:has(> .lead) { margin-top: 20px; }
        h2:has(+ .note) { margin-top: 30px; }
        h2:has(~ .later) { margin-top: 40px; }

        /* Forgiving :is() / :where() and :not() */
        a:any-link { color: rgb(1, 2, 3); }
        a:not([href]) { color: rgb(9, 8, 7); }

        /* Form states (note: per WHATWG HTML, disabled and non-text inputs match `:read-only`) */
        input:enabled:required:read-write { margin-top: 15px; }
        input:disabled { margin-top: 25px; }
        input[type="checkbox"]:checked { margin-top: 35px; }
        input:placeholder-shown:optional { margin-top: 45px; }
        input[type="text"]:enabled:read-only { margin-top: 55px; }
        button:default { margin-top: 65px; }
        progress:indeterminate { margin-top: 75px; }

        /* :lang() and :dir() */
        :lang(en) > body { font-size: 18px; }
        span:lang(fr) { font-size: 22px; }
        #ar-auto:dir(rtl) { margin-top: 85px; }

        /* Never-matching dynamic/target pseudos in base style */
        :target, :defined { display: none; }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    for (id, expected_mt) in [
        ("sec-desc", 10.0),
        ("sec-child", 20.0),
        ("h-before-note", 30.0),
        ("h-before-later", 40.0),
        ("in-legend", 15.0),
        ("in-disabled-fs", 25.0),
        ("chk", 35.0),
        ("ph", 45.0),
        ("ro", 55.0),
        ("default-submit", 65.0),
        ("indet-prog", 75.0),
        ("ar-auto", 85.0),
    ] {
        let node = doc.element_by_id(id).expect(id);
        assert_eq!(
            map.style(node).expect(id).margin.top,
            px(expected_mt),
            "mismatch on #{id}"
        );
    }

    let a_link = doc.element_by_id("a-link").expect("a-link");
    let a_nolink = doc.element_by_id("a-nolink").expect("a-nolink");
    assert_eq!(
        map.style(a_link).expect("a-link").color,
        Rgba::new(1, 2, 3, 255)
    );
    assert_eq!(
        map.style(a_nolink).expect("a-nolink").color,
        Rgba::new(9, 8, 7, 255)
    );

    let body = doc.body().expect("body");
    let fr_span = doc.element_by_id("fr-span").expect("fr-span");
    assert_eq!(map.style(body).expect("body").font_size, 18.0);
    assert_eq!(map.style(fr_span).expect("fr-span").font_size, 22.0);
}

#[test]
fn selector_specificity_ordering_in_cascade() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <ul>
            <li id="hero" class="featured">Item</li>
          </ul>
        </body></html>"#,
    );

    // Ordered from HIGHEST specificity at top of stylesheet to LOWEST at bottom, so higher
    // specificity must win purely by specificity against later source order!
    let css = r#"
        li:nth-child(1 of .featured#hero) { margin-top: 50px; } /* (1, 1, 1) */
        :is(.nope, #hero) { margin-top: 40px; }                 /* (1, 0, 0) */
        .featured { margin-top: 30px; }                         /* (0, 1, 0) */
        li { margin-top: 20px; }                                /* (0, 0, 1) */
        :where(#hero.featured) { margin-top: 10px; }            /* (0, 0, 0) */
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());
    let hero = doc.element_by_id("hero").expect("hero");
    assert_eq!(map.style(hero).expect("hero").margin.top, px(50.0));

    // Confirm `:where(#hero)` loses even to bare `li` when `li` comes first.
    let (sheet2, _) = parse_author(
        r#"
        li { margin-top: 20px; }
        :where(#hero) { margin-top: 10px; }
        "#,
    );
    let map2 = compute_styles(&doc, &[sheet2], &Viewport::default());
    assert_eq!(map2.style(hero).expect("hero").margin.top, px(20.0));
}

#[test]
fn media_queries_level_4_and_cascade_filtering() {
    let doc = Document::parse(r#"<!doctype html><html><body><div id="box"></div></body></html>"#);
    let css = r#"
        #box { margin-top: 1px; font-size: 16px; color: rgb(0, 0, 0); }

        @media print {
            #box { margin-top: 999px; }
        }
        @media screen and (400px < width <= 800px) and (orientation: portrait) {
            #box { margin-top: 20px; }
        }
        @media (width >= 60em) and (aspect-ratio >= 16/9) {
            #box { margin-top: 40px; }
        }
        @media (prefers-color-scheme: dark) and (prefers-reduced-motion: reduce) {
            #box { color: rgb(240, 240, 255); }
        }
        @media (hover: hover) and (pointer: fine) and (resolution: 1dppx) {
            #box { font-size: 20px; }
        }
        @media (unknown-future-feature: 42) {
            #box { font-size: 99px; }
        }
        @media not all and (unknown-future-feature: 42) {
            /* `not (all and unknown)` -> `not unknown` -> unknown -> false! */
            #box { font-size: 88px; }
        }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let box_id = doc.element_by_id("box").expect("box");

    // 1. Wide 1280x720 (80em, 16:9), light, no-preference
    let wide_vp = Viewport {
        width: 1280.0,
        height: 720.0,
        prefers_dark: false,
        prefers_reduced_motion: false,
    };
    let map_wide = compute_styles(&doc, std::slice::from_ref(&sheet), &wide_vp);
    assert_eq!(map_wide.style(box_id).expect("box").margin.top, px(40.0));
    assert_eq!(map_wide.style(box_id).expect("box").font_size, 20.0);
    assert_eq!(
        map_wide.style(box_id).expect("box").color,
        Rgba::new(0, 0, 0, 255)
    );

    // 2. Portrait 600x900, dark + reduced-motion
    let mobile_dark_vp = Viewport {
        width: 600.0,
        height: 900.0,
        prefers_dark: true,
        prefers_reduced_motion: true,
    };
    let map_mobile = compute_styles(&doc, &[sheet], &mobile_dark_vp);
    assert_eq!(map_mobile.style(box_id).expect("box").margin.top, px(20.0));
    assert_eq!(map_mobile.style(box_id).expect("box").font_size, 20.0);
    assert_eq!(
        map_mobile.style(box_id).expect("box").color,
        Rgba::new(240, 240, 255, 255)
    );
}

#[test]
fn supports_and_css_nesting() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <div id="card" class="card active">
            <span id="label" class="label">Text</span>
          </div>
        </body></html>"#,
    );

    let css = r#"
        @supports (display: flex) and (not (display: grid-bogus)) and selector(.card:has(> .label)) {
            .card {
                margin-top: 5px;
                &.active {
                    color: rgb(10, 120, 220);
                }
                .label {
                    font-size: 21px;
                }
                @media (min-width: 800px) {
                    font-size: 19px;
                }
                /* Declaration after nested rules must beat `margin-top: 5px` above */
                margin-top: 15px;
            }
        }
        @supports selector(::selection) or (unknown-prop: 10px) {
            .card { display: none; }
        }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    let card = doc.element_by_id("card").expect("card");
    let label = doc.element_by_id("label").expect("label");
    assert_eq!(map.style(card).expect("card").margin.top, px(15.0));
    assert_eq!(map.style(card).expect("card").font_size, 19.0);
    assert_eq!(
        map.style(card).expect("card").color,
        Rgba::new(10, 120, 220, 255)
    );
    assert_eq!(map.style(label).expect("label").font_size, 21.0);
}

#[test]
fn cascade_origins_importance_layers_and_inline_style() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <div id="layered" class="box"></div>
          <div id="imp-layered" class="box"></div>
          <div id="inline-normal" style="margin-top: 30px; color: rgb(1, 2, 3)"></div>
          <div id="inline-imp" style="margin-top: 90px !important"></div>
        </body></html>"#,
    );

    let (sheet1, d1) = Stylesheet::parse(
        r#"
        @layer reset, framework, theme;
        @layer theme {
            #layered { margin-top: 20px; }
            #imp-layered { margin-top: 20px !important; }
        }
        @layer reset {
            @layer sub {
                #imp-layered { margin-top: 77px !important; }
            }
            #layered { margin-top: 5px; }
            #imp-layered { margin-top: 66px !important; }
        }
        "#,
        Origin::Author,
        "s1.css",
        None,
    );
    let (sheet2, d2) = Stylesheet::parse(
        r#"
        /* Unlayered normal beats all layered normal, even with lower specificity! */
        div.box { margin-top: 50px; }

        /* Unlayered !important LOSES to layered !important (reset.sub wins)! */
        #imp-layered { margin-top: 1px !important; }

        /* Normal #id loses to normal inline style, but !important #id beats normal inline style */
        #inline-normal { margin-top: 10px; color: rgb(250, 0, 0) !important; }

        /* Inline style !important beats author rule !important */
        #inline-imp { margin-top: 10px !important; }
        "#,
        Origin::Author,
        "s2.css",
        None,
    );
    assert!(d1.is_empty() && d2.is_empty());

    let map = compute_styles(&doc, &[sheet1, sheet2], &Viewport::default());

    let layered = doc.element_by_id("layered").expect("layered");
    let imp_layered = doc.element_by_id("imp-layered").expect("imp-layered");
    let inline_normal = doc.element_by_id("inline-normal").expect("inline-normal");
    let inline_imp = doc.element_by_id("inline-imp").expect("inline-imp");

    assert_eq!(map.style(layered).expect("layered").margin.top, px(50.0));
    assert_eq!(
        map.style(imp_layered).expect("imp-layered").margin.top,
        px(77.0)
    );
    assert_eq!(
        map.style(inline_normal).expect("inline-normal").margin.top,
        px(30.0)
    );
    assert_eq!(
        map.style(inline_normal).expect("inline-normal").color,
        Rgba::new(250, 0, 0, 255)
    );
    assert_eq!(
        map.style(inline_imp).expect("inline-imp").margin.top,
        px(90.0)
    );
}

#[test]
fn css_wide_keywords_inherit_initial_unset_revert_and_revert_layer() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body style="margin-top: 18px; color: rgb(10, 20, 30)">
          <h1 id="h1-revert">Heading</h1>
          <div id="inh" style="margin-top: inherit"></div>
          <div id="init" style="color: initial; font-size: initial"></div>
          <div id="unset-box" style="margin-top: 99px; color: rgb(200, 0, 0)"></div>
          <div id="rev-layer" class="btn"></div>
        </body></html>"#,
    );

    let css = r#"
        h1 { font-size: 12px; display: inline; }
        #h1-revert { font-size: revert; display: revert; }

        #unset-box { margin-top: unset !important; color: unset !important; }

        @layer base {
            .btn { margin-top: 8px; }
        }
        @layer theme {
            #rev-layer { margin-top: 24px; }
            .btn { margin-top: 16px; }
        }
        /* Unlayered rule reverts back to `@layer theme`, where `#rev-layer` has `24px`,
           unless `#rev-layer` inside `@layer theme` also reverts! Let's revert unlayered -> theme: */
        #rev-layer { margin-top: revert-layer; }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    // `revert` rolls back to UA `h1 { display: block; font-size: 2em }` -> 32px, Block.
    let h1 = doc.element_by_id("h1-revert").expect("h1-revert");
    assert_eq!(map.style(h1).expect("h1").font_size, 32.0);
    assert_eq!(map.style(h1).expect("h1").display, Display::Block);

    let inh = doc.element_by_id("inh").expect("inh");
    assert_eq!(map.style(inh).expect("inh").margin.top, px(18.0));

    let init = doc.element_by_id("init").expect("init");
    assert_eq!(map.style(init).expect("init").color, Rgba::BLACK);
    assert_eq!(map.style(init).expect("init").font_size, 16.0);

    let unset_box = doc.element_by_id("unset-box").expect("unset-box");
    assert_eq!(map.style(unset_box).expect("unset").margin.top, px(0.0));
    assert_eq!(
        map.style(unset_box).expect("unset").color,
        Rgba::new(10, 20, 30, 255)
    );

    let rev_layer = doc.element_by_id("rev-layer").expect("rev-layer");
    assert_eq!(
        map.style(rev_layer).expect("rev-layer").margin.top,
        px(24.0)
    );
}

#[test]
fn custom_properties_var_fallbacks_shorthands_and_cycles() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html>
          <body style="color: rgb(5, 15, 25)">
            <div id="good"></div>
            <div id="cycled"></div>
          </body>
        </html>"#,
    );

    let css = r#"
        :root {
            --space: 14px;
            --SPACE: 99px;
            --empty: ;
            --derived: var(--space);
            --a: var(--b, 10px);
            --b: var(--a, 20px);
            --c1: var(--c2);
            --c2: var(--c3);
            --c3: var(--c1);
        }
        #good {
            /* Case-sensitive custom properties + nested fallback + shorthand expansion */
            margin: var(--derived) var(--empty);
            font-size: var(--missing, var(--also-missing, 26px));
            color: var(--a, rgb(0, 200, 100));
        }
        #cycled {
            margin-top: 50px;
            /* `--c1` is guaranteed-invalid and has no fallback -> behaves as `unset`! */
            color: var(--c1);
            font-size: var(--a);
        }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    let good = doc.element_by_id("good").expect("good");
    let gs = map.style(good).expect("good style");
    assert_eq!(gs.margin.top, px(14.0));
    assert_eq!(gs.font_size, 26.0);
    assert_eq!(gs.color, Rgba::new(0, 200, 100, 255));
    assert_eq!(
        gs.custom_properties.get("--SPACE").map(String::as_str),
        Some("99px")
    );
    assert!(!gs.custom_properties.contains_key("--a"));
    assert!(!gs.custom_properties.contains_key("--c1"));

    let cycled = doc.element_by_id("cycled").expect("cycled");
    let cs = map.style(cycled).expect("cycled style");
    // Inherited properties (`color`, `font-size`) behave as `unset` -> inherit from `<body>`.
    assert_eq!(cs.color, Rgba::new(5, 15, 25, 255));
    assert_eq!(cs.font_size, 16.0);
}

#[test]
fn shorthand_vs_longhand_source_order() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <div id="long-wins"></div>
          <div id="short-wins"></div>
          <div id="var-long-wins"></div>
          <div id="var-short-wins"></div>
        </body></html>"#,
    );

    let css = r#"
        :root { --one: 1px; }
        #long-wins { margin: 1px; margin-top: 2px; }
        #short-wins { margin-top: 2px; margin: 1px; }
        #var-long-wins { margin: var(--one); margin-top: 2px; }
        #var-short-wins { margin-top: 2px; margin: var(--one); }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    assert_eq!(
        map.style(doc.element_by_id("long-wins").expect("id"))
            .expect("s")
            .margin
            .top,
        px(2.0)
    );
    assert_eq!(
        map.style(doc.element_by_id("short-wins").expect("id"))
            .expect("s")
            .margin
            .top,
        px(1.0)
    );
    assert_eq!(
        map.style(doc.element_by_id("var-long-wins").expect("id"))
            .expect("s")
            .margin
            .top,
        px(2.0)
    );
    assert_eq!(
        map.style(doc.element_by_id("var-short-wins").expect("id"))
            .expect("s")
            .margin
            .top,
        px(1.0)
    );
}

#[test]
fn blockification_display_none_subtrees_and_rem_root_font_size() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html id="html-root" style="display: inline; font-size: 20px">
          <body>
            <h1 id="h1">Big</h1>
            <div id="flex-parent" style="display: flex">
              <span id="direct-flex-item">A</span>
              <span id="second-flex-item" style="display: inline">B</span>
              <div><span id="nested-non-flex-item">C</span></div>
            </div>
            <div id="gone" style="display: none">
              <p id="gone-child"><span id="gone-grandchild">hidden</span></p>
            </div>
          </body>
        </html>"#,
    );

    let map = compute_styles(&doc, &[], &Viewport::default());

    let html = doc.element_by_id("html-root").expect("html");
    assert_eq!(map.style(html).expect("html").display, Display::Block);
    assert_eq!(map.style(html).expect("html").font_size, 20.0);

    // `h1` is `2em` from UA sheet -> 2 * 20px = 40px.
    let h1 = doc.element_by_id("h1").expect("h1");
    assert_eq!(map.style(h1).expect("h1").font_size, 40.0);

    // Direct flex children are blockified, whereas grandchildren inside a block child stay inline!
    let item1 = doc.element_by_id("direct-flex-item").expect("item1");
    let item2 = doc.element_by_id("second-flex-item").expect("item2");
    let nested = doc.element_by_id("nested-non-flex-item").expect("nested");
    assert_eq!(map.style(item1).expect("item1").display, Display::Block);
    assert_eq!(map.style(item2).expect("item2").display, Display::Block);
    assert_eq!(map.style(nested).expect("nested").display, Display::Inline);

    // `display: none` subtree has `None` for the element and all descendants.
    for id in ["gone", "gone-child", "gone-grandchild"] {
        let n = doc.element_by_id(id).expect(id);
        assert!(map.get(n).is_none(), "expected None for #{id}");
    }
}

#[test]
fn pseudo_elements_keyframes_and_font_faces() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <input id="inp" placeholder="Email" style="font-size: 18px">
          <div id="div">Hello</div>
        </body></html>"#,
    );

    let css = r#"
        input::placeholder { color: rgb(120, 130, 140); }
        div::placeholder { color: rgb(1, 2, 3); }
        div::before { color: rgb(255, 0, 0); }

        /* Unsupported pseudo-elements deduplicated per pseudo-element */
        p::selection { color: white; }
        span::selection { color: yellow; }

        @keyframes pulse {
            from, 100% { margin-top: 0px; animation-timing-function: linear; }
            50% { margin-top: 1.5em; color: rgb(255, 0, 128); }
        }

        @font-face {
            font-family: "Fira Sans";
            src: local("Fira Sans"), url("../fonts/fira.woff2") format("woff2");
            font-weight: bold;
            font-style: normal;
        }
    "#;

    let (sheet, diags) = Stylesheet::parse(
        css,
        Origin::Author,
        "ui.css",
        Some("https://example.test/assets/css/ui.css"),
    );
    // 1 unsupported diagnostic: `::selection` (deduplicated).
    assert_eq!(diags.len(), 1, "diags: {diags:?}");
    assert!(diags.iter().all(|d| d.severity == Severity::Unsupported));

    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    let inp = doc.element_by_id("inp").expect("inp");
    let inp_el = map.get(inp).expect("inp el");
    let ph = inp_el.placeholder.as_deref().expect("input placeholder");
    assert_eq!(ph.color, Rgba::new(120, 130, 140, 255));
    assert_eq!(ph.font_size, 18.0);

    // `div` does not get `::placeholder`, and `::before` without `content` is `None`.
    let div = doc.element_by_id("div").expect("div");
    let div_el = map.get(div).expect("div el");
    assert!(div_el.placeholder.is_none());
    assert!(div_el.before.is_none());

    // `@keyframes` & `StyleMap::keyframe_style`
    let pulse = map.keyframes.get("pulse").expect("pulse keyframes");
    assert_eq!(pulse.len(), 3);
    assert_eq!(pulse[0].offset, 0.0);
    assert_eq!(pulse[0].easing, Some(Easing::Linear));
    assert_eq!(pulse[1].offset, 0.5);
    let kf_mid = map
        .keyframe_style(inp, &pulse[1])
        .expect("keyframe style on visible element");
    assert_eq!(kf_mid.color, Rgba::new(255, 0, 128, 255));

    // `@font-face`
    assert_eq!(map.font_faces.len(), 1);
    assert_eq!(map.font_faces[0].family, "Fira Sans");
    assert_eq!(map.font_faces[0].sources, vec!["../fonts/fira.woff2"]);
    assert_eq!(map.font_faces[0].weight, Some(700));
    assert!(!map.font_faces[0].italic);
    assert_eq!(
        map.font_faces[0].base_url.as_deref(),
        Some("https://example.test/assets/css/ui.css")
    );
}

#[test]
fn state_variants_hover_focus_within_visited_and_multi_dynamic_rejection() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <button id="btn" class="btn">Click</button>
          <div id="card" class="card">
            <h2 id="card-title" class="title"><span id="card-sub">Sub</span></h2>
          </div>
          <ul id="nav"><li id="nav-item"><span id="nav-label">Item</span></li></ul>
          <a id="nav-link" href="/docs">Docs</a>
        </body></html>"#,
    );

    let css = r#"
        .btn { margin-top: 4px; }
        .btn:hover { margin-top: 12px; }

        .card .title { color: rgb(20, 20, 20); }
        .card:hover .title { color: rgb(220, 80, 0); }

        #nav > li:focus-within > span { margin-top: 16px; }

        a:link { color: rgb(0, 100, 200); }
        a:visited { color: rgb(120, 0, 180); }

        /* Multi-element dynamic selector must be dropped with `Unsupported` diagnostic */
        .card:hover .btn:focus { margin-top: 99px; }
    "#;

    let (sheet, diags) = parse_author(css);
    assert_eq!(diags.len(), 1, "diags: {diags:?}");
    assert_eq!(diags[0].severity, Severity::Unsupported);

    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    // 1. `.btn:hover`
    let btn = doc.element_by_id("btn").expect("btn");
    let btn_vars: Vec<_> = map.variants_of(btn).collect();
    assert_eq!(btn_vars.len(), 1);
    assert_eq!(btn_vars[0].state, PseudoState::Hover);
    assert_eq!(
        btn_vars[0].changed,
        vec![(btn, {
            let mut s = map.style(btn).expect("btn").clone();
            s.margin.top = px(12.0);
            Box::new(s)
        })]
    );

    // 2. `.card:hover .title` updates `#card-title` AND inherits `color` onto `#card-sub`!
    let card = doc.element_by_id("card").expect("card");
    let card_title = doc.element_by_id("card-title").expect("card-title");
    let card_sub = doc.element_by_id("card-sub").expect("card-sub");
    let card_vars: Vec<_> = map.variants_of(card).collect();
    assert_eq!(card_vars.len(), 1);
    assert_eq!(card_vars[0].state, PseudoState::Hover);
    let changed_nodes: Vec<_> = card_vars[0].changed.iter().map(|(n, _)| *n).collect();
    assert!(changed_nodes.contains(&card_title));
    assert!(changed_nodes.contains(&card_sub));

    // 3. `li:focus-within > span`
    let nav_item = doc.element_by_id("nav-item").expect("nav-item");
    let nav_label = doc.element_by_id("nav-label").expect("nav-label");
    let li_vars: Vec<_> = map.variants_of(nav_item).collect();
    assert_eq!(li_vars.len(), 1);
    assert_eq!(li_vars[0].state, PseudoState::FocusWithin);
    assert_eq!(li_vars[0].changed.len(), 1);
    assert_eq!(li_vars[0].changed[0].0, nav_label);
    assert_eq!(li_vars[0].changed[0].1.margin.top, px(16.0));

    // 4. `a:visited`
    let nav_link = doc.element_by_id("nav-link").expect("nav-link");
    let link_vars: Vec<_> = map.variants_of(nav_link).collect();
    assert_eq!(link_vars.len(), 1);
    assert_eq!(link_vars[0].state, PseudoState::Visited);
    assert_eq!(link_vars[0].changed[0].1.color, Rgba::new(120, 0, 180, 255));
}

// A script can show the list, so the hover condition has to exist from the start.
#[test]
fn state_variants_list_conditions_under_a_display_none_ancestor() {
    let doc = Document::parse(r#"<ul id="list"><li id="item">x</li></ul>"#);
    let (sheet, _) = parse_author("ul { display: none; } li:hover { color: rgb(1, 2, 3); }");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());
    let item = doc.element_by_id("item").expect("item");
    let variants: Vec<_> = map.variants_of(item).collect();
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].state, PseudoState::Hover);
    assert!(variants[0].changed.is_empty());
}

#[test]
fn state_variants_nested_has_nth_child_of_and_logical_combinations() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <div id="card" class="card">
            <h2 id="card-title" class="title"><span id="card-sub">Sub</span></h2>
          </div>
          <div id="other-card" class="card">
            <h2 id="other-title" class="title">Other</h2>
          </div>
          <div id="plain-card" class="card">
            <p id="plain-text">No title</p>
          </div>

          <ul id="list">
            <li id="li-skip">Skip</li>
            <li id="li-first" class="item"><span id="li-first-label">First</span></li>
            <li id="li-second" class="item"><span id="li-second-label">Second</span></li>
          </ul>

          <section id="panel" class="panel">
            <button id="panel-trigger" class="trigger">Open</button>
            <p id="panel-body">Body</p>
          </section>
        </body></html>"#,
    );

    let css = r#"
        .card { margin-top: 4px; color: rgb(10, 20, 30); }
        .card:has(.title:hover) { margin-top: 16px; color: rgb(220, 80, 0); }

        ul > li { margin-top: 2px; color: rgb(30, 40, 50); }
        li:nth-child(1 of .item:hover) { margin-top: 14px; color: rgb(0, 180, 90); }
        li:nth-last-child(1 of .item:focus) { margin-top: 24px; }

        .panel { margin-top: 6px; color: rgb(40, 50, 60); }
        .panel:is(:has(.trigger:focus)) { margin-top: 18px; color: rgb(140, 20, 200); }

        /* Multi-element dynamic conditions must still be rejected! */
        .card:hover:has(.title:focus) { margin-top: 99px; }
        .card:has(.title:hover) .trigger:focus { margin-top: 99px; }
        li:nth-child(1 of .card:hover .item:focus) { margin-top: 99px; }
    "#;

    let (sheet, diags) = parse_author(css);
    assert_eq!(
        diags.len(),
        3,
        "expected 3 multi-dynamic diagnostics: {diags:?}"
    );
    assert!(diags.iter().all(|d| d.severity == Severity::Unsupported));

    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    let card = doc.element_by_id("card").expect("card");
    let card_title = doc.element_by_id("card-title").expect("card-title");
    let card_sub = doc.element_by_id("card-sub").expect("card-sub");
    let other_card = doc.element_by_id("other-card").expect("other-card");
    let other_title = doc.element_by_id("other-title").expect("other-title");
    let plain_card = doc.element_by_id("plain-card").expect("plain-card");
    let plain_text = doc.element_by_id("plain-text").expect("plain-text");

    let li_skip = doc.element_by_id("li-skip").expect("li-skip");
    let li_first = doc.element_by_id("li-first").expect("li-first");
    let li_first_label = doc.element_by_id("li-first-label").expect("li-first-label");
    let li_second = doc.element_by_id("li-second").expect("li-second");
    let li_second_label = doc
        .element_by_id("li-second-label")
        .expect("li-second-label");

    let panel = doc.element_by_id("panel").expect("panel");
    let panel_trigger = doc.element_by_id("panel-trigger").expect("panel-trigger");
    let panel_body = doc.element_by_id("panel-body").expect("panel-body");

    // Base styles must remain unchanged (no dynamic pseudo-classes active).
    assert_eq!(map.style(card).expect("card").margin.top, px(4.0));
    assert_eq!(
        map.style(card).expect("card").color,
        Rgba::new(10, 20, 30, 255)
    );
    assert_eq!(
        map.style(card_title).expect("card-title").color,
        Rgba::new(10, 20, 30, 255)
    );
    assert_eq!(map.style(li_first).expect("li-first").margin.top, px(2.0));
    assert_eq!(
        map.style(li_first).expect("li-first").color,
        Rgba::new(30, 40, 50, 255)
    );
    assert_eq!(map.style(li_second).expect("li-second").margin.top, px(2.0));
    assert_eq!(map.style(panel).expect("panel").margin.top, px(6.0));
    assert_eq!(
        map.style(panel).expect("panel").color,
        Rgba::new(40, 50, 60, 255)
    );

    // 1. `.card:has(.title:hover)` records condition element `#card-title` with changed styles
    // on `#card` and inherited `color` on `#card-title` and `#card-sub`, without affecting `#other-card`.
    let title_hover = map
        .variants_of(card_title)
        .find(|v| v.state == PseudoState::Hover)
        .expect("card-title hover variant");
    assert_eq!(title_hover.node, card_title);
    assert_eq!(title_hover.changed.len(), 3);
    assert_eq!(title_hover.changed[0].0, card);
    assert_eq!(title_hover.changed[0].1.margin.top, px(16.0));
    assert_eq!(title_hover.changed[0].1.color, Rgba::new(220, 80, 0, 255));
    assert_eq!(title_hover.changed[1].0, card_title);
    assert_eq!(title_hover.changed[1].1.color, Rgba::new(220, 80, 0, 255));
    assert_eq!(title_hover.changed[2].0, card_sub);
    assert_eq!(title_hover.changed[2].1.color, Rgba::new(220, 80, 0, 255));

    // `#other-title` only changes `#other-card` and `#other-title`, not `#card`.
    let other_title_hover = map
        .variants_of(other_title)
        .find(|v| v.state == PseudoState::Hover)
        .expect("other-title hover variant");
    assert_eq!(other_title_hover.node, other_title);
    assert_eq!(other_title_hover.changed.len(), 2);
    assert_eq!(other_title_hover.changed[0].0, other_card);
    assert_eq!(other_title_hover.changed[1].0, other_title);

    // `#plain-card` (no `.title` descendant) has no variants.
    assert!(map.variants_of(plain_card).next().is_none());
    assert!(map.variants_of(plain_text).next().is_none());

    // 2. `li:nth-child(1 of .item:hover)` matches `#li-first` (not `#li-skip` or `#li-second`),
    // and `li:nth-last-child(1 of .item:focus)` matches `#li-second` (not `#li-first`).
    let li_first_hover = map
        .variants_of(li_first)
        .find(|v| v.state == PseudoState::Hover)
        .expect("li-first hover variant");
    assert_eq!(li_first_hover.node, li_first);
    assert_eq!(li_first_hover.changed.len(), 2);
    assert_eq!(li_first_hover.changed[0].0, li_first);
    assert_eq!(li_first_hover.changed[0].1.margin.top, px(14.0));
    assert_eq!(
        li_first_hover.changed[0].1.color,
        Rgba::new(0, 180, 90, 255)
    );
    assert_eq!(li_first_hover.changed[1].0, li_first_label);
    assert_eq!(
        li_first_hover.changed[1].1.color,
        Rgba::new(0, 180, 90, 255)
    );
    assert!(
        !map.variants_of(li_first)
            .any(|v| v.state == PseudoState::Focus)
    );

    let li_second_focus = map
        .variants_of(li_second)
        .find(|v| v.state == PseudoState::Focus)
        .expect("li-second focus variant");
    assert_eq!(li_second_focus.node, li_second);
    assert_eq!(li_second_focus.changed.len(), 1);
    assert_eq!(li_second_focus.changed[0].0, li_second);
    assert_eq!(li_second_focus.changed[0].1.margin.top, px(24.0));
    assert!(
        !map.variants_of(li_second)
            .any(|v| v.state == PseudoState::Hover)
    );
    assert!(map.variants_of(li_second_label).next().is_none());
    assert!(map.variants_of(li_skip).next().is_none());

    // 3. Nested logical combination `.panel:is(:has(.trigger:focus))` records condition element
    // `#panel-trigger` and updates `#panel` plus inherited `color` on `#panel-body`. The button
    // keeps the UA sheet's own colour: Chromium reports rgb(0, 0, 0) for it under a coloured parent.
    let trigger_focus = map
        .variants_of(panel_trigger)
        .find(|v| v.state == PseudoState::Focus)
        .expect("panel-trigger focus variant");
    assert_eq!(trigger_focus.node, panel_trigger);
    assert_eq!(trigger_focus.changed.len(), 2);
    assert_eq!(trigger_focus.changed[0].0, panel);
    assert_eq!(trigger_focus.changed[0].1.margin.top, px(18.0));
    assert_eq!(
        trigger_focus.changed[0].1.color,
        Rgba::new(140, 20, 200, 255)
    );
    assert_eq!(trigger_focus.changed[1].0, panel_body);
    assert_eq!(
        trigger_focus.changed[1].1.color,
        Rgba::new(140, 20, 200, 255)
    );
}

#[test]
fn error_recovery_diagnostics_and_adversarial_inputs_never_panic() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html><body>
          <div id="hero" style="margin-top: 8px; bogus-inline: 1; font-size: 22px"></div>
        </body></html>"#,
    );

    let css = r#"
        #hero {
            color: rgb(10, 20, 30);
            color: not-a-valid-color;
            unknown-prop-xyz: 123;
            unknown-prop-xyz: 456;
            margin-top: 16px;
        }
        ???bad selector??? { display: none; }
        #hero { font-size: 18px; }
    "#;

    let (sheet, diags) = parse_author(css);
    // 1 invalid color + 1 deduplicated unknown property + 1 invalid selector = 3 diagnostics
    assert_eq!(diags.len(), 3, "diags: {diags:?}");
    assert!(diags.iter().all(|d| d.line.is_some()));

    let map = compute_styles(&doc, &[sheet], &Viewport::default());
    // Inline style diagnostic mentions `<div id=hero>`
    assert_eq!(map.diagnostics.len(), 1);
    assert_eq!(
        map.diagnostics[0].source,
        "style attribute of <div id=hero>"
    );

    let hero = doc.element_by_id("hero").expect("hero");
    let hs = map.style(hero).expect("hero style");
    // Valid declarations before and after errors in both stylesheet and inline style survived!
    assert_eq!(hs.color, Rgba::new(10, 20, 30, 255));
    assert_eq!(hs.margin.top, px(8.0));
    assert_eq!(hs.font_size, 22.0);

    // Adversarial inputs must never panic.
    let adversarial_samples = [
        "",
        "/* unclosed comment",
        "@media (((width > ))) { .a { color: ; } }",
        "li:nth-child(n-) { color: red; }",
        "li:nth-child(99999999999999999999999n+1) { color: red; }",
        ":is(,, ???, .ok) { margin-top: 5px; }",
        ":root { --a: var(; --b: var(--); color: var(--x, var(--y, )); }",
        "@import ; @layer ; @keyframes none {} @font-face {}",
        "a { content: \"unclosed string\n b { color: blue; }",
    ];
    for sample in adversarial_samples {
        let (adv_sheet, _adv_diags) = parse_author(sample);
        let _adv_map = compute_styles(&doc, &[adv_sheet], &Viewport::default());
    }
}

#[test]
fn collect_stylesheets_and_imports_with_cycles_diamond_and_depth_limit() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html>
          <head>
            <base href="https://example.test/app/pages/index.html">
            <link rel="stylesheet" href="../css/main.css">
            <link rel="alternate stylesheet" href="../css/alt.css">
            <link rel="stylesheet" href="../css/disabled.css" disabled>
            <link rel="stylesheet" href="../css/print.css" media="print">
            <style type="application/json">{"not": "css"}</style>
            <style>
              @import "inline_extra.css" layer(utilities);
              #target { font-size: 24px; }
            </style>
          </head>
          <body><div id="target"></div></body>
        </html>"#,
    );

    let mut loader = MemoryLoader::default()
        .with_file(
            "https://example.test/app/css/main.css",
            r#"
            @import "./sub/cycle_a.css";
            @import "./left.css";
            @import "./right.css";
            @import "./deep_0.css";
            #target { color: rgb(20, 40, 60); }
            "#,
        )
        .with_file(
            "https://example.test/app/css/sub/cycle_a.css",
            r#"@import "../sub/cycle_b.css"; #target { margin-top: 5px; }"#,
        )
        .with_file(
            "https://example.test/app/css/sub/cycle_b.css",
            r#"@import "cycle_a.css"; #target { margin-top: 10px; }"#,
        )
        // Diamond import: both `left.css` and `right.css` import `shared.css` (NOT a cycle!)
        .with_file(
            "https://example.test/app/css/left.css",
            r#"@import "./shared.css";"#,
        )
        .with_file(
            "https://example.test/app/css/right.css",
            r#"@import "./shared.css";"#,
        )
        .with_file(
            "https://example.test/app/css/shared.css",
            r#"#target { margin-top: 15px; }"#,
        )
        .with_file(
            "https://example.test/app/pages/inline_extra.css",
            r#"#target { margin-top: 99px; }"#,
        );

    // Build an import chain deeper than 16 levels (`deep_0.css` -> ... -> `deep_18.css`).
    for d in 0..19 {
        let url = format!("https://example.test/app/css/deep_{d}.css");
        let body = if d < 18 {
            format!("@import \"./deep_{}.css\";", d + 1)
        } else {
            "#target { display: none; }".to_owned()
        };
        loader = loader.with_file(&url, &body);
    }

    let vp = Viewport::default();
    let (sheets, diags) = collect_stylesheets(&doc, &mut loader, &vp);

    // Must report exactly 2 diagnostics: 1 for `cycle_a.css` cycle and 1 for depth > 16!
    assert_eq!(diags.len(), 2, "diags: {diags:?}");
    assert!(diags.iter().any(|d| d.message.contains("cycle")));
    assert!(diags.iter().any(|d| d.message.contains("depth")));

    let map = compute_styles(&doc, &sheets, &vp);
    let target = doc.element_by_id("target").expect("target");
    let ts = map.style(target).expect("target visible (deep_18 stopped)");

    // `inline_extra.css` was imported into `layer(utilities)`, so unlayered `shared.css`
    // (`margin-top: 15px`) beats layered `margin-top: 99px`!
    assert_eq!(ts.margin.top, px(15.0));
    assert_eq!(ts.color, Rgba::new(20, 40, 60, 255));
    assert_eq!(ts.font_size, 24.0);
}

#[test]
fn ua_stylesheet_defaults() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html>
          <head>
            <title>UA</title>
            <meta charset="utf-8">
            <link rel="icon" href="/favicon.ico">
            <style></style>
            <script></script>
          </head>
          <body>
            <h1 id="h1">H1</h1>
            <h2 id="h2">H2</h2>
            <h3 id="h3">H3</h3>
            <h4 id="h4">H4</h4>
            <h5 id="h5">H5</h5>
            <h6 id="h6">H6</h6>
            <p id="p">Paragraph <a id="a" href="/x">link</a> <mark id="mark">hi</mark></p>
            <hr id="hr">
            <blockquote id="bq">Quote</blockquote>
            <ul id="ul"><li id="li">Item</li></ul>
            <details id="det"><summary id="sum">More</summary><summary id="sum2">Two</summary></details>
            <summary id="sum-lone">Lone</summary>
            <dialog id="dlg-closed">Closed</dialog>
            <dialog id="dlg-open" open>Open</dialog>
            <input id="inp-hidden" type="hidden">
            <template id="tpl"><div>in template</div></template>
          </body>
        </html>"#,
    );

    let map = compute_styles(&doc, &[], &Viewport::default());
    assert!(
        map.diagnostics.is_empty(),
        "UA sheet must never emit user diagnostics: {:?}",
        map.diagnostics
    );

    let body = doc.body().expect("body");
    assert_eq!(map.style(body).expect("body").display, Display::Block);
    assert_eq!(map.style(body).expect("body").margin.top, px(8.0));

    let expected_heading_sizes = [
        ("h1", 32.0),
        ("h2", 24.0),
        ("h3", 16.0 * 1.17),
        ("h4", 16.0),
        ("h5", 16.0 * 0.83),
        ("h6", 16.0 * 0.67),
    ];
    for (id, size) in expected_heading_sizes {
        let h = doc.element_by_id(id).expect(id);
        let hs = map.style(h).expect(id);
        assert_eq!(hs.display, Display::Block);
        assert!(
            (hs.font_size - size).abs() < 0.01,
            "{id}: expected {size}, got {}",
            hs.font_size
        );
    }

    // `h1` computes `font-size: 2em` (32px, priority 0) BEFORE `margin-top: 0.67em`
    // (0.67 * 32px, priority 1)!
    let h1 = doc.element_by_id("h1").expect("h1");
    assert_eq!(map.style(h1).expect("h1").margin.top, px(0.67 * 32.0));

    let a = doc.element_by_id("a").expect("a");
    assert_eq!(map.style(a).expect("a").color, Rgba::new(0, 0, 0xee, 255));

    let mark = doc.element_by_id("mark").expect("mark");
    assert_eq!(map.style(mark).expect("mark").color, Rgba::BLACK);

    let hr = doc.element_by_id("hr").expect("hr");
    assert_eq!(
        map.style(hr).expect("hr").color,
        Rgba::new(128, 128, 128, 255)
    );

    // Chromium's getComputedStyle: only a details' first summary is a list item.
    assert_eq!(
        map.style(doc.element_by_id("sum").expect("sum"))
            .expect("sum")
            .display,
        Display::ListItem
    );

    for block_id in ["p", "hr", "bq", "ul", "det", "sum2", "sum-lone", "dlg-open"] {
        let el = doc.element_by_id(block_id).expect(block_id);
        assert_eq!(
            map.style(el).expect(block_id).display,
            Display::Block,
            "expected #{block_id} to have display: block"
        );
    }

    for hidden_id in ["dlg-closed", "inp-hidden", "tpl"] {
        let n = doc.element_by_id(hidden_id).expect(hidden_id);
        assert!(map.get(n).is_none(), "expected #{hidden_id} to be hidden");
    }
}

#[test]
fn performance_3000_elements_by_1000_rules_under_2s_in_debug() {
    // Construct a 3,000-element DOM tree (<html> + <head> + <body> + 2,997 body elements).
    // 999 cards * 3 elements per card (`<div class="card ..."><h3 class="hdr ..."><span class="txt ...">`)
    // = 2,997 elements + html + head + body = 3,000 elements!
    let mut html = String::with_capacity(240_000);
    html.push_str("<!doctype html><html><head></head><body>");
    for i in 0..999 {
        let group = i % 50;
        let mod7 = i % 7;
        html.push_str(&format!(
            "<div id=\"el-{i}\" class=\"card grp-{group} kind-{mod7}\" data-kind=\"k{mod7}\">\
               <h3 class=\"hdr title-{group}\"><span class=\"txt leaf-{mod7}\">Item {i}</span></h3>\
             </div>"
        ));
    }
    html.push_str("</body></html>");

    let doc = Document::parse(&html);
    let element_count = doc
        .descendants(doc.root())
        .filter(|&n| doc.element(n).is_some())
        .count();
    assert_eq!(element_count, 3_000);

    // Construct 1,000 varied CSS rules (IDs, classes, descendant/child combinators,
    // attribute selectors, `:nth-child`, and `@media`).
    let mut css = String::with_capacity(90_000);
    for r in 0..1_000 {
        match r % 5 {
            0 => css.push_str(&format!("#el-{r} {{ margin-top: {}px; }}\n", (r % 20) + 1)),
            1 => css.push_str(&format!(
                ".grp-{} > .hdr .txt {{ color: rgb({}, {}, 100); }}\n",
                r % 200,
                r % 255,
                (r * 3) % 255
            )),
            2 => css.push_str(&format!(
                "div.card[data-kind=\"k{}\"] {{ font-size: {}px; }}\n",
                r % 25,
                12 + (r % 10)
            )),
            3 => css.push_str(&format!(
                "body > .kind-{}:nth-child(2n+1) {{ margin-top: {}px; }}\n",
                r % 120,
                r % 16
            )),
            _ => css.push_str(&format!(
                "@media (min-width: 600px) {{ .title-{} > span.leaf-{} {{ font-size: 15px; }} }}\n",
                r % 150,
                r % 15
            )),
        }
    }

    let vp = Viewport::default();
    let start = Instant::now();
    let (sheet, diags) = Stylesheet::parse(&css, Origin::Author, "perf.css", None);
    assert!(diags.is_empty(), "diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &vp);
    let elapsed = start.elapsed();

    let sample = doc.element_by_id("el-0").expect("el-0");
    assert!(map.style(sample).is_some());
    assert!(
        elapsed < Duration::from_secs(2),
        "3,000 elements x 1,000 rules took {elapsed:?} (limit: 2s in debug)"
    );
}

#[test]
fn pre_import_layer_statement_ordering() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html>
          <head>
            <link rel="stylesheet" href="main.css">
          </head>
          <body><div id="t"></div></body>
        </html>"#,
    );

    let mut loader = MemoryLoader::default()
        .with_file(
            "main.css",
            r#"@layer first, second; @import "second.css" layer(second); @layer first { #t { margin-top: 10px; } }"#,
        )
        .with_file("second.css", r#"#t { margin-top: 20px; }"#);

    let vp = Viewport::default();
    let (sheets, diags) = collect_stylesheets(&doc, &mut loader, &vp);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let map = compute_styles(&doc, &sheets, &vp);
    let t = doc.element_by_id("t").expect("t");
    assert_eq!(map.style(t).expect("t").margin.top, px(20.0));
}

#[test]
fn atomic_invalid_layer_statement_ignored() {
    let doc = Document::parse(
        r#"<!doctype html><html><head></head><body><div id="x"></div></body></html>"#,
    );
    let css = r#"@layer first, initial, second; @layer second { #x { margin-top: 10px; } } @layer first { #x { margin-top: 20px; } }"#;

    let (sheet, diags) = Stylesheet::parse(css, Origin::Author, "atomic_layer.css", None);
    assert_eq!(
        diags.len(),
        1,
        "expected 1 diagnostic for invalid @layer statement: {diags:?}"
    );

    let map = compute_styles(&doc, &[sheet], &Viewport::default());
    let x = doc.element_by_id("x").expect("x");
    assert_eq!(map.style(x).expect("x").margin.top, px(20.0));
}

#[test]
fn keyframe_style_box_fixups_root_and_flex_items() {
    let doc = Document::parse(
        r#"<!doctype html><html id="root"><body>
            <div id="flex-parent" style="display: flex">
                <span id="flex-span">item</span>
                <div id="flex-div">item2</div>
            </div>
            <div id="plain-div">plain</div>
        </body></html>"#,
    );
    let css = r#"
        @keyframes make-inline {
            from { display: inline; }
            to { display: unset; }
        }
    "#;
    let (sheet, diags) = Stylesheet::parse(css, Origin::Author, "kf_fixup.css", None);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let map = compute_styles(&doc, &[sheet], &Viewport::default());
    let kfs = map
        .keyframes
        .get("make-inline")
        .expect("make-inline keyframes");
    assert_eq!(kfs.len(), 2);

    let root = doc.element_by_id("root").expect("root");
    let flex_span = doc.element_by_id("flex-span").expect("flex-span");
    let flex_div = doc.element_by_id("flex-div").expect("flex-div");
    let plain_div = doc.element_by_id("plain-div").expect("plain-div");

    // Root element must remain blockified even when keyframe sets display:inline or unset.
    assert_eq!(
        map.keyframe_style(root, &kfs[0]).expect("root kf0").display,
        Display::Block
    );
    assert_eq!(
        map.keyframe_style(root, &kfs[1]).expect("root kf1").display,
        Display::Block
    );

    // Flex items must remain blockified when keyframe sets display:inline or unset.
    assert_eq!(
        map.keyframe_style(flex_span, &kfs[0])
            .expect("flex-span kf0")
            .display,
        Display::Block
    );
    assert_eq!(
        map.keyframe_style(flex_span, &kfs[1])
            .expect("flex-span kf1")
            .display,
        Display::Block
    );
    assert_eq!(
        map.keyframe_style(flex_div, &kfs[0])
            .expect("flex-div kf0")
            .display,
        Display::Block
    );

    // Non-root, non-flex block element becomes inline in keyframe while base stays block.
    assert_eq!(
        map.keyframe_style(plain_div, &kfs[0])
            .expect("plain kf0")
            .display,
        Display::Inline
    );
    assert_eq!(
        map.style(plain_div).expect("plain base").display,
        Display::Block
    );
}

#[test]
fn keyframe_style_revert_and_revert_layer_rollback() {
    let doc = Document::parse(
        r#"<!doctype html><html><body style="font-size: 20px; color: rgb(10, 20, 30); --inherited: body-token">
            <h1 id="heading" class="title">Heading</h1>
            <div id="layered" class="card">Layered</div>
        </body></html>"#,
    );
    let css = r#"
        @layer base, theme;
        @layer base {
            #heading {
                display: inline;
                font-size: 12px;
                margin-top: 6px;
                color: rgb(100, 100, 100);
            }
            .card {
                margin-top: 8px;
                color: rgb(40, 50, 60);
            }
        }
        @layer theme {
            .title {
                display: flex;
                font-size: 14px;
                margin-top: 10px;
                color: rgb(200, 100, 50);
                --custom: theme-val;
                --inherited: author-token;
            }
            #layered {
                margin-top: 24px;
                color: rgb(70, 80, 90);
                --custom: layered-val;
            }
        }
        @keyframes kf-revert {
            to {
                display: revert;
                font-size: revert;
                margin-top: revert;
                color: revert;
                --custom: revert;
                --inherited: revert;
            }
        }
        @keyframes kf-revert-layer {
            to {
                display: inline;
                display: revert-layer;
                font-size: 99px;
                font-size: revert-layer;
                margin-top: 99px;
                margin-top: revert-layer;
                color: rgb(255, 0, 0);
                color: revert-layer;
                --custom: overridden;
                --custom: revert-layer;
            }
        }
    "#;
    let (sheet, diags) = Stylesheet::parse(css, Origin::Author, "kf_revert.css", None);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let map = compute_styles(&doc, &[sheet], &Viewport::default());
    let heading = doc.element_by_id("heading").expect("heading");
    let layered = doc.element_by_id("layered").expect("layered");

    // Base style on #heading comes from @layer theme.
    let heading_base = map.style(heading).expect("heading base");
    assert_eq!(heading_base.display, Display::Flex);
    assert_eq!(heading_base.font_size, 14.0);
    assert_eq!(heading_base.margin.top, px(10.0));
    assert_eq!(heading_base.color, Rgba::new(200, 100, 50, 255));

    // CSS Cascade 5 §7.3.4 & §7.3.5 Note: `revert` in `@keyframes` collapses animation
    // and author origins, rolling back to the User-Agent origin (`h1 { display: block;
    // font-size: 2em; margin: 0.67em 0 }`), not `initial` or `unset`.
    let kf_rev = &map.keyframes["kf-revert"][0];
    let heading_rev = map.keyframe_style(heading, kf_rev).expect("heading revert");
    assert_eq!(heading_rev.display, Display::Block);
    assert_eq!(heading_rev.font_size, 40.0); // 2em of body's 20px
    assert_eq!(heading_rev.margin.top, px(0.67 * 40.0)); // UA 0.67em of 40px
    assert_eq!(heading_rev.color, Rgba::new(10, 20, 30, 255)); // no UA color -> inherited
    assert!(!heading_rev.custom_properties.contains_key("--custom"));
    assert_eq!(
        heading_rev
            .custom_properties
            .get("--inherited")
            .map(String::as_str),
        Some("body-token")
    );

    // CSS Cascade 5 §7.3.5: the animation origin is not collapsed with the author
    // origin for `revert-layer` and forms its own cascade layer, so `revert-layer` in
    // `@keyframes` rolls back the keyframe layer to the element's normal cascaded value.
    let kf_rev_layer = &map.keyframes["kf-revert-layer"][0];
    let heading_rl = map
        .keyframe_style(heading, kf_rev_layer)
        .expect("heading revert-layer");
    assert_eq!(heading_rl.display, Display::Flex);
    assert_eq!(heading_rl.font_size, 14.0);
    assert_eq!(heading_rl.margin.top, px(10.0));
    assert_eq!(heading_rl.color, Rgba::new(200, 100, 50, 255));
    assert_eq!(
        heading_rl
            .custom_properties
            .get("--custom")
            .map(String::as_str),
        Some("theme-val")
    );

    let layered_rl = map
        .keyframe_style(layered, kf_rev_layer)
        .expect("layered revert-layer");
    assert_eq!(layered_rl.margin.top, px(24.0));
    assert_eq!(layered_rl.color, Rgba::new(70, 80, 90, 255));
    assert_eq!(
        layered_rl
            .custom_properties
            .get("--custom")
            .map(String::as_str),
        Some("layered-val")
    );
}

#[test]
fn keyframe_style_font_size_updates_em_dependent_properties() {
    let doc = Document::parse(
        r#"<!doctype html><html><body style="font-size: 16px">
            <div id="em-box">Box</div>
            <div id="em-shorthand">Shorthand</div>
            <h1 id="em-ua">Heading</h1>
        </body></html>"#,
    );
    let css = r#"
        #em-box {
            font-size: 16px;
            margin-top: 2em;
        }
        #em-shorthand {
            font-size: 10px;
            margin: 1.5em 0;
        }
        #em-ua {
            font-size: 10px;
        }
        @keyframes scale-font {
            from { font-size: 20px; }
            50% { font-size: 2.5em; }
            to { font-size: 30px; margin-top: 3em; }
        }
        @keyframes reverse-order-em {
            to { margin-top: 2.5em; font-size: 24px; }
        }
        @keyframes revert-font {
            to { font-size: revert; }
        }
    "#;
    let (sheet, diags) = Stylesheet::parse(css, Origin::Author, "kf_em.css", None);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let map = compute_styles(&doc, &[sheet], &Viewport::default());
    let em_box = doc.element_by_id("em-box").expect("em-box");
    let em_shorthand = doc.element_by_id("em-shorthand").expect("em-shorthand");
    let em_ua = doc.element_by_id("em-ua").expect("em-ua");

    let scale_kfs = &map.keyframes["scale-font"];
    assert_eq!(scale_kfs.len(), 3);

    // Base style is 16px font-size and 32px margin-top.
    assert_eq!(map.style(em_box).expect("base").font_size, 16.0);
    assert_eq!(map.style(em_box).expect("base").margin.top, px(32.0));

    // Keyframe changing only font-size to 20px recomputes base margin-top: 2em -> 40px.
    let kf0 = map.keyframe_style(em_box, &scale_kfs[0]).expect("kf0");
    assert_eq!(kf0.font_size, 20.0);
    assert_eq!(kf0.margin.top, px(40.0));

    // Keyframe changing font-size to 2.5em (40px) recomputes base margin-top: 2em -> 80px.
    let kf1 = map.keyframe_style(em_box, &scale_kfs[1]).expect("kf1");
    assert_eq!(kf1.font_size, 40.0);
    assert_eq!(kf1.margin.top, px(80.0));

    // Keyframe changing both font-size (30px) and margin-top (3em) -> 90px.
    let kf2 = map.keyframe_style(em_box, &scale_kfs[2]).expect("kf2");
    assert_eq!(kf2.font_size, 30.0);
    assert_eq!(kf2.margin.top, px(90.0));

    // Keyframe declaring margin-top: 2.5em before font-size: 24px -> 60px.
    let rev_kf = &map.keyframes["reverse-order-em"][0];
    let kf_rev = map.keyframe_style(em_box, rev_kf).expect("kf_rev");
    assert_eq!(kf_rev.font_size, 24.0);
    assert_eq!(kf_rev.margin.top, px(60.0));

    // Shorthand margin: 1.5em 0 in base style updates when keyframe changes font-size to 20px -> 30px.
    let kf_short = map
        .keyframe_style(em_shorthand, &scale_kfs[0])
        .expect("kf_short");
    assert_eq!(kf_short.font_size, 20.0);
    assert_eq!(kf_short.margin.top, px(30.0));

    // UA margin: 0.67em 0 on <h1> updates when keyframe reverts font-size to UA 2em (32px).
    let revert_font_kf = &map.keyframes["revert-font"][0];
    let kf_ua = map.keyframe_style(em_ua, revert_font_kf).expect("kf_ua");
    assert_eq!(kf_ua.font_size, 32.0);
    assert_eq!(kf_ua.margin.top, px(0.67 * 32.0));

    // Base styles remain intact after all keyframe evaluations.
    assert_eq!(map.style(em_box).expect("base after").font_size, 16.0);
    assert_eq!(map.style(em_box).expect("base after").margin.top, px(32.0));
}

#[test]
fn var_syntax_rejects_non_ident_first_arg_and_malformed_nested_calls() {
    let doc = Document::parse(
        r#"<!doctype html><html><body>
            <div id="plus"></div>
            <div id="slash"></div>
            <div id="nested-bad-ident"></div>
            <div id="nested-empty-name"></div>
            <div id="custom-prop-bad"></div>
            <div id="nested-valid"></div>
        </body></html>"#,
    );

    let css = r#"
        :root {
            --fallback-ten: 10px;
            /* Custom property with invalid var() first arg must be rejected at parse time */
            --custom-bad: 18px;
            --custom-bad: var(--a+b, 99px);
        }
        #plus {
            margin-top: 25px;
            margin-top: var(--a+b, 10px);
        }
        #slash {
            margin-top: 25px;
            margin-top: var(--a/b, 10px);
        }
        #nested-bad-ident {
            margin-top: 25px;
            margin-top: var(--missing, var(--x+y, 10px));
        }
        #nested-empty-name {
            margin-top: 25px;
            margin-top: var(--missing, var(--, 10px));
        }
        #custom-prop-bad {
            margin-top: var(--custom-bad, 40px);
        }
        #nested-valid {
            margin-top: var(--missing, var(--also-missing, var(--fallback-ten, 99px)));
        }
        @supports (margin-top: var(--a+b, 10px)) or (margin-top: var(--a/b, 10px)) or (margin-top: var(--ok, var(--bad/name, 10px))) {
            #nested-valid { margin-top: 999px; }
        }
    "#;

    let (sheet, diags) = parse_author(css);
    // 5 invalid `var()` declarations in rules (all rejected at parse time with Severity::Invalid).
    assert_eq!(
        diags.len(),
        5,
        "expected 5 invalid var() diagnostics, got: {diags:?}"
    );
    assert!(diags.iter().all(|d| d.severity == Severity::Invalid));

    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    // Invalid var() calls must be dropped at parse time so the preceding 25px declaration wins
    // (must NOT silently substitute their 10px fallback!).
    for id in ["plus", "slash", "nested-bad-ident", "nested-empty-name"] {
        let node = doc.element_by_id(id).expect(id);
        assert_eq!(
            map.style(node).expect(id).margin.top,
            px(25.0),
            "invalid var() on #{id} must not override prior declaration with fallback"
        );
    }

    // `--custom-bad: var(--a+b, 99px)` was rejected at parse time, preserving `--custom-bad: 18px`.
    let custom_bad = doc
        .element_by_id("custom-prop-bad")
        .expect("custom-prop-bad");
    assert_eq!(
        map.style(custom_bad).expect("custom-prop-bad").margin.top,
        px(18.0)
    );

    // Valid nested fallbacks continue to work and `@supports` rejected the invalid `var()` conditions.
    let nested_valid = doc.element_by_id("nested-valid").expect("nested-valid");
    assert_eq!(
        map.style(nested_valid).expect("nested-valid").margin.top,
        px(10.0)
    );
}

#[test]
fn css_wide_keywords_on_custom_properties_direct_vs_substituted() {
    let doc = Document::parse(
        r#"<!doctype html>
        <html>
          <body id="parent" style="margin-top: 30px; color: rgb(10, 20, 30)">
            <div id="child-direct-inherit"></div>
            <div id="child-missing-inherit"></div>
            <div id="child-direct-initial"></div>
            <div id="child-direct-unset"></div>
            <div id="child-direct-revert"></div>
            <div id="child-direct-revert-layer"></div>
            <div id="child-substituted-keywords"></div>
            <h1 id="h1-substituted-revert" style="font-size: 12px">Heading</h1>
            <div id="child-substituted-revert-layer" class="layered-box"></div>
          </body>
        </html>"#,
    );

    let css = r#"
        #parent {
            --b: 44px;
            --init-prop: 55px;
            --unset-prop: 66px;
            --rev-prop: 77px;
            --b-fb: 99px;
        }

        /* 1. Direct `--b: inherit; --a: var(--b); margin-top: var(--a)`:
              `--b` is a custom property, so `--b: inherit` inherits parent's `--b` (`44px`),
              `--a` resolves to `44px`, and `margin-top` becomes `44px`. */
        #child-direct-inherit {
            --b: 99px;
            --b: inherit;
            --a: var(--b);
            margin-top: var(--a);
        }

        /* 2. Direct `--b-missing: inherit; --a-missing: var(--b-missing); margin-top: var(--a-missing)`:
              When `--b-missing` is not defined on parent, `--b-missing: inherit` inherits the initial
              (guaranteed-invalid) value, NOT the literal token `"inherit"`. Thus `--a-missing` is also
              guaranteed-invalid, and `margin-top: var(--a-missing)` is invalid at computed-value time
              (`unset` -> `0px`, NOT `30px` from `margin-top: inherit`!). */
        #child-missing-inherit {
            margin-top: 12px;
            --b-missing: inherit;
            --a-missing: var(--b-missing);
            margin-top: var(--a-missing);
        }

        /* 3. Direct `initial`, `unset`, `revert`, `revert-layer` on custom properties: */
        #child-direct-initial {
            --init-prop: initial;
            --a: var(--init-prop, 19px);
            margin-top: var(--a);
        }
        #child-direct-unset {
            --unset-prop: 99px;
            --unset-prop: unset;
            --a: var(--unset-prop);
            margin-top: var(--a);
        }
        #child-direct-revert {
            --rev-prop: 99px;
            --rev-prop: revert;
            --a: var(--rev-prop);
            margin-top: var(--a);
        }
        @layer base {
            #child-direct-revert-layer { --layer-prop: 23px; }
        }
        @layer over {
            #child-direct-revert-layer {
                --layer-prop: 88px;
                --layer-prop: revert-layer;
                --a: var(--layer-prop);
                margin-top: var(--a);
            }
        }

        /* 4. Substituted CSS-wide keyword tokens via `var()` fallback:
              `--b-fb: var(--missing, inherit); --a-fb: var(--b-fb); margin-top: var(--a-fb)`. */
        #child-substituted-keywords {
            margin-top: 15px;
            color: rgb(200, 0, 0);
            --b-fb: var(--missing, inherit);
            --a-fb: var(--b-fb);
            margin-top: var(--a-fb);
            color: var(--missing, initial);
        }
        h1#h1-substituted-revert {
            font-size: var(--missing, revert) !important;
        }
        @layer first {
            .layered-box { margin-top: 21px; }
        }
        @layer second {
            #child-substituted-revert-layer {
                margin-top: 88px;
                margin-top: var(--missing, revert-layer);
            }
        }
    "#;

    let (sheet, diags) = parse_author(css);
    assert!(diags.is_empty(), "unexpected parse diags: {diags:?}");
    let map = compute_styles(&doc, &[sheet], &Viewport::default());

    let direct_inh = doc
        .element_by_id("child-direct-inherit")
        .expect("direct-inh");
    assert_eq!(
        map.style(direct_inh).expect("direct-inh").margin.top,
        px(44.0)
    );

    let missing_inh = doc
        .element_by_id("child-missing-inherit")
        .expect("missing-inh");
    assert_eq!(
        map.style(missing_inh).expect("missing-inh").margin.top,
        px(0.0)
    );

    let direct_init = doc
        .element_by_id("child-direct-initial")
        .expect("direct-init");
    assert_eq!(
        map.style(direct_init).expect("direct-init").margin.top,
        px(19.0)
    );

    let direct_unset = doc
        .element_by_id("child-direct-unset")
        .expect("direct-unset");
    assert_eq!(
        map.style(direct_unset).expect("direct-unset").margin.top,
        px(66.0)
    );

    let direct_rev = doc
        .element_by_id("child-direct-revert")
        .expect("direct-rev");
    assert_eq!(
        map.style(direct_rev).expect("direct-rev").margin.top,
        px(77.0)
    );

    let direct_rev_layer = doc
        .element_by_id("child-direct-revert-layer")
        .expect("direct-rev-layer");
    assert_eq!(
        map.style(direct_rev_layer)
            .expect("direct-rev-layer")
            .margin
            .top,
        px(23.0)
    );

    let sub_kw = doc
        .element_by_id("child-substituted-keywords")
        .expect("sub-kw");
    let sub_kw_style = map.style(sub_kw).expect("sub-kw");
    assert_eq!(
        sub_kw_style
            .custom_properties
            .get("--b-fb")
            .map(String::as_str),
        Some("inherit")
    );
    assert_eq!(sub_kw_style.margin.top, px(30.0));
    assert_eq!(sub_kw_style.color, Rgba::BLACK);

    let h1_rev = doc
        .element_by_id("h1-substituted-revert")
        .expect("h1-substituted-revert");
    assert_eq!(
        map.style(h1_rev).expect("h1-substituted-revert").font_size,
        32.0
    );

    let sub_rev_layer = doc
        .element_by_id("child-substituted-revert-layer")
        .expect("child-substituted-revert-layer");
    assert_eq!(
        map.style(sub_rev_layer)
            .expect("child-substituted-revert-layer")
            .margin
            .top,
        px(21.0)
    );
}
