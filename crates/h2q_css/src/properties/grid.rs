//! CSS Grid Layout 1 property parsers: track lists, template areas, auto-flow, and grid lines.

use std::collections::HashMap;

use swc_css_ast::ComponentValue;

use crate::{GridLine, Track};

use super::{
    cursor::{Cursor, is_css_wide_keyword, split_comma_separated},
    length::{UnitContext, eat_integer_or_calc, eat_length, parse_integer_cvs, parse_length_cvs},
};

/// Maximum number of expanded tracks allowed in a single track list (protects
/// against `repeat(999999999, 1fr)` resource exhaustion).
const MAX_EXPANDED_TRACKS: usize = 1000;

pub(crate) fn parse_grid_template_tracks(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Vec<Track>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(Vec::new());
    }

    let mut tracks = Vec::new();
    let mut saw_auto_repeat = false;

    skip_line_names(&mut cur)?;
    while !cur.is_eof() {
        let saved = cur.save();
        if let Some((fname, fargs)) = cur.eat_function() {
            if fname.eq_ignore_ascii_case("repeat") {
                parse_repeat(fargs, uctx, &mut tracks, &mut saw_auto_repeat)?;
                skip_line_names(&mut cur)?;
                continue;
            }
            cur.restore(saved);
        }

        let track = eat_track_size(&mut cur, uctx)?
            .ok_or_else(|| "expected grid track size".to_owned())?;
        if tracks.len() >= MAX_EXPANDED_TRACKS {
            return Err("grid track list exceeds maximum track count".to_owned());
        }
        tracks.push(track);
        skip_line_names(&mut cur)?;
    }

    if tracks.is_empty() {
        return Err("grid track list must contain at least one track".to_owned());
    }
    Ok(tracks)
}

pub(crate) fn parse_grid_auto_tracks(
    cvs: &[ComponentValue],
    uctx: &UnitContext,
) -> Result<Vec<Track>, String> {
    let mut cur = Cursor::new(cvs);
    let mut tracks = Vec::new();
    while !cur.is_eof() {
        let track = eat_track_size(&mut cur, uctx)?
            .ok_or_else(|| "expected grid-auto track size".to_owned())?;
        tracks.push(track);
    }
    if tracks.is_empty() {
        return Err("empty grid-auto-columns/rows".to_owned());
    }
    Ok(tracks)
}

fn skip_line_names(cur: &mut Cursor<'_>) -> Result<(), String> {
    while let Some(bracket_cvs) = cur.eat_bracket_block() {
        let mut inner = Cursor::new(bracket_cvs);
        while !inner.is_eof() {
            let id = inner
                .eat_custom_ident()
                .ok_or_else(|| "invalid token inside grid [line-names]".to_owned())?;
            if id.eq_ignore_ascii_case("span") || id.eq_ignore_ascii_case("auto") {
                return Err(format!("'{id}' is not a valid grid line name"));
            }
        }
    }
    Ok(())
}

fn parse_repeat(
    args: &[ComponentValue],
    uctx: &UnitContext,
    out: &mut Vec<Track>,
    saw_auto_repeat: &mut bool,
) -> Result<(), String> {
    let parts = split_comma_separated(args)?;
    if parts.len() != 2 {
        return Err("repeat() requires 2 comma-separated arguments".to_owned());
    }

    // Parse inner track list (nested repeat() is forbidden by CSS Grid 1).
    let mut inner_cur = Cursor::new(parts[1]);
    let mut inner_tracks = Vec::new();
    skip_line_names(&mut inner_cur)?;
    while !inner_cur.is_eof() {
        let track = eat_track_size(&mut inner_cur, uctx)?
            .ok_or_else(|| "invalid track inside repeat()".to_owned())?;
        inner_tracks.push(track);
        skip_line_names(&mut inner_cur)?;
    }
    if inner_tracks.is_empty() {
        return Err("repeat() track list cannot be empty".to_owned());
    }

    let mut count_cur = Cursor::new(parts[0]);
    if let Some(kw) = count_cur.eat_ident() {
        let fill = match kw.to_ascii_lowercase().as_str() {
            "auto-fill" => true,
            "auto-fit" => false,
            _ => return Err(format!("invalid repeat() count keyword '{kw}'")),
        };
        count_cur.expect_eof()?;
        if *saw_auto_repeat {
            return Err("only one auto-fill/auto-fit repeat() is allowed per track list".to_owned());
        }
        // CSS Grid 1 §7.2.3.2: bare flexible (`fr`) tracks cannot appear directly inside auto-repeat.
        for t in &inner_tracks {
            if matches!(t, Track::Fr(_)) {
                return Err("intrinsic/flexible track cannot be auto-repeated directly".to_owned());
            }
        }
        *saw_auto_repeat = true;
        out.push(Track::AutoRepeat {
            fill,
            tracks: inner_tracks,
        });
        return Ok(());
    }

    let count = parse_integer_cvs(parts[0])?;
    if count < 1 {
        return Err("repeat() count must be at least 1".to_owned());
    }
    let count_usize = count as usize;
    let added = count_usize
        .checked_mul(inner_tracks.len())
        .ok_or_else(|| "repeat() count overflow".to_owned())?;
    if out.len() + added > MAX_EXPANDED_TRACKS {
        return Err("repeat() expansion exceeds maximum track count".to_owned());
    }
    for _ in 0..count_usize {
        out.extend(inner_tracks.iter().cloned());
    }
    Ok(())
}

