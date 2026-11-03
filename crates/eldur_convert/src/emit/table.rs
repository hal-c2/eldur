//! `display: table` (CSS 2.1 §17, layout.md §8) → `GridLayout`.
//!
//! Why `GridLayout`: rows, columns and spans stay explicit in the QML
//! (`Layout.row`, `Layout.columnSpan`, …), which is how a person writes a
//! table, and it needs no delegate model the way `TableView` does. What it
//! cannot do is the sizing: stretch factors are not proportional, it rounds
//! fractional rows up one by one so they drift, and preferred sizes bound to
//! its own `implicitWidth` re-polish without end. So column widths and row
//! heights come from the `table*` functions emitted once on the root (see
//! [`SOLVER`]), and the grid only places what they computed.

use std::borrow::Cow;

use eldur_css::{
    BorderCollapse, BorderSide, BorderStyle, ComputedStyle, Display, Length, LengthPercentage,
    Rgba, Size, TableLayout, TextDecorationLine, TransformFn,
};
use eldur_js::TargetKind;
use eldur_qml::{Member, Object, Value};

use super::decoration::{Decoration, Insets, border_width};
use super::flow::{Emitted, MarginSet};
use super::text::font_family;
use super::units::{ROOT_ID, fmt_number, number, round};
use super::{Emitter, Target, hint};
use crate::boxes::{Content, Inline, LayoutBox, node_line};

#[cfg(test)]
mod tests;

/// Largest `colspan`, and largest `rowspan`, per HTML §4.9.11.
const MAX_COLSPAN: usize = 1000;
const MAX_ROWSPAN: usize = 65534;

/// Where a cell sits in the table's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slot {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
}

/// A row's cells as `(colspan, rowspan)`, `rowspan` 0 meaning "to the end of
/// the row group", and the index of the row's group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowSpans {
    pub group: usize,
    pub cells: Vec<(usize, usize)>,
}

/// The HTML table-forming algorithm (§4.9.12): cells take the first column
/// free of earlier rowspans; a rowspan stops at the end of its row group.
/// Returns the slots row by row and the number of columns.
pub(crate) fn place_cells(rows: &[RowSpans]) -> (Vec<Vec<Slot>>, usize) {
    // `taken[c]` is the first row below `c`'s current rowspan.
    let mut taken: Vec<usize> = Vec::new();
    let mut slots = Vec::with_capacity(rows.len());
    for (r, row) in rows.iter().enumerate() {
        let group_end = rows[r..]
            .iter()
            .position(|o| o.group != row.group)
            .map_or(rows.len(), |n| r + n);
        let mut column = 0;
        let mut placed = Vec::with_capacity(row.cells.len());
        for &(colspan, rowspan) in &row.cells {
            let free = |taken: &[usize], c: usize| taken.get(c).is_none_or(|&until| until <= r);
            while !free(&taken, column) {
                column += 1;
            }
            let column_span = colspan.clamp(1, MAX_COLSPAN);
            let until_end = group_end - r;
            let row_span = match rowspan {
                0 => until_end,
                n => n.min(MAX_ROWSPAN).min(until_end),
            };
            if taken.len() < column + column_span {
                taken.resize(column + column_span, 0);
            }
            for until in &mut taken[column..column + column_span] {
                *until = r + row_span;
            }
            placed.push(Slot {
                row: r,
                column,
                row_span,
                column_span,
            });
            column += column_span;
        }
        slots.push(placed);
    }
    let columns = taken.len();
    (slots, columns)
}

/// Who a border in the collapsing model belongs to, weakest first (CSS 2.1
/// §17.6.2.1 rule 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BorderOwner {
    Table,
    ColumnGroup,
    Column,
    RowGroup,
    Row,
    Cell,
}

fn style_rank(style: BorderStyle) -> u8 {
    match style {
        BorderStyle::Double => 8,
        BorderStyle::Solid => 7,
        BorderStyle::Dashed => 6,
        BorderStyle::Dotted => 5,
        BorderStyle::Ridge => 4,
        BorderStyle::Outset => 3,
        BorderStyle::Groove => 2,
        BorderStyle::Inset => 1,
        BorderStyle::None | BorderStyle::Hidden => 0,
    }
}

/// CSS 2.1 §17.6.2.1: whether `new` beats `old` on the same edge. `hidden`
/// wins over everything, `none` loses to everything, then wider, then the
/// style order, then the owner; on a full tie the one seen first, the
/// leftmost and topmost, stays.
pub(crate) fn beats(new: (&BorderSide, BorderOwner), old: (&BorderSide, BorderOwner)) -> bool {
    if old.0.style == BorderStyle::Hidden {
        return false;
    }
    if new.0.style == BorderStyle::Hidden {
        return true;
    }
    let (nw, ow) = (border_width(new.0), border_width(old.0));
    if nw != ow {
        return nw > ow;
    }
    let (ns, os) = (style_rank(new.0.style), style_rank(old.0.style));
    if ns != os {
        return ns > os;
    }
    new.1 > old.1
}

fn no_border() -> BorderSide {
    BorderSide::default()
}

/// The grid's edges in the collapsing border model.
pub(crate) struct Edges {
    /// `horizontal[r][c]`: the edge above row `r` in column `c`.
    horizontal: Vec<Vec<(BorderSide, BorderOwner)>>,
    /// `vertical[r][c]`: the edge left of column `c` in row `r`.
    vertical: Vec<Vec<(BorderSide, BorderOwner)>>,
    rows: usize,
    columns: usize,
}

impl Edges {
    pub(crate) fn new(rows: usize, columns: usize) -> Self {
        let empty = (no_border(), BorderOwner::Table);
        Self {
            horizontal: vec![vec![empty; columns]; rows + 1],
            vertical: vec![vec![empty; columns + 1]; rows],
            rows,
            columns,
        }
    }

    fn offer_h(&mut self, r: usize, c: usize, side: &BorderSide, owner: BorderOwner) {
        let edge = &mut self.horizontal[r][c];
        if beats((side, owner), (&edge.0, edge.1)) {
            *edge = (*side, owner);
        }
    }

    fn offer_v(&mut self, r: usize, c: usize, side: &BorderSide, owner: BorderOwner) {
        let edge = &mut self.vertical[r][c];
        if beats((side, owner), (&edge.0, edge.1)) {
            *edge = (*side, owner);
        }
    }

    /// Offers the borders of a box covering `slot`'s rows and columns.
    pub(crate) fn offer(&mut self, slot: Slot, style: &ComputedStyle, owner: BorderOwner) {
        let b = &style.border;
        for c in slot.column..slot.column + slot.column_span {
            self.offer_h(slot.row, c, &b.top, owner);
            self.offer_h(slot.row + slot.row_span, c, &b.bottom, owner);
        }
        for r in slot.row..slot.row + slot.row_span {
            self.offer_v(r, slot.column, &b.left, owner);
            self.offer_v(r, slot.column + slot.column_span, &b.right, owner);
        }
    }

    /// How far each column edge of the browser lies to the right of ours,
    /// from the first row (CSS 2.1 §17.6.2: the table's outer border width
    /// is half its first row's). Chromium's cells reach to the middle of
    /// their borders, in fractions of a pixel; ours draw an outer edge whole
    /// and an inner one's larger half in the cell after it (`cell_borders`).
    pub(crate) fn column_offsets(&self) -> Vec<f32> {
        let Some(row) = self.vertical.first() else {
            return vec![0.0; self.columns + 1];
        };
        let half = |c: usize| border_width(&row[c].0) / 2.0;
        (0..=self.columns)
            .map(|c| match c {
                0 => half(0),
                c if c == self.columns => -half(c),
                c => half(c) - half(c).floor(),
            })
            .collect()
    }

    /// The edges a cell at `slot` lies on, whole: a spanning cell's side is
    /// its widest segment.
    pub(crate) fn whole_sides(&self, slot: Slot) -> [BorderSide; 4] {
        let widest = |segments: Vec<&(BorderSide, BorderOwner)>| {
            let mut best = no_border();
            for (side, _) in segments {
                if border_width(side) > border_width(&best) {
                    best = *side;
                }
            }
            best
        };
        let columns = slot.column..slot.column + slot.column_span;
        let rows = slot.row..slot.row + slot.row_span;
        let top = widest(
            columns
                .clone()
                .map(|c| &self.horizontal[slot.row][c])
                .collect(),
        );
        let bottom = widest(
            columns
                .map(|c| &self.horizontal[slot.row + slot.row_span][c])
                .collect(),
        );
        let left = widest(
            rows.clone()
                .map(|r| &self.vertical[r][slot.column])
                .collect(),
        );
        let right = widest(
            rows.map(|r| &self.vertical[r][slot.column + slot.column_span])
                .collect(),
        );
        [top, right, bottom, left]
    }

