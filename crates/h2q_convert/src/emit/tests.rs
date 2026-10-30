//! Layout decisions of the flex and grid emitters.

use std::collections::BTreeMap;

use h2q_css::{Align, GridLine, Length, LengthAuto, LengthPercentage, Track};

use super::flex::{Cross, Pack, cross, pack, spacer_slots};
use h2q_qml::Value;

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
        TrackSize::Fixed(h2q_qml::Value::Number(220.0))
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
