//! Layout decisions of the flex and grid emitters.

use std::collections::BTreeMap;

use eldur_css::{Align, GridLine, Length, LengthAuto, LengthPercentage, Track};

use super::flex::{Cross, Pack, cross, pack, spacer_slots};
use eldur_qml::Value;

use super::grid::{
    AxisPos, Placed, TrackSize, area_ranges, place, resolve_axis, subdivide, track_size,
};
use crate::{Options, convert_str};

fn convert(html: &str) -> (String, Vec<String>) {
    let out = convert_str(html, &Options::default());
    let qml = out.main_qml().expect("Main.qml").to_owned();
    let diags = out.diagnostics.iter().map(ToString::to_string).collect();
    (qml, diags)
}

/// The object that follows the source comment `comment`, up to its
/// closing brace.
fn object_after<'q>(qml: &'q str, comment: &str) -> &'q str {
    let start = qml
        .find(&format!("// {comment}\n"))
        .unwrap_or_else(|| panic!("no `{comment}` in\n{qml}"));
    let rest = &qml[start..];
    let first = rest.lines().nth(1).expect("object line");
    let indent = &first[..first.len() - first.trim_start().len()];
    let close = format!("\n{indent}}}\n");
    let end = rest.find(&close).expect("closing brace") + close.len();
    &rest[..end]
}

/// Only the object's own lines, without nested objects.
fn own_lines(object: &str) -> Vec<&str> {
    let mut lines = object.lines().skip(1);
    let first = lines.next().expect("type line");
    let indent = first.len() - first.trim_start().len() + 4;
    lines
        .filter(|l| l.len() - l.trim_start().len() == indent && !l.trim().is_empty())
        .map(str::trim)
        .collect()
}

fn page(css: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><style>body{{margin:0}} {css}</style></head><body>{body}</body></html>"
    )
}

fn px(v: f32) -> Length {
    Length::Linear(LengthPercentage {
        px: v,
        percent: 0.0,
        vw: 0.0,
        vh: 0.0,
    })
}

// --- justify-content -------------------------------------------------

#[test]
fn justify_content_packs_in_visual_order() {
    assert_eq!(pack(Align::Normal, false), Pack::Start);
    assert_eq!(pack(Align::FlexStart, true), Pack::End);
    assert_eq!(pack(Align::FlexEnd, true), Pack::Start);
    // `start` follows the writing mode, not the flex direction.
    assert_eq!(pack(Align::Start, true), Pack::Start);
    assert_eq!(pack(Align::Center, true), Pack::Center);
}

#[test]
fn spacers_carry_the_free_space() {
    assert_eq!(spacer_slots(Pack::Start, 3, false), [0, 0, 0, 1]);
    assert_eq!(spacer_slots(Pack::End, 2, false), [1, 0, 0]);
    assert_eq!(spacer_slots(Pack::Center, 2, false), [1, 0, 1]);
    assert_eq!(spacer_slots(Pack::Between, 3, false), [0, 1, 1, 0]);
    assert_eq!(spacer_slots(Pack::Between, 1, false), [0, 1]);
    assert_eq!(spacer_slots(Pack::Between, 1, true), [1, 0]);
    // Interior shares are twice the edge ones.
    assert_eq!(spacer_slots(Pack::Around, 2, false), [1, 2, 1]);
    assert_eq!(spacer_slots(Pack::Evenly, 2, false), [1, 1, 1]);
}

#[test]
fn cross_axis_auto_margins_win_over_align_self() {
    let auto = LengthAuto::Auto;
    let zero = LengthAuto::Length(px(0.0));
    assert_eq!(
        cross(Align::Normal, Align::Auto, &zero, &zero),
        Cross::Stretch
    );
    assert_eq!(
        cross(Align::Center, Align::Auto, &zero, &zero),
        Cross::Center
    );
    assert_eq!(
        cross(Align::Center, Align::FlexEnd, &zero, &zero),
        Cross::End
    );
    assert_eq!(
        cross(Align::Stretch, Align::Auto, &auto, &auto),
        Cross::Center
    );
    assert_eq!(cross(Align::Stretch, Align::Auto, &auto, &zero), Cross::End);
    assert_eq!(
        cross(Align::Stretch, Align::Auto, &zero, &auto),
        Cross::Start
    );
    assert_eq!(
        cross(Align::Baseline, Align::Auto, &zero, &zero),
        Cross::Baseline
    );
}

// --- flex emission -----------------------------------------------------