    /// The borders a cell at `slot` draws: an inner edge is split between
    /// its two cells, larger half on the right/below (measured in Chromium:
    /// a 1px edge belongs to the cell after it); an outer edge is drawn whole
    /// inside the table box. Rows and columns start the same way in any
    /// column, so an edge's widest segment stands for a spanning cell's side.
    pub(crate) fn cell_borders(&self, slot: Slot) -> [BorderSide; 4] {
        let [mut top, mut right, mut bottom, mut left] = self.whole_sides(slot);
        let halve = |side: &mut BorderSide, large: bool| {
            side.width = if large {
                (side.width / 2.0).ceil()
            } else {
                (side.width / 2.0).floor()
            };
        };
        if slot.row != 0 {
            halve(&mut top, true);
        }
        if slot.column != 0 {
            halve(&mut left, true);
        }
        if slot.row + slot.row_span != self.rows {
            halve(&mut bottom, false);
        }
        if slot.column + slot.column_span != self.columns {
            halve(&mut right, false);
        }
        [top, right, bottom, left]
    }
}

/// A column's specified width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ColumnSpec {
    Auto,
    Px(f32),
    Percent(f32),
}

impl ColumnSpec {
    fn of(length: &Length, extra: f32) -> Self {
        match length {
            Length::Linear(l) if l.percent == 0.0 && l.vw == 0.0 && l.vh == 0.0 => {
                Self::Px(l.px + extra)
            }
            Length::Linear(l) if l.px == 0.0 && l.vw == 0.0 && l.vh == 0.0 => {
                Self::Percent(l.percent)
            }
            _ => Self::Auto,
        }
    }

    /// The larger of two widths specified for one column; a percentage
    /// beats a length (CSS 2.1 §17.5.2.2 leaves the rest to the browser).
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Auto, o) | (o, Self::Auto) => o,
            (Self::Percent(a), Self::Percent(b)) => Self::Percent(a.max(b)),
            (p @ Self::Percent(_), _) | (_, p @ Self::Percent(_)) => p,
            (Self::Px(a), Self::Px(b)) => Self::Px(a.max(b)),
        }
    }

    fn qml(self) -> String {
        match self {
            Self::Auto => "null".to_owned(),
            Self::Px(px) => fmt_number(round(px)),
            Self::Percent(p) => format!("\"{}%\"", fmt_number(round(p))),
        }
    }
}

/// Column widths and row heights. The extents are read from the cells inside
/// a binding, so they follow text and font changes.
///
/// Auto layout is the distribution of css-tables-3 §3.9, as Chromium does it
/// (measured: `tests/table_widths/`). A column's minimum is its min-content
/// width, a width in px or % notwithstanding; its maximum is its max-content
/// width, or its px width if larger. The column widths step through four
/// guesses: all at their minimum; percent columns at their share of the
/// table; px columns at their maximum too; auto columns too. A width between
/// two guesses grows the columns that differ in proportion to how much they
/// differ. Width past the last goes to auto columns with content in
/// proportion to their maximum, failing those to px columns, then to percent
/// columns, then to all alike. Percentages past 100% in total are cut.
///
/// A table with percent columns and `width: auto` is wide enough for each to
/// have its share: the rest at max-content over what they leave, or one at
/// max-content over its percentage.
///
/// In the collapsing model a cell draws its outer border whole, where in the
/// browser half of it is the table's, and an inner one's larger half, where
/// the browser's cells reach to its middle. `tableEdges` on the grid says how
/// far each column edge of the browser's lies right of ours: the columns
/// share width as the browser's do, and their edges move back to ours.
///
/// Results are snapped by cumulative rounding so column edges land on the
/// pixels the browser's do, and so `GridLayout` has no fraction to round up;
/// but a column at its max-content width is rounded up, since a text wraps
/// in a fraction less than it needs, and a table at max-content is as wide
/// as those columns.
const SOLVER: &[(&str, &[&str], &str)] = &[
    (
        "tableExtents",
        &["grid", "specs"],
        r#"const n = specs.length
const edges = grid.tableEdges || []
const shift = b => edges[b] || 0
const min = new Array(n).fill(0)
const max = new Array(n).fill(0)
const spanning = []
// Rows a script added are not in the grid; the runtime says what their cells need.
const cells = Array.from(grid.children).concat(grid.eldurCreatedCells || [])
for (let i = 0; i < cells.length; ++i) {
    const cell = cells[i]
    const slot = cell.tableSlot
    if (!slot || cell.visible === false)
        continue
    const more = shift(slot[1] + slot[3]) - shift(slot[1])
    const need = { slot: slot, min: cell.minContentWidth + more, max: cell.implicitWidth + more }
    if (slot[3] > 1) {
        spanning.push(need)
        continue
    }
    min[slot[1]] = Math.max(min[slot[1]], need.min)
    max[slot[1]] = Math.max(max[slot[1]], need.max)
}
for (const cell of spanning) {
    const first = cell.slot[1]
    const last = first + cell.slot[3]
    const gaps = grid.columnSpacing * (cell.slot[3] - 1)
    for (const [widths, wanted] of [[min, cell.min], [max, cell.max]]) {
        let have = 0
        let weight = 0
        for (let c = first; c < last; ++c) {
            have += widths[c]
            weight += max[c]
        }
        const missing = wanted - gaps - have
        if (missing <= 0)
            continue
        const add = []
        for (let c = first; c < last; ++c)
            add.push(weight > 0 ? missing * max[c] / weight : missing / cell.slot[3])
        for (let c = first; c < last; ++c)
            widths[c] += add[c - first]
    }
}
for (let c = 0; c < n; ++c) {
    max[c] = Math.max(max[c], min[c])
    if (typeof specs[c] === "number")
        max[c] = Math.max(max[c], specs[c])
}
// Percentages count up to 100% in all, in column order.
const percents = []
let total = 0
for (let c = 0; c < n; ++c) {
    const p = typeof specs[c] === "string" ? Math.max(0, Math.min(parseFloat(specs[c]), 100 - total)) : 0
    percents.push(p)
    total += p
}
// What our cells have past the browser's: its outer border halves.
const outside = shift(0) - shift(n)
// A column whose content is one word cannot wrap; a word's overhang past its
// advance is under a pixel, a space and a letter more.
const wraps = max.map((m, c) => m - min[c] > 1)
return { min: min, max: max, percents: percents, shift: shift, outside: outside, wraps: wraps }"#,
    ),
    (
        "tableMaxContentWidth",
        &["grid", "specs", "fixed"],
        r#"let sum = grid.columnSpacing * (specs.length - 1)
if (fixed) {
    for (let c = 0; c < specs.length; ++c)
        sum += typeof specs[c] === "number" ? specs[c] : 0
    return sum
}
const extents = root.tableExtents(grid, specs)
let all = 0
let rest = 0
let percent = 0
let wanted = 0
for (let c = 0; c < specs.length; ++c) {
    all += extents.max[c]
    if (typeof specs[c] !== "string")
        rest += extents.max[c]
    else if (extents.percents[c] > 0)
        wanted = Math.max(wanted, extents.max[c] * 100 / extents.percents[c])
    percent += extents.percents[c]
}
if (percent > 0 && percent < 100)
    wanted = Math.max(wanted, rest * 100 / (100 - percent))
// Columns come out whole pixels, those at max-content that can wrap rounded up.
let whole = 0
for (let c = 0; c < specs.length; ++c) {
    const drawn = extents.max[c] + extents.shift(c) - extents.shift(c + 1)
    whole += extents.wraps[c] ? Math.ceil(drawn - 1e-6) : drawn
}
return sum + Math.max(Math.max(all, wanted) + extents.outside, whole)"#,
    ),
    (
        "tableMinContentWidth",
        &["grid", "specs", "fixed"],
        r#"let sum = grid.columnSpacing * (specs.length - 1)
if (fixed) {
    for (let c = 0; c < specs.length; ++c)
        sum += typeof specs[c] === "number" ? specs[c] : 0
    return sum
}
const extents = root.tableExtents(grid, specs)
for (let c = 0; c < specs.length; ++c)
    sum += extents.min[c]
return sum + extents.outside"#,
    ),
    (
        "tableColumnWidths",
        &["grid", "specs", "fixed"],
        r#"const n = specs.length
if (n === 0)
    return []
const extents = fixed ? null : root.tableExtents(grid, specs)
const shift = fixed ? (b => 0) : extents.shift
const space = Math.max(0, grid.width - grid.columnSpacing * (n - 1) - (fixed ? 0 : extents.outside))
const px = c => typeof specs[c] === "number"
const pc = c => typeof specs[c] === "string"
let widths
if (fixed) {
    // Specified widths stand; the rest is shared equally, there being no
    // content widths to weigh.
    widths = specs.map((s, c) => px(c) ? s : pc(c) ? parseFloat(s) * space / 100 : 0)
    const left = space - widths.reduce((a, b) => a + b, 0)
    const auto = specs.map((s, c) => !px(c) && !pc(c))
    for (const takes of [auto, specs.map((s, c) => px(c)), specs.map((s, c) => pc(c))]) {
        const sum = widths.reduce((a, w, c) => a + (takes[c] ? w : 0), 0)
        const count = takes.filter(t => t).length
        if (left <= 0 || count === 0)
            continue
        const equal = takes === auto || sum <= 0
        widths = widths.map((w, c) => takes[c] ? w + left * (equal ? 1 / count : w / sum) : w)
        break
    }
} else {
    const min = extents.min
    const share = c => Math.max(min[c], extents.percents[c] * space / 100)
    const guesses = [
        min.slice(),
        min.map((m, c) => pc(c) ? share(c) : m),
        min.map((m, c) => pc(c) ? share(c) : px(c) ? extents.max[c] : m),
        min.map((m, c) => pc(c) ? share(c) : extents.max[c])
    ]
    const sums = guesses.map(g => g.reduce((a, b) => a + b, 0))
    widths = guesses[0]
    for (let g = 1; g < guesses.length; ++g) {
        if (space >= sums[g]) {
            widths = guesses[g]
            continue
        }
        if (space > sums[g - 1]) {
            const t = (space - sums[g - 1]) / (sums[g] - sums[g - 1])
            widths = guesses[g].map((w, c) => guesses[g - 1][c] + (w - guesses[g - 1][c]) * t)
        }
        break
    }
    const excess = space - sums[guesses.length - 1]
    if (excess > 0) {
        widths = widths.slice()
        const auto = c => !px(c) && !pc(c)
        const takers = [
            c => auto(c) && extents.max[c] > 0 ? extents.max[c] : 0,
            c => px(c) ? Math.max(widths[c], 1e-9) : 0,
            c => pc(c) && extents.percents[c] > 0 ? extents.percents[c] : 0,
            c => 1
        ]
        for (const weigh of takers) {
            const weights = widths.map((w, c) => weigh(c))
            const total = weights.reduce((a, b) => a + b, 0)
            if (total <= 0)
                continue
            for (let c = 0; c < n; ++c)
                widths[c] += excess * weights[c] / total
            break
        }
    }
}
let edge = shift(0)
let from = 0
for (let c = 0; c < n; ++c) {
    edge += widths[c]
    let to = Math.round(edge - shift(c + 1))
    // A text wraps in a column a fraction narrower than it: one given its
    // max-content width keeps all of it, rounded up.
    if (!fixed && extents.wraps[c] && widths[c] >= extents.max[c] - 1e-6)
        to = Math.max(to, from + Math.ceil(extents.max[c] + shift(c) - shift(c + 1) - 1e-6))
    widths[c] = to - from
    from = to
}
return widths"#,
    ),
    (
        "tableRowHeights",
        &["grid", "minimums"],
        r#"const heights = minimums.slice()
const spanning = []
for (let i = 0; i < grid.children.length; ++i) {
    const cell = grid.children[i]
    const slot = cell.tableSlot
    if (!slot)
        continue
    if (slot[2] > 1)
        spanning.push(cell)
    else
        heights[slot[0]] = Math.max(heights[slot[0]], cell.implicitHeight)
}
for (const cell of spanning) {
    const first = cell.tableSlot[0]
    const span = cell.tableSlot[2]
    let have = grid.rowSpacing * (span - 1)
    for (let r = first; r < first + span; ++r)
        have += heights[r]
    if (cell.implicitHeight > have) {
        for (let r = first; r < first + span; ++r)
            heights[r] += (cell.implicitHeight - have) / span
    }
}
let edge = 0
for (let r = 0; r < heights.length; ++r) {
    const next = edge + heights[r]
    heights[r] = Math.round(next) - Math.round(edge)
    edge = next
}
return heights"#,
    ),
];

