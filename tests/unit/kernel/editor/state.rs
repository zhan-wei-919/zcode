use super::super::syntax::{HighlightKind, HighlightSpan, SemanticToken};
use super::*;

fn span(start: usize, end: usize, kind: HighlightKind) -> HighlightSpan {
    HighlightSpan { start, end, kind }
}

fn sem_tok(text: &str, kind: Option<HighlightKind>) -> SemanticToken {
    SemanticToken {
        text: text.into(),
        semantic_kind: kind,
    }
}

fn make_lines(start: usize, len: usize) -> Vec<Vec<SemanticToken>> {
    (0..len)
        .map(|i| {
            vec![sem_tok(
                &format!("L{}", start.saturating_add(i)),
                Some(HighlightKind::Keyword),
            )]
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

    let mut expected: Vec<Vec<SemanticToken>> = Vec::new();
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

    let mut expected: Vec<Vec<SemanticToken>> = Vec::new();
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

    let mut expected: Vec<Vec<SemanticToken>> = Vec::new();
    expected.extend(make_lines(0, 2));
    expected.extend(make_lines(100, 3));
    expected.extend(make_lines(5, 2));

    assert_eq!(state.segments.len(), 1);
    assert_eq!(state.segments[0].start_line, 0);
    assert_eq!(state.segments[0].lines, expected);
}

fn has_semantic_kind(tokens: &[SemanticToken]) -> bool {
    tokens.iter().any(|t| t.semantic_kind.is_some())
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
            vec![
                sem_tok("package", Some(HighlightKind::Keyword)),
                sem_tok(" main", None),
            ],
            vec![
                sem_tok("func", Some(HighlightKind::Keyword)),
                sem_tok(" main() {}", None),
            ],
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
        tab.semantic_tokens_line(0).unwrap_or_default(),
        &[
            sem_tok("package", Some(HighlightKind::Keyword)),
            sem_tok(" main", None)
        ]
    );
    assert!(tab
        .semantic_tokens_line(1)
        .is_some_and(|tokens| !has_semantic_kind(tokens)));
    assert_eq!(
        tab.semantic_tokens_line(2).unwrap_or_default(),
        &[
            sem_tok("func", Some(HighlightKind::Keyword)),
            sem_tok(" main() {}", None)
        ]
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
    lines[0] = vec![
        sem_tok("package", Some(HighlightKind::Keyword)),
        sem_tok(" tools", None),
    ];
    lines[1] = Vec::new();
    lines[2] = vec![sem_tok("import (", None)];
    lines[3] = vec![
        sem_tok("    \"", None),
        sem_tok("context", Some(HighlightKind::Namespace)),
        sem_tok("\"", None),
    ];
    lines[4] = vec![sem_tok("    \"\"", None)];
    lines[5] = vec![sem_tok(")", None)];
    lines[6] = Vec::new();
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
        tab.semantic_tokens_line(0).unwrap_or_default(),
        &[
            sem_tok("package", Some(HighlightKind::Keyword)),
            sem_tok(" tools", None)
        ]
    );
    assert!(tab
        .semantic_tokens_line(3)
        .is_some_and(|tokens| !has_semantic_kind(tokens)));
    assert!(tab
        .semantic_tokens_line(4)
        .is_some_and(|tokens| !has_semantic_kind(tokens)));
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
    lines[0] = vec![
        sem_tok("package", Some(HighlightKind::Keyword)),
        sem_tok(" test", None),
    ];
    lines[1] = Vec::new();
    lines[2] = vec![sem_tok("import (", None)];
    lines[3] = vec![
        sem_tok("\t\"", None),
        sem_tok("context", Some(HighlightKind::Namespace)),
        sem_tok("\"", None),
    ];
    lines[4] = vec![sem_tok(")", Some(HighlightKind::Keyword))];
    lines[5] = Vec::new();
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
        tab.semantic_tokens_line(0).unwrap_or_default(),
        &[
            sem_tok("package", Some(HighlightKind::Keyword)),
            sem_tok(" test", None)
        ]
    );
    assert!(tab
        .semantic_tokens_line(3)
        .is_some_and(|tokens| !has_semantic_kind(tokens)));
    assert_eq!(
        tab.semantic_tokens_line(4).unwrap_or_default(),
        &[sem_tok(")", Some(HighlightKind::Keyword))]
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
            vec![
                sem_tok("package", Some(HighlightKind::Keyword)),
                sem_tok(" main", None),
            ],
            vec![
                sem_tok("func", Some(HighlightKind::Keyword)),
                sem_tok(" main() {}", None),
            ],
        ],
    );
    assert!(tab.semantic_highlight.is_some());

    let insert_at = tab.buffer.rope().line_to_char(1) + 4;
    let op = tab
        .buffer
        .replace_range_op_adjust_cursor(insert_at, insert_at, "x", OpId::root());

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert!(tab.semantic_tokens_line(1).is_some_and(has_semantic_kind));
    assert!(tab.pending_semantic_highlight.is_none());
}

