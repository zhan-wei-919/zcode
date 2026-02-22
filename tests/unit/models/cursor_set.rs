use super::*;
use crate::models::{Granularity, Selection};
use ropey::Rope;

fn sel(anchor: (usize, usize), cursor: (usize, usize)) -> Selection {
    let rope = Rope::from_str("abcdef\nabcdef\nabcdef\n");
    let mut s = Selection::new(anchor, Granularity::Char);
    s.update_cursor(cursor, &rope);
    s
}

#[test]
fn merge_overlapping_dedup_secondary_points() {
    let mut secondaries = vec![
        SecondaryCursor {
            pos: (0, 1),
            selection: None,
            goal_col: None,
        },
        SecondaryCursor {
            pos: (0, 1),
            selection: None,
            goal_col: None,
        },
    ];

    let res = merge_overlapping((0, 0), None, &mut secondaries);
    assert_eq!(res.primary_pos, (0, 0));
    assert!(res.primary_selection.is_none());
    assert_eq!(secondaries.len(), 1);
    assert_eq!(secondaries[0].pos, (0, 1));
    assert!(secondaries[0].selection.is_none());
}

#[test]
fn merge_overlapping_secondary_inside_primary_selection() {
    let primary = sel((0, 2), (0, 5));
    let mut secondaries = vec![SecondaryCursor {
        pos: (0, 3),
        selection: None,
        goal_col: None,
    }];

    let res = merge_overlapping((0, 5), Some(&primary), &mut secondaries);
    assert_eq!(secondaries.len(), 0);
    let sel = res.primary_selection.expect("primary selection preserved");
    assert_eq!(sel.range(), ((0, 2), (0, 5)));
    assert_eq!(res.primary_pos, (0, 5));
}

#[test]
fn merge_overlapping_extends_primary_reverse_orientation() {
    let primary = sel((0, 5), (0, 2)); // reverse
    let sec = sel((0, 0), (0, 4));
    let mut secondaries = vec![SecondaryCursor {
        pos: sec.cursor(),
        selection: Some(sec),
        goal_col: None,
    }];

    let res = merge_overlapping((0, 2), Some(&primary), &mut secondaries);
    assert_eq!(secondaries.len(), 0);
    let sel = res.primary_selection.expect("primary selection present");
    assert_eq!(sel.anchor(), (0, 5));
    assert_eq!(sel.cursor(), (0, 0));
    assert_eq!(sel.range(), ((0, 0), (0, 5)));
    assert_eq!(res.primary_pos, (0, 0));
}

#[test]
fn selections_for_row_merges_overlaps() {
    let primary = sel((0, 1), (0, 3));
    let sec = sel((0, 2), (0, 5));
    let secondaries = vec![SecondaryCursor {
        pos: sec.cursor(),
        selection: Some(sec),
        goal_col: None,
    }];

    let ranges = selections_for_row(Some(&primary), &secondaries, 0);
    assert_eq!(ranges, vec![(1, 5)]);
}

#[test]
fn selections_for_row_middle_line_is_full_width() {
    let multi = sel((0, 2), (2, 1));
    let ranges = selections_for_row(Some(&multi), &[], 1);
    assert_eq!(ranges, vec![(0, usize::MAX)]);
}