/// Source-over of `top` on `bottom`.
fn composite(top: Rgba, bottom: Rgba) -> Rgba {
    if top.a == 255 || bottom.a == 0 {
        return top;
    }
    if top.a == 0 {
        return bottom;
    }
    let (ta, ba) = (f32::from(top.a) / 255.0, f32::from(bottom.a) / 255.0);
    let a = ta + ba * (1.0 - ta);
    let channel = |t: u8, b: u8| {
        let v = (f32::from(t) * ta + f32::from(b) * ba * (1.0 - ta)) / a;
        v.round().clamp(0.0, 255.0) as u8
    };
    Rgba::new(
        channel(top.r, bottom.r),
        channel(top.g, bottom.g),
        channel(top.b, bottom.b),
        (a * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

struct RowRef<'t, 'l> {
    row: &'t LayoutBox<'l>,
    group: Option<&'t LayoutBox<'l>>,
}

/// One column as `<col>`/`<colgroup>` declare it.
#[derive(Clone)]
struct ColumnBox<'t, 'l> {
    col: Option<&'t LayoutBox<'l>>,
    group: Option<&'t LayoutBox<'l>>,
}

/// The parts of a table box, rows in display order: the first header group,
/// then bodies and any other header groups, then the footer groups (CSS 2.1
/// §17.2; only the first of each actually repeats on paged media).
struct Parts<'t, 'l> {
    captions: Vec<&'t LayoutBox<'l>>,
    columns: Vec<ColumnBox<'t, 'l>>,
    rows: Vec<RowRef<'t, 'l>>,
    /// The group index of each row: rowspans stop at a change.
    groups: Vec<usize>,
}

fn is_group(d: Display) -> bool {
    matches!(
        d,
        Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup
    )
}

fn boxes<'t, 'l>(b: &'t LayoutBox<'l>) -> &'t [LayoutBox<'l>] {
    match &b.content {
        Content::Blocks(children) => children,
        _ => &[],
    }
}

impl<'a> Emitter<'a, '_> {
    fn attr_int(&self, b: &LayoutBox<'_>, name: &str) -> Option<usize> {
        let node = b.node?;
        let value = self.doc.element(node)?.attr(name)?;
        value.trim().parse::<usize>().ok()
    }

    /// What a part displays as. One hidden from the start comes as a block
    /// (`boxes::hidden_box`); its element says what it is.
    fn part_display(&self, part: &LayoutBox<'_>) -> Display {
        let tag = part
            .node
            .and_then(|n| self.doc.element(n))
            .map(eldur_dom::Element::local_name);
        match (part.style.visibility, tag) {
            (eldur_css::Visibility::Collapse, Some("thead")) => Display::TableHeaderGroup,
            (eldur_css::Visibility::Collapse, Some("tbody")) => Display::TableRowGroup,
            (eldur_css::Visibility::Collapse, Some("tfoot")) => Display::TableFooterGroup,
            (eldur_css::Visibility::Collapse, Some("caption")) => Display::TableCaption,
            _ => part.style.display,
        }
    }

    fn table_parts<'t, 'l>(&self, table: &'t LayoutBox<'l>) -> Parts<'t, 'l> {
        let mut captions = Vec::new();
        let mut columns = Vec::new();
        let mut header = None;
        let mut body = Vec::new();
        let mut footer = Vec::new();
        let span = |b: &LayoutBox<'_>| self.attr_int(b, "span").filter(|&n| n > 0).unwrap_or(1);
        for child in boxes(table) {
            match self.part_display(child) {
                Display::TableCaption => captions.push(child),
                Display::TableColumn => {
                    columns.extend(std::iter::repeat_n(
                        ColumnBox {
                            col: Some(child),
                            group: None,
                        },
                        span(child),
                    ));
                }
                Display::TableColumnGroup => {
                    let cols = boxes(child);
                    if cols.is_empty() {
                        columns.extend(std::iter::repeat_n(
                            ColumnBox {
                                col: None,
                                group: Some(child),
                            },
                            span(child),
                        ));
                    }
                    for col in cols {
                        columns.extend(std::iter::repeat_n(
                            ColumnBox {
                                col: Some(col),
                                group: Some(child),
                            },
                            span(col),
                        ));
                    }
                }
                Display::TableHeaderGroup if header.is_none() => header = Some(child),
                Display::TableFooterGroup => footer.push(child),
                _ => body.push(child),
            }
        }
        let mut rows = Vec::new();
        let mut groups = Vec::new();
        let ordered = header.into_iter().chain(body).chain(footer);
        let mut group_index = 0;
        for child in ordered {
            if is_group(self.part_display(child)) {
                for row in boxes(child) {
                    rows.push(RowRef {
                        row,
                        group: Some(child),
                    });
                    groups.push(group_index);
                }
                group_index += 1;
            } else {
                // Rows directly in the table share an implicit group.
                rows.push(RowRef {
                    row: child,
                    group: None,
                });
                groups.push(group_index);
            }
        }
        Parts {
            captions,
            columns,
            rows,
            groups,
        }
    }

    /// A `display: table`/`inline-table` box and its captions.
    /// A table with no rows that a script is seen to fill. Cells are placed
    /// on one grid worked out from the document's rows, so one with none has
    /// nothing to work from: its sections stack, and rows lay out their cells.
    pub(super) fn table_to_fill(&self, b: &LayoutBox<'_>) -> bool {
        let gains = |n: &LayoutBox<'_>| n.node.is_some_and(|n| self.gains.contains(&n));
        let parts = boxes(b);
        (gains(b) || parts.iter().any(gains))
            && parts.iter().all(|p| {
                boxes(p).is_empty()
                    && !matches!(
                        p.style.display,
                        Display::TableRow
                            | Display::TableCaption
                            | Display::TableColumn
                            | Display::TableColumnGroup
                    )
            })
    }

    pub(super) fn column_to_fill(
        &mut self,
        b: &LayoutBox<'_>,
        inherited: TextDecorationLine,
        line: Option<u32>,
    ) -> Emitted {
        let mt = self.vertical_margin(&b.style.margin.top, line);
        let mb = self.vertical_margin(&b.style.margin.bottom, line);
        let mut column = Object::new("ColumnLayout");
        column.set("spacing", 0.0);
        // Shrink-to-fit, as a table with `width: auto` is (CSS 2.1 §17.5.2).
        column.set("Layout.maximumWidth", Value::expr("implicitWidth"));
        for part in boxes(b) {
            let obj = self.item(part, inherited).obj;
            column.push_child(obj);
        }
        Emitted {
            obj: column,
            top: MarginSet::of(mt),
            bottom: MarginSet::of(mb),
            collapse_through: false,
        }
    }

    pub(super) fn table(
        &mut self,
        b: &LayoutBox<'_>,
        inherited: TextDecorationLine,
        line: Option<u32>,
    ) -> Emitted {
        let style = &*b.style;
        let decoration = TextDecorationLine {
            underline: inherited.underline || style.text_decoration_line.underline,
            overline: inherited.overline || style.text_decoration_line.overline,
            line_through: inherited.line_through || style.text_decoration_line.line_through,
        };
        let parts = self.table_parts(b);
        let spans: Vec<RowSpans> = parts
            .rows
            .iter()
            .zip(&parts.groups)
            .map(|(row, &group)| RowSpans {
                group,
                cells: boxes(row.row)
                    .iter()
                    .map(|cell| {
                        (
                            self.attr_int(cell, "colspan").unwrap_or(1),
                            self.attr_int(cell, "rowspan").unwrap_or(1),
                        )
                    })
                    .collect(),
            })
            .collect();
        let (slots, grid_columns) = place_cells(&spans);
        let columns = grid_columns.max(parts.columns.len());
        let collapse = style.border_collapse == BorderCollapse::Collapse;
        let fixed =
            style.table_layout == TableLayout::Fixed && matches!(&style.width, Size::Length(_));

        let edges = collapse.then(|| self.collapsed_edges(b, &parts, &slots, columns));
        let made = b.node.is_some_and(|n| self.made_tables.contains(&n));
        let specs = {
            // Fixed layout reads the first row alone (CSS 2.1 §17.5.2.1). A made
            // table's probe rows stand for any row: its cells' widths are counted
            // by the runtime from the cells a script makes.
            let read_rows = match (made, fixed) {
                (true, _) => 0,
                (false, true) => 1,
                (false, false) => usize::MAX,
            };
            self.column_specs(&parts, &slots, columns, read_rows, edges.as_ref(), line)
        };

        let live = self.is_live_table(b);
        let grid_id = self.ids.claim(&format!("{} grid", hint(self.doc, b)));
        let mut grid =
            Object::new(if live { "ColumnLayout" } else { "GridLayout" }).with_id(&grid_id);
        let (spacing_h, spacing_v) = if collapse {
            (0.0, 0.0)
        } else {
            style.border_spacing
        };
        if live {
            // The runtime sets each section's distance from the one before:
            // an empty section has none, which a layout's `spacing` cannot do.
            grid.set("spacing", 0.0);
            for (name, value) in [("columnSpacing", spacing_h), ("rowSpacing", spacing_v)] {
                grid.members.push(Member::PropertyDecl {
                    ty: "real".to_owned(),
                    name: name.to_owned(),
                    value: Some(number(value)),
                    readonly: true,
                    required: false,
                });
            }
        } else {
            grid.set("columns", Value::Number(columns.max(1) as f64));
            grid.set("columnSpacing", number(spacing_h));
            grid.set("rowSpacing", number(spacing_v));
        }
        let specs_expr = format!(
            "[{}]",
            specs.iter().map(|s| s.qml()).collect::<Vec<_>>().join(", ")
        );
        let declare = |grid: &mut Object, ty: &str, name: &str, value: String| {
            grid.members.push(Member::PropertyDecl {
                ty: ty.to_owned(),
                name: name.to_owned(),
                value: Some(Value::expr(value)),
                readonly: true,
                required: false,
            });
        };
        if live {
            // A script's rows can add columns.
            grid.members.push(Member::PropertyDecl {
                ty: "var".to_owned(),
                name: "columnSpecs".to_owned(),
                value: Some(Value::expr(specs_expr)),
                readonly: false,
                required: false,
            });
        } else {
            declare(&mut grid, "var", "columnSpecs", specs_expr);
        }
        if self.runtime_dom {
            grid.members.push(Member::PropertyDecl {
                ty: "var".to_owned(),
                name: "eldurCreatedCells".to_owned(),
                value: Some(Value::expr("[]")),
                readonly: false,
                required: false,
            });
        }
        if live {
            // The runtime writes these: see `_tableLayout`.
            let writable = |ty: &str, name: &str, value: &str| Member::PropertyDecl {
                ty: ty.to_owned(),
                name: name.to_owned(),
                value: Some(Value::expr(value)),
                readonly: false,
                required: false,
            };
            grid.members.push(writable("var", "columnWidths", "[]"));
            grid.members.push(writable("bool", "eldurEmpty", "false"));
            grid.members.push(writable("real", "maxContentWidth", "0"));
            grid.members.push(writable("real", "minContentWidth", "0"));
            grid.members.push(writable(
                "bool",
                "eldurFixed",
                if fixed { "true" } else { "false" },
            ));
        } else {
            declare(
                &mut grid,
                "var",
                "columnWidths",
                format!("{ROOT_ID}.tableColumnWidths(this, columnSpecs, {fixed})"),
            );
        }
        if let Some(edges) = edges.as_ref() {
            let offsets = edges.column_offsets();
            if offsets.iter().any(|&o| o != 0.0) {
                let list: Vec<String> = offsets.iter().map(|&o| fmt_number(f64::from(o))).collect();
                declare(
                    &mut grid,
                    "var",
                    "tableEdges",
                    format!("[{}]", list.join(", ")),
                );
            }
        }
        if !live {
            let row_minimums = parts
                .rows
                .iter()
                .map(|r| fmt_number(round(row_minimum(r.row))))
                .collect::<Vec<_>>()
                .join(", ");
            declare(
                &mut grid,
                "var",
                "rowHeights",
                format!("{ROOT_ID}.tableRowHeights(this, [{row_minimums}])"),
            );
        }
        if !live {
            declare(
                &mut grid,
                "real",
                "maxContentWidth",
                format!("{ROOT_ID}.tableMaxContentWidth(this, columnSpecs, {fixed})"),
            );
            declare(
                &mut grid,
                "real",
                "minContentWidth",
                format!("{ROOT_ID}.tableMinContentWidth(this, columnSpecs, {fixed})"),
            );
        }

        let mut fonts: Vec<(String, String)> = Vec::new();
        let mut objects: Vec<Vec<Object>> = Vec::new();
        let mut outer_edge = 0.0_f32;
        for (row, row_slots) in parts.rows.iter().zip(&slots) {
            self.table_row_diagnostics(row, live, line);
            let moves = self.row_translations(row, line);
            let mut row_objects = Vec::new();
            for (cell, &slot) in boxes(row.row).iter().zip(row_slots) {
                let mut cell_style = cell.style.clone().into_owned();
                if !moves.is_empty() {
                    // CSS order: the outer box's transform applies last.
                    let own = std::mem::take(&mut cell_style.transform);
                    cell_style.transform = moves.iter().cloned().chain(own).collect();
                }
                cell_style.background_color = self.cell_background(cell, row, &parts, slot);
                // A cell a script makes paints all four of its own borders
                // wherever it lands, and the runtime shares them with its
                // neighbours; the sides it won at its probe's place are no
                // answer for another.
                let made_cell = cell.node.is_some_and(|n| self.made_tables.contains(&n));
                if let Some(edges) = edges.as_ref().filter(|_| !made_cell) {
                    let [top, right, bottom, left] = edges.cell_borders(slot);
                    if slot.column + slot.column_span == columns {
                        outer_edge = outer_edge.max(right.width);
                    }
                    cell_style.border.top = top;
                    cell_style.border.right = right;
                    cell_style.border.bottom = bottom;
                    cell_style.border.left = left;
                }
                let mut painted = cell.clone();
                painted.style = Cow::Owned(cell_style);
                let emitted = self.item(&painted, decoration);
                let mut obj = emitted.obj;
                self.place_cell(
                    &mut obj, &painted, slot, &grid_id, &mut grid, &mut fonts, live,
                );
                if live {
                    row_objects.push(obj);
                } else {
                    grid.push_child(obj);
                }
            }
            objects.push(row_objects);
        }

        if live {
            // Last-column cells draw the outer border whole; a browser's hold half
            // of it and the table the rest, so it is no content to share width by.
            grid.members.push(Member::PropertyDecl {
                ty: "real".to_owned(),
                name: "eldurOuterEdge".to_owned(),
                value: Some(Value::expr(fmt_number(round(outer_edge)))),
                readonly: true,
                required: false,
            });
            self.live_parts(b, &parts, objects, &mut grid, line);
        } else {
            // A script's empty row group to fill: it has no cells to place, but
            // needs an object the rows it creates can go in.
            for child in boxes(b) {
                if is_group(child.style.display)
                    && boxes(child).is_empty()
                    && child.node.is_some_and(|n| self.wanted.contains(&n))
                {
                    let mut obj = self.item(child, decoration).obj;
                    obj.set("Layout.columnSpan", Value::Number(columns.max(1) as f64));
                    obj.set("Layout.fillWidth", true);
                    grid.push_child(obj);
                }
            }
        }

        // The table's own box. In the collapsing model its border and padding
        // are gone: the border went to the edges, padding does not apply.
        let mut own = style.clone();
        if collapse {
            own.border = Default::default();
            own.padding = Default::default();
        }
        let paint_background = b.node.is_none() || b.node != self.canvas_node;
        let base = self.decoration(&own, paint_background, line);
        // Separate borders: spacing also runs along the table's outer edges.
        let deco = Decoration {
            insets: Insets {
                top: base.insets.top + spacing_v,
                right: base.insets.right + spacing_h,
                bottom: base.insets.bottom + spacing_v,
                left: base.insets.left + spacing_h,
            },
            paints: base.paints,
            clips: base.clips,
            scroll: base.scroll,
            gradients: base.gradients,
            shadows: base.shadows,
            inset_shadows: base.inset_shadows,
            hint: hint(self.doc, b),
            strips: None,
            plain_edges: false,
        };
        let insets = deco.insets;
        let mut body = self.wrap(
            &own,
            &deco,
            paint_background,
            Some((grid, grid_id.clone())),
            false,
            line,
        );
        // Not the grid's own `implicitWidth`: it follows the preferred widths
        // it is given, which follow this, and that re-polishes without end.
        let plus_insets = |name: &str| {
            let h = round(insets.horizontal());
            if h == 0.0 {
                format!("{grid_id}.{name}")
            } else {
                format!("{grid_id}.{name} + {}", fmt_number(h))
            }
        };
        body.set("implicitWidth", Value::expr(plus_insets("maxContentWidth")));
        let auto_width = !matches!(style.width, Size::Length(_));

        let mut obj = if parts.captions.is_empty() {
            body
        } else {
            let mut stack = Object::new("ColumnLayout");
            stack.set("spacing", Value::Number(0.0));
            let (top, bottom): (Vec<_>, Vec<_>) = parts
                .captions
                .iter()
                .partition(|c| c.style.caption_side == eldur_css::CaptionSide::Top);
            for caption in top {
                let item = self.table_caption(caption, decoration);
                stack.push_child(item);
            }
            body.set("Layout.fillWidth", true);
            body.set(
                "Layout.minimumWidth",
                Value::expr(plus_insets("minContentWidth")),
            );
            stack.push_child(body);
            for caption in bottom {
                let item = self.table_caption(caption, decoration);
                stack.push_child(item);
            }
            // A fractional caption would move the table by its rounded height.
            self.carry(&mut stack);
            stack
        };
        // A table is never narrower than its content; with `width: auto` it is
        // also no wider than it needs (shrink-to-fit, CSS 2.1 §17.5.2).
        obj.set(
            "Layout.minimumWidth",
            Value::expr(plus_insets("minContentWidth")),
        );
        if !auto_width && obj.type_name == "ColumnLayout" {
            // Layouts fill by default, past their preferred width.
            obj.set("Layout.fillWidth", false);
        }
        if auto_width && matches!(style.max_width, Size::None) {
            obj.set(
                "Layout.maximumWidth",
                Value::expr(plus_insets("maxContentWidth")),
            );
        }
        if !matches!(style.height, Size::Auto) {
            self.diags.info(
                "css",
                line,
                "`height` on a table does not stretch its rows".to_owned(),
            );
        }
        if live && parts.captions.is_empty() {
            // A table with no rows has no box, border-spacing and all.
            for name in ["Layout.minimumWidth", "Layout.maximumWidth"] {
                if let Some(Value::Expr(width)) = obj.get(name).cloned() {
                    obj.set(
                        name,
                        Value::expr(format!("{grid_id}.eldurEmpty ? 0 : {width}")),
                    );
                }
            }
            obj.set(
                "Layout.maximumHeight",
                Value::expr(format!(
                    "{grid_id}.eldurEmpty ? 0 : Number.POSITIVE_INFINITY"
                )),
            );
        }
        self.name_table(&mut obj, b);
        self.visibility(&mut obj, style);
        let mt = self.vertical_margin(&style.margin.top, line);
        let mb = self.vertical_margin(&style.margin.bottom, line);
        Emitted {
            obj,
            top: MarginSet::of(mt),
            bottom: MarginSet::of(mb),
            collapse_through: false,
        }
    }

    /// Whether this table's rows and sections are objects of their own. A
    /// table made of `display` values has no such elements to hand a script.
    pub(super) fn is_live_table(&self, b: &LayoutBox<'_>) -> bool {
        let tag = |p: &LayoutBox<'_>| {
            p.node
                .and_then(|n| self.doc.element(n))
                .map(eldur_dom::Element::local_name)
        };
        let row_fits = |row: &LayoutBox<'_>| {
            tag(row) == Some("tr")
                && boxes(row)
                    .iter()
                    .all(|c| matches!(tag(c), Some("td" | "th")))
        };
        // A row that can leave the layout needs an item of its own to say so.
        let row_hides = |row: &LayoutBox<'_>| {
            self.hides_items || row.node.is_some_and(|n| self.collapsed.contains(&n))
        };
        let hides = boxes(b).iter().any(|part| match tag(part) {
            Some("thead" | "tbody" | "tfoot") => {
                row_hides(part) || boxes(part).iter().any(row_hides)
            }
            _ => row_hides(part),
        });
        b.node.is_some_and(|n| {
            hides || self.live_tables.contains(&n) || self.made_tables.contains(&n)
        }) && tag(b) == Some("table")
            && boxes(b).iter().all(|part| match tag(part) {
                Some("thead" | "tbody" | "tfoot") => boxes(part).iter().all(row_fits),
                Some("caption" | "colgroup" | "col") => true,
                _ => row_fits(part),
            })
    }

    /// A live table's parts in the order they show. Cells are placed by the
    /// runtime's `_tableLayout`: rows hold them, the columns come from the one
    /// solver static and created rows share.
    fn live_parts(
        &mut self,
        table: &LayoutBox<'_>,
        parts: &Parts<'_, '_>,
        objects: Vec<Vec<Object>>,
        grid: &mut Object,
        line: Option<u32>,
    ) {
        for part in boxes(table) {
            if matches!(
                part.style.display,
                Display::TableColumn | Display::TableColumnGroup
            ) {
                let obj = self.live_column(part, line);
                grid.push_child(obj);
            }
        }
        let mut rows = objects.into_iter();
        let mut r = 0;
        while r < parts.rows.len() {
            let group = parts.rows[r].group;
            let index = parts.groups[r];
            let mut members = Vec::new();
            while r < parts.rows.len() && parts.groups[r] == index {
                let cells = rows.next().unwrap_or_default();
                members.push(self.live_row(parts.rows[r].row, cells, line));
                r += 1;
            }
            match group {
                Some(section) => {
                    let obj = self.live_section(section, members, line);
                    grid.push_child(obj);
                }
                None => {
                    for row in members {
                        grid.push_child(row);
                    }
                }
            }
        }
        // Sections with no rows yet show nothing but are still elements.
        for part in boxes(table) {
            if is_section(self.doc, part) && boxes(part).is_empty() {
                let obj = self.live_section(part, Vec::new(), line);
                grid.push_child(obj);
            }
        }
    }

    fn live_part(&mut self, mut obj: Object, part: &LayoutBox<'_>, line: Option<u32>) -> Object {
        self.name_table(&mut obj, part);
        if part.style.visibility == eldur_css::Visibility::Collapse {
            obj.set("visible", false);
        }
        let emitted = Emitted {
            obj,
            top: MarginSet::of(0.0),
            bottom: MarginSet::of(0.0),
            collapse_through: false,
        };
        self.transformed(emitted, part, line).obj
    }

    fn live_section(
        &mut self,
        section: &LayoutBox<'_>,
        rows: Vec<Object>,
        line: Option<u32>,
    ) -> Object {
        let mut obj = Object::new("ColumnLayout");
        obj.set("Layout.fillWidth", true);
        obj.set("spacing", 0.0);
        margin_properties(&mut obj);
        for row in rows {
            obj.push_child(row);
        }
        self.live_part(obj, section, line)
    }

    fn live_row(&mut self, row: &LayoutBox<'_>, cells: Vec<Object>, line: Option<u32>) -> Object {
        let mut obj = Object::new("Item");
        obj.set("Layout.fillWidth", true);
        obj.set("Layout.preferredHeight", Value::expr("implicitHeight"));
        margin_properties(&mut obj);
        let minimum = row_minimum(row);
        if minimum > 0.0 {
            obj.members.push(Member::PropertyDecl {
                ty: "real".to_owned(),
                name: "eldurMinHeight".to_owned(),
                value: Some(number(minimum)),
                readonly: true,
                required: false,
            });
        }
        for cell in cells {
            obj.push_child(cell);
        }
        self.live_part(obj, row, line)
    }

    /// `<col>` and `<colgroup>` show only through cells, but a script can
    /// find them and the runtime reads their backgrounds.
    fn live_column(&mut self, part: &LayoutBox<'_>, line: Option<u32>) -> Object {
        let mut obj = Object::new("Item");
        obj.set("visible", false);
        for col in boxes(part) {
            let child = self.live_column(col, line);
            obj.push_child(child);
        }
        self.live_part(obj, part, line)
    }

    fn table_caption(&mut self, caption: &LayoutBox<'_>, decoration: TextDecorationLine) -> Object {
        let emitted = self.item(caption, decoration);
        let mut obj = emitted.obj;
        // The caption wraps to the table's width; it does not set it.
        obj.set("Layout.fillWidth", true);
        obj.set("Layout.preferredWidth", Value::Number(0.0));
        if emitted.top.value() > 0.0 {
            obj.set("Layout.topMargin", number(emitted.top.value()));
        }
        if emitted.bottom.value() > 0.0 {
            obj.set("Layout.bottomMargin", number(emitted.bottom.value()));
        }
        obj
    }

    fn name_table(&mut self, obj: &mut Object, b: &LayoutBox<'_>) {
        let wanted = b.node.is_some_and(|n| self.wanted.contains(&n));
        let html_id = b
            .node
            .and_then(|n| self.doc.element(n))
            .and_then(|e| e.id())
            .map(str::to_owned);
        let id = match (&html_id, wanted) {
            (Some(id), _) => self.claim_html_id(id),
            (None, true) => self.ids.claim(&hint(self.doc, b)),
            (None, false) => return,
        };
        obj.id = Some(id.clone());
        if let (Some(node), true) = (b.node, wanted) {
            if let Some(html_id) = html_id {
                obj.set("objectName", Value::String(html_id));
            }
            self.targets.insert(
                node,
                Target {
                    qml_id: id,
                    kind: TargetKind::Box,
                },
            );
        }
    }

    /// The row's and its group's `transform`s, outermost first, when they
    /// are px translations: the rows have no item of their own, so their
    /// cells take them. Anything else turns about the row's own box, which
    /// a cell cannot do for it.
    fn row_translations(
        &mut self,
        row: &RowRef<'_, '_>,
        table_line: Option<u32>,
    ) -> Vec<TransformFn> {
        let mut out = Vec::new();
        for part in [row.group, Some(row.row)].into_iter().flatten() {
            let px = |l: &LengthPercentage| Length::Linear(*l).as_px().is_some();
            let moves = part.style.transform.iter().all(|f| match f {
                TransformFn::Translate(x, y) => px(x) && px(y),
                _ => false,
            });
            if moves {
                out.extend(part.style.transform.iter().cloned());
            } else {
                let line = part
                    .node
                    .and_then(|n| node_line(self.doc, n))
                    .or(table_line);
                let what = part.node.map_or_else(
                    || "a table row".to_owned(),
                    |n| crate::boxes::describe(self.doc, n),
                );
                self.diags.warning(
                    "css",
                    line,
                    format!(
                        "`transform` on {what} is left out: only px translations move its cells"
                    ),
                );
            }
        }
        out
    }

    fn table_row_diagnostics(&mut self, row: &RowRef<'_, '_>, live: bool, table_line: Option<u32>) {
        for part in [Some(row.row), row.group].into_iter().flatten() {
            let line = part
                .node
                .and_then(|n| node_line(self.doc, n))
                .or(table_line);
            let what = part.node.map_or_else(
                || "a table row".to_owned(),
                |n| crate::boxes::describe(self.doc, n),
            );
            if !live && part.node.is_some_and(|n| self.wanted.contains(&n)) {
                self.diags.warning(
                    "js",
                    line,
                    format!("{what} has no QML object of its own, so scripts cannot reach it"),
                );
            }
            if part.style.opacity < 1.0 {
                self.diags
                    .info("css", line, format!("`opacity` on {what} is not converted"));
            }
            if part
                .style
                .background_layers
                .iter()
                .any(|l| l.image.is_some())
            {
                self.diags.warning(
                    "css",
                    line,
                    format!("background images on {what} are not converted"),
                );
            }
            if part.style.visibility == eldur_css::Visibility::Collapse {
                self.diags.info(
                    "css",
                    line,
                    format!("`visibility: collapse` on {what} is not converted"),
                );
            }
        }
    }

    /// CSS 2.1 §17.5.1: column group, column, row group, row and cell
    /// backgrounds paint in layers. Layout items cannot overlap in a
    /// `GridLayout`, so the cell paints all of them, flattened.
    fn cell_background(
        &self,
        cell: &LayoutBox<'_>,
        row: &RowRef<'_, '_>,
        parts: &Parts<'_, '_>,
        slot: Slot,
    ) -> Rgba {
        let column = parts.columns.get(slot.column);
        let layers = [
            column.and_then(|c| c.group),
            column.and_then(|c| c.col),
            row.group,
            Some(row.row),
            Some(cell),
        ];
        layers
            .iter()
            .flatten()
            .fold(Rgba::TRANSPARENT, |under, layer| {
                composite(layer.style.background_color, under)
            })
    }

    fn collapsed_edges(
        &self,
        table: &LayoutBox<'_>,
        parts: &Parts<'_, '_>,
        slots: &[Vec<Slot>],
        columns: usize,
    ) -> Edges {
        let rows = parts.rows.len();
        let mut edges = Edges::new(rows, columns);
        if rows == 0 || columns == 0 {
            return edges;
        }
        let whole = Slot {
            row: 0,
            column: 0,
            row_span: rows,
            column_span: columns,
        };
        edges.offer(whole, &table.style, BorderOwner::Table);
        for (c, column) in parts.columns.iter().enumerate() {
            let slot = Slot {
                column: c,
                column_span: 1,
                ..whole
            };
            if let Some(group) = column.group {
                // A column group's sides are its outer columns' only.
                let first = c == 0
                    || parts.columns[c - 1]
                        .group
                        .is_none_or(|g| !std::ptr::eq(g, group));
                let last = parts
                    .columns
                    .get(c + 1)
                    .is_none_or(|n| n.group.is_none_or(|g| !std::ptr::eq(g, group)));
                let mut style = group.style.clone().into_owned();
                if !first {
                    style.border.left = no_border();
                }
                if !last {
                    style.border.right = no_border();
                }
                edges.offer(slot, &style, BorderOwner::ColumnGroup);
            }
            if let Some(col) = column.col {
                edges.offer(slot, &col.style, BorderOwner::Column);
            }
        }
        let mut r = 0;
        while r < rows {
            let group = parts.rows[r].group;
            let index = parts.groups[r];
            let end = (r..rows)
                .find(|&o| parts.groups[o] != index)
                .unwrap_or(rows);
            if let Some(group) = group {
                edges.offer(
                    Slot {
                        row: r,
                        row_span: end - r,
                        ..whole
                    },
                    &group.style,
                    BorderOwner::RowGroup,
                );
            }
            r = end;
        }
        for (r, row) in parts.rows.iter().enumerate() {
            edges.offer(
                Slot {
                    row: r,
                    row_span: 1,
                    ..whole
                },
                &row.row.style,
                BorderOwner::Row,
            );
            for (cell, &slot) in boxes(row.row).iter().zip(&slots[r]) {
                edges.offer(slot, &cell.style, BorderOwner::Cell);
            }
        }
        edges
    }

    fn column_specs(
        &mut self,
        parts: &Parts<'_, '_>,
        slots: &[Vec<Slot>],
        columns: usize,
        read_rows: usize,
        edges: Option<&Edges>,
        line: Option<u32>,
    ) -> Vec<ColumnSpec> {
        let mut specs = vec![ColumnSpec::Auto; columns];
        for (c, spec) in specs.iter_mut().enumerate() {
            let Some(column) = parts.columns.get(c) else {
                continue;
            };
            for part in [column.group, column.col].into_iter().flatten() {
                if let Size::Length(w) = &part.style.width {
                    *spec = ColumnSpec::of(w, 0.0);
                } else if let Some(px) = part
                    .node
                    .and_then(|n| self.doc.element(n))
                    .and_then(|e| e.attr("width"))
                    .and_then(|w| w.trim().trim_end_matches("px").parse::<f32>().ok())
                {
                    *spec = ColumnSpec::Px(px);
                }
            }
        }
        // A `<col>` width is the column's; cells only speak for columns that
        // have none, in the first `read_rows` rows.
        let declared: Vec<bool> = specs.iter().map(|s| *s != ColumnSpec::Auto).collect();
        for (r, row) in parts.rows.iter().enumerate().take(read_rows) {
            for (cell, &slot) in boxes(row.row).iter().zip(&slots[r]) {
                let Size::Length(w) = &cell.style.width else {
                    continue;
                };
                if declared.get(slot.column).copied().unwrap_or(true) {
                    continue;
                }
                if slot.column_span > 1 {
                    self.diags.info(
                        "css",
                        line,
                        "`width` on a spanning table cell is ignored".to_owned(),
                    );
                    continue;
                }
                let insets = self.insets(&cell.style, line);
                // Collapsed, a cell's border box holds half of each edge it
                // lies on, whoever's border won it (CSS 2.1 §17.6.2).
                let borders = match edges {
                    Some(edges) => {
                        let [_, right, _, left] = edges.whole_sides(slot);
                        (border_width(&left) + border_width(&right)) / 2.0
                    }
                    None => {
                        border_width(&cell.style.border.left)
                            + border_width(&cell.style.border.right)
                    }
                };
                let padding = insets.horizontal()
                    - border_width(&cell.style.border.left)
                    - border_width(&cell.style.border.right);
                let extra = match cell.style.box_sizing {
                    eldur_css::BoxSizing::ContentBox => padding + borders,
                    eldur_css::BoxSizing::BorderBox => 0.0,
                };
                specs[slot.column] = specs[slot.column].merge(ColumnSpec::of(w, extra));
            }
        }
        specs
    }

    /// `Layout.*` of a cell and what the solver reads from it.
    #[allow(clippy::too_many_arguments)]
    fn place_cell(
        &mut self,
        obj: &mut Object,
        cell: &LayoutBox<'_>,
        slot: Slot,
        grid_id: &str,
        grid: &mut Object,
        fonts: &mut Vec<(String, String)>,
        live: bool,
    ) {
        let Slot {
            row,
            column,
            row_span,
            column_span,
        } = slot;
        if !live {
            obj.set("Layout.row", Value::Number(row as f64));
            obj.set("Layout.column", Value::Number(column as f64));
            if row_span > 1 {
                obj.set("Layout.rowSpan", Value::Number(row_span as f64));
            }
            if column_span > 1 {
                obj.set("Layout.columnSpan", Value::Number(column_span as f64));
            }
        }
        let sum = |list: &str, spacing: &str, from: usize, span: usize| {
            let mut terms: Vec<String> = (from..from + span)
                .map(|i| format!("{grid_id}.{list}[{i}]"))
                .collect();
            if span > 1 {
                terms.push(format!("{} * {grid_id}.{spacing}", span - 1));
            }
            terms.join(" + ")
        };
        if !live {
            obj.set(
                "Layout.preferredWidth",
                Value::expr(sum("columnWidths", "columnSpacing", column, column_span)),
            );
            obj.set(
                "Layout.preferredHeight",
                Value::expr(sum("rowHeights", "rowSpacing", row, row_span)),
            );
        }
        if let Size::Length(h) = &cell.style.height
            && let Some(px) = h.as_px()
        {
            let line = cell.node.and_then(|n| node_line(self.doc, n));
            let insets = self.insets(&cell.style, line);
            let extra = match cell.style.box_sizing {
                eldur_css::BoxSizing::ContentBox => insets.vertical(),
                eldur_css::BoxSizing::BorderBox => 0.0,
            };
            if let Some(Value::Expr(implicit)) = obj.get("implicitHeight").cloned() {
                obj.set(
                    "implicitHeight",
                    Value::expr(format!(
                        "Math.max({implicit}, {})",
                        fmt_number(round(px + extra))
                    )),
                );
            } else {
                obj.set("implicitHeight", number(px + extra));
            }
        }
        let min_content = self.cell_min_content(cell, grid, fonts);
        if !live {
            obj.members.push(Member::PropertyDecl {
                ty: "var".to_owned(),
                name: "tableSlot".to_owned(),
                value: Some(Value::expr(format!(
                    "[{row}, {column}, {row_span}, {column_span}]"
                ))),
                readonly: true,
                required: false,
            });
        }
        obj.members.push(Member::PropertyDecl {
            ty: "real".to_owned(),
            name: "minContentWidth".to_owned(),
            value: Some(Value::expr(min_content)),
            readonly: true,
            required: false,
        });
    }

    /// The width a cell cannot go below: its widest word, measured when the
    /// page runs since only Qt knows the font. Content that does not wrap has
    /// its full width as minimum.
    fn cell_min_content(
        &mut self,
        cell: &LayoutBox<'_>,
        grid: &mut Object,
        fonts: &mut Vec<(String, String)>,
    ) -> String {
        let line = cell.node.and_then(|n| node_line(self.doc, n));
        match self.longest_word(cell, line) {
            Some(MinContent::Nothing(insets)) => fmt_number(round(insets)),
            Some(MinContent::Word {
                words,
                style,
                insets,
            }) => {
                let metrics = self.font_metrics(&style, grid, fonts);
                let widths: Vec<String> = words
                    .iter()
                    .map(|w| format!("{metrics}.advanceWidth({})", eldur_qml::quote_string(w)))
                    .collect();
                let widest = if widths.len() == 1 {
                    widths[0].clone()
                } else {
                    format!("Math.max({})", widths.join(", "))
                };
                let insets = round(insets);
                if insets == 0.0 {
                    widest
                } else {
                    format!("{widest} + {}", fmt_number(insets))
                }
            }
            None => "implicitWidth".to_owned(),
        }
    }

    /// `None` when the cell holds something only its own layout can size.
    fn longest_word(&mut self, b: &LayoutBox<'_>, line: Option<u32>) -> Option<MinContent> {
        use crate::boxes::BoxKind;
        let insets = self.insets(&b.style, line).horizontal();
        match &b.content {
            _ if !matches!(b.kind, BoxKind::Block | BoxKind::Table) => None,
            _ if matches!(
                b.style.display,
                Display::Table | Display::InlineTable | Display::InlineBlock
            ) =>
            {
                None
            }
            Content::Empty => Some(MinContent::Nothing(insets)),
            Content::Inline(items) => {
                if matches!(
                    b.style.white_space,
                    eldur_css::WhiteSpace::NoWrap | eldur_css::WhiteSpace::Pre
                ) {
                    return None;
                }
                let mut plain = String::new();
                plain_text(items, &mut plain);
                Some(MinContent::Word {
                    words: widest_words(&plain),
                    style: Box::new(b.style.clone().into_owned()),
                    insets,
                })
            }
            Content::Blocks(children) => {
                let mut best: Option<MinContent> = Some(MinContent::Nothing(0.0));
                for child in children {
                    let found = self.longest_word(child, line)?;
                    if found.estimate() > best.as_ref().map_or(0.0, MinContent::estimate) {
                        best = Some(found);
                    }
                }
                best.map(|m| m.plus(insets))
            }
        }
    }

    fn font_metrics(
        &mut self,
        style: &ComputedStyle,
        grid: &mut Object,
        fonts: &mut Vec<(String, String)>,
    ) -> String {
        let family = font_family(&style.font_family);
        let size = f64::from(style.font_size).round().max(1.0);
        let weight = f64::from(style.font_weight);
        let italic = style.font_style != eldur_css::FontStyle::Normal;
        let key = format!("{family}/{size}/{weight}/{italic}");
        if let Some((_, id)) = fonts.iter().find(|(k, _)| *k == key) {
            return id.clone();
        }
        let id = self.ids.claim("table font");
        let mut metrics = Object::new("FontMetrics").with_id(&id);
        metrics.set("font.family", super::fonts::family_value(style));
        metrics.set("font.pixelSize", Value::Number(size));
        if style.font_weight != 400 {
            metrics.set("font.weight", Value::Number(weight));
        }
        if italic {
            metrics.set("font.italic", true);
        }
        grid.push_child(metrics);
        fonts.push((key, id.clone()));
        id
    }

    /// The solver, on the root, when the page has tables.
    pub(super) fn table_solver(root: &mut Object) {
        for (name, params, body) in SOLVER {
            root.members.push(Member::Function {
                name: (*name).to_owned(),
                params: params.iter().map(|p| (*p).to_owned()).collect(),
                body: body.lines().map(str::to_owned).collect(),
            });
        }
    }
}

