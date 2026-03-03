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

#[test]
fn invalidate_semantic_highlight_shifts_on_line_start_newline_edits() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "package main\nfunc main() {}\n",
        &config,
    );

    tab.set_semantic_highlight(
        0,
        vec![
            vec![span(0, 7, HighlightKind::Keyword)], // package
            vec![span(0, 4, HighlightKind::Keyword)], // func
        ],
    );
    assert!(tab.semantic_highlight.is_some());

    let insert_at = tab.buffer.rope().line_to_char(1);
    let op = tab.buffer.replace_range_op_adjust_cursor(
        insert_at,
        insert_at,
        "import \"fmt\"\n",
        OpId::root(),
    );

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert_eq!(
        tab.semantic_highlight_line(0).unwrap_or_default(),
        &[span(0, 7, HighlightKind::Keyword)]
    );
    assert!(tab
        .semantic_highlight_line(1)
        .is_some_and(|spans| spans.is_empty()));
    assert_eq!(
        tab.semantic_highlight_line(2).unwrap_or_default(),
        &[span(0, 4, HighlightKind::Keyword)]
    );
    assert!(tab.pending_semantic_highlight.is_none());
}

#[test]
fn invalidate_semantic_highlight_clears_only_replaced_block_on_multiline_replace() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "package tools\n\nimport (\n    \"context\"\n    \"\"\n)\n",
        &config,
    );

    // Simulate a semantic token span for the import path `context` (without quotes).
    // Before formatting, the line uses spaces for indentation.
    let mut lines = vec![Vec::new(); tab.buffer.len_lines().max(1)];
    lines[0] = vec![span(0, 7, HighlightKind::Keyword)];
    lines[3] = vec![span(5, 12, HighlightKind::Namespace)];
    tab.set_semantic_highlight(0, lines);
    assert!(tab.semantic_highlight.is_some());

    // Simulate formatter output: same line count, different indentation + reordered imports.
    let start_char = tab.buffer.rope().line_to_char(2);
    let end_char = tab.buffer.rope().len_chars();
    let op = tab.buffer.replace_range_op_adjust_cursor(
        start_char,
        end_char,
        "import (\n\t\"\"\n\t\"context\"\n)\n",
        OpId::root(),
    );

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert_eq!(
        tab.semantic_highlight_line(0).unwrap_or_default(),
        &[span(0, 7, HighlightKind::Keyword)]
    );
    assert!(tab
        .semantic_highlight_line(3)
        .is_some_and(|spans| spans.is_empty()));
    assert!(tab
        .semantic_highlight_line(4)
        .is_some_and(|spans| spans.is_empty()));
    assert!(tab.pending_semantic_highlight.is_none());
}

#[test]
fn invalidate_semantic_highlight_clears_only_replaced_line_on_single_line_replace() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "package test\n\nimport (\n\t\"context\"\n)\n",
        &config,
    );

    let mut lines = vec![Vec::new(); tab.buffer.len_lines().max(1)];
    lines[0] = vec![span(0, 7, HighlightKind::Keyword)];
    lines[3] = vec![span(2, 9, HighlightKind::Namespace)];
    lines[4] = vec![span(0, 1, HighlightKind::Keyword)];
    tab.set_semantic_highlight(0, lines);
    assert!(tab.semantic_highlight.is_some());

    let line_start = tab.buffer.rope().line_to_char(3);
    let line_end = tab.buffer.rope().line_to_char(4);
    let op = tab.buffer.replace_range_op_adjust_cursor(
        line_start,
        line_end,
        "\t\"fmt\"\n",
        OpId::root(),
    );

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert_eq!(
        tab.semantic_highlight_line(0).unwrap_or_default(),
        &[span(0, 7, HighlightKind::Keyword)]
    );
    assert!(tab
        .semantic_highlight_line(3)
        .is_some_and(|spans| spans.is_empty()));
    assert_eq!(
        tab.semantic_highlight_line(4).unwrap_or_default(),
        &[span(0, 1, HighlightKind::Keyword)]
    );
    assert!(tab.pending_semantic_highlight.is_none());
}

