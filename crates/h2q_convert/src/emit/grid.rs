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

use h2q_css::{Align, BoxSizing, ComputedStyle, GridLine, Size, TextDecorationLine, Track};
use h2q_qml::{Member, Object, Value};

use super::Emitter;
use super::flow::{has_height, is_layout, is_viewport_relative, size_value};
use super::units::{fmt_number, length_expr, number, round};
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
                TrackSize::Auto { min } if !self.has_fr() => fill(false, *min, None),
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
        let definite_height = has_height(style);
        let row_tracks = tracks_for(
            &style.grid_template_rows,
            &style.grid_auto_rows,
            placement.rows,
            definite_height.then_some("parent.height"),
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
        let rows = AxisPlan::new(row_tracks, row_gap, definite_height, false);
        let unequal_stretch = cols.unequal_fr() || rows.unequal_fr();
        if unequal_stretch {
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
        let filler_row = definite_height && !rows.absorbs() && placement.rows > 0;
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
        let gap_margins = carrier_row && row_gap > 0.0;
        let explicit = !trivial || !placeholders.is_empty() || filler_col || filler_row;

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
        for (child, p) in items.iter().zip(&placement.items) {
            let mut obj = self.item(child, decoration).obj;
            self.grid_item(&mut obj, child, style, p, &cols, &rows, explicit);
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
        grid
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
                Size::Length(l) if horizontal || l.as_px().is_some() || is_viewport_relative(l) => {
                    Some(size_value(l, extra))
                }
                _ => None,
            };
            let props = plan.props(start, span, layout_item);
            match (own, align) {
                // A sized item keeps its size in its area.
                (Some(v), align) => {
                    obj.set(&format!("Layout.preferred{dim}"), v);
                    if let Some(a) = align {
                        flags.push(a);
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
            if let h2q_css::LengthAuto::Length(l) = margin
                && let Some(px) = l.as_px()
                && px > 0.0
            {
                obj.set(name, number(px));
            } else if !matches!(margin, h2q_css::LengthAuto::Length(l) if l.as_px().is_some_and(|v| v >= 0.0))
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
