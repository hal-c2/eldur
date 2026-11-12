//! Grid containers → `GridLayout` (layout.md §4).
//!
//! Placement (CSS Grid 2 §8) is resolved here, so the `GridLayout` only
//! sizes tracks. Qt has no track list: a track's size goes on the items
//! that occupy it alone (and on a placeholder `Item` when none does).
//!
//! Measured Qt behaviour this relies on: extra space goes to the cells
//! even when nothing fills (so fixed-only tracks get a filler track), and
//! fill items with a preferred size of 0 share space equally while stretch
//! factors are unreliable (so unequal `fr` tracks are split into equal
//! sub-tracks, see [`subdivide`]).

use std::collections::{BTreeMap, HashSet};

use eldur_css::{
    Align, BoxSizing, ComputedStyle, GridLine, Length, LengthPercentage, Size, TextDecorationLine,
    Track,
};
use eldur_qml::{Member, Object, Value};

use super::flex::{Pack, pack, spacer_slots};
use super::flow::{has_height, is_layout, is_viewport_relative, size_value};
use super::units::{fmt_number, length_expr, number, round};
use super::{Emitter, Given};
use crate::boxes::{LayoutBox, describe, node_line};

/// One axis of an item's position before auto-placement: a 0-based start
/// line, or none; and its span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AxisPos {
    pub start: Option<usize>,
    pub span: usize,
}

impl AxisPos {
    pub(crate) fn auto(span: usize) -> Self {
        Self { start: None, span }
    }
    #[cfg(test)]
    pub(crate) fn at(start: usize, span: usize) -> Self {
        Self {
            start: Some(start),
            span,
        }
    }
}

/// A placed item, 0-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed {
    pub row: usize,
    pub col: usize,
    pub row_span: usize,
    pub col_span: usize,
}

/// The result of placement: positions in item order, and the size of the
/// implicit grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placement {
    pub items: Vec<Placed>,
    pub rows: usize,
    pub cols: usize,
}

/// Resolves `start`/`end` of one axis (CSS Grid 2 §8.3). `explicit` is the
/// number of explicit tracks; `areas` maps area names to their 0-based
/// line range on this axis. Lines before the grid are clamped to its
/// start (implicit tracks before the explicit grid are not modelled), and
/// unknown names count as `auto`; both return `lossy`.
pub(crate) fn resolve_axis(
    start: &GridLine,
    end: &GridLine,
    explicit: usize,
    areas: &BTreeMap<String, (usize, usize)>,
) -> (AxisPos, bool) {
    let mut lossy = false;
    let lines = explicit as i64 + 1;
    let mut line = |l: &GridLine, is_start: bool| -> Option<i64> {
        match l {
            GridLine::Line(n) if *n > 0 => Some(i64::from(*n) - 1),
            GridLine::Line(n) if *n < 0 => Some(lines + i64::from(*n)),
            GridLine::Named(name) => {
                let (base, side) = if let Some(b) = name.strip_suffix("-start") {
                    (b, Some(true))
                } else if let Some(b) = name.strip_suffix("-end") {
                    (b, Some(false))
                } else {
                    (name.as_str(), None)
                };
                match areas.get(base) {
                    Some(&(s, e)) => {
                        let use_start = side.unwrap_or(is_start);
                        Some(if use_start { s as i64 } else { e as i64 })
                    }
                    None => {
                        lossy = true;
                        None
                    }
                }
            }
            _ => None,
        }
    };
    let s = line(start, true);
    let e = line(end, false);
    let span_of = |l: &GridLine| match l {
        GridLine::Span(n) => Some(i64::from((*n).max(1))),
        _ => None,
    };
    let (pos_start, span) = match (s, e) {
        (Some(s), Some(e)) if e == s => (Some(s), 1),
        (Some(s), Some(e)) => (Some(s.min(e)), (e - s).abs()),
        (Some(s), None) => (Some(s), span_of(end).unwrap_or(1)),
        (None, Some(e)) => {
            let span = span_of(start).unwrap_or(1);
            (Some(e - span), span)
        }
        (None, None) => (None, span_of(start).or_else(|| span_of(end)).unwrap_or(1)),
    };
    let pos_start = pos_start.map(|s| {
        if s < 0 {
            lossy = true;
        }
        s.max(0) as usize
    });
    (
        AxisPos {
            start: pos_start,
            span: span.max(1) as usize,
        },
        lossy,
    )
}

/// Sparse auto-placement (CSS Grid 2 §8.5). `items` are `(row, col)` in
/// `order`-modified document order. With `column_flow` the roles of rows
/// and columns swap.
pub(crate) fn place(
    items: &[(AxisPos, AxisPos)],
    explicit_rows: usize,
    explicit_cols: usize,
    column_flow: bool,
) -> Placement {
    if column_flow {
        let swapped: Vec<_> = items.iter().map(|&(r, c)| (c, r)).collect();
        let p = place_rows(&swapped, explicit_rows);
        return Placement {
            items: p
                .items
                .into_iter()
                .map(|i| Placed {
                    row: i.col,
                    col: i.row,
                    row_span: i.col_span,
                    col_span: i.row_span,
                })
                .collect(),
            rows: p.cols,
            cols: p.rows,
        };
    }
    place_rows(items, explicit_cols)
}

/// Row-flow placement; `explicit_cols` is the "minor" axis size.
fn place_rows(items: &[(AxisPos, AxisPos)], explicit_cols: usize) -> Placement {
    let mut occupied: HashSet<(usize, usize)> = HashSet::new();
    let mut out: Vec<Option<Placed>> = vec![None; items.len()];
    let fits = |occ: &HashSet<(usize, usize)>, r: usize, c: usize, rs: usize, cs: usize| {
        (r..r + rs).all(|r| (c..c + cs).all(|c| !occ.contains(&(r, c))))
    };
    let mark = |occ: &mut HashSet<(usize, usize)>, p: &Placed| {
        for r in p.row..p.row + p.row_span {
            for c in p.col..p.col + p.col_span {
                occ.insert((r, c));
            }
        }
    };
    // 1. Items with a definite position in both axes.
    for (i, (r, c)) in items.iter().enumerate() {
        if let (Some(row), Some(col)) = (r.start, c.start) {
            let p = Placed {
                row,
                col,
                row_span: r.span,
                col_span: c.span,
            };
            mark(&mut occupied, &p);
            out[i] = Some(p);
        }
    }
    // 2. Items locked to a row: earliest free column past the previous
    //    item placed in that row by this step.
    let mut row_cursor: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, (r, c)) in items.iter().enumerate() {
        if let (Some(row), None) = (r.start, c.start) {
            let mut col = row_cursor.get(&row).copied().unwrap_or(0);
            while !fits(&occupied, row, col, r.span, c.span) {
                col += 1;
            }
            let p = Placed {
                row,
                col,
                row_span: r.span,
                col_span: c.span,
            };
            mark(&mut occupied, &p);
            row_cursor.insert(row, col + c.span);
            out[i] = Some(p);
        }
    }
    // 3. The implicit grid's columns.
    let mut cols = explicit_cols.max(1);
    for (i, (_, c)) in items.iter().enumerate() {
        cols = cols.max(c.span);
        if let Some(p) = &out[i] {
            cols = cols.max(p.col + p.col_span);
        } else if let Some(start) = c.start {
            cols = cols.max(start + c.span);
        }
    }
    // 4. Everything else, with the cursor.
    let (mut cur_r, mut cur_c) = (0usize, 0usize);
    for (i, (r, c)) in items.iter().enumerate() {
        if out[i].is_some() {
            continue;
        }
        let p = if let Some(col) = c.start {
            if col < cur_c {
                cur_r += 1;
            }
            cur_c = col;
            while !fits(&occupied, cur_r, col, r.span, c.span) {
                cur_r += 1;
            }
            Placed {
                row: cur_r,
                col,
                row_span: r.span,
                col_span: c.span,
            }
        } else {
            loop {
                if cur_c + c.span > cols {
                    cur_r += 1;
                    cur_c = 0;
                    continue;
                }
                if fits(&occupied, cur_r, cur_c, r.span, c.span) {
                    break;
                }
                cur_c += 1;
            }
            Placed {
                row: cur_r,
                col: cur_c,
                row_span: r.span,
                col_span: c.span,
            }
        };
        mark(&mut occupied, &p);
        // The cursor stays on the item's start; overlap checks move the
        // next item on.
        out[i] = Some(p);
    }
    let items: Vec<Placed> = out.into_iter().flatten().collect();
    let rows = items.iter().map(|p| p.row + p.row_span).max().unwrap_or(0);
    let cols = items
        .iter()
        .map(|p| p.col + p.col_span)
        .max()
        .unwrap_or(0)
        .max(cols);
    Placement { items, rows, cols }
}

