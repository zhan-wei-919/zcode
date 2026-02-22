//! Multi-cursor helpers.
//!
//! Primary cursor/selection remains in `TextBuffer`. This module only manages secondary cursors.

use super::{Granularity, Selection};
use ropey::Rope;

#[derive(Debug, Clone)]
pub struct SecondaryCursor {
    pub pos: (usize, usize), // (row, col) in grapheme units
    pub selection: Option<Selection>,
    pub goal_col: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct MergeOverlappingResult {
    pub primary_pos: (usize, usize),
    pub primary_selection: Option<Selection>,
}

fn normalized_selection(selection: Option<&Selection>) -> Option<Selection> {
    selection.filter(|s| !s.is_empty()).cloned().or(None)
}

fn normalized_secondary_selection(selection: &Option<Selection>) -> Option<Selection> {
    selection.as_ref().filter(|s| !s.is_empty()).cloned()
}

fn selection_bounds(selection: &Selection) -> ((usize, usize), (usize, usize)) {
    selection.range()
}

fn point_in_selection(pos: (usize, usize), selection: &Selection) -> bool {
    selection.contains(pos)
}

fn overlaps(
    a: Option<&Selection>,
    a_pos: (usize, usize),
    b: Option<&Selection>,
    b_pos: (usize, usize),
) -> bool {
    match (a, b) {
        (Some(a_sel), Some(b_sel)) => {
            let (a_start, a_end) = selection_bounds(a_sel);
            let (b_start, b_end) = selection_bounds(b_sel);
            // half-open intersection
            a_start < b_end && b_start < a_end
        }
        (Some(a_sel), None) => point_in_selection(b_pos, a_sel),
        (None, Some(b_sel)) => point_in_selection(a_pos, b_sel),
        (None, None) => a_pos == b_pos,
    }
}

fn union_bounds(
    left: Option<&Selection>,
    left_pos: (usize, usize),
    right: Option<&Selection>,
    right_pos: (usize, usize),
) -> ((usize, usize), (usize, usize)) {
    let (l_start, l_end) = left.map(selection_bounds).unwrap_or((left_pos, left_pos));
    let (r_start, r_end) = right
        .map(selection_bounds)
        .unwrap_or((right_pos, right_pos));
    (l_start.min(r_start), l_end.max(r_end))
}

fn selection_from_bounds_with_primary_orientation(
    bounds: ((usize, usize), (usize, usize)),
    primary_pos: (usize, usize),
    primary_selection: Option<&Selection>,
) -> (Option<Selection>, (usize, usize)) {
    let (start, end) = bounds;
    if start == end {
        return (None, primary_pos);
    }

    let (anchor, cursor) = match primary_selection {
        Some(sel) if !sel.is_empty() => {
            let anchor = sel.anchor();
            let cursor = sel.cursor();
            if cursor >= anchor {
                (start, end)
            } else {
                (end, start)
            }
        }
        _ => (start, end),
    };

    let mut selection = Selection::new(anchor, Granularity::Char);
    // Granularity::Char ignores rope, but the API requires one.
    selection.update_cursor(cursor, &Rope::new());
    (Some(selection), cursor)
}

/// Merges overlapping secondary cursors/selections and merges any overlaps into primary.
///
/// - Primary cursor/selection is preserved as the "winner" for any overlap group containing it.
/// - When merging, selections are replaced by their union; empty selections are treated as absent.
/// - Secondary cursors are kept sorted by position.
pub fn merge_overlapping(
    primary_pos: (usize, usize),
    primary_selection: Option<&Selection>,
    secondaries: &mut Vec<SecondaryCursor>,
) -> MergeOverlappingResult {
    let mut primary_sel = normalized_selection(primary_selection);

    let dummy_rope = Rope::new();

    for c in &mut *secondaries {
        if let Some(sel) = normalized_secondary_selection(&c.selection) {
            c.pos = sel.cursor();
            c.selection = Some(sel);
        } else {
            c.selection = None;
        }
    }

    // Sort secondaries by position for stable behavior.
    secondaries.sort_by_key(|c| c.pos);

    // Merge secondaries into primary when overlapping.
    let mut i = 0usize;
    while i < secondaries.len() {
        let sec_pos = secondaries[i].pos;
        let sec_sel = normalized_secondary_selection(&secondaries[i].selection);
        let overlaps_primary =
            overlaps(primary_sel.as_ref(), primary_pos, sec_sel.as_ref(), sec_pos);
        if overlaps_primary {
            let bounds = union_bounds(primary_sel.as_ref(), primary_pos, sec_sel.as_ref(), sec_pos);
            let (new_sel, new_primary_pos) = selection_from_bounds_with_primary_orientation(
                bounds,
                primary_pos,
                primary_sel.as_ref(),
            );
            primary_sel = new_sel;
            // Primary position becomes the active end when a selection exists.
            let _ = new_primary_pos;
            secondaries.remove(i);
            continue;
        }
        i += 1;
    }

    // Merge overlapping secondaries among themselves.
    // At this point, secondaries are sorted by pos, and none overlap with primary.
    let mut out: Vec<SecondaryCursor> = Vec::with_capacity(secondaries.len());
    for mut cur in secondaries.drain(..) {
        cur.selection = normalized_secondary_selection(&cur.selection);
        if let Some(prev) = out.last_mut() {
            let prev_pos = prev.pos;
            let prev_sel = prev.selection.as_ref();
            let cur_pos = cur.pos;
            let cur_sel = cur.selection.as_ref();
            if overlaps(prev_sel, prev_pos, cur_sel, cur_pos) {
                let bounds = union_bounds(prev_sel, prev_pos, cur_sel, cur_pos);
                if bounds.0 == bounds.1 {
                    prev.selection = None;
                    prev.pos = bounds.0;
                    continue;
                }
                // For secondary-secondary merge, keep the earlier cursor as representative.
                let mut selection = Selection::new(bounds.0, Granularity::Char);
                selection.update_cursor(bounds.1, &dummy_rope);
                prev.selection = Some(selection);
                prev.pos = bounds.1;
                continue;
            }
        }
        out.push(cur);
    }
    *secondaries = out;

    let primary_selection = primary_sel;
    let primary_pos = primary_selection
        .as_ref()
        .map(|s| s.cursor())
        .unwrap_or(primary_pos);

    MergeOverlappingResult {
        primary_pos,
        primary_selection,
    }
}

fn selection_range_for_row(selection: &Selection, row: usize) -> Option<(usize, usize)> {
    let ((start_row, start_col), (end_row, end_col)) = selection.range();
    if row < start_row || row > end_row {
        return None;
    }

    let (sel_start, sel_end) = if row == start_row && row == end_row {
        (start_col, end_col)
    } else if row == start_row {
        (start_col, usize::MAX)
    } else if row == end_row {
        (0, end_col)
    } else {
        (0, usize::MAX)
    };
    Some((sel_start, sel_end))
}

pub fn selections_for_row(
    primary_selection: Option<&Selection>,
    secondaries: &[SecondaryCursor],
    row: usize,
) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    if let Some(sel) = primary_selection.filter(|s| !s.is_empty()) {
        if let Some(r) = selection_range_for_row(sel, row) {
            if r.0 != r.1 {
                ranges.push(r);
            }
        }
    }
    for c in secondaries {
        let Some(sel) = c.selection.as_ref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if let Some(r) = selection_range_for_row(sel, row) {
            if r.0 != r.1 {
                ranges.push(r);
            }
        }
    }