#[test]
fn invalidate_semantic_highlight_keeps_on_single_line_pure_insert() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "package main\nfunc main() {}\n",
        &config,
    );

    tab.set_semantic_highlight(
        0,
        vec![
            vec![span(0, 7, HighlightKind::Keyword)],
            vec![span(0, 4, HighlightKind::Keyword)],
        ],
    );
    assert!(tab.semantic_highlight.is_some());

    let insert_at = tab.buffer.rope().line_to_char(1) + 4;
    let op = tab
        .buffer
        .replace_range_op_adjust_cursor(insert_at, insert_at, "x", OpId::root());

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert!(tab.pending_semantic_highlight.is_none());
}

#[test]
fn invalidate_semantic_highlight_keeps_on_single_line_replace_without_newline() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "let con = 1;\n",
        &config,
    );

    let mut lines = vec![Vec::new(); tab.buffer.len_lines().max(1)];
    lines[0] = vec![span(4, 7, HighlightKind::Namespace)];
    tab.set_semantic_highlight(0, lines);
    assert!(tab.semantic_highlight.is_some());

    let start = tab.buffer.rope().line_to_char(0) + 4;
    let end = start + 3;
    let op = tab
        .buffer
        .replace_range_op_adjust_cursor(start, end, "context", OpId::root());

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert_eq!(
        tab.semantic_highlight_line(0).unwrap_or_default(),
        &[span(4, 11, HighlightKind::Namespace)]
    );
    assert!(tab.pending_semantic_highlight.is_none());
}

#[test]
fn syntax_cache_keeps_stale_go_string_span_until_async_patch() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "package tools\n\nimport (\n    \"context\"\n    \"\"\n)\n",
        &config,
    );

    let total_lines = tab.buffer.len_lines().max(1);
    let mut lines = vec![Vec::new(); total_lines];
    // Tree-sitter string span before formatting (`    \"context\"`).
    lines[3] = vec![span(5, 14, HighlightKind::String)];
    tab.syntax_highlight_cache
        .as_mut()
        .expect("go file has syntax cache")
        .apply_patch(0, lines);

    let start_char = tab.buffer.rope().line_to_char(2);
    let end_char = tab.buffer.rope().len_chars();
    let op = tab.buffer.replace_range_op_adjust_cursor(
        start_char,
        end_char,
        "import (\n\t\"\"\n\t\"context\"\n)\n",
        OpId::root(),
    );
    tab.apply_syntax_edit(&op);

    let cache = tab
        .syntax_highlight_cache
        .as_ref()
        .expect("syntax cache remains present");

    // After edit, this line is marked dirty and waiting for async syntax recompute.
    assert!(cache
        .dirty_segments()
        .iter()
        .any(|(start, end)| *start <= 3 && 3 < *end));

    // Cache still holds old offsets until patch arrives.
    assert_eq!(
        cache.line(3).expect("line cached").as_ref(),
        &[span(5, 14, HighlightKind::String)]
    );
    // New text is `\t\"context\"`, so correct span would start at 2.
    assert_ne!(
        cache.line(3).expect("line cached").as_ref(),
        &[span(2, 11, HighlightKind::String)]
    );
}

#[test]
fn highlight_lines_shared_overlays_opaque_spans_on_dirty_lines() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "package tools\n\nimport (\n    \"context\"\n)\n",
        &config,
    );

    let total_lines = tab.buffer.len_lines().max(1);
    let mut lines = vec![Vec::new(); total_lines];
    lines[3] = vec![span(4, 13, HighlightKind::String)];
    tab.syntax_highlight_cache
        .as_mut()
        .expect("go file has syntax cache")
        .apply_patch(0, lines);

    let line_start = tab.buffer.rope().line_to_char(3);
    let line_end = tab.buffer.rope().line_to_char(4);
    let op = tab.buffer.replace_range_op_adjust_cursor(
        line_start,
        line_end,
        "\t\"context\"\n",
        OpId::root(),
    );
    tab.apply_syntax_edit(&op);

    let cache = tab
        .syntax_highlight_cache
        .as_ref()
        .expect("syntax cache remains present");
    assert_eq!(
        cache.line(3).expect("line cached").as_ref(),
        &[span(4, 13, HighlightKind::String)]
    );

    let rendered = tab.highlight_lines_shared(3, 4).expect("syntax available");
    assert_eq!(rendered[0].as_ref(), &[span(1, 10, HighlightKind::String)]);
}
