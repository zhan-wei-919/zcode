use super::super::syntax::{HighlightKind, HighlightSpan};
use super::*;

fn span(start: usize, end: usize, kind: HighlightKind) -> HighlightSpan {
    HighlightSpan { start, end, kind }
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
fn highlight_lines_shared_uses_full_fallback_when_dirty_line_cache_is_empty() {
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
    assert!(rendered[0].iter().any(|span| {
        matches!(
            span.kind,
            HighlightKind::Keyword | HighlightKind::KeywordControl
        ) && span.start <= 1
            && 1 < span.end
    }));
}

#[test]
fn highlight_lines_shared_uses_full_fallback_before_async_patch_when_cache_is_empty() {
    use crate::kernel::editor::compute_highlight_patches;
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "u\n", &config);

    let total_lines = tab.buffer.len_lines().max(1);
    let mut lines = vec![Vec::new(); total_lines];
    lines[0] = vec![span(0, 1, HighlightKind::Variable)];
    tab.syntax_highlight_cache
        .as_mut()
        .expect("rust file has syntax cache")
        .apply_patch(0, lines);

    let line_start = tab.buffer.rope().line_to_char(0);
    let line_end = tab.buffer.rope().line_to_char(1);
    let op = tab.buffer.replace_range_op_adjust_cursor(
        line_start,
        line_end,
        "use crossterm\n",
        OpId::root(),
    );
    tab.apply_syntax_edit(&op);

    let stale = tab.highlight_lines_shared(0, 1).expect("syntax available");
    let line = tab.buffer.rope().line(0).to_string();
    let use_idx = line.find("use").expect("use token");
    assert!(stale[0].iter().any(|span| {
        matches!(
            span.kind,
            HighlightKind::Keyword | HighlightKind::KeywordControl
        ) && span.start <= use_idx
            && use_idx < span.end
    }));

    let syntax = tab.syntax().expect("rust syntax available");
    let patches = compute_highlight_patches(
        syntax.language(),
        syntax.tree(),
        tab.buffer.rope(),
        &[(0, 1)],
    );
    let cache = tab
        .syntax_highlight_cache
        .as_mut()
        .expect("syntax cache remains present");
    for patch in patches {
        cache.apply_patch(patch.start_line, patch.lines);
    }

    let refreshed = tab.highlight_lines_shared(0, 1).expect("syntax available");
    assert!(refreshed[0].iter().any(|span| {
        matches!(
            span.kind,
            HighlightKind::Keyword | HighlightKind::KeywordControl
        ) && span.start <= use_idx
            && use_idx < span.end
    }));
}

#[test]
fn highlight_lines_shared_uses_full_fallback_when_dirty_line_keeps_stale_nonopaque_cache() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "value\n", &config);

    let total_lines = tab.buffer.len_lines().max(1);
    let mut lines = vec![Vec::new(); total_lines];
    lines[0] = vec![span(0, 5, HighlightKind::Variable)];
    tab.syntax_highlight_cache
        .as_mut()
        .expect("rust file has syntax cache")
        .apply_patch(0, lines);

    let insert_at = tab.buffer.rope().line_to_char(0);
    let op = tab
        .buffer
        .replace_range_op_adjust_cursor(insert_at, insert_at, "let ", OpId::root());
    tab.apply_syntax_edit(&op);

    let cache = tab
        .syntax_highlight_cache
        .as_ref()
        .expect("syntax cache remains present");
    assert!(cache.is_line_dirty(0));
    assert_eq!(
        cache.line(0).map(|line| line.as_slice()),
        Some(&[span(4, 9, HighlightKind::Variable)][..])
    );

    let rendered = tab.highlight_lines_shared(0, 1).expect("syntax available");
    assert!(rendered[0].iter().any(|span| {
        matches!(
            span.kind,
            HighlightKind::Keyword | HighlightKind::KeywordControl
        ) && span.start == 0
            && 0 < span.end
    }));
    assert!(rendered[0].iter().any(|span| {
        matches!(
            span.kind,
            HighlightKind::Variable | HighlightKind::Parameter
        ) && span.start <= 4
            && 4 < span.end
    }));
}

#[test]
fn identifier_pos_at_is_exact_while_identifier_pos_at_or_before_backtracks() {
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    let config = EditorConfig::default();
    let tab = EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "foo \n", &config);

    assert_eq!(tab.identifier_pos_at((0, 0)), Some((0, 0)));
    assert_eq!(tab.identifier_pos_at((0, 2)), Some((0, 2)));
    assert_eq!(tab.identifier_pos_at((0, 3)), None);

    assert_eq!(tab.identifier_pos_at_or_before((0, 3)), Some((0, 2)));
}