    if ranges.len() <= 1 {
        return ranges;
    }

    ranges.sort_by_key(|r| (r.0, r.1));
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(last) = out.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        out.push((start, end));
    }
    out
}

pub fn secondary_positions_for_row(secondaries: &[SecondaryCursor], row: usize) -> Vec<usize> {
    let mut cols: Vec<usize> = secondaries
        .iter()
        .filter_map(|c| (c.pos.0 == row).then_some(c.pos.1))
        .collect();
    cols.sort_unstable();
    cols.dedup();
    cols
}

pub fn secondary_cursor_positions(secondaries: &[SecondaryCursor]) -> Vec<(usize, usize)> {
    secondaries.iter().map(|c| c.pos).collect()
}

pub fn adjust_char_offset_after_edit(
    offset: usize,
    start: usize,
    end: usize,
    inserted_len: usize,
) -> usize {
    if offset < start {
        return offset;
    }
    if offset < end {
        return start.saturating_add(inserted_len);
    }

    let deleted_len = end.saturating_sub(start);
    if inserted_len >= deleted_len {
        offset.saturating_add(inserted_len.saturating_sub(deleted_len))
    } else {
        offset.saturating_sub(deleted_len.saturating_sub(inserted_len))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/models/cursor_set.rs"]
mod tests;
