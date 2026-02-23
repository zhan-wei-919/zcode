use super::super::syntax::{HighlightKind, HighlightSpan};
use super::*;

fn span(start: usize, end: usize, kind: HighlightKind) -> HighlightSpan {
    HighlightSpan { start, end, kind }
}

fn make_lines(start: usize, len: usize) -> Vec<Vec<HighlightSpan>> {
    (0..len)
        .map(|i| {
            vec![HighlightSpan {
                start: start + i,
                end: start + i + 1,
                kind: HighlightKind::Keyword,
            }]
        })
        .collect()
}

#[test]
fn replace_range_inserts_into_empty() {
    let mut state = SemanticHighlightState {
        version: 0,
        segments: Vec::new(),
    };

    let new_lines = make_lines(100, 2);
    state.replace_range(10, 12, new_lines.clone());

    assert_eq!(state.segments.len(), 1);
    assert_eq!(state.segments[0].start_line, 10);
    assert_eq!(state.segments[0].lines, new_lines);
}

#[test]
fn replace_range_updates_inside_single_segment() {
    let mut state = SemanticHighlightState {
        version: 0,
        segments: vec![SemanticHighlightSegment::new(0, make_lines(0, 10))],
    };

    state.replace_range(3, 5, make_lines(100, 2));

    let mut expected: Vec<Vec<HighlightSpan>> = Vec::new();
    expected.extend(make_lines(0, 3));
    expected.extend(make_lines(100, 2));
    expected.extend(make_lines(5, 5));

    assert_eq!(state.segments.len(), 1);
    assert_eq!(state.segments[0].start_line, 0);
    assert_eq!(state.segments[0].lines, expected);
}

#[test]
fn replace_range_bridges_gap_and_merges_segments() {
    let mut state = SemanticHighlightState {
        version: 0,
        segments: vec![
            SemanticHighlightSegment::new(0, make_lines(0, 2)),
            SemanticHighlightSegment::new(5, make_lines(5, 3)),
        ],
    };

    state.replace_range(1, 6, make_lines(100, 5));

    let mut expected: Vec<Vec<HighlightSpan>> = Vec::new();
    expected.extend(make_lines(0, 1));
    expected.extend(make_lines(100, 5));
    expected.extend(make_lines(6, 2));

    assert_eq!(state.segments.len(), 1);
    assert_eq!(state.segments[0].start_line, 0);
    assert_eq!(state.segments[0].lines, expected);
}

#[test]
fn replace_range_inserts_in_gap_and_merges_with_neighbors() {
    let mut state = SemanticHighlightState {
        version: 0,
        segments: vec![
            SemanticHighlightSegment::new(0, make_lines(0, 2)),
            SemanticHighlightSegment::new(5, make_lines(5, 2)),
        ],
    };

    state.replace_range(2, 5, make_lines(100, 3));

    let mut expected: Vec<Vec<HighlightSpan>> = Vec::new();
    expected.extend(make_lines(0, 2));
    expected.extend(make_lines(100, 3));
    expected.extend(make_lines(5, 2));

    assert_eq!(state.segments.len(), 1);
    assert_eq!(state.segments[0].start_line, 0);
    assert_eq!(state.segments[0].lines, expected);
}

// ── apply_byte_edit tests ──────────────────────────────────────────

/// Simulate: `let result = compute();`
/// Semantic spans on line 0:
///   "result"  [4,10) = Variable
///   "compute" [13,20) = Function
///
/// User appends "_value" right after "result" (insert 6 bytes at offset 10).
/// Line becomes: `let result_value = compute();`
///
/// Expected: "result" span stays [4,10), "compute" shifts to [19,26).
/// The "_value" portion [10,16) has NO semantic coverage → falls back to tree-sitter.
/// This means the single token "result_value" is split across two highlight systems.
#[test]
fn apply_byte_edit_insert_at_token_end_causes_split() {
    let mut state = SemanticHighlightState {
        version: 0,
        segments: vec![SemanticHighlightSegment::new(
            0,
            vec![vec![
                span(4, 10, HighlightKind::Variable),
                span(13, 20, HighlightKind::Function),
            ]],
        )],
    };

    // Insert "_value" (6 bytes) at offset 10 (right after "result")
    state.apply_byte_edit(0, 10, 0, 6);

    let spans = &state.segments[0].lines[0];
    // "result" span: start < insert point → unchanged
    assert_eq!(spans[0], span(4, 10, HighlightKind::Variable));
    // "compute" span: shifted right by 6
    assert_eq!(spans[1], span(19, 26, HighlightKind::Function));
    // Gap [10,19) has no semantic coverage — "result_value" is color-split
}

/// Insert at the START of a token.
/// "HashMap" [4,11) = Type
/// Insert "XX" at offset 4 → "XXHashMap"
/// Expected: span shifts to [6,13), "XX" at [4,6) has no coverage.
#[test]
fn apply_byte_edit_insert_at_token_start_shifts_span() {
    let mut state = SemanticHighlightState {
        version: 0,
        segments: vec![SemanticHighlightSegment::new(
            0,
            vec![vec![span(4, 11, HighlightKind::Type)]],
        )],
    };

    state.apply_byte_edit(0, 4, 0, 2);

    let spans = &state.segments[0].lines[0];
    assert_eq!(spans[0], span(6, 13, HighlightKind::Type));
}

/// Insert in the MIDDLE of a token.
/// "result" [4,10) = Variable
/// Insert "XX" at offset 7 → "resXXult"
/// Expected: span expands to [4,12) — start unchanged, end shifted.
#[test]
fn apply_byte_edit_insert_mid_token_expands_span() {
    let mut state = SemanticHighlightState {
        version: 0,
        segments: vec![SemanticHighlightSegment::new(
            0,
            vec![vec![span(4, 10, HighlightKind::Variable)]],
        )],
    };

    state.apply_byte_edit(0, 7, 0, 2);

    let spans = &state.segments[0].lines[0];
    assert_eq!(spans[0], span(4, 12, HighlightKind::Variable));
}