fn row_minimum(row: &LayoutBox<'_>) -> f32 {
    match &row.style.height {
        Size::Length(h) => h.as_px().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// The margins `_tableLayout` gives a live row or section, as a created one has.
fn margin_properties(obj: &mut Object) {
    for (name, margin) in [
        ("eldurSide", "Layout.leftMargin"),
        ("eldurTop", "Layout.topMargin"),
        ("eldurBottom", "Layout.bottomMargin"),
    ] {
        obj.members.push(Member::PropertyDecl {
            ty: "real".to_owned(),
            name: name.to_owned(),
            value: Some(Value::Number(0.0)),
            readonly: false,
            required: false,
        });
        obj.set(margin, Value::expr(name));
    }
    obj.set("Layout.rightMargin", Value::expr("eldurSide"));
}

fn is_section(doc: &eldur_dom::Document, part: &LayoutBox<'_>) -> bool {
    part.node
        .and_then(|n| doc.element(n))
        .is_some_and(|e| matches!(e.local_name(), "thead" | "tbody" | "tfoot"))
}

enum MinContent {
    /// No text; only the insets take room.
    Nothing(f32),
    Word {
        /// The words that may be widest: which one is, only Qt can say.
        words: Vec<String>,
        style: Box<ComputedStyle>,
        insets: f32,
    },
}

impl MinContent {
    /// Which word is longest, before Qt can measure one.
    fn estimate(&self) -> f32 {
        match self {
            Self::Nothing(insets) => *insets,
            Self::Word {
                words,
                style,
                insets,
            } => words.first().map_or(0.0, |w| em_estimate(w)) * style.font_size + insets,
        }
    }

    fn plus(self, extra: f32) -> Self {
        match self {
            Self::Nothing(insets) => Self::Nothing(insets + extra),
            Self::Word {
                words,
                style,
                insets,
            } => Self::Word {
                words,
                style,
                insets: insets + extra,
            },
        }
    }
}

/// A word's width in ems, roughly: enough to tell which words may be the
/// widest (a capital or an `m` is twice an `i`).
fn em_estimate(word: &str) -> f32 {
    word.chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | 'I' | '.' | ',' | ':' | ';' | '\'' | '!' | '|' => 0.28,
            'f' | 't' | 'r' | '(' | ')' | '[' | ']' | ' ' => 0.38,
            'm' | 'w' | 'M' | 'W' => 0.85,
            c if c.is_uppercase() => 0.68,
            c if c.is_ascii_digit() => 0.56,
            _ => 0.55,
        })
        .sum()
}

/// The text's words that may be its widest, widest first by estimate: those
/// within a fifth of the widest estimate, at most four. Lines break at spaces
/// and after hyphens.
fn widest_words(text: &str) -> Vec<String> {
    let mut words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .collect();
    words.sort_by(|a, b| em_estimate(b).total_cmp(&em_estimate(a)).then(a.cmp(b)));
    words.dedup();
    let widest = words.first().map_or(0.0, |w| em_estimate(w));
    let mut out: Vec<String> = words
        .into_iter()
        .take_while(|w| em_estimate(w) >= widest * 0.8)
        .take(4)
        .map(str::to_owned)
        .collect();
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn plain_text(items: &[Inline<'_>], out: &mut String) {
    for item in items {
        match item {
            Inline::Text(t) => out.push_str(t),
            Inline::LineBreak => out.push(' '),
            Inline::Span(span) => plain_text(&span.children, out),
            Inline::Atomic(_) => out.push(' '),
        }
    }
}
