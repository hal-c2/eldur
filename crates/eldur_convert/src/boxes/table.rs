//! Anonymous table objects (CSS 2.1 §17.2.1): what sits directly inside a
//! table, row group or row but is not the right kind of box gets wrapped,
//! and table-internal boxes outside a table get a table around them, so
//! the emitter only ever sees table > row group or row > cell.

use std::borrow::Cow;

use eldur_css::{ComputedStyle, Display};

use super::{BoxKind, Child, Content, LayoutBox, anonymous_style, group, whitespace};

fn is_row_group(d: Display) -> bool {
    matches!(
        d,
        Display::TableRowGroup | Display::TableHeaderGroup | Display::TableFooterGroup
    )
}

fn is_proper_table_child(d: Display) -> bool {
    is_row_group(d)
        || matches!(
            d,
            Display::TableRow
                | Display::TableCaption
                | Display::TableColumn
                | Display::TableColumnGroup
        )
}

fn is_internal(d: Display) -> bool {
    is_proper_table_child(d) || d == Display::TableCell
}

fn child_display(c: &Child<'_>) -> Option<Display> {
    match c {
        Child::Block(b) => Some(b.style.display),
        Child::Inline(_) => None,
    }
}

fn is_collapsible_space(c: &Child<'_>) -> bool {
    match c {
        Child::Inline(whitespace::Raw::Text { text, white_space }) => {
            matches!(
                white_space,
                eldur_css::WhiteSpace::Normal | eldur_css::WhiteSpace::NoWrap
            ) && text.chars().all(|ch| ch.is_ascii_whitespace())
        }
        _ => false,
    }
}

fn anonymous<'a>(parent: &ComputedStyle, display: Display, content: Content<'a>) -> LayoutBox<'a> {
    let mut style = anonymous_style(parent);
    style.display = display;
    LayoutBox {
        node: None,
        style: Cow::Owned(style),
        kind: BoxKind::Table,
        content,
        marker: None,
        pseudo: None,
    }
}

/// Wraps maximal runs of children that fail `fits` with `wrap`. Whitespace
/// at the edges of a run, or between proper children, generates nothing.
fn wrap_runs<'a>(
    children: Vec<Child<'a>>,
    fits: impl Fn(Display) -> bool,
    mut wrap: impl FnMut(Vec<Child<'a>>) -> LayoutBox<'a>,
) -> Vec<Child<'a>> {
    let mut out = Vec::new();
    let mut run: Vec<Child<'a>> = Vec::new();
    for child in children {
        if child_display(&child).is_some_and(&fits) {
            flush(&mut run, &mut out, &mut wrap);
            out.push(child);
        } else {
            run.push(child);
        }
    }
    flush(&mut run, &mut out, &mut wrap);
    out
}

fn flush<'a>(
    run: &mut Vec<Child<'a>>,
    out: &mut Vec<Child<'a>>,
    wrap: &mut impl FnMut(Vec<Child<'a>>) -> LayoutBox<'a>,
) {
    let mut items = std::mem::take(run);
    while items.last().is_some_and(is_collapsible_space) {
        items.pop();
    }
    let start = items
        .iter()
        .position(|c| !is_collapsible_space(c))
        .unwrap_or(items.len());
    items.drain(..start);
    if !items.is_empty() {
        out.push(Child::Block(Box::new(wrap(items))));
    }
}

fn anonymous_cell<'a>(children: Vec<Child<'a>>, parent: &'a ComputedStyle) -> LayoutBox<'a> {
    anonymous(parent, Display::TableCell, group(children, parent))
}

fn blocks<'a>(children: Vec<Child<'a>>) -> Content<'a> {
    let boxes: Vec<LayoutBox<'a>> = children
        .into_iter()
        .filter_map(|c| match c {
            Child::Block(b) => Some(*b),
            Child::Inline(_) => None,
        })
        .collect();
    if boxes.is_empty() {
        Content::Empty
    } else {
        Content::Blocks(boxes)
    }
}

fn anonymous_row<'a>(children: Vec<Child<'a>>, parent: &'a ComputedStyle) -> LayoutBox<'a> {
    let cells = wrap_runs(
        children,
        |d| d == Display::TableCell,
        |run| anonymous_cell(run, parent),
    );
    anonymous(parent, Display::TableRow, blocks(cells))
}

/// Children of a table, row group, row or column group, with the anonymous
/// boxes they need, as that box's content.
pub(super) fn internal_content<'a>(
    display: Display,
    children: Vec<Child<'a>>,
    style: &'a ComputedStyle,
) -> Content<'a> {
    let fixed = match display {
        Display::Table | Display::InlineTable => {
            wrap_runs(children, is_proper_table_child, |run| {
                anonymous_row(run, style)
            })
        }
        Display::TableRow => wrap_runs(
            children,
            |d| d == Display::TableCell,
            |run| anonymous_cell(run, style),
        ),
        Display::TableColumnGroup | Display::TableColumn => {
            // Only `col` matters in a column group; columns have no boxes.
            children
                .into_iter()
                .filter(|c| child_display(c) == Some(Display::TableColumn))
                .collect()
        }
        _ => wrap_runs(
            children,
            |d| d == Display::TableRow,
            |run| anonymous_row(run, style),
        ),
    };
    blocks(fixed)
}

/// Children of any other box: table-internal boxes outside a table get an
/// anonymous table around each run (with the inline whitespace between
/// them dropped).
pub(super) fn wrap_orphans<'a>(
    children: Vec<Child<'a>>,
    parent: &'a ComputedStyle,
) -> Vec<Child<'a>> {
    if !children
        .iter()
        .any(|c| child_display(c).is_some_and(is_internal))
    {
        return children;
    }
    let mut out = Vec::new();
    let mut run: Vec<Child<'a>> = Vec::new();
    for child in children {
        let internal = child_display(&child).is_some_and(is_internal);
        if internal || (is_collapsible_space(&child) && !run.is_empty()) {
            run.push(child);
        } else {
            push_table(&mut run, &mut out, parent);
            out.push(child);
        }
    }
    push_table(&mut run, &mut out, parent);
    out
}

fn push_table<'a>(run: &mut Vec<Child<'a>>, out: &mut Vec<Child<'a>>, parent: &'a ComputedStyle) {
    if run.is_empty() {
        return;
    }
    let children = std::mem::take(run);
    let fixed = wrap_runs(children, is_proper_table_child, |r| {
        anonymous_row(r, parent)
    });
    out.push(Child::Block(Box::new(anonymous(
        parent,
        Display::Table,
        blocks(fixed),
    ))));
}