pub(crate) fn eat_track_size(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
) -> Result<Option<Track>, String> {
    let saved = cur.save();
    if let Some((fname, fargs)) = cur.eat_function() {
        if fname.eq_ignore_ascii_case("minmax") {
            let parts = split_comma_separated(fargs)?;
            if parts.len() != 2 {
                return Err("minmax() requires 2 arguments".to_owned());
            }
            let mut min_cur = Cursor::new(parts[0]);
            let min_track = eat_track_breadth(&mut min_cur, uctx, false)?
                .ok_or_else(|| "invalid minimum breadth in minmax()".to_owned())?;
            min_cur.expect_eof()?;

            let mut max_cur = Cursor::new(parts[1]);
            let max_track = eat_track_breadth(&mut max_cur, uctx, true)?
                .ok_or_else(|| "invalid maximum breadth in minmax()".to_owned())?;
            max_cur.expect_eof()?;

            return Ok(Some(Track::MinMax(
                Box::new(min_track),
                Box::new(max_track),
            )));
        }
        if fname.eq_ignore_ascii_case("fit-content") {
            let len = parse_length_cvs(fargs, uctx, true)?;
            return Ok(Some(Track::FitContent(len)));
        }
        cur.restore(saved);
    }

    eat_track_breadth(cur, uctx, true)
}

fn eat_track_breadth(
    cur: &mut Cursor<'_>,
    uctx: &UnitContext,
    allow_flex: bool,
) -> Result<Option<Track>, String> {
    let saved = cur.save();
    if let Some(id) = cur.eat_ident() {
        match id.to_ascii_lowercase().as_str() {
            "auto" => return Ok(Some(Track::Auto)),
            "min-content" => return Ok(Some(Track::MinContent)),
            "max-content" => return Ok(Some(Track::MaxContent)),
            _ => cur.restore(saved),
        }
    }

    if let Some((val, unit, _)) = cur.eat_dimension() {
        if unit.eq_ignore_ascii_case("fr") {
            if !allow_flex {
                return Err("flexible 'fr' unit is not allowed as minmax() minimum".to_owned());
            }
            if val < 0.0 {
                return Err("negative 'fr' track breadth is not allowed".to_owned());
            }
            return Ok(Some(Track::Fr(val)));
        }
        cur.restore(saved);
    }

    if let Some(len) = eat_length(cur, uctx, true)? {
        return Ok(Some(Track::Length(len)));
    }

    Ok(None)
}

/// Parses `grid-template-areas` (`none` or one string per row) and verifies
/// that all rows have identical non-zero length and every named area is a rectangle.
pub(crate) fn parse_grid_template_areas(
    cvs: &[ComponentValue],
) -> Result<Vec<Vec<Option<String>>>, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("none") {
        cur.expect_eof()?;
        return Ok(Vec::new());
    }

    let mut rows = Vec::new();
    while let Some(row_str) = cur.eat_string() {
        let cells = tokenize_area_row(row_str)?;
        if cells.is_empty() {
            return Err("grid-template-areas row cannot be empty".to_owned());
        }
        rows.push(cells);
    }
    cur.expect_eof()?;

    if rows.is_empty() {
        return Err("expected 'none' or area strings in grid-template-areas".to_owned());
    }

    validate_area_rectangles(&rows)?;
    Ok(rows)
}

pub(crate) fn tokenize_area_row(row: &str) -> Result<Vec<Option<String>>, String> {
    let mut cells = Vec::new();
    let bytes = row.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip ASCII whitespace per CSS Grid 1 §7.3.1.
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Null cell token: one or more `.` characters.
        if bytes[i] == b'.' {
            while i < bytes.len() && bytes[i] == b'.' {
                i += 1;
            }
            cells.push(None);
            continue;
        }
        // Named cell token: CSS name code points.
        let start = i;
        while i < bytes.len() {
            let ch = row[i..].chars().next().unwrap_or('\0');
            if is_area_name_char(ch) {
                i += ch.len_utf8();
            } else {
                break;
            }
        }
        if i == start {
            return Err(format!("invalid character in grid-template-areas row '{row}'"));
        }
        let name = &row[start..i];
        if is_css_wide_keyword(name) {
            return Err(format!("'{name}' cannot be used as a grid area name"));
        }
        cells.push(Some(name.to_owned()));
    }

    Ok(cells)
}

