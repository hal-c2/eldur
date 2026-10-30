use super::*;
use h2q_css::Rgba;

fn row(group: usize, cells: &[(usize, usize)]) -> RowSpans {
    RowSpans {
        group,
        cells: cells.to_vec(),
    }
}

fn at(row: usize, column: usize, row_span: usize, column_span: usize) -> Slot {
    Slot {
        row,
        column,
        row_span,
        column_span,
    }
}

fn side(width: f32, style: BorderStyle, r: u8) -> BorderSide {
    BorderSide {
        width,
        style,
        color: Rgba::new(r, 0, 0, 255),
        ..BorderSide::default()
    }
}

#[test]
fn cells_skip_columns_taken_by_rowspans() {
    let (slots, columns) =
        place_cells(&[row(0, &[(1, 2), (1, 1), (1, 1)]), row(0, &[(1, 1), (1, 1)])]);
    assert_eq!(columns, 3);
    assert_eq!(slots[0], [at(0, 0, 2, 1), at(0, 1, 1, 1), at(0, 2, 1, 1)]);
    assert_eq!(slots[1], [at(1, 1, 1, 1), at(1, 2, 1, 1)]);
}

#[test]
fn colspan_widens_the_grid_and_is_clamped() {
    let (slots, columns) = place_cells(&[row(0, &[(2, 1), (1, 1)]), row(0, &[(0, 1)])]);
    assert_eq!(columns, 3);
    assert_eq!(slots[0][1], at(0, 2, 1, 1));
    assert_eq!(slots[1][0], at(1, 0, 1, 1), "colspan=0 counts as 1");
    let (_, huge) = place_cells(&[row(0, &[(5000, 1)])]);
    assert_eq!(huge, MAX_COLSPAN);
}

#[test]
fn rowspan_stops_at_the_end_of_its_group() {
    let (slots, _) = place_cells(&[row(0, &[(1, 9)]), row(0, &[(1, 1)]), row(1, &[(1, 1)])]);
    assert_eq!(slots[0][0].row_span, 2);
    assert_eq!(slots[2][0], at(2, 0, 1, 1), "column 0 is free again");
}

#[test]
fn rowspan_zero_runs_to_the_end_of_the_group() {
    let (slots, _) = place_cells(&[
        row(0, &[(1, 0), (1, 1)]),
        row(0, &[(1, 1)]),
        row(0, &[(1, 1)]),
        row(1, &[(1, 1)]),
    ]);
    assert_eq!(slots[0][0].row_span, 3);
    assert_eq!(slots[2][0].column, 1);
}

#[test]
fn collapse_precedence_follows_css2_17_6_2_1() {
    use BorderOwner::{Cell, Row, Table};
    let solid1 = side(1.0, BorderStyle::Solid, 1);
    let solid2 = side(2.0, BorderStyle::Solid, 2);
    let dashed2 = side(2.0, BorderStyle::Dashed, 3);
    let double2 = side(2.0, BorderStyle::Double, 4);
    let hidden = BorderSide {
        style: BorderStyle::Hidden,
        ..BorderSide::default()
    };
    let none = BorderSide::default();

    assert!(beats((&hidden, Table), (&solid2, Cell)), "hidden wins");
    assert!(!beats((&solid2, Cell), (&hidden, Table)), "hidden stays");
    assert!(beats((&solid1, Table), (&none, Cell)), "none loses");
    assert!(beats((&solid2, Table), (&solid1, Cell)), "wider wins");
    assert!(
        beats((&solid2, Table), (&dashed2, Cell)),
        "solid over dashed"
    );
    assert!(
        beats((&double2, Table), (&solid2, Cell)),
        "double over solid"
    );
    assert!(beats((&solid2, Cell), (&solid2, Row)), "cell over row");
    assert!(!beats((&solid2, Cell), (&solid2, Cell)), "the first stays");
}

#[test]
fn collapsed_edges_are_shared_between_neighbours() {
    let mut edges = Edges::new(1, 2);
    let mut left = ComputedStyle::default();
    left.border.right = side(1.0, BorderStyle::Solid, 1);
    let mut right = ComputedStyle::default();
    right.border.left = side(3.0, BorderStyle::Solid, 2);
    let (a, b) = (at(0, 0, 1, 1), at(0, 1, 1, 1));
    edges.offer(a, &left, BorderOwner::Cell);
    edges.offer(b, &right, BorderOwner::Cell);
    let [_, a_right, _, _] = edges.cell_borders(a);
    let [_, _, _, b_left] = edges.cell_borders(b);
    assert_eq!(a_right.color, b_left.color, "one edge, one colour");
    assert_eq!(b_left.color, Rgba::new(2, 0, 0, 255));
    assert_eq!(
        (a_right.width, b_left.width),
        (1.0, 2.0),
        "larger half after"
    );
}

#[test]
fn collapsed_outer_edges_stay_whole() {
    let mut edges = Edges::new(1, 1);
    let mut table = ComputedStyle::default();
    table.border.top = side(3.0, BorderStyle::Solid, 1);
    table.border.left = side(3.0, BorderStyle::Solid, 1);
    edges.offer(at(0, 0, 1, 1), &table, BorderOwner::Table);
    let [top, _, _, left] = edges.cell_borders(at(0, 0, 1, 1));
    assert_eq!((top.width, left.width), (3.0, 3.0));
}

#[test]
fn composite_is_source_over() {
    let red = Rgba::new(255, 0, 0, 255);
    let half_blue = Rgba::new(0, 0, 255, 128);
    assert_eq!(composite(red, half_blue), red);
    assert_eq!(composite(Rgba::TRANSPARENT, red), red);
    assert_eq!(composite(half_blue, Rgba::TRANSPARENT), half_blue);
    let mixed = composite(half_blue, red);
    assert_eq!((mixed.r, mixed.b, mixed.a), (127, 128, 255));
}

#[test]
fn column_specs_merge_and_print() {
    use ColumnSpec::{Auto, Percent, Px};
    assert_eq!(Auto.merge(Px(3.0)), Px(3.0));
    assert_eq!(Px(3.0).merge(Px(5.0)), Px(5.0));
    assert_eq!(Px(3.0).merge(Percent(5.0)), Percent(5.0));
    assert_eq!(Auto.qml(), "null");
    assert_eq!(Px(30.0).qml(), "30");
    assert_eq!(Percent(25.0).qml(), "\"25%\"");
}