/// A 0-based `[start, end)` line range.
pub(crate) type LineRange = (usize, usize);
/// An area's rows and columns.
pub(crate) type AreaRange = (LineRange, LineRange);

/// Bounding boxes of `grid-template-areas`: name → (rows, cols), each a
/// 0-based `[start, end)` line range. Non-rectangular areas make the
/// declaration invalid in CSS; the cascade has already dropped those.
pub(crate) fn area_ranges(areas: &[Vec<Option<String>>]) -> BTreeMap<String, AreaRange> {
    let mut out: BTreeMap<String, AreaRange> = BTreeMap::new();
    for (r, row) in areas.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if let Some(name) = cell {
                let e = out.entry(name.clone()).or_insert(((r, r + 1), (c, c + 1)));
                e.0.0 = e.0.0.min(r);
                e.0.1 = e.0.1.max(r + 1);
                e.1.0 = e.1.0.min(c);
                e.1.1 = e.1.1.max(c + 1);
            }
        }
    }
    out
}

/// What a track becomes in Qt.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TrackSize {
    /// Pinned to a size: px, or an expression of the grid's size.
    Fixed(Value),
    Fr {
        fr: f32,
        min: Option<f32>,
    },
    /// Content-sized; stretches when the axis has no `fr` track.
    Auto {
        min: Option<f32>,
    },
    /// `minmax(<px>, <px>)`.
    Range {
        min: f32,
        max: f32,
    },
}

/// Classifies a track. `base` is the grid's size the percentages are of
/// (`parent.width`; `None` for rows, whose percentages need a definite
/// height). Returns whether something was approximated.
pub(crate) fn track_size(track: &Track, base: Option<&str>) -> (TrackSize, bool) {
    let px = |t: &Track| match t {
        Track::Length(l) => l.as_px(),
        _ => None,
    };
    match track {
        Track::Length(l) => match (l.as_px(), base) {
            (Some(px), _) => (TrackSize::Fixed(number(px)), false),
            (None, Some(base)) => (TrackSize::Fixed(Value::expr(length_expr(l, base))), false),
            (None, None) if is_viewport_relative(l) => (
                TrackSize::Fixed(Value::expr(length_expr(l, "parent.height"))),
                false,
            ),
            (None, None) => (TrackSize::Auto { min: None }, true),
        },
        Track::Fr(fr) => (TrackSize::Fr { fr: *fr, min: None }, false),
        Track::Auto => (TrackSize::Auto { min: None }, false),
        Track::MinContent | Track::MaxContent | Track::FitContent(_) => {
            (TrackSize::Auto { min: None }, true)
        }
        Track::MinMax(min, max) => {
            let min_px = px(min);
            let lossy = min_px.is_none() && !matches!(**min, Track::Auto);
            match &**max {
                Track::Fr(fr) => (
                    TrackSize::Fr {
                        fr: *fr,
                        min: min_px,
                    },
                    lossy,
                ),
                Track::Length(l) if l.as_px().is_some() => match min_px {
                    Some(lo) => (
                        TrackSize::Range {
                            min: lo,
                            max: l.as_px().unwrap_or(lo),
                        },
                        false,
                    ),
                    None => (TrackSize::Fixed(number(l.as_px().unwrap_or(0.0))), true),
                },
                Track::Auto => (TrackSize::Auto { min: min_px }, lossy),
                _ => (TrackSize::Auto { min: min_px }, true),
            }
        }
        Track::AutoRepeat { .. } => (TrackSize::Auto { min: None }, true),
    }
}

/// The sizing of one grid axis.
struct AxisPlan {
    tracks: Vec<TrackSize>,
    gap: f32,
    horizontal: bool,
    /// `align-content: normal` or `stretch`: `auto` tracks take free space.
    stretch: bool,
}

/// Most equal sub-columns an axis of unequal `fr` tracks is split into.
const MAX_SUBTRACKS: usize = 12;

/// Splits an axis of px and unequal `fr` tracks into equal sub-tracks:
/// the number of sub-tracks each track becomes (1 for px tracks), with
/// ratios within 1% of the `fr` ratios when 12 sub-tracks allow it, else
/// the closest. `None` when the `fr` are equal or other tracks are
/// present.
///
/// Qt cannot size unequal `fr` tracks directly: stretch factors are not
/// proportional (measured 2:1 → 4:1 with a preferred size of 1), fill items
/// with proportional preferred sizes do not shrink proportionally, and
/// preferred sizes bound to the grid's width re-polish the layout without
/// end. Equal fill sub-tracks are exact; [`GapSplit`] keeps the gaps
/// between tracks.
pub(crate) fn subdivide(tracks: &[TrackSize]) -> Option<Vec<usize>> {
    let all_px_fr = tracks.iter().all(|t| match t {
        TrackSize::Fixed(Value::Number(_)) => true,
        TrackSize::Fr { min, .. } => min.is_none(),
        _ => false,
    });
    let frs: Vec<f32> = tracks
        .iter()
        .filter_map(|t| match t {
            TrackSize::Fr { fr, .. } => Some(*fr),
            _ => None,
        })
        .collect();
    if !all_px_fr || !frs.windows(2).any(|w| w[0] != w[1]) {
        return None;
    }
    let total: f32 = frs.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut best: Option<(f32, Vec<usize>)> = None;
    for k in frs.len()..=MAX_SUBTRACKS {
        let counts: Vec<usize> = frs
            .iter()
            .map(|f| ((f / total * k as f32).round() as usize).max(1))
            .collect();
        let sum: usize = counts.iter().sum();
        if sum > MAX_SUBTRACKS {
            continue;
        }
        let err = frs
            .iter()
            .zip(&counts)
            .map(|(f, c)| (*c as f32 / sum as f32 - f / total).abs())
            .fold(0.0, f32::max);
        if best.as_ref().is_none_or(|(e, _)| err < *e - 1e-6) {
            best = Some((err, counts));
        }
        if err < 0.01 {
            break;
        }
    }
    let (_, counts) = best?;
    let mut counts = counts.into_iter();
    Some(
        tracks
            .iter()
            .map(|t| match t {
                TrackSize::Fr { .. } => counts.next().unwrap_or(1),
                _ => 1,
            })
            .collect(),
    )
}