#[test]
fn invalidate_semantic_highlight_clears_touched_token_on_single_line_replace_without_newline() {
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
    lines[0] = vec![
        sem_tok("let ", None),
        sem_tok("con", Some(HighlightKind::Namespace)),
        sem_tok(" = 1;", None),
    ];
    tab.set_semantic_highlight(0, lines);
    assert!(tab.semantic_highlight.is_some());

    let start = tab.buffer.rope().line_to_char(0) + 4;
    let end = start + 3;
    let op = tab
        .buffer
        .replace_range_op_adjust_cursor(start, end, "context", OpId::root());

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert!(tab
        .semantic_tokens_line(0)
        .is_some_and(|tokens| !has_semantic_kind(tokens)));
    assert!(tab.pending_semantic_highlight.is_none());
}

#[test]
fn invalidate_semantic_highlight_clears_touched_token_on_mid_token_inserts() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "String\n", &config);

    tab.set_semantic_highlight(0, vec![vec![sem_tok("String", Some(HighlightKind::Type))]]);

    let insert_at = tab.buffer.rope().line_to_char(0) + 3;
    let op = tab
        .buffer
        .replace_range_op_adjust_cursor(insert_at, insert_at, "_", OpId::root());

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert_eq!(
        tab.semantic_tokens_line(0).unwrap_or_default(),
        &[sem_tok("Str_ing", None)]
    );
}

#[test]
fn invalidate_semantic_highlight_clears_touched_token_on_mid_token_newline_inserts() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "foobar\n", &config);

    tab.set_semantic_highlight(
        0,
        vec![vec![sem_tok("foobar", Some(HighlightKind::Function))]],
    );

    let insert_at = tab.buffer.rope().line_to_char(0) + 3;
    let op = tab
        .buffer
        .replace_range_op_adjust_cursor(insert_at, insert_at, "\n", OpId::root());

    tab.invalidate_semantic_highlight_on_edit(&op);
    assert!(tab.semantic_highlight.is_some());
    assert_eq!(
        tab.semantic_tokens_line(0).unwrap_or_default(),
        &[sem_tok("foo", None)]
    );
    assert_eq!(
        tab.semantic_tokens_line(1).unwrap_or_default(),
        &[sem_tok("bar", None)]
    );
}

#[test]
fn syntax_cache_clears_multiline_edit_spans_until_async_patch() {
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

    // Cache drops potentially-misaligned spans for multiline edits until patch arrives.
    assert!(cache.line(3).is_none());
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
    assert!(cache.line(3).is_none());

    let rendered = tab.highlight_lines_shared(3, 4).expect("syntax available");
    assert_eq!(rendered[0].as_ref(), &[span(1, 10, HighlightKind::String)]);
}

#[test]
fn highlight_lines_shared_drops_cached_spans_on_multiline_same_line_count_edit() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "package p\n\nfunc f() {\n    if true {\n        return\n    }\n}\n",
        &config,
    );

    let total_lines = tab.buffer.len_lines().max(1);
    let mut lines = vec![Vec::new(); total_lines];
    // "    if" → keyword span for `if` at [4,6).
    lines[3] = vec![span(4, 6, HighlightKind::Keyword)];
    tab.syntax_highlight_cache
        .as_mut()
        .expect("go file has syntax cache")
        .apply_patch(0, lines);

    // Simulate formatter output: same line count but indentation changes across multiple lines.
    let start_char = tab.buffer.rope().line_to_char(2);
    let end_char = tab.buffer.rope().len_chars();
    let op = tab.buffer.replace_range_op_adjust_cursor(
        start_char,
        end_char,
        "func f() {\n\tif true {\n\t\treturn\n\t}\n}\n",
        OpId::root(),
    );
    tab.apply_syntax_edit(&op);

    let cache = tab
        .syntax_highlight_cache
        .as_ref()
        .expect("syntax cache remains present");
    assert!(cache.is_line_dirty(3));

    // Cached spans for dirty lines must not be reused when the edit spans multiple lines.
    assert!(cache.line(3).is_none());
    let rendered = tab.highlight_lines_shared(3, 4).expect("syntax available");
    assert!(rendered[0].is_empty());
}
