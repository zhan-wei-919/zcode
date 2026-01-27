use super::super::syntax::HighlightKind;
use super::*;

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