impl AxisPlan {
    fn new(tracks: Vec<TrackSize>, gap: f32, fr_active: bool, horizontal: bool) -> Self {
        let mut tracks = tracks;
        if !fr_active {
            for t in &mut tracks {
                if let TrackSize::Fr { min, .. } = t {
                    *t = TrackSize::Auto { min: *min };
                }
            }
        }
        Self {
            tracks,
            gap,
            horizontal,
            stretch: true,
        }
    }

    fn unequal_fr(&self) -> bool {
        let frs: Vec<f32> = self
            .tracks
            .iter()
            .filter_map(|t| match t {
                TrackSize::Fr { fr, .. } => Some(*fr),
                _ => None,
            })
            .collect();
        frs.windows(2).any(|w| w[0] != w[1])
    }

    fn is_fr(&self, track: usize) -> bool {
        matches!(self.tracks.get(track), Some(TrackSize::Fr { .. }))
    }

    fn has_fr(&self) -> bool {
        self.tracks
            .iter()
            .any(|t| matches!(t, TrackSize::Fr { .. }))
    }

    fn content_sized(&self, start: usize, span: usize) -> bool {
        self.tracks
            .iter()
            .skip(start)
            .take(span)
            .all(|t| matches!(t, TrackSize::Auto { .. }))
    }

    /// Whether some track takes free space; otherwise a filler track does.
    fn absorbs(&self) -> bool {
        self.tracks.iter().any(|t| {
            matches!(
                t,
                TrackSize::Fr { .. } | TrackSize::Auto { .. } | TrackSize::Range { .. }
            )
        })
    }

    /// `Layout.*` for an item spanning `[start, start + span)` that
    /// stretches in this axis. `None` when the track is content-sized and
    /// the item keeps its natural size.
    fn props(&self, start: usize, span: usize, layout_item: bool) -> Vec<(String, Value)> {
        let dim = if self.horizontal { "Width" } else { "Height" };
        let tracks =
            &self.tracks[start.min(self.tracks.len())..(start + span).min(self.tracks.len())];
        let gaps = self.gap * span.saturating_sub(1) as f32;
        let pinned = |v: Value| {
            vec![
                (format!("Layout.preferred{dim}"), v.clone()),
                (format!("Layout.minimum{dim}"), v.clone()),
                (format!("Layout.maximum{dim}"), v),
            ]
        };
        let fill = |pref_zero: bool, min: Option<f32>, max: Option<f32>| {
            let mut v = vec![(format!("Layout.fill{dim}"), Value::Bool(true))];
            if pref_zero {
                v.push((format!("Layout.preferred{dim}"), Value::Number(0.0)));
            }
            if let Some(m) = min {
                v.push((format!("Layout.minimum{dim}"), number(m)));
            }
            match max {
                Some(m) => v.push((format!("Layout.maximum{dim}"), number(m))),
                None if layout_item => v.push((
                    format!("Layout.maximum{dim}"),
                    Value::expr("Number.POSITIVE_INFINITY"),
                )),
                None => {}
            }
            v
        };
        if tracks.is_empty() {
            return fill(false, None, None);
        }
        if let [track] = tracks {
            return match track {
                TrackSize::Fixed(v) => pinned(v.clone()),
                TrackSize::Fr { min, .. } => fill(true, *min, None),
                TrackSize::Auto { min } if !self.has_fr() && self.stretch => {
                    fill(false, *min, None)
                }
                TrackSize::Auto { min: Some(m) } => {
                    vec![(format!("Layout.minimum{dim}"), number(*m))]
                }
                TrackSize::Auto { min: None } => Vec::new(),
                TrackSize::Range { min, max } => fill(false, Some(*min), Some(*max)),
            };
        }
        if tracks
            .iter()
            .all(|t| matches!(t, TrackSize::Fixed(Value::Number(_))))
        {
            let sum: f32 = tracks
                .iter()
                .map(|t| match t {
                    TrackSize::Fixed(Value::Number(n)) => *n as f32,
                    _ => 0.0,
                })
                .sum();
            return pinned(number(sum + gaps));
        }
        let any_fr = tracks.iter().any(|t| matches!(t, TrackSize::Fr { .. }));
        fill(any_fr, None, None)
    }
}

/// Makes room for free space between rows: `slots[i]` empty rows go before
/// row `i`, each sharing it equally. Every child has its `Layout.row`.
fn spread_rows(grid: &mut Object, slots: &[u32]) {
    let moved = |row: usize| row + slots[..=row.min(slots.len() - 1)].iter().sum::<u32>() as usize;
    for kid in grid.children_mut() {
        let Some(Value::Number(row)) = kid.get("Layout.row").cloned() else {
            continue;
        };
        let row = row as usize;
        if let Some(Value::Number(span)) = kid.get("Layout.rowSpan").cloned() {
            let last = row + span as usize - 1;
            kid.set(
                "Layout.rowSpan",
                Value::Number((moved(last) - moved(row) + 1) as f64),
            );
        }
        kid.set("Layout.row", Value::Number(moved(row) as f64));
    }
    for (i, n) in slots.iter().enumerate() {
        let before = i + slots[..i].iter().sum::<u32>() as usize;
        for k in 0..*n as usize {
            grid.push_child(
                Object::new("Item")
                    .prop("Layout.row", Value::Number((before + k) as f64))
                    .prop("Layout.column", Value::Number(0.0))
                    .prop("Layout.fillHeight", true),
            );
        }
    }
}

/// `justify-items`/`align-items` overridden by `-self` as a Qt flag;
/// `None` when the item stretches (`normal` stretches non-replaced boxes,
/// CSS Box Alignment 3 §6).
fn self_alignment(items: Align, own: Align, horizontal: bool) -> Option<&'static str> {
    let a = if own == Align::Auto { items } else { own };
    match (a, horizontal) {
        (Align::Normal | Align::Auto | Align::Stretch, _) => None,
        (Align::Start | Align::FlexStart | Align::Left, true) => Some("Qt.AlignLeft"),
        (Align::End | Align::FlexEnd | Align::Right, true) => Some("Qt.AlignRight"),
        (Align::Center, true) => Some("Qt.AlignHCenter"),
        (_, true) => Some("Qt.AlignLeft"),
        (Align::End | Align::FlexEnd, false) => Some("Qt.AlignBottom"),
        (Align::Center, false) => Some("Qt.AlignVCenter"),
        (Align::Baseline, false) => Some("Qt.AlignBaseline"),
        (_, false) => Some("Qt.AlignTop"),
    }
}