#[test]
fn row_space_between_puts_spacers_between_items_and_gap_on_margins() {
    let (qml, _) = convert(&page(
        ".bar{display:flex;justify-content:space-between;gap:10px}",
        r#"<div class="bar"><span>a</span><span>b</span><span>c</span></div>"#,
    ));
    let bar = object_after(&qml, r#"<div class="bar">"#);
    assert!(bar.contains("RowLayout {"), "{bar}");
    assert!(own_lines(bar).contains(&"spacing: 0"), "{bar}");
    assert_eq!(bar.matches("Item {").count(), 2, "{bar}");
    // Gaps sit between items, also across the spacers.
    assert_eq!(bar.matches("Layout.leftMargin: 10").count(), 2, "{bar}");
}

#[test]
fn flex_start_ends_with_a_spacer_and_items_shrink_to_fit() {
    let (qml, _) = convert(&page(
        ".bar{display:flex;gap:8px}",
        r#"<div class="bar"><span>a</span><span>b</span></div>"#,
    ));
    let bar = object_after(&qml, r#"<div class="bar">"#);
    // The last child is the spacer.
    assert!(bar.rfind("Item {") > bar.rfind("Text {"), "{bar}");
    assert_eq!(bar.matches("Item {").count(), 1, "{bar}");
    // `flex-shrink: 1`: fill up to the natural width, rounded up because
    // Qt rounds item widths down and a text a fraction too narrow wraps.
    assert!(
        bar.contains("Layout.maximumWidth: Math.ceil(implicitWidth)"),
        "{bar}"
    );
    // Default cross alignment is stretch.
    assert!(bar.contains("Layout.fillHeight: true"), "{bar}");
}

#[test]
fn min_width_beats_a_shrink_to_fit_cap() {
    let (qml, _) = convert(&page(
        ".bar{display:flex}.bar span{min-width:64px}",
        r#"<div class="bar"><span>a</span></div>"#,
    ));
    let bar = object_after(&qml, r#"<div class="bar">"#);
    assert!(
        bar.contains("Layout.maximumWidth: Math.max(Math.ceil(implicitWidth), 64)"),
        "{bar}"
    );
    assert!(bar.contains("Layout.minimumWidth: 64"), "{bar}");
}

#[test]
fn growing_items_fill_without_spacers() {
    let (qml, diags) = convert(&page(
        ".row{display:flex;gap:12px;justify-content:center} .a{flex:1} .b{flex:1}",
        r#"<div class="row"><div class="a">a</div><div class="b">b</div></div>"#,
    ));
    let row = object_after(&qml, r#"<div class="row">"#);
    assert!(own_lines(row).contains(&"spacing: 12"), "{row}");
    assert!(!row.contains("Item {"), "{row}");
    let items = &row[row.find(r#"<div class="a">"#).expect("present")..];
    assert_eq!(items.matches("Layout.fillWidth: true").count(), 2, "{row}");
    assert!(!row.contains("StretchFactor"), "{row}");
    assert!(
        !diags.iter().any(|d| d.contains("stretch factors")),
        "{diags:?}"
    );
}

#[test]
fn unequal_grow_uses_stretch_factors_and_says_so() {
    let (qml, diags) = convert(&page(
        ".row{display:flex} .a{flex:1} .b{flex:2}",
        r#"<div class="row"><div class="a">a</div><div class="b">b</div></div>"#,
    ));
    assert!(qml.contains("Layout.horizontalStretchFactor: 2"), "{qml}");
    assert!(
        diags.iter().any(|d| d.contains("stretch factors")),
        "{diags:?}"
    );
}

#[test]
fn fixed_basis_without_shrink_is_pinned() {
    let (qml, _) = convert(&page(
        ".row{display:flex} .side{flex:0 0 240px}",
        r#"<div class="row"><div class="side">a</div><div>b</div></div>"#,
    ));
    let side = object_after(&qml, r#"<div class="side">"#);
    assert!(side.contains("Layout.preferredWidth: 240"), "{side}");
    assert!(side.contains("Layout.minimumWidth: 240"), "{side}");
    assert!(!side.contains("Layout.fillWidth"), "{side}");
}

#[test]
fn aligned_nested_layouts_do_not_fill_the_cross_axis() {
    let (qml, _) = convert(&page(
        ".bar{display:flex;align-items:center;height:72px} .nav{display:flex}",
        r#"<div class="bar"><b>Logo</b><nav class="nav"><a href="/">a</a><a href="/b">b</a></nav></div>"#,
    ));
    let nav = object_after(&qml, r#"<nav class="nav">"#);
    let own = own_lines(nav);
    assert!(own.contains(&"Layout.fillHeight: false"), "{nav}");
    assert!(own.contains(&"Layout.alignment: Qt.AlignVCenter"), "{nav}");
}

#[test]
fn column_items_growing_from_zero_keep_their_content_height() {
    let (qml, _) = convert(&page(
        ".card{display:flex;flex-direction:column} .body{flex:1} .scroll{flex:1;overflow:hidden}",
        r#"<div class="card"><div class="body">text</div><div class="scroll">more</div></div>"#,
    ));
    let body = object_after(&qml, r#"<div class="body">"#);
    assert!(
        own_lines(body).contains(&"Layout.fillHeight: true"),
        "{body}"
    );
    // The automatic minimum size is the content: no zero preferred size.
    assert!(!body.contains("Layout.preferredHeight: 0"), "{body}");
    // A scroll container's automatic minimum is 0.
    let scroll = object_after(&qml, r#"<div class="scroll">"#);
    assert!(
        own_lines(scroll).contains(&"Layout.preferredHeight: 0"),
        "{scroll}"
    );
}

#[test]
fn column_centres_items_at_their_fit_content_width() {
    let (qml, _) = convert(&page(
        ".hero{display:flex;flex-direction:column;align-items:center;justify-content:center;\
         min-height:300px} h1{max-width:500px;margin:0}",
        r#"<section class="hero"><h1>Title</h1><p>x</p></section>"#,
    ));
    let hero = object_after(&qml, r#"<section class="hero">"#);
    assert!(hero.contains("ColumnLayout {"), "{hero}");
    assert!(
        hero.contains("Layout.maximumWidth: Math.min(Math.ceil(implicitWidth), 500)"),
        "{hero}"
    );
    assert!(hero.contains("Layout.alignment: Qt.AlignHCenter"), "{hero}");
    // `justify-content: center`: a spacer on each side.
    assert_eq!(hero.matches("Layout.fillHeight: true").count(), 2, "{hero}");
    // The layout fills the box's height, so the spacers have space.
    assert!(hero.contains("anchors.bottom: parent.bottom"), "{hero}");
}

#[test]
fn auto_margin_takes_the_free_space() {
    let (qml, _) = convert(&page(
        ".col{display:flex;flex-direction:column;height:400px;justify-content:center} \
         .foot{margin-top:auto}",
        r#"<div class="col"><p>a</p><div class="foot">b</div></div>"#,
    ));
    let col = object_after(&qml, r#"<div class="col">"#);
    // One spacer, before `.foot`; `justify-content` has no free space left.
    let items = &col[col.find("<p>").expect("present")..];
    assert_eq!(items.matches("Item {").count(), 1, "{col}");
    let spacer = col.find("Item {").expect("present");
    let foot = col.find(r#"<div class="foot">"#).expect("present");
    assert!(spacer < foot, "{col}");
}

#[test]
fn row_reverse_is_emitted_in_reverse() {
    let (qml, _) = convert(&page(
        ".row{display:flex;flex-direction:row-reverse}",
        r#"<div class="row"><p class="one">1</p><p class="two">2</p></div>"#,
    ));
    let one = qml.find(r#"<p class="one">"#).expect("present");
    let two = qml.find(r#"<p class="two">"#).expect("present");
    assert!(two < one, "{qml}");
    // `flex-start` is the right edge: the spacer leads.
    let row = object_after(&qml, r#"<div class="row">"#);
    assert!(row.find("Item {") < row.find(r#"<p class="two">"#), "{row}");
}

#[test]
fn order_sorts_items() {
    let (qml, _) = convert(&page(
        ".row{display:flex} .first{order:-1}",
        r#"<div class="row"><p class="a">a</p><p class="first">f</p></div>"#,
    ));
    assert!(qml.find(r#"<p class="first">"#) < qml.find(r#"<p class="a">"#));
}

#[test]
fn baseline_alignment_uses_qt_baseline() {
    let (qml, _) = convert(&page(
        ".row{display:flex;align-items:baseline}",
        r#"<div class="row"><span>a</span><b>b</b></div>"#,
    ));
    assert!(qml.contains("Layout.alignment: Qt.AlignBaseline"), "{qml}");
}

#[test]
fn wrapping_row_is_a_flow_with_item_widths() {
    let (qml, diags) = convert(&page(
        ".cards{display:flex;flex-wrap:wrap;gap:24px} .card{width:280px;box-sizing:border-box} \
         .rigid{flex-shrink:0}",
        r#"<div class="cards"><div class="card">a</div><div class="card">b</div><div class="card rigid">c</div></div>"#,
    ));
    let cards = object_after(&qml, r#"<div class="cards">"#);
    assert!(cards.contains("Flow {"), "{cards}");
    assert!(own_lines(cards).contains(&"spacing: 24"), "{cards}");
    // Shrinkable items shrink to a line narrower than their basis.
    assert_eq!(
        cards.matches("width: Math.min(280, parent.width)").count(),
        2,
        "{cards}"
    );
    let rigid = object_after(cards, r#"<div class="card rigid">"#);
    assert!(own_lines(rigid).contains(&"width: 280"), "{rigid}");
    assert!(
        !diags.iter().any(|d| d.contains("not implemented")),
        "{diags:?}"
    );
}

#[test]
fn wrapping_items_growing_from_one_basis_become_an_auto_fit_grid() {
    let (qml, _) = convert(&page(
        ".stats{display:flex;flex-wrap:wrap;gap:16px} .stat{flex:1 1 180px}",
        r#"<div class="stats"><div class="stat">a</div><div class="stat">b</div><div class="stat">c</div></div>"#,
    ));
    let stats = object_after(&qml, r#"<div class="stats">"#);
    assert!(stats.contains("GridLayout {"), "{stats}");
    assert!(
        stats.contains("readonly property real minCell: 180"),
        "{stats}"
    );
    assert!(stats.contains("columns: Math.min(3, fitCols)"), "{stats}");
}

#[test]
fn column_wrap_is_reported() {
    let (_, diags) = convert(&page(
        ".c{display:flex;flex-direction:column;flex-wrap:wrap}",
        r#"<div class="c"><p>a</p></div>"#,
    ));
    assert!(
        diags
            .iter()
            .any(|d| d.contains("laid out without wrapping")),
        "{diags:?}"
    );
}

// --- grid placement ----------------------------------------------------

fn no_areas() -> BTreeMap<String, (usize, usize)> {
    BTreeMap::new()
}

#[test]
fn grid_lines_resolve_to_start_and_span() {
    let a = no_areas();
    let r = |s, e| resolve_axis(&s, &e, 3, &a).0;
    assert_eq!(r(GridLine::Auto, GridLine::Auto), AxisPos::auto(1));
    assert_eq!(r(GridLine::Span(2), GridLine::Auto), AxisPos::auto(2));
    assert_eq!(r(GridLine::Line(2), GridLine::Auto), AxisPos::at(1, 1));
    assert_eq!(r(GridLine::Line(1), GridLine::Line(3)), AxisPos::at(0, 2));
    assert_eq!(r(GridLine::Line(3), GridLine::Line(1)), AxisPos::at(0, 2));
    // `-1` is the last explicit line.
    assert_eq!(r(GridLine::Line(1), GridLine::Line(-1)), AxisPos::at(0, 3));
    assert_eq!(r(GridLine::Line(2), GridLine::Span(2)), AxisPos::at(1, 2));
    assert_eq!(r(GridLine::Span(2), GridLine::Line(4)), AxisPos::at(1, 2));
}

#[test]
fn grid_areas_resolve_by_name() {
    let areas = area_ranges(&[
        vec![Some("head".into()), Some("head".into())],
        vec![Some("side".into()), Some("main".into())],
    ]);
    assert_eq!(areas["head"], ((0, 1), (0, 2)));
    let cols: BTreeMap<_, _> = areas.iter().map(|(k, v)| (k.clone(), v.1)).collect();
    let name = || GridLine::Named("head".into());
    assert_eq!(
        resolve_axis(&name(), &name(), 2, &cols).0,
        AxisPos::at(0, 2)
    );
    let (pos, lossy) = resolve_axis(&GridLine::Named("nope".into()), &GridLine::Auto, 2, &cols);
    assert_eq!((pos, lossy), (AxisPos::auto(1), true));
}

fn at(row: usize, col: usize, row_span: usize, col_span: usize) -> Placed {
    Placed {
        row,
        col,
        row_span,
        col_span,
    }
}

#[test]
fn auto_placement_is_sparse_and_skips_occupied_cells() {
    let auto = AxisPos::auto(1);
    let p = place(
        &[
            (auto, AxisPos::auto(2)),
            (AxisPos::auto(2), auto),
            (auto, auto),
            (auto, AxisPos::auto(2)),
            (auto, auto),
        ],
        0,
        3,
        false,
    );
    assert_eq!(
        p.items,
        [
            at(0, 0, 1, 2),
            at(0, 2, 2, 1),
            at(1, 0, 1, 1),
            // Does not fit beside the tall item: next row; sparse, so the
            // hole at (1, 1) stays.
            at(2, 0, 1, 2),
            at(2, 2, 1, 1),
        ]
    );
    assert_eq!((p.rows, p.cols), (3, 3));
}

#[test]
fn definite_items_are_placed_first() {
    let auto = AxisPos::auto(1);
    let p = place(
        &[(auto, auto), (AxisPos::at(0, 1), AxisPos::at(0, 1))],
        0,
        2,
        false,
    );
    // The auto item takes the first free cell.
    assert_eq!(p.items, [at(0, 1, 1, 1), at(0, 0, 1, 1)]);
}

#[test]
fn row_locked_items_follow_each_other() {
    let auto = AxisPos::auto(1);
    let row1 = AxisPos::at(1, 1);
    let p = place(&[(row1, auto), (row1, auto), (auto, auto)], 0, 2, false);
    assert_eq!(p.items, [at(1, 0, 1, 1), at(1, 1, 1, 1), at(0, 0, 1, 1)]);
}

#[test]
fn column_flow_fills_columns_first() {
    let auto = AxisPos::auto(1);
    let p = place(&[(auto, auto), (auto, auto), (auto, auto)], 2, 0, true);
    assert_eq!(p.items, [at(0, 0, 1, 1), at(1, 0, 1, 1), at(0, 1, 1, 1)]);
    assert_eq!((p.rows, p.cols), (2, 2));
}

#[test]
fn tracks_classify() {
    assert_eq!(
        track_size(&Track::Length(px(220.0)), Some("parent.width")).0,
        TrackSize::Fixed(eldur_qml::Value::Number(220.0))
    );
    assert_eq!(
        track_size(&Track::Fr(2.0), None).0,
        TrackSize::Fr { fr: 2.0, min: None }
    );
    assert_eq!(
        track_size(
            &Track::MinMax(Box::new(Track::Length(px(100.0))), Box::new(Track::Fr(1.0))),
            None
        )
        .0,
        TrackSize::Fr {
            fr: 1.0,
            min: Some(100.0)
        }
    );
    assert!(track_size(&Track::MinContent, None).1);
}

// --- grid emission -----------------------------------------------------

#[test]
fn template_areas_place_items_and_size_tracks() {
    let (qml, diags) = convert(&page(
        ".app{display:grid;grid-template-areas:\"h h\" \"s m\";grid-template-columns:220px 1fr;\
         grid-template-rows:64px 1fr;height:100vh} .h{grid-area:h} .s{grid-area:s} .m{grid-area:m}",
        r#"<div class="app"><div class="m">m</div><div class="h">h</div><div class="s">s</div></div>"#,
    ));
    let app = object_after(&qml, r#"<div class="app">"#);
    assert!(app.contains("GridLayout {"), "{app}");
    assert!(app.contains("columnSpacing: 0"), "{app}");
    let h = object_after(&qml, r#"<div class="h">"#);
    assert!(h.contains("Layout.columnSpan: 2"), "{h}");
    assert!(h.contains("Layout.preferredHeight: 64"), "{h}");
    let s = object_after(&qml, r#"<div class="s">"#);
    assert!(s.contains("Layout.row: 1"), "{s}");
    assert!(s.contains("Layout.column: 0"), "{s}");
    assert!(s.contains("Layout.preferredWidth: 220"), "{s}");
    let m = object_after(&qml, r#"<div class="m">"#);
    assert!(m.contains("Layout.fillWidth: true"), "{m}");
    assert!(m.contains("Layout.fillHeight: true"), "{m}");
    assert!(
        !diags.iter().any(|d| d.contains("not implemented")),
        "{diags:?}"
    );
}

#[test]
fn equal_fr_columns_fill_from_zero() {
    let (qml, _) = convert(&page(
        ".g{display:grid;grid-template-columns:repeat(3,1fr);gap:16px;grid-auto-rows:120px} \
         .wide{grid-column:span 2}",
        r#"<div class="g"><div class="wide">w</div><div>a</div><div>b</div></div>"#,
    ));
    let g = object_after(&qml, r#"<div class="g">"#);
    assert!(own_lines(g).contains(&"columns: 3"), "{g}");
    let wide = object_after(&qml, r#"<div class="wide">"#);
    assert!(wide.contains("Layout.columnSpan: 2"), "{wide}");
    assert!(wide.contains("Layout.preferredHeight: 120"), "{wide}");
    assert!(g.contains("Layout.preferredWidth: 0"), "{g}");
}

#[test]
fn unequal_fr_is_split_into_equal_sub_tracks() {
    let fr = |fr| TrackSize::Fr { fr, min: None };
    let px = TrackSize::Fixed(Value::Number(100.0));
    assert_eq!(
        subdivide(&[px.clone(), fr(1.0), fr(3.0)]),
        Some(vec![1, 1, 3])
    );
    assert_eq!(
        subdivide(&[fr(2.0), fr(1.0), fr(1.0), fr(1.0)]),
        Some(vec![2, 1, 1, 1])
    );
    // 1.1 : 0.9 within 1%: 5 : 4.
    assert_eq!(subdivide(&[fr(1.1), fr(0.9)]), Some(vec![5, 4]));
    // Equal `fr` share space as they are.
    assert_eq!(subdivide(&[fr(1.0), fr(1.0)]), None);
    // `auto` tracks take space Qt cannot share proportionally.
    assert_eq!(
        subdivide(&[TrackSize::Auto { min: None }, fr(1.0), fr(2.0)]),
        None
    );
    // More than 12 sub-tracks: the closest split that fits.
    let split = subdivide(&[fr(1.0), fr(13.0)]).expect("split");
    assert!(split.iter().sum::<usize>() <= 12, "{split:?}");
    assert_eq!(split[0], 1);
}

#[test]
fn unequal_fr_columns_span_sub_columns() {
    let (qml, diags) = convert(&page(
        ".g{display:grid;grid-template-columns:100px 1fr 3fr;gap:10px}",
        r#"<div class="g"><div>a</div><div>b</div><div>c</div><div>d</div><div>e</div><div>f</div></div>"#,
    ));
    let g = object_after(&qml, r#"<div class="g">"#);
    let own = own_lines(g);
    assert!(own.contains(&"columns: 5"), "{g}");
    // The carrier row would add a row gap, so the gap is a top margin.
    assert!(own.contains(&"rowSpacing: 0"), "{g}");
    assert!(g.contains("Layout.columnSpan: 3"), "{g}");
    assert!(!g.contains("parent.width"), "no width bindings: {g}");
    // Every sub-column has a single-span carrier: the zero-height row 2.
    assert_eq!(g.matches("Layout.row: 2").count(), 3, "{g}");
    assert_eq!(g.matches("Layout.topMargin: 10").count(), 3, "{g}");
    assert!(
        diags.iter().any(|d| d.contains("5 equal sub-columns")),
        "{diags:?}"
    );
}

#[test]
fn fixed_columns_get_a_filler() {
    let (qml, _) = convert(&page(
        ".g{display:grid;grid-template-columns:100px 100px}",
        r#"<div class="g"><p>a</p><p>b</p></div>"#,
    ));
    let g = object_after(&qml, r#"<div class="g">"#);
    assert!(own_lines(g).contains(&"columns: 3"), "{g}");
    assert!(g.contains("Layout.column: 2"), "{g}");
}

#[test]
fn auto_fit_minmax_columns_follow_the_width() {
    let (qml, _) = convert(&page(
        ".g{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:20px}",
        r#"<div class="g"><p>a</p><p>b</p></div>"#,
    ));
    assert!(qml.contains("readonly property real minCell: 200"), "{qml}");
    assert!(qml.contains("columns: Math.min(2, fitCols)"), "{qml}");
    assert!(qml.contains("uniformCellWidths: true"), "{qml}");
}

#[test]
fn auto_fill_keeps_empty_columns() {
    let (qml, _) = convert(&page(
        ".g{display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr))}",
        r#"<div class="g"><p>a</p></div>"#,
    ));
    assert!(qml.contains("columns: fitCols"), "{qml}");
    assert!(qml.contains(".fitCols - 1) *"), "{qml}");
}

#[test]
fn justify_items_aligns_in_auto_tracks() {
    let (qml, _) = convert(&page(
        ".g{display:grid;justify-items:center;align-items:start}",
        r#"<div class="g"><p>a</p></div>"#,
    ));
    assert!(
        qml.contains("Layout.alignment: Qt.AlignHCenter | Qt.AlignTop"),
        "{qml}"
    );
}

#[test]
fn aligned_layout_grid_items_do_not_fill() {
    let (qml, _) = convert(&page(
        ".g{display:grid;align-items:start} .row{display:flex}",
        r#"<div class="g"><div class="row"><span>a</span></div></div>"#,
    ));
    let row = object_after(&qml, r#"<div class="row">"#);
    assert!(
        own_lines(row).contains(&"Layout.fillHeight: false"),
        "{row}"
    );
}

#[test]
fn column_flow_places_explicitly() {
    let (qml, _) = convert(&page(
        ".g{display:grid;grid-auto-flow:column;grid-template-rows:auto auto}",
        r#"<div class="g"><p class="a">a</p><p class="b">b</p><p class="c">c</p></div>"#,
    ));
    let c = object_after(&qml, r#"<p class="c">"#);
    assert!(c.contains("Layout.row: 0"), "{c}");
    assert!(c.contains("Layout.column: 1"), "{c}");
}

#[test]
fn dense_is_reported() {
    let (_, diags) = convert(&page(
        ".g{display:grid;grid-auto-flow:row dense}",
        r#"<div class="g"><p>a</p></div>"#,
    ));
    assert!(diags.iter().any(|d| d.contains("dense")), "{diags:?}");
}

#[test]
fn block_flow_in_a_stretched_grid_row_stays_at_the_top() {
    // Stretching a ColumnLayout spreads its children apart; a CSS block's
    // children stay where they are and the block's end is simply lower.
    let (qml, _) = convert(&page(
        "#g{display:grid;grid-template-columns:1fr 1fr} #t{height:300px}",
        "<div id=g><div id=t></div><div><p>a</p><p>b</p></div></div>",
    ));
    let column = qml
        .split("ColumnLayout {")
        .find(|part| part.contains("text: \"a\"") && part.contains("Layout.preferredWidth"))
        .expect("the second grid item");
    assert!(column.contains("Layout.fillHeight: false"), "{qml}");
    assert!(column.contains("Qt.AlignTop"), "{qml}");
}

#[test]
fn percentage_radius_of_a_sized_box_is_resolved() {
    let (qml, _) = convert(&page(
        "#d{width:6px;height:6px;border-radius:50%;background:red}",
        "<div id=d></div>",
    ));
    assert!(qml.contains("radius: 3"), "{qml}");
}

#[test]
fn absolute_child_of_a_positioned_box_is_an_overlay() {
    let (qml, diags) = convert(&page(
        "#p{position:relative;padding-left:16px} \
         #p::before{content:\"\";position:absolute;left:0;top:10px;width:6px;height:6px;background:red}",
        "<div id=p>text</div>",
    ));
    assert!(qml.contains("y: 10"), "{qml}");
    assert!(qml.contains("width: 6"), "{qml}");
    assert!(!diags.iter().any(|d| d.contains("position")), "{diags:?}");
}

#[test]
fn outside_marker_hangs_outside_the_layout() {
    let (qml, _) = convert(&page(
        "ul{margin:0;padding:0 0 0 40px;font-size:16px;line-height:24px}",
        "<ul><li>a</li></ul>",
    ));
    assert!(
        qml.contains("x: -16"),
        "gap 11 + size 5 left of the box\n{qml}"
    );
    assert!(qml.contains("radius: 2.5"), "{qml}");
    let (qml, _) = convert(&page(
        "",
        "<ol style=\"list-style-type:lower-alpha\"><li value=3>a</li></ol>",
    ));
    assert!(qml.contains("text: \"c. \""), "{qml}");
    assert!(qml.contains("anchors.right: parent.left"), "{qml}");
}

#[test]
fn table_is_a_grid_with_explicit_cells() {
    let (qml, _) = convert(&page(
        "td{padding:2px}",
        "<table><tr><td rowspan=2>a</td><td colspan=2>b</td></tr><tr><td>c</td><td>d</td></tr></table>",
    ));
    assert!(qml.contains("GridLayout {"), "{qml}");
    assert!(qml.contains("columns: 3"), "{qml}");
    assert!(qml.contains("Layout.rowSpan: 2"), "{qml}");
    assert!(qml.contains("Layout.columnSpan: 2"), "{qml}");
    assert!(qml.contains("function tableColumnWidths("), "{qml}");
}

#[test]
fn pages_without_tables_get_no_solver() {
    let (qml, _) = convert(&page("", "<p>a</p>"));
    assert!(!qml.contains("tableColumnWidths"), "{qml}");
}

#[test]
fn collapsed_table_loses_its_padding_and_separated_one_gets_spacing() {
    let (collapsed, _) = convert(&page(
        "table{border-collapse:collapse;padding:30px}",
        "<table><tr><td>a</td></tr></table>",
    ));
    assert!(!collapsed.contains("anchors.topMargin: 30"), "{collapsed}");
    let (separate, _) = convert(&page(
        "table{border-collapse:separate;border-spacing:3px 5px}",
        "<table><tr><td>a</td><td>b</td></tr></table>",
    ));
    assert!(separate.contains("columnSpacing: 3"), "{separate}");
    assert!(separate.contains("rowSpacing: 5"), "{separate}");
    assert!(separate.contains("anchors.leftMargin: 3"), "{separate}");
    assert!(separate.contains("anchors.topMargin: 5"), "{separate}");
}

#[test]
fn cell_vertical_align_picks_the_anchor() {
    let (qml, _) = convert(&page(
        "td{vertical-align:bottom}",
        "<table><tr><td>a</td></tr></table>",
    ));
    assert!(qml.contains("anchors.bottom: parent.bottom"), "{qml}");
    let (qml, _) = convert(&page(
        "td{vertical-align:middle}",
        "<table><tr><td>a</td></tr></table>",
    ));
    assert!(
        qml.contains("anchors.verticalCenter: parent.verticalCenter"),
        "{qml}"
    );
}

#[test]
fn caption_side_orders_the_caption_and_rows_flatten_backgrounds() {
    let (qml, _) = convert(&page(
        "caption{caption-side:bottom} tr{background:#ff0000}",
        "<table><caption>cap</caption><tr><td>a</td></tr></table>",
    ));
    let (cell, caption) = (qml.find("text: \"a\""), qml.find("text: \"cap\""));
    assert!(cell < caption, "{qml}");
    assert!(qml.contains("color: \"#ff0000\""), "{qml}");
}

#[test]
fn block_flow_in_a_stretched_flex_row_stays_at_the_top() {
    let (qml, _) = convert(&page(
        "#r{display:flex} #t{height:300px}",
        "<div id=r><div id=t></div><div><p>a</p><p>b</p></div></div>",
    ));
    let column = qml
        .split("ColumnLayout {")
        .find(|part| part.contains("text: \"a\"") && part.contains("Qt.AlignTop"))
        .unwrap_or_else(|| panic!("the column is not top-aligned\n{qml}"));
    assert!(column.contains("Layout.fillHeight: false"), "{qml}");
}

#[test]
fn sized_table_with_a_caption_keeps_its_width() {
    let (qml, _) = convert(&page(
        "table{width:480px}",
        "<table><caption>c</caption><tr><td>a</td></tr></table>",
    ));
    let stack = qml
        .split("ColumnLayout {")
        .find(|part| part.contains("Layout.preferredWidth: 480"))
        .unwrap_or_else(|| panic!("no sized stack\n{qml}"));
    assert!(stack.contains("Layout.fillWidth: false"), "{qml}");
}

#[test]
fn overlay_keeps_its_z_index() {
    let (qml, _) = convert(&page(
        "#p{position:relative} .a{position:absolute;top:0;left:0;z-index:2}",
        "<div id=p><div class=a>x</div></div>",
    ));
    assert!(qml.contains("z: 2"), "{qml}");
}

#[test]
fn overlay_keeps_its_max_width() {
    let (qml, _) = convert(&page(
        "#p{position:relative} .a{position:absolute;left:0;bottom:0;max-width:480px}",
        "<div id=p><div class=a>x</div></div>",
    ));
    assert!(qml.contains("width: Math.min(implicitWidth, 480)"), "{qml}");
}

// The line puts a run's lines on their baselines itself; what the converter
// does for a lone Text's half-leading would move the words off them.
#[test]
fn a_run_on_a_line_keeps_no_half_leading_of_its_own() {
    let (qml, _) = convert(&page(
        "p{font:16px/24px sans-serif} .b{display:inline-block}",
        "<p>Before <span class=b>box</span> after</p>",
    ));
    let flow = object_after(&qml, "<p>");
    let run = flow.split("text: \"Before ").nth(1).expect("the run");
    let run = &run[..run.find('}').unwrap_or(run.len())];
    assert!(!run.contains("lineHeightMode"), "{run}");
    assert!(!run.contains("Translate"), "{run}");
}

#[test]
fn a_line_with_an_inline_block_is_laid_out_as_runs_and_boxes() {
    let (qml, diags) = convert(&page(
        "p{font:16px/24px sans-serif} .b{display:inline-block;padding:2px 8px;background:#eee;\
         font-size:12px;line-height:16px}",
        "<p>Notes for <b>bold</b>er <span class=b>NEW</span> items</p>",
    ));
    let flow = object_after(&qml, "<p>");
    assert_eq!(
        flow.lines().nth(1).map(str::trim),
        Some("EldurLines {"),
        "{flow}"
    );
    // Text is cut only at the box, so each run is shaped whole with its spans.
    assert!(flow.contains("Notes for <b>bold</b>er"), "{flow}");
    assert!(flow.contains("items"), "{flow}");
    assert!(flow.contains("lineHeight: 24"), "{flow}");
    // The badge keeps its box; the line puts its baseline on the text's when it
    // runs (`inline_lines` in apps_e2e.rs holds the positions to Chromium's).
    assert!(flow.contains("color: \"#eeeeee\""), "{flow}");
    assert!(
        !diags.iter().any(|d| d.contains("inline text")),
        "{diags:?}"
    );
}

#[test]
fn atomic_inlines_keep_their_margins_and_centre_in_the_line() {
    let (qml, _) = convert(&page(
        ".c{text-align:center} .t{display:inline-block;margin:0 8px 8px 0;padding:4px}",
        "<div class=c><span class=t>a</span> <span class=t>b</span></div>",
    ));
    let flow = object_after(&qml, "<div class=\"c\">");
    assert!(flow.contains("textAlign: Text.AlignHCenter"), "{flow}");
    assert!(flow.contains("width: tBox.width + 8"), "{flow}");
    assert!(flow.contains("height: tBox.height + 8"), "{flow}");
    // The space between the boxes is a run of its own.
    assert!(flow.contains("text: \" \""), "{flow}");
}

#[test]
fn box_shadows_are_rectangular_shadows_behind_the_box() {
    let (qml, diags) = convert(&page(
        ".s{background:#fff;border-radius:8px;\
         box-shadow:0 4px 12px rgba(0,0,0,.25),0 0 0 2px #c7d2fe}",
        "<div class=s>x</div>",
    ));
    let shadow = object_after(&qml, "<div class=\"s\">");
    assert!(qml.contains("import QtQuick.Effects"), "{qml}");
    let first = shadow.find("RectangularShadow").expect("shadow");
    let last = shadow.rfind("RectangularShadow").expect("shadow");
    assert_ne!(first, last, "{shadow}");
    // The first shadow in CSS paints on top, so it comes last.
    assert!(shadow[first..last].contains("spread: 2"), "{shadow}");
    assert!(
        shadow[last..].contains("offset: Qt.vector2d(0, 4)"),
        "{shadow}"
    );
    // Qt's blur fades faster than CSS's: measured against Chromium.
    assert!(shadow[last..].contains("blur: 16.4"), "{shadow}");
    assert!(shadow[last..].contains("color: \"#40000000\""), "{shadow}");
    assert!(shadow.contains("z: -1"), "{shadow}");
    assert!(shadow.contains("radius: 8"), "{shadow}");
    assert!(!diags.iter().any(|d| d.contains("box-shadow")), "{diags:?}");
}

#[test]
fn inset_box_shadows_are_drawn_inside_the_box() {
    let (qml, diags) = convert(&page(
        ".s{box-shadow:inset 0 1px 2px #000}",
        "<div class=s>x</div>",
    ));
    // An outer shadow's `RectangularShadow` would draw outside the box.
    assert!(!qml.contains("RectangularShadow"), "{qml}");
    assert!(qml.contains("Inset\n"), "{qml}");
    assert!(qml.contains("ShapePath.OddEvenFill"), "{qml}");
    assert!(!diags.iter().any(|d| d.contains("box-shadow")), "{diags:?}");
}

#[test]
fn a_clipping_box_keeps_its_shadow_outside_the_clip() {
    let (qml, _) = convert(&page(
        ".s{background:#fff;overflow:hidden;box-shadow:0 2px 8px #000}",
        "<div class=s>x</div>",
    ));
    let outer = object_after(&qml, "<div class=\"s\">");
    assert_eq!(
        outer.lines().nth(1).map(str::trim),
        Some("Item {"),
        "{outer}"
    );
    let shadow = outer.find("RectangularShadow").expect("shadow");
    let clip = outer.find("clip: true").expect("clip");
    assert!(shadow < clip, "{outer}");
    assert!(
        outer.contains("implicitHeight: sClip.implicitHeight"),
        "{outer}"
    );
}

#[test]
fn transforms_apply_in_css_order_around_the_origin() {
    let (qml, diags) = convert(&page(
        ".t{width:100px;height:50px;background:#eee;\
         transform:translate(-50%, 10px) rotate(45deg) scale(2)}",
        "<div class=t></div>",
    ));
    let obj = object_after(&qml, "<div class=\"t\">");
    // QML applies the first transform of the list first, CSS the last.
    let scale = obj.find("Scale {").expect("scale");
    let rotation = obj.find("Rotation {").expect("rotation");
    let translate = obj.find("Translate {").expect("translate");
    assert!(scale < rotation && rotation < translate, "{obj}");
    assert!(obj.contains("angle: 45"), "{obj}");
    assert!(obj.contains("xScale: 2"), "{obj}");
    // Percentages are of the box's own size, as is the default origin.
    assert!(obj.contains("x: tBox.width * -0.5"), "{obj}");
    assert!(obj.contains("origin.x: tBox.width * 0.5"), "{obj}");
    assert!(obj.contains("origin.y: tBox.height * 0.5"), "{obj}");
    assert!(!diags.iter().any(|d| d.contains("transform")), "{diags:?}");
}

#[test]
fn controls_tables_and_placeholders_are_transformed_too() {
    // Chromium moves each of these by its own translate (geometry.mjs).
    let (qml, diags) = convert(&page(
        "",
        "<div><input id=field style=\"transform:translateX(20px)\"></div>\
         <div><button id=go style=\"transform:translate(10px,5px)\">Go</button></div>\
         <table id=t style=\"transform:translateX(30px)\"><tr><td>a</td></tr></table>\
         <fieldset id=f style=\"transform:translateX(11px)\"><legend>L</legend>x</fieldset>\
         <img id=pic width=80 height=40 style=\"transform:translateX(13px)\">",
    ));
    for (tag, x) in [
        ("<input id=\"field\">", 20),
        ("<button id=\"go\">", 10),
        ("<table id=\"t\">", 30),
        ("<fieldset id=\"f\">", 11),
        ("<img id=\"pic\">", 13),
    ] {
        let obj = object_after(&qml, tag);
        let t = obj.find("transform: Translate {").expect(tag);
        assert!(obj[t..].contains(&format!("x: {x}")), "{tag}: {obj}");
    }
    assert!(object_after(&qml, "<button id=\"go\">").contains("y: 5"));
    assert!(!diags.iter().any(|d| d.contains("transform")), "{diags:?}");
}

#[test]
fn a_row_translation_moves_its_cells() {
    // Chromium: the cell lands at table + row + cell translation.
    let (qml, diags) = convert(&page(
        "",
        "<table><tbody style=\"transform:translateY(2px)\">\
         <tr style=\"transform:translateX(7px)\">\
         <td id=c style=\"transform:translateX(3px)\">a</td></tr></tbody></table>",
    ));
    // The only translations on the page, all on the one cell.
    let list = &qml[qml.find("transform: [").expect("a list")..];
    let own = list.find("x: 3").expect("the cell's own");
    let row = list.find("x: 7").expect("the row's");
    let group = list.find("y: 2").expect("the group's");
    // QML applies the list from the front: the cell's own first.
    assert!(own < row && row < group, "{qml}");
    assert!(!diags.iter().any(|d| d.contains("transform")), "{diags:?}");
}

#[test]
fn a_row_rotation_is_reported() {
    let (qml, diags) = convert(&page(
        "",
        "<table><tr style=\"transform:rotate(5deg)\"><td id=c>a</td></tr></table>",
    ));
    assert!(!qml.contains("Rotation"), "{qml}");
    assert!(
        diags.iter().any(|d| d.contains("`transform` on <tr>")),
        "{diags:?}"
    );
}

#[test]
fn transform_origin_in_px_is_a_number() {
    let (qml, _) = convert(&page(
        ".t{width:100px;height:50px;background:#eee;transform:rotate(10deg);\
         transform-origin:0 0}",
        "<div class=t></div>",
    ));
    let obj = object_after(&qml, "<div class=\"t\">");
    assert!(obj.contains("angle: 10"), "{obj}");
    assert!(!obj.contains("origin.x"), "{obj}");
}

#[test]
fn a_transformed_text_keeps_its_baseline_shift_first() {
    let (qml, _) = convert(&page(
        "p{font:16px/24px sans-serif;transform:translateX(10px)}",
        "<p>x</p>",
    ));
    let text = object_after(&qml, "<p>");
    // Chromium's baseline is 14 + floor((24 - 17) / 2) = 17 down, Qt's 14.
    let nudge = text.find("y: 3\n").expect("nudge");
    let moved = text.find("x: 10").expect("translate");
    assert!(nudge < moved, "{text}");
}

#[test]
fn fixed_box_leaves_the_flow_for_the_window() {
    // Chromium: header at (0, 0) 1280×20; p at y 30 (the body's padding
    // only, the header takes no space).
    let (qml, diags) = convert(&page(
        "body{padding-top:30px} header{position:fixed;top:0;left:0;right:0;height:20px;\
         background:red} p{margin:0}",
        "<header>h</header><p>x</p>",
    ));
    // The window's children are at indent 4; the header follows the
    // Flickable, so it does not scroll.
    let flickable = qml.find("\n    Flickable {").expect("flickable");
    let flickable_end = flickable + qml[flickable..].find("\n    }\n").expect("end");
    let header = &qml[flickable_end..];
    assert!(
        header.starts_with("\n    }\n\n    // <header>\n    Rectangle {"),
        "{qml}"
    );
    assert!(header.contains("color: \"#ff0000\""), "{qml}");
    assert!(header.contains("anchors.left: parent.left"), "{qml}");
    assert!(header.contains("anchors.right: parent.right"), "{qml}");
    assert!(header.contains("height: 20"), "{qml}");
    assert!(
        !diags.iter().any(|d| d.contains("position: fixed")),
        "{diags:?}"
    );
}

#[test]
fn glyphs_sit_on_chromiums_whole_pixel_baseline() {
    // The shifts are what tools/visual/paint/gen.mjs measured: Chromium's
    // baseline below the line's top less the row Qt paints a fixed line's
    // on (layout.md §1.2). `.a` .. `.f` are the fonts it probes.
    let (qml, _) = convert(&page(
        "p{margin:0} .a{font:15px/1.5 sans-serif} .b{font:17px/1.5 sans-serif} \
         .c{font:bold 28px/1.5 sans-serif} .d{font:16px/1.5 serif} \
         .e{font:16px/1.1 sans-serif} .f{font:13px/1 sans-serif}",
        "<p class=a>a</p><p class=b>b</p><p class=c>c</p><p class=d>d</p>\
         <p class=e>e</p><p class=f>f</p>",
    ));
    let measured = include_str!("../../../../tools/visual/paint/shifts.txt");
    let mut seen = 0;
    for line in measured.lines().filter(|l| !l.starts_with('#')) {
        let fields: Vec<&str> = line.split(' ').collect();
        let [class, _, _, shift] = fields[..] else {
            panic!("{line}");
        };
        let p = object_after(&qml, &format!("<p class=\"{class}\">"));
        match p.find("transform: Translate {") {
            Some(t) => {
                let y = p[t..].lines().nth(1).map(str::trim);
                assert_eq!(y, Some(format!("y: {shift}").as_str()), "{p}");
            }
            None => assert_eq!(shift, "0", "{p}"),
        }
        seen += 1;
    }
    assert_eq!(seen, 6, "{measured}");
}

#[test]
fn richtext_lines_shift_to_chromiums_baseline() {
    // The same lines as tools/visual/paint/gen.mjs's RICH; `rich.txt` is what
    // it measured on both sides.
    const CODE: &str = "font-family:monospace;background:#eee";
    let cases = [
        (
            "r1",
            "font:16px/1.6 sans-serif",
            format!("{CODE};font-size:14px;padding:2px 5px"),
        ),
        (
            "r2",
            "font:14px/20px sans-serif",
            format!("{CODE};font-size:12px"),
        ),
        (
            "r3",
            "font:16px/1.5 sans-serif",
            format!("{CODE};font-size:16px"),
        ),
        (
            "r4",
            "font:16px/1.5 sans-serif",
            format!("{CODE};font-size:24px"),
        ),
        (
            "r5",
            "font:15px/1.5 sans-serif",
            "background:#eee;font-family:serif".to_owned(),
        ),
        (
            "r6",
            "font:16px/1.5 serif",
            format!("{CODE};font-size:14px"),
        ),
        (
            "r7",
            "font:16px/1.6 sans-serif",
            "background:#eee".to_owned(),
        ),
        (
            "r8",
            "font:14px/1.5 sans-serif",
            "background:#eee;font-size:24px".to_owned(),
        ),
    ];
    // Qt's line with a monospace run of the body's size came out a row above
    // anything its metrics predict (layout.md §1.2, known limits).
    let known = [("r3", "-2")];
    let measured = include_str!("../../../../tools/visual/paint/rich.txt");
    for (class, css, run) in cases {
        let (qml, _) = convert(&page(
            &format!("p{{margin:0;{css}}}"),
            &format!("<p class={class}>Hx<span style=\"{run}\">H</span></p>"),
        ));
        let line = measured
            .lines()
            .find(|l| l.starts_with(class))
            .unwrap_or_else(|| panic!("{class} in {measured}"));
        let measured = line.rsplit(' ').next().expect("rich.txt: name shifts");
        let want = known
            .iter()
            .find(|(k, _)| *k == class)
            .map_or(measured, |(_, v)| v);
        let p = object_after(&qml, &format!("<p class=\"{class}\">"));
        assert!(p.contains("Text.RichText"), "{p}");
        let t = p
            .find("transform: Translate {")
            .unwrap_or_else(|| panic!("{p}"));
        let y = p[t..].lines().nth(1).map(str::trim);
        assert_eq!(y, Some(format!("y: {want}").as_str()), "{class}: {p}");
    }
}

/// The `renderType` of the object holding the first `text`.
fn render_type_of(qml: &str, text: &str) -> Option<String> {
    let at = qml.find(text).expect("text in the output");
    let open = qml[..at].rfind('{').expect("object opens");
    let close = open + qml[open..].find("\n").expect("line") + 1;
    let indent = qml[close..].len() - qml[close..].trim_start().len();
    let end = format!("\n{}}}", " ".repeat(indent - 4));
    let body = &qml[close..close + qml[close..].find(&end).expect("object closes")];
    body.lines()
        .find(|l| {
            l.len() - l.trim_start().len() == indent && l.trim_start().starts_with("renderType:")
        })
        .map(|l| l.trim().to_owned())
}

const NATIVE: &str = "renderType: Text.NativeRendering";

#[test]
fn text_asks_for_native_glyphs() {
    // Distance fields are softer than Chromium's glyphs: 89.40 vs 90.45 over
    // the corpus. A baseline's Translate moves whole pixels and does not count.
    let (qml, _) = convert(&page(
        "p{font:15px/22px sans-serif}",
        "<p>plain</p><p>a <code>rich</code> line</p>",
    ));
    assert!(qml.contains("Translate"), "{qml}");
    assert_eq!(
        render_type_of(&qml, "\"plain\"").as_deref(),
        Some(NATIVE),
        "{qml}"
    );
    assert_eq!(
        render_type_of(&qml, "rich").as_deref(),
        Some(NATIVE),
        "{qml}"
    );
}

#[test]
fn text_under_a_scale_or_rotation_keeps_qts_default() {
    // Native glyphs resample badly: 20-pricing's featured card, 86.21 vs 85.84.
    for transform in ["scale(1.05)", "rotate(-8deg)", "matrix(1, 0.2, 0, 1, 0, 0)"] {
        let (qml, _) = convert(&page(
            &format!(".t{{transform:{transform}}}"),
            "<div class=t><div><p>turned</p></div></div><p>still</p>",
        ));
        assert_eq!(
            render_type_of(&qml, "\"turned\""),
            None,
            "{transform}: {qml}"
        );
        assert_eq!(
            render_type_of(&qml, "\"still\"").as_deref(),
            Some(NATIVE),
            "{qml}"
        );
    }
    // Moved by whole pixels the glyphs stay as rasterised.
    let (qml, _) = convert(&page(
        ".t{transform:translate(4px,8px)}",
        "<div class=t><p>moved</p></div>",
    ));
    assert_eq!(
        render_type_of(&qml, "\"moved\"").as_deref(),
        Some(NATIVE),
        "{qml}"
    );
}

#[test]
fn control_text_asks_for_native_glyphs() {
    let (qml, _) = convert(&page(
        "",
        "<input value=one><textarea>two</textarea><input type=number>\
         <button>three</button><select><option>four</select>",
    ));
    for item in ["TextField {", "TextArea {", "TextInput {"] {
        assert!(qml.contains(item), "{item}: {qml}");
    }
    assert_eq!(qml.matches(NATIVE).count(), 5 + 1, "{qml}"); // and the arrow
    let button = &qml[qml.find("Button {").expect("button")..];
    let label = &button[button.find("contentItem: Text {").expect("label")..];
    assert!(label.contains(".palette.buttonText"), "{label}");
    assert!(label.contains(NATIVE), "{label}");
}

#[test]
fn a_fractional_caption_keeps_the_table_on_its_exact_y() {
    // Chromium: a 37.5px caption puts the table's rows 37.5px down. A floored
    // caption height moved them half a pixel up (08-table).
    let (qml, _) = convert(&page(
        "caption{padding-bottom:12px;font:17px/25.5px sans-serif}",
        "<table><caption>c</caption><tr><td>a</td></tr></table>",
    ));
    assert!(!qml.contains("Math.floor(implicitHeight)"), "{qml}");
    assert!(qml.contains("eldurCarry"), "{qml}");
}

#[test]
fn a_middle_table_cell_centres_exactly() {
    // A cell's content is centred without Qt's rounding, on both ends of the
    // anchor; the values behind it are `cN` in tools/visual/paint/gen.mjs.
    let (qml, _) = convert(&page(
        "table{border-collapse:collapse} td{border:1px solid #000;padding:4px}",
        "<table><tr><td>a</td></tr></table>",
    ));
    assert_eq!(
        qml.matches("anchors.alignWhenCentered: false").count(),
        2,
        "{qml}"
    );
    // `vertical-align: top` is not centred.
    let (qml, _) = convert(&page(
        "td{vertical-align:top;padding:4px}",
        "<table><tr><td>a</td></tr></table>",
    ));
    assert!(!qml.contains("alignWhenCentered"), "{qml}");
}

#[test]
fn tight_line_height_does_not_grow_the_last_line() {
    // Chromium: `font: 96px/1` is 96px tall and `16px/1` two lines 32px.
    // Qt gives the last fixed-height line the font's own height (108px and
    // 18px), so the padding takes the difference back.
    let (qml, _) = convert(&page(
        ".d{margin:0;font:96px/1 sans-serif} .e{margin:0;font:16px/1.5 sans-serif}",
        "<p class=d>0</p><p class=e>0</p>",
    ));
    let d = object_after(&qml, "<p class=\"d\">");
    assert!(d.contains("lineHeight: 96"), "{d}");
    assert!(
        d.contains("bottomPadding: Math.min(0, lineHeight - Math.ceil(metrics.height))"),
        "{d}"
    );
    // The metrics are of the Text's own font.
    let metrics = d
        .split("readonly property FontMetrics metrics: FontMetrics {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_else(|| panic!("no metrics in {d}"));
    assert!(metrics.contains("font.family: \"sans-serif\""), "{d}");
    assert!(metrics.contains("font.pixelSize: 96"), "{d}");
    // A line taller than the font needs nothing.
    let e = object_after(&qml, "<p class=\"e\">");
    assert!(!e.contains("bottomPadding"), "{e}");
}

#[test]
fn percentage_height_resolves_against_a_definite_parent() {
    // Chromium: .f 8px (100% of 8px), .g 10px (50% of the 20px content
    // box), .h 15px (50% of 40px border box less 10px padding), .i auto.
    let (qml, diags) = convert(&page(
        "div{background:#eee} .t{height:8px} .f{height:100%;width:50%} \
         .u{height:20px;padding:5px} .g{height:50%} \
         .v{height:40px;padding:5px;box-sizing:border-box} .h{height:50%} .i{height:50%}",
        "<div class=t><div class=f></div></div><div class=u><div class=g></div></div>\
         <div class=v><div class=h></div></div><div class=w><div class=i>x</div></div>",
    ));
    for (comment, height) in [
        ("<div class=\"f\">", "Layout.preferredHeight: 8"),
        ("<div class=\"g\">", "Layout.preferredHeight: 10"),
        ("<div class=\"h\">", "Layout.preferredHeight: 15"),
    ] {
        let obj = object_after(&qml, comment);
        assert!(own_lines(obj).contains(&height), "{obj}");
    }
    // Auto: its own content height (carry.rs lays a painted box out at
    // it), not a resolved length.
    let i = object_after(&qml, "<div class=\"i\">");
    assert!(
        own_lines(i)
            .iter()
            .filter(|l| l.contains("preferredHeight"))
            .all(|l| *l == "Layout.preferredHeight: implicitHeight"),
        "{i}"
    );
    assert_eq!(
        diags
            .iter()
            .filter(|d| d.contains("percentage height"))
            .count(),
        1,
        "{diags:?}"
    );
}

#[test]
fn text_of_a_flex_or_grid_box_is_placed_as_its_one_item() {
    // Chromium (getBoundingClientRect): .a's "SK" at (10.7, 12) in a 40×40
    // box, centred both ways; .b centred in 200×52; .c's "x" at the bottom
    // right of 100×60; .d's text wraps to the full 100px, lines at x 0.
    let (qml, _) = convert(&page(
        "div{background:#eee;font:14px sans-serif} \
         .a{display:grid;place-items:center;width:40px;height:40px} \
         .b{display:flex;align-items:center;justify-content:center;height:52px;width:200px} \
         .c{display:flex;flex-direction:column;justify-content:flex-end;\
            align-items:flex-end;height:60px;width:100px} \
         .d{display:flex;justify-content:center;width:100px}",
        "<div class=a>SK</div><div class=b>Sign in</div><div class=c>x</div>\
         <div class=d>one two three four five six</div>",
    ));
    let text = |class: &str| {
        let obj = object_after(&qml, &format!("<div class=\"{class}\">"));
        obj.split("Text {")
            .nth(1)
            .unwrap_or_else(|| panic!("no Text in {obj}"))
            .to_owned()
    };
    let a = text("a");
    assert!(a.contains("verticalAlignment: Text.AlignVCenter"), "{a}");
    assert!(
        a.contains("horizontalAlignment: lineCount > 1 ? Text.AlignLeft : Text.AlignHCenter"),
        "{a}"
    );
    let b = text("b");
    assert!(b.contains("verticalAlignment: Text.AlignVCenter"), "{b}");
    assert!(
        b.contains("horizontalAlignment: lineCount > 1 ? Text.AlignLeft : Text.AlignHCenter"),
        "{b}"
    );
    let c = text("c");
    assert!(c.contains("verticalAlignment: Text.AlignBottom"), "{c}");
    assert!(
        c.contains("horizontalAlignment: lineCount > 1 ? Text.AlignLeft : Text.AlignRight"),
        "{c}"
    );
    // Wrapped, the item is the box's width and `text-align` places the lines.
    let d = text("d");
    assert!(
        d.contains("horizontalAlignment: lineCount > 1 ? Text.AlignLeft : Text.AlignHCenter"),
        "{d}"
    );
    assert!(!d.contains("verticalAlignment"), "{d}");
}

#[test]
fn fr_ratio_reported_only_when_sub_columns_approximate_it() {
    let diags_for = |cols: &str| {
        convert(&page(
            &format!(".g{{display:grid;grid-template-columns:{cols};gap:20px}}"),
            "<div class=g><div>a</div><div>b</div></div>",
        ))
        .1
    };
    let exact = diags_for("2fr 1fr");
    assert!(!exact.iter().any(|d| d.contains("`fr`")), "{exact:?}");
    let inexact = diags_for("1.1fr 0.9fr");
    assert!(
        inexact
            .iter()
            .any(|d| d.contains("1.1:0.9") && d.contains("5:4")),
        "{inexact:?}"
    );
}

#[test]
fn a_text_that_is_one_link_without_underline_is_not_underlined() {
    // Chromium: `li a` computes `text-decoration-line: none`, colour
    // rgb(51, 65, 85); without the rule, `underline` and rgb(0, 0, 238).
    let (qml, diags) = convert(&page(
        ".n a{color:#334155;text-decoration:none}",
        "<ul class=n><li><a href=\"#f\">Features</a></li></ul>\
         <p class=n>See <a href=\"#d\">docs</a>.</p>\
         <ul><li><a href=\"#u\">Underlined</a></li></ul>",
    ));
    let nav = object_after(&qml, "<li>");
    // The link is the whole text, so its colour is the Text's.
    assert!(
        nav.contains("text: \"Features\"") && nav.contains("color: \"#334155\""),
        "{nav}"
    );
    assert!(!nav.contains("<a "), "{nav}");
    assert!(
        nav.contains("TapHandler {") && nav.contains("root.scrollToFragment(\"#f\")"),
        "{nav}"
    );
    // A link inside other text stays a StyledText link, underlined.
    let p = object_after(&qml, "<p class=\"n\">");
    assert!(p.contains("<a href=\\\"#d\\\">docs</a>"), "{p}");
    assert_eq!(
        diags.iter().filter(|d| d.contains("is underlined")).count(),
        1,
        "{diags:?}"
    );
    // An underlined link keeps StyledText's link.
    let u = qml.split("<a href=\\\"#u\\\">").count();
    assert_eq!(u, 2, "{qml}");
}

#[test]
fn a_right_float_sits_at_the_right_edge() {
    // Chromium: in a 400px box, a 50% `float: right` with a 6px right
    // margin is at x 194 of it; a left float stays at the left.
    let (qml, _) = convert(&page(
        ".c{width:400px} .r{float:right;width:50%;height:10px;margin-right:6px} \
         .l{float:left;width:100px;height:10px}",
        "<div class=c><div class=r></div><div class=l></div></div>",
    ));
    let r = own_lines(object_after(&qml, "<div class=\"r\">"));
    assert!(r.contains(&"Layout.alignment: Qt.AlignRight"), "{r:?}");
    assert!(r.contains(&"Layout.rightMargin: 6"), "{r:?}");
    let l = own_lines(object_after(&qml, "<div class=\"l\">"));
    assert!(
        !l.iter().any(|p| p.starts_with("Layout.alignment")),
        "{l:?}"
    );
}

#[test]
fn a_layout_with_a_width_does_not_fill_its_column() {
    // Chromium, in a 600px box: .g is 568px wide at x 16. A Qt Layout
    // fills by default, so it has to be told not to.
    let (qml, _) = convert(&page(
        ".c{width:600px} .g{display:grid;grid-template-columns:1fr 1fr;\
         width:calc(100% - 32px);margin:0 auto}",
        "<div class=c><main class=g><p>a</p><p>b</p></main></div>",
    ));
    let g = own_lines(object_after(&qml, "<main class=\"g\">"));
    assert!(g.contains(&"Layout.fillWidth: false"), "{g:?}");
}

#[test]
fn rounded_overflow_clip_masks_the_corners_with_the_backdrop() {
    // Chromium (11-positioning): at the bottom-left corner of a 12px
    // rounded `overflow: hidden` box, the absolutely placed caption is
    // clipped and the page's #f3f4f6 shows. QML's `clip` is square and the
    // software renderer has no masking, so the corners outside the
    // rounded rect are painted over in the colour behind the box.
    let (qml, diags) = convert(&page(
        "body{background:#f3f4f6} .m{position:relative;width:320px;height:220px;\
         border-radius:12px;overflow:hidden;background:#7f9cf5} \
         .c{position:absolute;bottom:0;left:0;right:0;background:#000}",
        "<figure class=m><figcaption class=c>Studio</figcaption></figure>",
    ));
    let m = object_after(&qml, "<figure class=\"m\">");
    let caption = m.find("Studio").expect("caption");
    let mask = m.rfind("Shape {").expect("corner mask");
    assert!(caption < mask, "the mask is drawn over the content: {m}");
    let mask = &m[mask..];
    assert!(mask.contains("fillRule: ShapePath.OddEvenFill"), "{m}");
    assert!(mask.contains("fillColor: \"#f3f4f6\""), "{m}");
    assert!(mask.contains("radius: 12"), "{m}");
    assert!(qml.contains("import QtQuick.Shapes"), "{qml}");
    assert!(!diags.iter().any(|d| d.contains("square")), "{diags:?}");
}

#[test]
fn rounded_clip_mask_composites_a_translucent_backdrop() {
    // Behind the box: rgba(0,0,0,0.5) over white; Chromium paints
    // rgb(127, 127, 127) there.
    let (qml, _) = convert(&page(
        ".p{background:rgba(0,0,0,0.5);padding:10px} \
         .m{width:100px;height:50px;border-radius:8px;overflow:hidden}",
        "<div class=p><div class=m><p>x</p></div></div>",
    ));
    let m = object_after(&qml, "<div class=\"m\">");
    assert!(m.contains("fillColor: \"#7f7f7f\""), "{m}");
}

#[test]
fn rounded_clip_redraws_the_border_over_the_content() {
    let (qml, _) = convert(&page(
        ".m{width:100px;height:50px;border:3px solid #123456;border-radius:8px;\
         overflow:hidden} .m p{margin:0;background:#f00;height:50px}",
        "<div class=m><p>x</p></div>",
    ));
    let m = object_after(&qml, "<div class=\"m\">");
    let content = m.find("\"x\"").expect("content");
    let after = &m[content..];
    assert!(after.contains("fillColor: \"#ffffff\""), "{m}");
    assert!(
        after.contains("border.width: 3") && after.contains("border.color: \"#123456\""),
        "the border is drawn again over the content: {m}"
    );
}

#[test]
fn rounded_clip_over_a_gradient_masks_what_it_holds() {
    let (qml, diags) = convert(&page(
        ".p{background:linear-gradient(red,blue)} \
         .m{width:100px;height:50px;border-radius:8px;overflow:hidden}",
        "<div class=p><div class=m><p>x</p></div></div>",
    ));
    let m = object_after(&qml, "<div class=\"m\">");
    assert!(!m.contains("OddEvenFill"), "{m}");
    assert!(m.contains("EldurRoundClip"), "{m}");
    assert!(m.contains("layer.enabled: true"), "{m}");
    assert!(!diags.iter().any(|d| d.contains("square")), "{diags:?}");
}

#[test]
fn a_rounded_clip_with_nothing_to_clip_takes_no_layer() {
    let (qml, _) = convert(&page(
        ".p{background:linear-gradient(red,blue)} \
         .m{width:100px;height:50px;border-radius:8px;overflow:hidden;background:#fff}",
        "<div class=p><div class=m></div></div>",
    ));
    assert!(!qml.contains("layer.enabled"), "{qml}");
}

#[test]
fn a_fragment_link_scrolls_the_page_to_its_target() {
    // Chromium: following `#b` scrolls the border-box top of #b to the top
    // of the viewport, clamped to the page's end; `#` and `#top` scroll to
    // the top; an unknown fragment does nothing. Other links open outside.
    let (qml, _) = convert(&page(
        ".n a{text-decoration:none}",
        "<ul class=n><li><a href=\"#b\">B</a></li><li><a href=\"https://x.org/\">X</a></li></ul>\
         <p>See <a href=\"#b\">b</a> or <a href=\"https://x.org/\">x</a>.</p>\
         <h2 id=b>B</h2>",
    ));
    let f = qml
        .find("function scrollToFragment(")
        .map(|i| &qml[i..])
        .expect("scrollToFragment on the root");
    assert!(f.contains("\"b\": b"), "{f}");
    assert!(
        f.contains("mapToItem(flickable.contentItem, 0, 0).y"),
        "{f}"
    );
    assert!(
        f.contains("flickable.contentHeight - flickable.height"),
        "{f}"
    );
    assert!(f.contains("\"top\""), "{f}");

    let bare = object_after(&qml, "<li>");
    assert!(bare.contains("root.scrollToFragment(\"#b\")"), "{bare}");
    assert!(!bare.contains("openUrlExternally"), "{bare}");
    assert!(
        bare.contains("cursorShape: Qt.PointingHandCursor"),
        "{bare}"
    );
    let external = qml.find("Qt.openUrlExternally(\"https://x.org/\")");
    assert!(external.is_some(), "{qml}");

    let p = object_after(&qml, "<p>");
    assert!(p.contains("link.startsWith(\"#\")"), "{p}");
    assert!(p.contains("root.scrollToFragment(link)"), "{p}");
    assert!(p.contains("Qt.openUrlExternally(link)"), "{p}");
    assert!(p.contains("linkAt("), "a pointing hand over links: {p}");
}

#[test]
fn a_page_without_fragment_links_has_no_scroll_function() {
    let (qml, _) = convert(&page("", "<p><a href=\"https://x.org/\">x</a> y</p>"));
    assert!(!qml.contains("scrollToFragment"), "{qml}");
    assert!(qml.contains("onLinkActivated"), "{qml}");
}

#[test]
fn an_unblurred_inset_shadow_is_a_ring_inside_the_padding_box() {
    // Chromium (18-dashboard): `inset 3px 0 0` on an 8px rounded box is a
    // 3px bar of the shadow colour along the left edge, curved with the
    // corners, over the background and under the text.
    let (qml, diags) = convert(&page(
        ".a{padding:9px 12px;border-radius:8px;background:#1f2937;\
         box-shadow:inset 3px 0 0 #6366f1}",
        "<div class=a>Overview</div>",
    ));
    let a = object_after(&qml, "<div class=\"a\">");
    let shadow = a.find("Shape {").expect("inset shadow shape");
    let text = a.find("Overview").expect("text");
    assert!(shadow < text, "under the content: {a}");
    let shape = &a[shadow..text];
    assert!(shape.contains("fillColor: \"#6366f1\""), "{a}");
    assert!(shape.contains("fillRule: ShapePath.OddEvenFill"), "{a}");
    // The hole: shifted right by 3, cut off at the right edge.
    assert!(shape.contains("x: 3"), "{a}");
    assert!(shape.contains(".width - 3"), "{a}");
    assert!(shape.contains("radius: 8"), "{a}");
    assert!(!diags.iter().any(|d| d.contains("inset")), "{diags:?}");
}

#[test]
fn an_inset_spread_is_a_ring_of_its_width() {
    // `inset 0 0 0 1px` (13-custom-properties): a 1px ring; the hole's
    // corners are the padding box's, less the spread.
    let (qml, _) = convert(&page(
        ".s{width:56px;height:56px;border-radius:6px;background:#2563eb;\
         box-shadow:inset 0 0 0 1px rgba(0,0,0,0.08)}",
        "<div class=s></div>",
    ));
    let s = object_after(&qml, "<div class=\"s\">");
    assert!(s.contains("fillColor: \"#14000000\""), "{s}");
    assert!(s.contains("x: 1") && s.contains("y: 1"), "{s}");
    assert!(s.contains(".width - 2") && s.contains(".height - 2"), "{s}");
    assert!(s.contains("radius: 5"), "{s}");
}

#[test]
fn a_blurred_inset_shadow_fades_in_bands() {
    // Chromium: `inset 0 0 20px #000` on a white box is, 0.5px in from the
    // edge, alpha 119/255, and 20px in, 4/255: 1 - Φ(d / 10). The bands
    // are nested rings whose alphas compose to it.
    use super::shadow::inset_bands;
    let bands = inset_bands(20.0, 1.0);
    assert!(bands.len() > 1 && bands.len() <= 13, "{bands:?}");
    let coverage = |depth: f32| {
        1.0 - bands
            .iter()
            .filter(|b| depth < b.depth)
            .map(|b| 1.0 - b.alpha)
            .product::<f32>()
    };
    for (depth, chromium) in [(0.5, 119.0), (5.5, 74.0), (10.5, 42.0), (20.5, 3.0)] {
        let got = coverage(depth) * 255.0;
        assert!(
            (got - chromium).abs() <= 16.0,
            "depth {depth}: {got} vs {chromium} in {bands:?}"
        );
    }
    // Deep outside the hole edge it is the full colour.
    assert!(coverage(-30.0) > 0.95, "{bands:?}");
}

#[test]
fn a_float_run_shares_a_line_and_what_follows_wraps_beside_it() {
    let (qml, diags) = convert(&page(
        ".f{float:left;width:100px;height:20px}",
        "<div><div class=f>a</div><div class=f>b</div><p>after</p></div>\
         <div><div class=f>alone</div></div>",
    ));
    assert_eq!(qml.matches("Flow {").count(), 1, "{qml}");
    // Only the lone float is stacked as a block.
    assert_eq!(
        diags
            .iter()
            .filter(|d| d.contains("floats do not share a line"))
            .count(),
        1,
        "{diags:?}"
    );
    // The paragraph's lines move beside the floats (float_wrap_e2e
    // measures where).
    assert_eq!(qml.matches("onLineLaidOut").count(), 1, "{qml}");
    assert!(
        !diags
            .iter()
            .any(|d| d.contains("is placed below the floats")),
        "{diags:?}"
    );
}

// Carry (carry.rs): Qt rounds each ColumnLayout cell up to a whole pixel,
// so an item's top margin takes back what the cell before it was rounded
// up by.

const CARRY_CSS: &str = "p{margin:0 0 13.3px;font:16px/27.2px sans-serif}";

#[test]
fn a_fractional_line_height_hands_its_rounding_to_the_next_item() {
    let (qml, _) = convert(&page(
        CARRY_CSS,
        r#"<p id="a">One</p><p id="b">Two</p><p id="c">Three</p>"#,
    ));
    let a = own_lines(object_after(&qml, r#"<p id="a">"#));
    assert!(
        a.contains(&"property real eldurCarry: Math.ceil(Layout.topMargin + Math.ceil(implicitHeight)) - Layout.topMargin - implicitHeight + 0.000001"),
        "{a:?}"
    );
    let b = own_lines(object_after(&qml, r#"<p id="b">"#));
    assert!(
        b.contains(&"Layout.topMargin: 13.3 - a.eldurCarry"),
        "{b:?}"
    );
    // The last cell is rounded once: its height is not rounded first.
    let c = own_lines(object_after(&qml, r#"<p id="c">"#));
    assert!(
        c.contains(&"Layout.topMargin: 13.3 - b.eldurCarry"),
        "{c:?}"
    );
    assert!(
        c.contains(&"Layout.preferredHeight: implicitHeight"),
        "{c:?}"
    );
}

#[test]
fn a_box_around_a_carried_column_paints_its_css_height() {
    // 02-block-flow's hero: a column's excess is in its implicit height for
    // the column above to take back, but the box's bottom edge is Chromium's
    // only at the box's exact height (layout.md §1.2, tools/visual/paint).
    let (qml, _) = convert(&page(
        ".c{padding:10px;background:#eee;margin-bottom:9.3px} p{margin:0;font:16px/27.2px sans-serif}",
        r#"<div id="a" class="c"><p>One</p><p>Two</p></div><div id="b" class="c"><p>Three</p></div>"#,
    ));
    let a = own_lines(object_after(&qml, r#"<div id="a" class="c">"#));
    let height = a
        .iter()
        .find(|l| l.starts_with("Layout.preferredHeight: implicitHeight - "))
        .unwrap_or_else(|| panic!("{a:?}"));
    assert!(height.ends_with(".eldurCarry"), "{a:?}");
    assert!(
        a.iter()
            .any(|l| l.starts_with("property real eldurCarry: Math.ceil(Layout.topMargin - (")),
        "{a:?}"
    );
    let b = own_lines(object_after(&qml, r#"<div id="b" class="c">"#));
    assert!(
        b.iter()
            .any(|l| l.ends_with(".eldurCarry") && l.starts_with("Layout.topMargin: 9.3 - ")),
        "{b:?}"
    );
}

#[test]
fn a_rounded_box_antialiases_only_while_it_sits_whole() {
    // Its straight edges would spread over two rows otherwise, where
    // Chromium's are one (layout.md §1.2, tools/visual/paint).
    let (qml, _) = convert(&page(
        ".r{border-radius:4px;height:10px;background:#eee} .t{transform:rotate(10deg)} \
         .s{height:10px;background:#eee}",
        r#"<div class="r"></div><div class="r t"></div><div class="s"></div>"#,
    ));
    let line = own_lines(object_after(&qml, r#"<div class="r">"#))
        .into_iter()
        .find(|l| l.starts_with("antialiasing: "))
        .map(str::to_owned);
    assert_eq!(
        line.as_deref(),
        Some(
            "antialiasing: Math.round(x * 64) % 64 === 0 && Math.round(y * 64) % 64 === 0 \
             && Math.round(width * 64) % 64 === 0 && Math.round(height * 64) % 64 === 0"
        ),
        "{qml}"
    );
    for plain in [r#"<div class="r t">"#, r#"<div class="s">"#] {
        let o = object_after(&qml, plain);
        assert!(!o.contains("antialiasing"), "{o}");
    }
}

#[test]
fn whole_heights_need_no_carry() {
    let (qml, _) = convert(&page(
        "p{margin:0 0 12px;font:16px/24px sans-serif} div{height:30px;background:#eee}",
        r#"<p id="a">One</p><div id="d"></div><p id="b">Two</p>"#,
    ));
    assert!(!qml.contains("eldurCarry"), "{qml}");
    let d = own_lines(object_after(&qml, r#"<div id="d">"#));
    assert!(d.contains(&"Layout.topMargin: 12"), "{d:?}");
}

#[test]
fn fixed_fractional_heights_carry_as_numbers() {
    // 10.5 px boxes: the first cell is 11, so the second box starts 0.5
    // earlier; its cell (3.5 + 10.5) is whole, so the third does not.
    let (qml, _) = convert(&page(
        ".half{height:10.5px;margin:0 0 4px;background:#eee}",
        r#"<div class="half" id="h1"></div><div class="half" id="h2"></div><div class="half" id="h3"></div>"#,
    ));
    assert!(!qml.contains("eldurCarry"), "{qml}");
    let h2 = own_lines(object_after(&qml, r#"<div id="h2" class="half">"#));
    assert!(h2.contains(&"Layout.topMargin: 3.5"), "{h2:?}");
    let h3 = own_lines(object_after(&qml, r#"<div id="h3" class="half">"#));
    assert!(h3.contains(&"Layout.topMargin: 4"), "{h3:?}");
}

/// 09-forms' overshoot in the prototype: the carry was taken off the
/// giving item's bottom margin, and a fixed-height item after it (an
/// `<input>` with `Layout.preferredHeight`) never added it back. The
/// carry belongs to the item that takes it, and a fixed height takes it
/// and hands its own on.
#[test]
fn a_fixed_height_after_a_fractional_one_takes_the_carry() {
    let (qml, _) = convert(&page(
        "label{display:block;margin-bottom:5px;font:14px/19.6px sans-serif} input{display:block;height:41px;box-sizing:border-box} p{margin:0;font:16px/27.2px sans-serif}",
        r#"<label id="l">Name</label><input id="i"><p id="k">After</p>"#,
    ));
    let l = own_lines(object_after(&qml, r#"<label id="l">"#));
    assert!(
        !l.iter().any(|m| m.starts_with("Layout.bottomMargin")),
        "{l:?}"
    );
    let i = own_lines(object_after(&qml, r#"<input id="i">"#));
    assert!(i.contains(&"Layout.topMargin: 5 - l.eldurCarry"), "{i:?}");
    assert!(
        i.contains(&"property real eldurCarry: Math.ceil(Layout.topMargin + 41) - Layout.topMargin - 41 + 0.000001"),
        "{i:?}"
    );
    let k = own_lines(object_after(&qml, r#"<p id="k">"#));
    assert!(k.contains(&"Layout.topMargin: -i.eldurCarry"), "{k:?}");
}

#[test]
fn a_nested_column_hands_on_its_last_rounding() {
    let (qml, _) = convert(&page(
        CARRY_CSS,
        r#"<section id="s"><p id="d">Four</p><p id="e">Five</p></section><p id="g">Six</p>"#,
    ));
    let e = own_lines(object_after(&qml, r#"<p id="e">"#));
    assert!(
        e.iter().any(|m| m.starts_with("property real eldurCarry:")),
        "{e:?}"
    );
    let g = own_lines(object_after(&qml, r#"<p id="g">"#));
    assert!(
        g.iter()
            .any(|m| m.starts_with("Layout.topMargin: 13.3 - ") && m.ends_with(".eldurCarry")),
        "{g:?}"
    );
    assert!(!g.contains(&"Layout.topMargin: 13.3"), "{g:?}");
}

#[test]
fn a_flex_column_carries_past_its_spacer() {
    let (qml, _) = convert(&page(
        ".f{display:flex;flex-direction:column} label{margin-bottom:5px;font:14px/19.6px sans-serif} input{height:41px;box-sizing:border-box}",
        r#"<div class="f"><label id="fl">Flex</label><input id="fi"></div>"#,
    ));
    let fi = own_lines(object_after(&qml, r#"<input id="fi">"#));
    assert!(
        fi.contains(&"Layout.topMargin: 5 - fl.eldurCarry"),
        "{fi:?}"
    );
}

#[test]
fn a_hidden_item_passes_the_carry_on() {
    let (qml, _) = convert(&page(
        CARRY_CSS,
        r#"<p id="a">One</p><p id="x" hidden>Hidden</p><p id="b">Two</p><button id="t">t</button>
        <script>document.getElementById("t").onclick = () => { document.getElementById("x").hidden = false; };</script>"#,
    ));
    let x = own_lines(object_after(&qml, r#"<p id="x">"#));
    assert!(
        x.iter()
            .any(|m| m.starts_with("property real eldurCarry: visible ? (")
                && m.ends_with(") : a.eldurCarry")),
        "{x:?}"
    );
    let b = own_lines(object_after(&qml, r#"<p id="b">"#));
    assert!(
        b.contains(&"Layout.topMargin: 13.3 - x.eldurCarry"),
        "{b:?}"
    );
}

#[test]
fn absolute_without_positioned_ancestor_goes_to_the_initial_containing_block() {
    let (qml, diags) = convert(&page(
        ".a{position:absolute;top:10px;left:20px;width:30px;height:40px}",
        "<section><div class=a>x</div><p>y</p></section>",
    ));
    let block = &qml[qml.find("id: initialContainingBlock").expect("the block")..];
    assert!(block.contains("width: flickable.width"), "{qml}");
    assert!(block.contains("x: 20"), "{qml}");
    assert!(qml.contains("initialContainingBlock.childrenRect"), "{qml}");
    // With its offsets set it reads nothing from where it was.
    assert!(!qml.contains("Slot"), "{qml}");
    assert!(!qml.contains("staticX"), "{qml}");
    assert!(!diags.iter().any(|d| d.contains("position")), "{diags:?}");
}

#[test]
fn a_page_without_such_a_box_has_no_initial_containing_block_item() {
    let (qml, _) = convert(&page(
        "#p{position:relative} .a{position:absolute;top:0;left:0}",
        "<div id=p><div class=a>x</div></div>",
    ));
    assert!(!qml.contains("initialContainingBlock"), "{qml}");
    assert!(!qml.contains("staticX"), "{qml}");
}

#[test]
fn a_box_without_offsets_reads_its_static_position_from_a_slot() {
    let (qml, _) = convert(&page(
        ".a{position:absolute;width:5px;height:5px}",
        "<p>y</p><div class=a></div>",
    ));
    assert!(qml.contains("function staticY(item, block)"), "{qml}");
    assert!(
        qml.contains("y: root.staticY(aSlot, flickable.contentItem)"),
        "{qml}"
    );
}

#[test]
fn inline_box_without_a_left_after_text_says_its_x_is_approximate() {
    let (_, diags) = convert(&page(
        ".a{position:absolute;top:5px}",
        "<p>text <span class=a>x</span></p>",
    ));
    assert!(
        diags
            .iter()
            .any(|d| d.contains("<span") && d.contains("line")),
        "{diags:?}"
    );
    let (_, diags) = convert(&page(
        ".a{position:absolute;top:5px}",
        "<p><span class=a>x</span>text</p>",
    ));
    assert!(!diags.iter().any(|d| d.contains("<span")), "{diags:?}");
}

#[test]
fn what_hoisting_cannot_keep_is_reported() {
    let (_, diags) = convert(&page(
        ".a{position:absolute;top:0}",
        "<div style=\"opacity:.5\"><div class=a>x</div></div>",
    ));
    assert!(diags.iter().any(|d| d.contains("opacity")), "{diags:?}");
    let (_, diags) = convert(&page(
        ".a{position:absolute;top:0}",
        "<table><tr style=\"position:relative\"><td><div class=a>x</div></td></tr></table>",
    ));
    assert!(diags.iter().any(|d| d.contains("<tr")), "{diags:?}");
    let (_, diags) = convert(&page(
        ".a{position:absolute;width:5px}",
        "<div style=\"display:flex;justify-content:center\"><div class=a></div></div>",
    ));
    assert!(diags.iter().any(|d| d.contains("alignment")), "{diags:?}");
}

#[test]
fn a_box_a_script_only_writes_text_into_stays_a_text() {
    let (qml, _) = convert(&page(
        "",
        "<div id=a></div><script>document.getElementById('a').textContent='x'</script>",
    ));
    assert!(!qml.contains("flow"), "{qml}");
    assert!(qml.contains("Text {\n"), "{qml}");
}

#[test]
fn a_box_a_script_fills_has_a_column_and_no_height_until_then() {
    let (qml, _) = convert(&page(
        "",
        "<div id=a></div><script>var d=document.createElement('div');\
         document.getElementById('a').appendChild(d)</script>",
    ));
    assert!(qml.contains("ColumnLayout {\n"), "{qml}");
    assert!(qml.contains("text === \"\" ? 0"), "{qml}");
}

#[test]
fn an_inline_level_box_takes_its_place_from_the_parents_text_align() {
    let (left, _) = convert(&page("", "<button>Go</button>"));
    assert!(!left.contains("Qt.AlignHCenter"), "{left}");
    let (centred, _) = convert(&page(
        "",
        "<div style=\"text-align:center\"><button>Go</button></div>",
    ));
    assert!(
        centred.contains("textAlign: Text.AlignHCenter"),
        "{centred}"
    );
    assert!(!left.contains("textAlign: Text.AlignHCenter"), "{left}");
}

#[test]
fn a_page_without_a_script_emits_nothing_for_hidden_elements() {
    let (main, _) = convert(
        r#"<html><head><style>.h{display:none}</style></head><body>
        <p>shown</p><div class="h"><p>secret one</p></div>
        <p style="display:none">secret two</p></body></html>"#,
    );
    assert!(main.contains("shown"));
    assert!(!main.contains("secret"), "{main}");
}

#[test]
fn a_filter_is_one_effect_on_the_box_and_what_it_holds() {
    let (qml, _) = convert(&page(
        ".b { filter: grayscale(1) blur(4px); }",
        r#"<div class="b"><p>x</p></div>"#,
    ));
    assert!(qml.contains("layer.enabled: true"), "{qml}");
    assert!(qml.contains("EldurFilter"), "{qml}");
    assert!(qml.contains("colorMatrix"), "{qml}");
    assert!(qml.contains("blurDeviation: 4"), "{qml}");
}

#[test]
fn opacity_takes_a_layer_only_with_more_than_one_thing_to_fade() {
    let (group, _) = convert(&page(
        ".b { opacity: .5; background: red; }",
        r#"<div class="b"><p>x</p></div>"#,
    ));
    assert!(group.contains("layer.enabled: true"), "{group}");
    let (alone, _) = convert(&page(
        ".b { opacity: .5; background: red; width: 9px; height: 9px; }",
        r#"<div class="b"></div>"#,
    ));
    assert!(!alone.contains("layer.enabled"), "{alone}");
}

#[test]
fn text_shadow_is_a_copy_of_the_text_behind_it() {
    let (qml, _) = convert(&page(
        ".b { text-shadow: 1px 2px 4px red; }",
        r#"<p class="b">x</p>"#,
    ));
    assert!(qml.contains("EldurTextShadow"), "{qml}");
    assert!(qml.contains("blurDeviation: 2"), "{qml}");
}

#[test]
fn a_style_a_rectangle_cannot_draw_goes_to_the_drawn_border() {
    let (qml, diags) = convert(&page(".b{border:4px dashed #000}", "<div class=b>x</div>"));
    assert!(qml.contains("EldurBorder"), "{qml}");
    assert!(
        qml.contains(r#"sideStyles: ["dashed", "dashed", "dashed", "dashed"]"#),
        "{qml}"
    );
    assert!(
        !diags.iter().any(|d| d.contains("drawn solid")),
        "{diags:?}"
    );
}

#[test]
fn a_plain_border_stays_on_the_rectangle() {
    let (qml, _) = convert(&page(".b{border:4px solid #000}", "<div class=b>x</div>"));
    assert!(!qml.contains("EldurBorder"), "{qml}");
    assert!(qml.contains("border.width: 4"), "{qml}");
}

#[test]
fn an_outline_takes_no_room_and_sits_outside_the_box() {
    let (qml, _) = convert(&page(
        ".b{outline:3px solid red;outline-offset:2px}",
        "<div class=b>x</div>",
    ));
    assert!(qml.contains("anchors.margins: -5"), "{qml}");
    assert!(qml.contains("border.width: 3"), "{qml}");
}