fn is_area_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch >= '\u{80}'
}

pub(crate) fn validate_area_rectangles(rows: &[Vec<Option<String>>]) -> Result<(), String> {
    let Some(first) = rows.first() else {
        return Ok(());
    };
    let expected_cols = first.len();
    if expected_cols == 0 {
        return Err("grid-template-areas row has zero columns".to_owned());
    }

    // (min_r, max_r, min_c, max_c, count)
    let mut bounds: HashMap<&str, (usize, usize, usize, usize, usize)> = HashMap::new();

    for (r, row) in rows.iter().enumerate() {
        if row.len() != expected_cols {
            return Err("grid-template-areas rows must all have the same number of columns".to_owned());
        }
        for (c, cell) in row.iter().enumerate() {
            if let Some(name) = cell.as_deref() {
                bounds
                    .entry(name)
                    .and_modify(|b| {
                        b.0 = b.0.min(r);
                        b.1 = b.1.max(r);
                        b.2 = b.2.min(c);
                        b.3 = b.3.max(c);
                        b.4 += 1;
                    })
                    .or_insert((r, r, c, c, 1));
            }
        }
    }

    for (name, (min_r, max_r, min_c, max_c, count)) in bounds {
        let rect_area = (max_r - min_r + 1) * (max_c - min_c + 1);
        if rect_area != count {
            return Err(format!("grid area '{name}' does not form a single rectangle"));
        }
    }

    Ok(())
}

pub(crate) fn parse_grid_auto_flow(cvs: &[ComponentValue]) -> Result<bool, String> {
    let mut cur = Cursor::new(cvs);
    let mut dir: Option<bool> = None;
    let mut dense = false;

    while let Some(id) = cur.eat_ident() {
        match id.to_ascii_lowercase().as_str() {
            "row" => {
                if dir.replace(false).is_some() {
                    return Err("duplicate direction in grid-auto-flow".to_owned());
                }
            }
            "column" => {
                if dir.replace(true).is_some() {
                    return Err("duplicate direction in grid-auto-flow".to_owned());
                }
            }
            "dense" => {
                if dense {
                    return Err("duplicate 'dense' in grid-auto-flow".to_owned());
                }
                dense = true;
            }
            _ => return Err(format!("invalid grid-auto-flow keyword '{id}'")),
        }
    }
    cur.expect_eof()?;

    if dir.is_none() && !dense {
        return Err("empty grid-auto-flow".to_owned());
    }
    Ok(dir.unwrap_or(false))
}

/// Parses `<grid-line>`: `auto`, `<int != 0>`, `<custom-ident>`, `span <int >= 1>`,
/// `span <custom-ident>` (and order permutations allowed by CSS Grid 1 §8.3).
pub(crate) fn parse_grid_line(cvs: &[ComponentValue]) -> Result<GridLine, String> {
    let mut cur = Cursor::new(cvs);
    if cur.eat_ident_eq("auto") {
        cur.expect_eof()?;
        return Ok(GridLine::Auto);
    }

    let mut has_span = false;
    let mut line_num: Option<i32> = None;
    let mut line_ident: Option<String> = None;

    while !cur.is_eof() {
        if cur.eat_ident_eq("span") {
            if has_span {
                return Err("duplicate 'span' in grid line".to_owned());
            }
            has_span = true;
            continue;
        }
        if let Some(n) = eat_integer_or_calc(&mut cur)? {
            if line_num.replace(n).is_some() {
                return Err("duplicate integer in grid line".to_owned());
            }
            continue;
        }
        if let Some(id) = cur.eat_custom_ident() {
            if id.eq_ignore_ascii_case("auto") {
                return Err("'auto' cannot combine with other grid line tokens".to_owned());
            }
            if line_ident.replace(id.to_owned()).is_some() {
                return Err("duplicate identifier in grid line".to_owned());
            }
            continue;
        }
        return Err("invalid token in grid line".to_owned());
    }

    if has_span {
        if let Some(n) = line_num
            && n < 1
        {
            return Err("grid span integer must be >= 1".to_owned());
        }
        return match (line_num, line_ident) {
            (_, Some(name)) => Ok(GridLine::Named(name)),
            (Some(n), None) => Ok(GridLine::Span(n as u32)),
            (None, None) => Err("'span' requires a positive integer or line name".to_owned()),
        };
    }

    match (line_num, line_ident) {
        (Some(0), _) => Err("grid line number cannot be 0".to_owned()),
        (Some(n), None) => Ok(GridLine::Line(n)),
        (Some(n), Some(_)) => Ok(GridLine::Line(n)),
        (None, Some(name)) => Ok(GridLine::Named(name)),
        (None, None) => Err("empty grid line".to_owned()),
    }
}