impl Emitter<'_, '_> {
    /// An empty grid a script fills. What it comes to hold is not known, so
    /// no cell can be kept free for a placeholder carrying a track: the
    /// runtime gives each item its cell and its column's size.
    pub(super) fn grid_to_fill(
        &mut self,
        b: &LayoutBox<'_>,
        decoration: TextDecorationLine,
        line: Option<u32>,
    ) -> Object {
        let mut grid = self.grid(b, &[], decoration, line);
        let style = &*b.style;
        // A fixed size as itself, a fraction as its negative.
        let tracks: Option<Vec<f64>> = if style.grid_template_columns.is_empty() {
            Some(vec![-1.0])
        } else {
            style
                .grid_template_columns
                .iter()
                .map(|t| match t {
                    Track::Length(l) => l.as_px().map(f64::from),
                    Track::Fr(fr) => Some(-f64::from(*fr)),
                    _ => None,
                })
                .collect()
        };
        let Some(tracks) = tracks else {
            return grid;
        };
        let n = tracks.len();
        let fills = tracks.iter().any(|t| *t < 0.0);
        grid.members.retain(|m| !matches!(m, Member::Child(_)));
        grid.set("columns", Value::Number((n + usize::from(!fills)) as f64));
        let column_gap = self.gap_px(style.column_gap.as_ref(), line);
        let row_gap = self.gap_px(style.row_gap.as_ref(), line);
        grid.set("columnSpacing", number(column_gap));
        grid.set("rowSpacing", number(row_gap));
        grid.members.push(Member::PropertyDecl {
            ty: "var".to_owned(),
            name: "eldurTracks".to_owned(),
            value: Some(Value::List(tracks.into_iter().map(Value::Number).collect())),
            readonly: false,
            required: false,
        });
        if !fills {
            grid.push_child(
                Object::new("Item")
                    .prop("Layout.row", Value::Number(0.0))
                    .prop("Layout.column", Value::Number(n as f64))
                    .prop("Layout.fillWidth", true),
            );
        }
        grid
    }

