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
fn apply_edit_shape_shift_same_line_keeps_spans_marks_dirty() {
    let rope_old = Rope::from_str("a\nbc\nd");
    let rope_new = Rope::from_str("a\nbxc\nd");

    let mut cache = AsyncSyntaxHighlightCache::new_for_rope(&rope_old);
    cache.apply_patch(0, vec![dummy_span(), dummy_span(), dummy_span()]);

    let before = cache.line(1).cloned().expect("line 1 spans");

    let edit = InputEdit {
        start_byte: 0,
        old_end_byte: 0,
        new_end_byte: 0,
        start_position: Point { row: 1, column: 1 },
        old_end_position: Point { row: 1, column: 1 },
        new_end_position: Point { row: 1, column: 2 },
    };
    cache.apply_edit_shape_shift(&rope_new, &edit);

    let after = cache.line(1).expect("line 1 spans");
    assert!(Arc::ptr_eq(after, &before));
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
