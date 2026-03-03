use super::*;
use crate::kernel::editor::{HighlightKind, HighlightSpan};
use ropey::Rope;
use std::sync::Arc;
use tree_sitter::{InputEdit, Point, Range};

fn dummy_span() -> Vec<HighlightSpan> {
    vec![HighlightSpan {
        start: 0,
        end: 1,
        kind: HighlightKind::Keyword,
    }]
}

#[test]
fn apply_edit_shape_shift_splice_keeps_alignment() {
    let rope_old = Rope::from_str("a\nbc\nd");
    let rope_new = Rope::from_str("a\nb\nc\nd");

    let mut cache = AsyncSyntaxHighlightCache::new_for_rope(&rope_old);
    cache.apply_patch(0, vec![dummy_span(), dummy_span(), dummy_span()]);

    let old_line2 = cache.line(2).cloned().expect("line 2 spans");
    assert_eq!(cache.dirty_segments(), Vec::<(usize, usize)>::new());

    let edit = InputEdit {
        start_byte: 0,
        old_end_byte: 0,
        new_end_byte: 0,
        start_position: Point { row: 1, column: 1 },
        old_end_position: Point { row: 1, column: 1 },
        new_end_position: Point { row: 2, column: 0 },
    };
    cache.apply_edit_shape_shift(&rope_new, &edit);

    assert_eq!(cache.line_count, rope_new.len_lines().max(1));
    assert!(cache.line(0).is_some());
    assert!(cache.line(1).is_none());
    assert!(cache.line(2).is_none());
    assert!(cache.dirty[1]);
    assert!(cache.dirty[2]);
    let line3 = cache.line(3).expect("line 3 spans");
    assert!(Arc::ptr_eq(line3, &old_line2));
    assert!(!cache.dirty[3]);
}

#[test]
fn apply_edit_shape_shift_same_line_shifts_spans_marks_dirty() {
    let rope_old = Rope::from_str("a\nbc\nd");
    let rope_new = Rope::from_str("a\nxbc\nd");

    let mut cache = AsyncSyntaxHighlightCache::new_for_rope(&rope_old);
    cache.apply_patch(0, vec![dummy_span(), dummy_span(), dummy_span()]);

    let edit = InputEdit {
        start_byte: rope_old.line_to_byte(1),
        old_end_byte: rope_old.line_to_byte(1),
        new_end_byte: rope_old.line_to_byte(1) + 1,
        start_position: Point { row: 1, column: 0 },
        old_end_position: Point { row: 1, column: 0 },
        new_end_position: Point { row: 1, column: 1 },
    };
    cache.apply_edit_shape_shift(&rope_new, &edit);

    let after = cache.line(1).expect("line 1 spans");
    assert_eq!(
        after.as_slice(),
        &[HighlightSpan {
            start: 1,
            end: 2,
            kind: HighlightKind::Keyword,
        }]
    );
    assert!(cache.dirty[1]);
}

#[test]
fn ensure_shape_new_lines_none_and_dirty() {
    let rope_old = Rope::from_str("a\nb");
    let rope_new = Rope::from_str("a\nb\nc\nd");

    let mut cache = AsyncSyntaxHighlightCache::new_for_rope(&rope_old);
    cache.apply_patch(0, vec![dummy_span(), dummy_span()]);
    assert_eq!(cache.line_count, 2);
    assert_eq!(cache.dirty_segments(), Vec::<(usize, usize)>::new());

    cache.ensure_shape_for_rope(&rope_new);
    assert_eq!(cache.line_count, 4);
    assert!(cache.line(0).is_some());
    assert!(cache.line(1).is_some());
    assert!(cache.line(2).is_none());
    assert!(cache.line(3).is_none());
    assert!(cache.dirty[2]);
    assert!(cache.dirty[3]);
}

#[test]
fn mark_dirty_keeps_existing_spans() {
    let rope = Rope::from_str("a\nb\nc");
    let mut cache = AsyncSyntaxHighlightCache::new_for_rope(&rope);
    cache.apply_patch(0, vec![dummy_span(), dummy_span(), dummy_span()]);

    let before = cache.line(1).cloned().expect("line 1 spans");

    let start_byte = rope.line_to_byte(1);
    let end_byte = rope.line_to_byte(2);
    let range = Range {
        start_byte,
        end_byte,
        start_point: Point { row: 1, column: 0 },
        end_point: Point { row: 1, column: 1 },
    };
    cache.mark_dirty_from_changed_ranges(&rope, &[range]);

    assert!(cache.dirty[1]);
    let after = cache.line(1).expect("line 1 spans");
    assert!(Arc::ptr_eq(after, &before));
}

#[test]
fn apply_patch_clears_dirty_and_replaces_lines() {
    let rope = Rope::from_str("a\nb\nc");
    let mut cache = AsyncSyntaxHighlightCache::new_for_rope(&rope);

    cache.apply_patch(0, vec![dummy_span(), dummy_span(), dummy_span()]);
    assert_eq!(cache.dirty_segments(), Vec::<(usize, usize)>::new());

    cache.dirty[1] = true;
    cache.apply_patch(
        1,
        vec![vec![HighlightSpan {
            start: 0,
            end: 2,
            kind: HighlightKind::String,
        }]],
    );

    assert!(!cache.dirty[1]);
    let spans = cache.line(1).expect("line 1 spans");
    assert_eq!(
        spans.as_slice(),
        &[HighlightSpan {
            start: 0,
            end: 2,
            kind: HighlightKind::String,
        }]
    );
}

#[test]
fn dirty_segments_with_budget_centers_around_line() {
    let rope = Rope::from_str("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");
    let cache = AsyncSyntaxHighlightCache::new_for_rope(&rope);

    let segments = cache.dirty_segments_with_budget(5, 3);
    assert_eq!(segments.len(), 1);
    let (start, end) = segments[0];
    assert_eq!(end - start, 3);
    assert!(start <= 5 && 5 < end);
}

#[test]
fn dirty_segments_with_budget_advances_after_patch() {
    let rope = Rope::from_str("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");
    let mut cache = AsyncSyntaxHighlightCache::new_for_rope(&rope);

    let first = cache.dirty_segments_with_budget(0, 3);
    assert_eq!(first, vec![(0, 3)]);
    cache.apply_patch(0, vec![dummy_span(), dummy_span(), dummy_span()]);

    let second = cache.dirty_segments_with_budget(0, 3);
    assert_eq!(second, vec![(3, 6)]);
}