    pub(super) fn grid(
        &mut self,
        b: &LayoutBox<'_>,
        children: &[LayoutBox<'_>],
        decoration: TextDecorationLine,
        line: Option<u32>,
    ) -> Object {
        let style = &*b.style;
        let mut items: Vec<&LayoutBox<'_>> = children.iter().collect();
        items.sort_by_key(|c| c.style.order);
        let column_gap = self.gap_px(style.column_gap.as_ref(), line);
        let row_gap = self.gap_px(style.row_gap.as_ref(), line);

        if let [Track::AutoRepeat { fill, tracks }] = style.grid_template_columns.as_slice()
            && let Some(min_cell) = repeat_min_cell(tracks)
        {
            let explicit = items.iter().any(|c| {
                let s = &c.style;
                [
                    &s.grid_column_start,
                    &s.grid_column_end,
                    &s.grid_row_start,
                    &s.grid_row_end,
                ]
                .iter()
                .any(|l| **l != GridLine::Auto)
            });
            if explicit {
                self.diags.info(
                    "qml",
                    line,
                    format!(
                        "grid placement of the items of the auto-repeating {} is not converted",
                        self.what(b)
                    ),
                );
            }
            if style.grid_auto_flow_column {
                self.diags.info(
                    "qml",
                    line,
                    "`grid-auto-flow: column` with auto-repeated columns is laid out by row"
                        .to_owned(),
                );
            }
            let objs: Vec<(Object, Option<String>)> = items
                .iter()
                .map(|c| {
                    let obj = self.item(c, decoration).obj;
                    (obj, c.node.map(|n| describe(self.doc, n)))
                })
                .collect();
            let mut grid = self.auto_fit_grid(b, min_cell, !*fill, column_gap, row_gap, objs);
            if let Some(h) = auto_row_px(style) {
                for obj in grid.children_mut() {
                    obj.set("Layout.preferredHeight", number(h));
                    obj.set("Layout.minimumHeight", number(h));
                    obj.set("Layout.maximumHeight", number(h));
                }
            }
            return grid;
        }

        let mut lossy = false;
        let areas = area_ranges(&style.grid_template_areas);
        let area_rows: BTreeMap<String, (usize, usize)> =
            areas.iter().map(|(k, v)| (k.clone(), v.0)).collect();
        let area_cols: BTreeMap<String, (usize, usize)> =
            areas.iter().map(|(k, v)| (k.clone(), v.1)).collect();
        let explicit_cols = style.grid_template_columns.len().max(
            style
                .grid_template_areas
                .iter()
                .map(Vec::len)
                .max()
                .unwrap_or(0),
        );
        let explicit_rows = style
            .grid_template_rows
            .len()
            .max(style.grid_template_areas.len());
        if style
            .grid_template_columns
            .iter()
            .chain(&style.grid_template_rows)
            .any(|t| matches!(t, Track::AutoRepeat { .. }))
        {
            self.diags.info(
                "qml",
                line,
                format!(
                    "`repeat(auto-fill|auto-fit, …)` on {} is only converted as the whole \
                     column list with a px minimum; it is laid out as one `auto` track",
                    self.what(b)
                ),
            );
        }

        let mut specs = Vec::with_capacity(items.len());
        let mut trivial = !style.grid_auto_flow_column;
        for c in &items {
            let s = &c.style;
            let (row, l1) = resolve_axis(
                &s.grid_row_start,
                &s.grid_row_end,
                explicit_rows,
                &area_rows,
            );
            let (col, l2) = resolve_axis(
                &s.grid_column_start,
                &s.grid_column_end,
                explicit_cols,
                &area_cols,
            );
            lossy |= l1 || l2;
            trivial &= row == AxisPos::auto(1) && col == AxisPos::auto(1);
            specs.push((row, col));
        }
        if lossy {
            self.diags.info(
                "css",
                line,
                format!(
                    "grid lines before the grid or with unknown names on the items of {} are \
                     approximated",
                    self.what(b)
                ),
            );
        }
        let placement = place(
            &specs,
            explicit_rows,
            explicit_cols,
            style.grid_auto_flow_column,
        );

        let mut approx = false;
        let mut tracks_for = |template: &[Track], auto: &[Track], n: usize, base: Option<&str>| {
            (0..n)
                .map(|i| {
                    let t = template.get(i).cloned().or_else(|| {
                        (!auto.is_empty())
                            .then(|| auto[(i - template.len().min(i)) % auto.len()].clone())
                    });
                    let (size, lossy) = track_size(&t.unwrap_or(Track::Auto), base);
                    approx |= lossy;
                    size
                })
                .collect::<Vec<_>>()
        };
        let col_tracks = tracks_for(
            &style.grid_template_columns,
            &style.grid_auto_columns,
            placement.cols,
            Some("parent.width"),
        );
        let mut placement = placement;
        let mut gap_split = None;
        let col_tracks = match subdivide(&col_tracks) {
            Some(counts) => {
                let mut offsets = vec![0];
                for c in &counts {
                    offsets.push(offsets.last().copied().unwrap_or(0) + c);
                }
                let frs: Vec<f32> = col_tracks
                    .iter()
                    .filter_map(|t| match t {
                        TrackSize::Fr { fr, .. } => Some(*fr),
                        _ => None,
                    })
                    .collect();
                let all_fr = frs.len() == col_tracks.len();
                if all_fr && column_gap > 0.0 {
                    gap_split = Some(GapSplit::new(&offsets, column_gap));
                }
                let total: f32 = frs.iter().sum();
                let k = offsets.last().copied().unwrap_or(0) as f32;
                let exact = frs
                    .iter()
                    .zip(&counts)
                    .all(|(f, c)| (f / total - *c as f32 / k).abs() < 1e-4);
                if all_fr && !exact {
                    let ratio = |v: Vec<String>| v.join(":");
                    self.diags.info(
                        "qml",
                        line,
                        format!(
                            "`fr` columns {} on {} are split into equal sub-columns as {}",
                            ratio(frs.iter().map(|f| fmt_number(round(*f))).collect()),
                            self.what(b),
                            ratio(counts.iter().map(usize::to_string).collect()),
                        ),
                    );
                }
                for p in &mut placement.items {
                    let end = offsets[p.col + p.col_span];
                    p.col = offsets[p.col];
                    p.col_span = end - p.col;
                }
                placement.cols = offsets.last().copied().unwrap_or(0);
                if gap_split.is_none() {
                    self.diags.info(
                        "qml",
                        line,
                        format!(
                            "unequal `fr` columns on {} are split into {} equal sub-columns; \
                             items are off by a fraction of the gap",
                            self.what(b),
                            placement.cols
                        ),
                    );
                }
                col_tracks
                    .into_iter()
                    .zip(counts)
                    .flat_map(|(t, k)| match t {
                        TrackSize::Fr { .. } => vec![TrackSize::Fr { fr: 1.0, min: None }; k],
                        t => vec![t],
                    })
                    .collect()
            }
            None => col_tracks,
        };
        let given = self.own_given;
        // A percentage of an auto height is auto (CSS 2 §10.5).
        let definite_height = match &style.height {
            Size::Length(l) if !is_viewport_relative(l) && l.as_px().is_none() => {
                self.percent_resolves()
                    || matches!(&style.min_height, Size::Length(m) if m.as_px() != Some(0.0))
            }
            _ => has_height(style),
        };
        let definite = definite_height || given == Given::Definite;
        let tall = definite || given == Given::Stretched;
        let row_tracks = tracks_for(
            &style.grid_template_rows,
            &style.grid_auto_rows,
            placement.rows,
            definite.then_some("parent.height"),
        );
        if approx {
            self.diags.info(
                "qml",
                line,
                format!(
                    "content-sized grid tracks (`min-content`, `max-content`, `fit-content()`) \
                     on {} are laid out as `auto`",
                    self.what(b)
                ),
            );
        }
        let column_gap = if gap_split.is_some() { 0.0 } else { column_gap };
        let cols = AxisPlan::new(col_tracks, column_gap, true, true);
        let mut rows = AxisPlan::new(row_tracks, row_gap, definite, false);
        rows.stretch = matches!(
            style.align_content,
            Align::Normal | Align::Auto | Align::Stretch
        );
        // Free space `align-content` hands out; `fr` tracks leave none.
        let packed = (tall && !rows.stretch && !rows.has_fr() && placement.rows > 0)
            .then(|| pack(style.align_content, false));
        let row_slots = packed
            .filter(|p| *p != Pack::Start)
            .map(|p| spacer_slots(p, placement.rows, false));
        if cols.unequal_fr() {
            self.diags.info(
                "qml",
                line,
                format!(
                    "unequal `fr` tracks next to `auto`, `%` or `minmax()` tracks on {} \
                     are laid out as equal",
                    self.what(b)
                ),
            );
        }

        let filler_col = !cols.absorbs();
        let filler_row = placement.rows > 0
            && tall
            && (packed == Some(Pack::Start) || (rows.stretch && !rows.absorbs()));
        // Tracks no item occupies alone get a placeholder in a free cell,
        // or in a zero-height carrier row after the last one.
        let mut occupied = HashSet::new();
        for p in &placement.items {
            for r in p.row..p.row + p.row_span {
                for c in p.col..p.col + p.col_span {
                    occupied.insert((r, c));
                }
            }
        }
        let carried_cols: HashSet<usize> = placement
            .items
            .iter()
            .filter(|p| p.col_span == 1)
            .map(|p| p.col)
            .collect();
        let carried_rows: HashSet<usize> = placement
            .items
            .iter()
            .filter(|p| p.row_span == 1)
            .map(|p| p.row)
            .collect();
        let mut placeholders: Vec<(usize, usize, bool, bool)> = Vec::new();
        let mut carrier_row = false;
        for c in (0..placement.cols).filter(|c| !carried_cols.contains(c)) {
            match (0..placement.rows).find(|r| !occupied.contains(&(*r, c))) {
                Some(r) => {
                    occupied.insert((r, c));
                    placeholders.push((r, c, true, !carried_rows.contains(&r)));
                }
                None => {
                    carrier_row = true;
                    placeholders.push((placement.rows, c, true, false));
                }
            }
        }
        for r in (0..placement.rows).filter(|r| !carried_rows.contains(r)) {
            if placeholders.iter().any(|p| p.0 == r) {
                continue;
            }
            if let Some(c) = (0..placement.cols).find(|c| !occupied.contains(&(r, *c))) {
                occupied.insert((r, c));
                placeholders.push((r, c, false, true));
            }
        }
        // Qt puts row spacing before the carrier row too, so the row gap
        // becomes the top margin of what starts in a later row.
        let gap_margins = (carrier_row || row_slots.is_some()) && row_gap > 0.0;
        let explicit =
            !trivial || !placeholders.is_empty() || filler_col || filler_row || row_slots.is_some();

        let mut grid = Object::new("GridLayout");
        grid.set(
            "columns",
            Value::Number((placement.cols + usize::from(filler_col)) as f64),
        );
        grid.set("columnSpacing", number(column_gap));
        grid.set(
            "rowSpacing",
            number(if gap_margins { 0.0 } else { row_gap }),
        );
        // Only a plain run of same-sized cells can follow its items out of the
        // layout: spans and other tracks would change more than the cell.
        let flows = explicit
            && row_slots.is_none()
            && placeholders.is_empty()
            && !gap_margins
            && gap_split.is_none()
            && (1..placement.cols).all(|c| cols.props(c, 1, true) == cols.props(0, 1, true))
            && placement.items.iter().enumerate().all(|(i, p)| {
                p.row_span == 1 && p.col_span == 1 && p.row * placement.cols + p.col == i
            });
        let mut cell_follows = false;
        for (child, p) in items.iter().zip(&placement.items) {
            let stretches = self_alignment(style.align_items, child.style.align_self, false)
                .is_none()
                && !matches!(&child.style.height, Size::Length(l) if !matches!(l, Length::Linear(p) if p.percent != 0.0));
            let single_content_row = placement.cols == 1
                && !tall
                && style.grid_template_rows.is_empty()
                && style.grid_auto_rows.is_empty();
            let pinned_track =
                p.row_span == 1 && matches!(rows.tracks.get(p.row), Some(TrackSize::Fixed(_)));
            self.child_given = if !stretches || single_content_row {
                Given::Content
            } else if definite || pinned_track {
                Given::Definite
            } else {
                Given::Stretched
            };
            let mut obj = self.item(child, decoration).obj;
            self.grid_item(&mut obj, child, style, p, &cols, &rows, explicit);
            cell_follows |= flows && (self.hides_items || obj.get("visible").is_some());
            if gap_margins && p.row > 0 {
                add_margin(&mut obj, "Layout.topMargin", row_gap);
            }
            if let Some(split) = &gap_split {
                let (left, right) = split.margins(p.col, p.col + p.col_span);
                for (name, px) in [("Layout.leftMargin", left), ("Layout.rightMargin", right)] {
                    if px != 0.0 {
                        add_margin(&mut obj, name, px);
                    }
                }
            }
            if self.options.source_comments
                && let Some(node) = child.node
            {
                grid.members.push(Member::Comment(describe(self.doc, node)));
            }
            grid.push_child(obj);
        }
        if cell_follows {
            let across = placement.cols;
            for obj in grid.members.iter_mut().filter_map(|m| match m {
                Member::Child(o) => Some(o),
                _ => None,
            }) {
                obj.set(
                    "Layout.row",
                    Value::expr(format!(
                        "parent && parent.eldurCell ? Math.floor(parent.eldurCell(this) / {across}) : 0"
                    )),
                );
                obj.set(
                    "Layout.column",
                    Value::expr(format!(
                        "parent && parent.eldurCell ? parent.eldurCell(this) % {across} : 0"
                    )),
                );
            }
            grid.members.push(Member::Function {
                name: "eldurCell".to_owned(),
                params: vec!["item".to_owned()],
                body: [
                    "// `display: none` takes no cell (CSS Grid 2 §8).",
                    "let cell = 0",
                    "for (const kid of children) {",
                    "    if (kid === item) break",
                    "    if (kid.visible) ++cell",
                    "}",
                    "return cell",
                ]
                .map(str::to_owned)
                .to_vec(),
            });
        }
        for (r, c, size_col, size_row) in placeholders {
            let mut ph = Object::new("Item");
            ph.set("Layout.row", Value::Number(r as f64));
            ph.set("Layout.column", Value::Number(c as f64));
            if size_col {
                for (k, v) in cols.props(c, 1, false) {
                    ph.set(&k, v);
                }
            }
            if size_row {
                for (k, v) in rows.props(r, 1, false) {
                    ph.set(&k, v);
                }
            }
            if gap_margins && r > 0 && r < placement.rows {
                add_margin(&mut ph, "Layout.topMargin", row_gap);
            }
            if let Some(split) = &gap_split {
                // Only its share matters: it keeps the sub-columns equal.
                add_margin(&mut ph, "Layout.rightMargin", split.share());
            }
            grid.push_child(ph);
        }
        // Qt caps a row at what its items can take, past an `fr` share.
        let capped_fr = grid.children().zip(&placement.items).any(|(kid, p)| {
            rows.is_fr(p.row)
                && matches!(kid.get("Layout.maximumHeight"), Some(Value::Number(_) | Value::Expr(_)))
                && !matches!(kid.get("Layout.maximumHeight"), Some(Value::Expr(e)) if e == "Number.POSITIVE_INFINITY")
        });
        if ((rows.stretch && tall && !rows.has_fr())
            || (definite && (rows.unequal_fr() || capped_fr)))
            && !cell_follows
            && !carrier_row
            && !self.row_carriers(
                &mut grid,
                &placement,
                &rows,
                definite,
                column_gap,
                gap_split.is_some(),
            )
            && rows.unequal_fr()
        {
            self.diags.info(
                "qml",
                line,
                format!(
                    "unequal `fr` rows on {} are laid out as equal",
                    self.what(b)
                ),
            );
        }
        let last_row = placement.rows + usize::from(carrier_row);
        if filler_col {
            let mut f = Object::new("Item");
            f.set("Layout.row", Value::Number(0.0));
            f.set("Layout.column", Value::Number(placement.cols as f64));
            f.set("Layout.fillWidth", true);
            grid.push_child(f);
        }
        if filler_row {
            let mut f = Object::new("Item");
            f.set("Layout.row", Value::Number(last_row as f64));
            f.set("Layout.column", Value::Number(0.0));
            f.set("Layout.fillHeight", true);
            grid.push_child(f);
        }
        if let Some(slots) = &row_slots {
            spread_rows(&mut grid, slots);
        }
        // One column of content-high rows is a column: each row is as high as
        // its one item, so stretching it means nothing and the sub-pixel
        // positions can be kept as in a block column. Only when nothing gives
        // the grid more height than that.
        if placement.cols == 1
            && !explicit
            && !carrier_row
            && !tall
            && style.grid_template_rows.is_empty()
            && style.grid_auto_rows.is_empty()
        {
            for obj in grid.children_mut() {
                if matches!(obj.get("Layout.fillHeight"), Some(Value::Bool(true))) {
                    obj.set("Layout.fillHeight", false);
                }
            }
            self.carry(&mut grid);
        }
        grid
    }

    /// Qt shares a grid's free height by the rows' preferred heights, and a row
    /// nothing fills takes none; CSS gives each `auto` track an equal part
    /// (CSS Grid 2 §12.8). A zero-wide item in a last column makes each row
    /// take part. When the grid's height comes from outside it, the item says
    /// its row's height outright, as the height cannot depend on it, and so
    /// unequal `fr` rows take their shares. Otherwise Qt's sharing stands, which
    /// is CSS's only for rows of equal heights. Whether it could do this.
    fn row_carriers(
        &mut self,
        grid: &mut Object,
        placement: &Placement,
        rows: &AxisPlan,
        definite: bool,
        column_gap: f32,
        split: bool,
    ) -> bool {
        let auto_rows: Vec<usize> = (0..placement.rows)
            .filter(|r| matches!(rows.tracks.get(*r), Some(TrackSize::Auto { .. })))
            .collect();
        let frs = rows.has_fr();
        if (auto_rows.len() < 2 && !frs)
            || self.no_carry
            || self.hides_items
            || split
            || (placement.cols > 1 && column_gap > 0.0)
            || placement.items.iter().any(|p| p.row_span > 1)
            || !rows.tracks.iter().all(|t| {
                matches!(
                    t,
                    TrackSize::Auto { .. }
                        | TrackSize::Fr { .. }
                        | TrackSize::Fixed(Value::Number(_))
                )
            })
        {
            return false;
        }
        let kids: Vec<&mut Object> = grid.children_mut().take(placement.items.len()).collect();
        let any_fixed = kids.iter().zip(&placement.items).any(|(k, p)| {
            auto_rows.contains(&p.row)
                && !matches!(k.get("Layout.fillHeight"), Some(Value::Bool(true)))
        });
        if kids.iter().any(|k| k.get("visible").is_some()) || (!definite && !any_fixed) {
            return false;
        }
        let mut terms: Vec<Vec<String>> = vec![Vec::new(); placement.rows];
        for (kid, p) in kids.into_iter().zip(&placement.items) {
            // The new column would otherwise take part in auto-placement.
            kid.set("Layout.row", Value::Number(p.row as f64));
            kid.set("Layout.column", Value::Number(p.col as f64));
            let margin = |name: &str| match kid.get(name) {
                Some(Value::Number(n)) => *n,
                _ => 0.0,
            };
            let margins = margin("Layout.topMargin") + margin("Layout.bottomMargin");
            let own = match kid.get("Layout.preferredHeight") {
                Some(Value::Number(n)) => Some(*n),
                _ => None,
            };
            let height = match own {
                Some(n) => fmt_number(n + margins),
                None => {
                    let id = self.item_id(kid);
                    if margins == 0.0 {
                        format!("{id}.implicitHeight")
                    } else {
                        format!("{id}.implicitHeight + {}", fmt_number(margins))
                    }
                }
            };
            terms[p.row].push(height);
        }
        let col = grid.get("columns").and_then(|c| match c {
            Value::Number(n) => Some(*n),
            _ => None,
        });
        let col = col.unwrap_or(1.0);
        grid.set("columns", Value::Number(col + 1.0));
        if placement.cols == 1 {
            grid.set("columnSpacing", Value::Number(0.0));
        }
        let mut bases = Vec::new();
        for (r, track) in rows.tracks.iter().enumerate() {
            let fr = match track {
                TrackSize::Fr { fr, .. } => Some(*fr),
                _ => None,
            };
            if let Some(fr) = fr {
                let mut carrier = Object::new("Item");
                carrier.set("Layout.row", Value::Number(r as f64));
                carrier.set("Layout.column", Value::Number(col));
                carrier.set(
                    "Layout.preferredHeight",
                    Value::expr(format!(
                        "Math.round(parent.eldurFree * {})",
                        fmt_number(f64::from(fr))
                    )),
                );
                grid.push_child(carrier);
                continue;
            }
            if !auto_rows.contains(&r) {
                continue;
            }
            let min = match track {
                TrackSize::Auto { min: Some(m) } => fmt_number(f64::from(*m)),
                _ => "0".to_owned(),
            };
            let base = format!("Math.max({min}, {})", terms[r].join(", "));
            let mut carrier = Object::new("Item");
            carrier.id = Some(self.ids.claim("gridRow"));
            carrier.members.push(Member::PropertyDecl {
                ty: "real".to_owned(),
                name: "eldurBase".to_owned(),
                value: Some(Value::expr(base)),
                readonly: false,
                required: false,
            });
            carrier.set("Layout.row", Value::Number(r as f64));
            carrier.set("Layout.column", Value::Number(col));
            if frs {
                carrier.set("Layout.preferredHeight", Value::expr("eldurBase"));
            } else if definite {
                carrier.set(
                    "Layout.preferredHeight",
                    Value::expr("eldurBase + parent.eldurFree"),
                );
            } else {
                carrier.set("Layout.fillHeight", true);
                carrier.set("Layout.preferredHeight", Value::expr("eldurBase"));
            }
            bases.push(format!("{}.eldurBase", carrier.id.clone().expect("set")));
            grid.push_child(carrier);
        }
        if definite {
            let fixed: f64 = rows
                .tracks
                .iter()
                .filter_map(|t| match t {
                    TrackSize::Fixed(Value::Number(n)) => Some(*n),
                    _ => None,
                })
                .sum();
            let gaps = f64::from(rows.gap) * placement.rows.saturating_sub(1) as f64;
            let taken = fmt_number(fixed + gaps);
            let shares: f64 = rows
                .tracks
                .iter()
                .filter_map(|t| match t {
                    TrackSize::Fr { fr, .. } => Some(f64::from(*fr)),
                    _ => None,
                })
                .sum();
            let shares = if frs { shares } else { bases.len() as f64 };
            let bases: String = bases.iter().map(|b| format!(" - {b}")).collect();
            let free = format!(
                "Math.max(0, (height - {taken}{bases}) / {})",
                fmt_number(shares)
            );
            // Qt rounds each row up; a row a hair over the height would raise it.
            let free = if frs {
                free
            } else {
                format!("Math.floor({free})")
            };
            grid.members.push(Member::PropertyDecl {
                ty: "real".to_owned(),
                name: "eldurFree".to_owned(),
                value: Some(Value::expr(free)),
                readonly: false,
                required: false,
            });
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn grid_item(
        &mut self,
        obj: &mut Object,
        b: &LayoutBox<'_>,
        container: &ComputedStyle,
        p: &Placed,
        cols: &AxisPlan,
        rows: &AxisPlan,
        explicit: bool,
    ) {
        let style = &*b.style;
        let line = b.node.and_then(|n| node_line(self.doc, n));
        if explicit {
            obj.set("Layout.row", Value::Number(p.row as f64));
            obj.set("Layout.column", Value::Number(p.col as f64));
        }
        if p.row_span > 1 {
            obj.set("Layout.rowSpan", Value::Number(p.row_span as f64));
        }
        if p.col_span > 1 {
            obj.set("Layout.columnSpan", Value::Number(p.col_span as f64));
        }
        let insets = self.insets(style, line);
        let (extra_w, extra_h) = match style.box_sizing {
            BoxSizing::ContentBox => (insets.horizontal(), insets.vertical()),
            BoxSizing::BorderBox => (0.0, 0.0),
        };
        let layout_item = is_layout(&obj.type_name);
        let h_align = self_alignment(container.justify_items, style.justify_self, true);
        let v_align = self_alignment(container.align_items, style.align_self, false);
        // A column is as wide as what it holds: an item narrower than its fixed track
        // keeps the rest of it open, or the next column would start at its edge.
        if p.col_span == 1
            && h_align.is_none()
            && let Some(TrackSize::Fixed(Value::Number(track))) = cols.tracks.get(p.col)
            && let Size::Length(l) = &style.width
            && let Some(width) = l.as_px()
            && f64::from(width + extra_w) < *track
        {
            add_margin(
                obj,
                "Layout.rightMargin",
                (*track - f64::from(width + extra_w)) as f32,
            );
        }
        let mut flags = Vec::new();
        let mut ignored_alignment = false;
        for (plan, start, span, align, size, horizontal, extra) in [
            (
                cols,
                p.col,
                p.col_span,
                h_align,
                &style.width,
                true,
                extra_w,
            ),
            (
                rows,
                p.row,
                p.row_span,
                v_align,
                &style.height,
                false,
                extra_h,
            ),
        ] {
            let dim = if horizontal { "Width" } else { "Height" };
            let own = match size {
                // `100%` is the grid area, which filling it gives; a
                // binding to the grid's width would be the whole grid.
                Size::Length(Length::Linear(l))
                    if horizontal && *l == LengthPercentage::percent(100.0) =>
                {
                    None
                }
                Size::Length(l) if horizontal || l.as_px().is_some() || is_viewport_relative(l) => {
                    Some(size_value(l, extra))
                }
                _ => None,
            };
            let props = plan.props(start, span, layout_item);
            let in_fr = !horizontal && span == 1 && plan.is_fr(start);
            // A percentage of the grid area cannot size the tracks that make the
            // area: the item takes its `fr` share like the rest of its tracks and
            // stops at its own width. Given as its preferred width, it would widen
            // its tracks past their share and squeeze the `fr` columns beside them.
            let shares = horizontal
                && matches!(size, Size::Length(l) if l.as_px().is_none() && !is_viewport_relative(l))
                && (start..start + span).all(|t| plan.is_fr(t));
            match (own, align) {
                (Some(v), align) if shares => {
                    obj.set("Layout.preferredWidth", Value::Number(0.0));
                    obj.set("Layout.fillWidth", true);
                    obj.set("Layout.maximumWidth", v);
                    flags.push(align.unwrap_or("Qt.AlignLeft"));
                }
                // Its size is no part of what the track is given: all `fr`
                // rows are at 0, and it only stops short of filling.
                (Some(v), align) if in_fr => {
                    obj.set("Layout.preferredHeight", Value::Number(0.0));
                    obj.set("Layout.fillHeight", true);
                    obj.set("Layout.maximumHeight", v);
                    flags.push(align.unwrap_or("Qt.AlignTop"));
                }
                (None, Some(a)) if in_fr => {
                    obj.set("Layout.preferredHeight", Value::Number(0.0));
                    obj.set("Layout.fillHeight", true);
                    obj.set("Layout.maximumHeight", Value::expr("implicitHeight"));
                    flags.push(a);
                }
                // A sized item keeps its size in its area.
                (Some(v), align) => {
                    obj.set(&format!("Layout.preferred{dim}"), v);
                    match align {
                        Some(a) => flags.push(a),
                        // Stretching gives way to a set height, at the start.
                        None if !horizontal => flags.push("Qt.AlignTop"),
                        None => {}
                    }
                    // Layouts fill by default, past their preferred size.
                    if layout_item {
                        obj.set(&format!("Layout.fill{dim}"), false);
                    }
                }
                // Stretching block flow would spread its children apart; a CSS
                // block's stay at the top and only its unpainted end moves.
                (None, None)
                    if !horizontal
                        && obj.type_name == "ColumnLayout"
                        && b.kind == crate::boxes::BoxKind::Block
                        && plan.content_sized(start, span) =>
                {
                    for (k, v) in props.into_iter().filter(|(k, _)| !k.contains("fill")) {
                        obj.set(&k, v);
                    }
                    obj.set("Layout.fillHeight", false);
                    flags.push("Qt.AlignTop");
                }
                (None, None) => {
                    if !horizontal && layout_item && props.is_empty() {
                        obj.set("Layout.fillHeight", false);
                    }
                    for (k, v) in props {
                        obj.set(&k, v);
                    }
                }
                // Content-sized tracks fit the item, so it can align.
                (None, Some(a)) if plan.content_sized(start, span) => {
                    for (k, v) in props.into_iter().filter(|(k, _)| !k.contains("fill")) {
                        obj.set(&k, v);
                    }
                    if layout_item {
                        obj.set(&format!("Layout.fill{dim}"), false);
                    }
                    flags.push(a);
                }
                (None, Some(_)) => {
                    // Qt sizes tracks from their items: an aligned item
                    // in a sized track still carries the track's size.
                    ignored_alignment = true;
                    for (k, v) in props {
                        obj.set(&k, v);
                    }
                }
            }
        }
        if !flags.is_empty() {
            obj.set("Layout.alignment", Value::expr(flags.join(" | ")));
        }
        if ignored_alignment {
            self.diags.info(
                "qml",
                line,
                format!(
                    "`justify-self`/`align-self` on {} in a sized grid track is laid out as \
                     `stretch`",
                    self.what(b)
                ),
            );
        }
        let m = &style.margin;
        let mut nonzero_margin = false;
        for (name, margin) in [
            ("Layout.leftMargin", &m.left),
            ("Layout.rightMargin", &m.right),
            ("Layout.topMargin", &m.top),
            ("Layout.bottomMargin", &m.bottom),
        ] {
            if let eldur_css::LengthAuto::Length(l) = margin
                && let Some(px) = l.as_px()
                && px > 0.0
            {
                obj.set(name, number(px));
            } else if !matches!(margin, eldur_css::LengthAuto::Length(l) if l.as_px().is_some_and(|v| v >= 0.0))
            {
                nonzero_margin = true;
            }
        }
        if nonzero_margin {
            self.diags.info(
                "qml",
                line,
                format!(
                    "auto, negative or percentage margins on the grid item {} are ignored",
                    self.what(b)
                ),
            );
        }
    }

    /// `repeat(auto-fill|auto-fit, minmax(<px>, 1fr))` (layout.md §4.1),
    /// also used for wrapping flex items that grow from one basis: the
    /// column count follows the grid's width. With `fit`, columns never
    /// outnumber the items and the cells stretch; otherwise (`auto-fill`)
    /// empty columns keep their share.
    pub(super) fn auto_fit_grid(
        &mut self,
        b: &LayoutBox<'_>,
        min_cell: f32,
        fit: bool,
        column_gap: f32,
        row_gap: f32,
        items: Vec<(Object, Option<String>)>,
    ) -> Object {
        let id = self
            .ids
            .claim(&format!("{} grid", super::hint(self.doc, b)));
        let n = items.len();
        let mut grid = Object::new("GridLayout").with_id(&id);
        grid.members.push(Member::PropertyDecl {
            ty: "real".to_owned(),
            name: "minCell".to_owned(),
            value: Some(number(min_cell)),
            readonly: true,
            required: false,
        });
        grid.members.push(Member::PropertyDecl {
            ty: "int".to_owned(),
            name: "fitCols".to_owned(),
            value: Some(Value::expr(
                "Math.max(1, Math.floor((width + columnSpacing) / (minCell + columnSpacing)))",
            )),
            readonly: true,
            required: false,
        });
        grid.set(
            "columns",
            Value::expr(if fit {
                format!("Math.min({}, fitCols)", n.max(1))
            } else {
                "fitCols".to_owned()
            }),
        );
        grid.set("columnSpacing", number(column_gap));
        grid.set("rowSpacing", number(row_gap));
        if fit {
            grid.set("uniformCellWidths", true);
        }
        for (mut obj, comment) in items {
            let layout_item = is_layout(&obj.type_name);
            if fit {
                obj.set("Layout.fillWidth", true);
                obj.set("Layout.preferredWidth", Value::Number(0.0));
            } else {
                obj.set(
                    "Layout.preferredWidth",
                    Value::expr(format!(
                        "({id}.width - ({id}.fitCols - 1) * {id}.columnSpacing) / {id}.fitCols"
                    )),
                );
            }
            obj.set("Layout.fillHeight", true);
            if layout_item {
                obj.set(
                    "Layout.maximumHeight",
                    Value::expr("Number.POSITIVE_INFINITY"),
                );
                if fit {
                    obj.set(
                        "Layout.maximumWidth",
                        Value::expr("Number.POSITIVE_INFINITY"),
                    );
                }
            }
            if self.options.source_comments
                && let Some(c) = comment
            {
                grid.members.push(Member::Comment(c));
            }
            grid.push_child(obj);
        }
        grid
    }
}

/// The px minimum of `repeat(auto-…, minmax(<px>, <fr>|auto))` or
/// `repeat(auto-…, <px>)`.
fn repeat_min_cell(tracks: &[Track]) -> Option<f32> {
    match tracks {
        [Track::Length(l)] => l.as_px(),
        [Track::MinMax(min, max)] if matches!(**max, Track::Fr(_) | Track::Auto) => match &**min {
            Track::Length(l) => l.as_px(),
            _ => None,
        },
        _ => None,
    }
}

fn auto_row_px(style: &ComputedStyle) -> Option<f32> {
    match style.grid_auto_rows.as_slice() {
        [Track::Length(l)] => l.as_px(),
        _ => None,
    }
}

/// The column gap of `fr` tracks split into equal sub-columns, moved from
/// `columnSpacing` (which would put it between the sub-columns of a track
/// too) into the items' margins.
///
/// With no spacing, sub-column `j` of `k` spans `[jW/k, (j+1)W/k)`; CSS
/// puts track `t` at `t·g + (W − G)·o_t/k`, where `o_t` is its first
/// sub-column and `G` all the gaps. The difference is the item's left
/// margin, and its right margin likewise at its end. Every sub-column
/// then carries `G/k` of margin, so Qt still shares the width equally.
struct GapSplit {
    /// The track each sub-column line starts, if any.
    track_at: Vec<Option<usize>>,
    sub_columns: usize,
    gap: f32,
    total_gap: f32,
}

impl GapSplit {
    fn new(offsets: &[usize], gap: f32) -> Self {
        let sub_columns = offsets.last().copied().unwrap_or(0);
        let mut track_at = vec![None; sub_columns + 1];
        for (t, o) in offsets.iter().enumerate() {
            track_at[*o] = Some(t);
        }
        Self {
            track_at,
            sub_columns,
            gap,
            total_gap: gap * offsets.len().saturating_sub(2) as f32,
        }
    }

    /// The margin each sub-column carries.
    fn share(&self) -> f32 {
        self.total_gap / self.sub_columns as f32
    }

    /// Left and right margins of an item in sub-columns `start..end`,
    /// which start and end on track lines.
    fn margins(&self, start: usize, end: usize) -> (f32, f32) {
        let before = |line: usize| self.share() * line as f32;
        let left = self.track_at[start].map_or(0.0, |t| t as f32 * self.gap - before(start));
        let right = self.track_at[end].map_or(0.0, |t| before(end) - (t as f32 - 1.0) * self.gap);
        (left, right)
    }
}

/// Adds `px` to a px `Layout.*Margin` (or sets it).
fn add_margin(obj: &mut Object, name: &str, px: f32) {
    let current = match obj.get(name) {
        Some(Value::Number(n)) => *n as f32,
        _ => 0.0,
    };
    obj.set(name, number(current + px));
}
