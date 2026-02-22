use super::*;
use crate::kernel::editor::{HighlightKind, HighlightSpan, TabId};
use std::path::PathBuf;

#[test]
fn test_rust_brace_pair_and_electric_enter() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "fn main() ",
        &config,
    );

    let end = tab.buffer.line_grapheme_len(0);
    tab.buffer.set_cursor(0, end);

    let _ = tab.apply_command(Command::InsertChar('{'), 0, &config);
    assert_eq!(tab.buffer.text(), "fn main() {}");
    assert_eq!(tab.buffer.cursor(), (0, "fn main() {".len()));

    let _ = tab.apply_command(Command::InsertNewline, 0, &config);
    assert_eq!(tab.buffer.text(), "fn main() {\n    \n}");
    assert_eq!(tab.buffer.cursor(), (1, 4));
}

#[test]
fn test_electric_enter_with_whitespace_between_braces() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "fn main() ",
        &config,
    );

    let end = tab.buffer.line_grapheme_len(0);
    tab.buffer.set_cursor(0, end);
    let _ = tab.apply_command(Command::InsertChar('{'), 0, &config);

    let _ = tab.apply_command(Command::InsertChar(' '), 0, &config);
    let _ = tab.apply_command(Command::InsertChar(' '), 0, &config);
    assert_eq!(tab.buffer.text(), "fn main() {  }");

    let _ = tab.apply_command(Command::InsertNewline, 0, &config);
    assert_eq!(tab.buffer.text(), "fn main() {\n    \n}");
    assert_eq!(tab.buffer.cursor(), (1, 4));
}

#[test]
fn test_go_paren_pair_and_electric_enter() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.go"),
        "import ()",
        &config,
    );

    tab.buffer.set_cursor(0, "import (".len());

    let _ = tab.apply_command(Command::InsertNewline, 0, &config);
    assert_eq!(tab.buffer.text(), "import (\n    \n)");
    assert_eq!(tab.buffer.cursor(), (1, 4));
}

#[test]
fn test_replace_is_undoable() {
    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.txt"), "foo foo", &config);

    let m = Match::new(0, 3, 0, 0);
    assert!(tab.replace_current_match(&m, "bar", config.tab_size));
    assert_eq!(tab.buffer.text(), "bar foo");

    let (changed, _) = tab.apply_command(Command::Undo, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "foo foo");
}

#[test]
fn test_replace_with_empty_string_deletes_and_undo() {
    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.txt"), "foo foo", &config);

    let m = Match::new(4, 7, 0, 4);
    assert!(tab.replace_current_match(&m, "", config.tab_size));
    assert_eq!(tab.buffer.text(), "foo ");

    let (changed, _) = tab.apply_command(Command::Undo, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "foo foo");
}

#[test]
fn test_paste_over_selection_single_undo() {
    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.txt"), "abc", &config);

    tab.buffer.set_cursor(0, 1);
    tab.buffer
        .set_selection(Some(Selection::new((0, 1), Granularity::Char)));
    tab.buffer.update_selection_cursor((0, 2));

    assert!(tab.insert_text("X", config.tab_size));
    assert_eq!(tab.buffer.text(), "aXc");

    let (changed, _) = tab.apply_command(Command::Undo, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "abc");
}

#[test]
fn test_cursor_left_does_not_extend_empty_char_selection() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "out = out.bg(to_ratatui_color(bg));",
        &config,
    );

    let end = tab.buffer.line_grapheme_len(0);
    assert!(tab.place_cursor(0, end, crate::models::Granularity::Char, config.tab_size));
    assert!(tab.end_selection_gesture());
    assert!(
        tab.buffer.selection().is_none(),
        "single click should not keep empty char selection"
    );

    let (changed, _) = tab.apply_command(Command::CursorLeft, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.cursor(), (0, end - 1));
    assert!(
        tab.buffer.selection().is_none(),
        "plain cursor move should clear empty char selection"
    );
}

#[test]
fn test_cursor_left_clears_empty_selection_created_by_shift_right_at_line_end() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "println!(\"{}\", self.payload);",
        &config,
    );

    let end = tab.buffer.line_grapheme_len(0);
    tab.buffer.set_cursor(0, end);

    let (changed, _) = tab.apply_command(Command::ExtendSelectionRight, 0, &config);
    assert!(!changed, "at line end, shift+right should not move cursor");
    assert!(
        tab.buffer
            .selection()
            .is_some_and(|selection| selection.is_empty()),
        "boundary shift selection currently leaves an empty selection marker"
    );

    let (changed, _) = tab.apply_command(Command::CursorLeft, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.cursor(), (0, end - 1));
    assert!(
        tab.buffer.selection().is_none(),
        "plain left should clear residual empty selection instead of extending it"
    );
}

#[test]
fn test_cursor_down_clears_empty_selection_created_by_shift_up_at_file_start() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "first\nsecond",
        &config,
    );

    tab.buffer.set_cursor(0, 0);

    let (changed, _) = tab.apply_command(Command::ExtendSelectionUp, 0, &config);
    assert!(!changed, "at file start, shift+up should not move cursor");
    assert!(
        tab.buffer
            .selection()
            .is_some_and(|selection| selection.is_empty()),
        "boundary shift selection currently leaves an empty selection marker"
    );

    let (changed, _) = tab.apply_command(Command::CursorDown, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.cursor(), (1, 0));
    assert!(
        tab.buffer.selection().is_none(),
        "plain down should clear residual empty selection instead of extending it"
    );
}

#[test]
fn test_cursor_up_restores_original_column_after_visiting_shorter_line() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "pub fn take_log_rx(&mut self) -> Option<Receiver<String>> {\nself.log_rx.take()",
        &config,
    );

    let first_line_end = tab.buffer.line_grapheme_len(0);
    let second_line_end = tab.buffer.line_grapheme_len(1);
    assert!(first_line_end > second_line_end);

    tab.buffer.set_cursor(0, first_line_end);

    let (changed, _) = tab.apply_command(Command::CursorDown, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.cursor(), (1, second_line_end));

    let (changed, _) = tab.apply_command(Command::CursorUp, 0, &config);
    assert!(changed);
    assert_eq!(
        tab.buffer.cursor(),
        (0, first_line_end),
        "cursor should return to original long-line column"
    );
}

#[test]
fn test_cursor_up_restores_original_column_after_multiple_short_lines() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "very very very long first line\nshort\ns",
        &config,
    );

    let long_col = tab.buffer.line_grapheme_len(0);
    tab.buffer.set_cursor(0, long_col);

    let _ = tab.apply_command(Command::CursorDown, 0, &config);
    let _ = tab.apply_command(Command::CursorDown, 0, &config);
    assert_eq!(tab.buffer.cursor(), (2, 1));

    let _ = tab.apply_command(Command::CursorUp, 0, &config);
    assert_eq!(tab.buffer.cursor(), (1, 5));
    let _ = tab.apply_command(Command::CursorUp, 0, &config);
    assert_eq!(tab.buffer.cursor(), (0, long_col));
}

#[test]
fn test_horizontal_move_resets_vertical_goal_column() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "abcdefghij\nxy",
        &config,
    );

    tab.buffer.set_cursor(0, 10);
    let _ = tab.apply_command(Command::CursorDown, 0, &config);
    assert_eq!(tab.buffer.cursor(), (1, 2));

    let _ = tab.apply_command(Command::CursorLeft, 0, &config);
    assert_eq!(tab.buffer.cursor(), (1, 1));

    let _ = tab.apply_command(Command::CursorUp, 0, &config);
    assert_eq!(
        tab.buffer.cursor(),
        (0, 1),
        "after horizontal move, up should use current column instead of stale goal"
    );
}

#[test]
fn test_auto_pair_and_skip_closing() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "", &config);

    let _ = tab.apply_command(Command::InsertChar('('), 0, &config);
    assert_eq!(tab.buffer.text(), "()");
    assert_eq!(tab.buffer.cursor(), (0, 1));

    let _ = tab.apply_command(Command::InsertChar(')'), 0, &config);
    assert_eq!(tab.buffer.text(), "()");
    assert_eq!(tab.buffer.cursor(), (0, 2));

    tab = EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "", &config);
    let _ = tab.apply_command(Command::InsertChar('"'), 0, &config);
    assert_eq!(tab.buffer.text(), "\"\"");
    assert_eq!(tab.buffer.cursor(), (0, 1));

    let _ = tab.apply_command(Command::InsertChar('"'), 0, &config);
    assert_eq!(tab.buffer.text(), "\"\"");
    assert_eq!(tab.buffer.cursor(), (0, 2));

    tab = EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "", &config);
    let _ = tab.apply_command(Command::InsertChar('\''), 0, &config);
    assert_eq!(tab.buffer.text(), "''");
    assert_eq!(tab.buffer.cursor(), (0, 1));

    let _ = tab.apply_command(Command::InsertChar('\''), 0, &config);
    assert_eq!(tab.buffer.text(), "''");
    assert_eq!(tab.buffer.cursor(), (0, 2));
}

#[test]
fn test_c_auto_pair_and_electric_enter() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.c"),
        "int main() ",
        &config,
    );

    let end = tab.buffer.line_grapheme_len(0);
    tab.buffer.set_cursor(0, end);

    let _ = tab.apply_command(Command::InsertChar('{'), 0, &config);
    assert_eq!(tab.buffer.text(), "int main() {}");
    assert_eq!(tab.buffer.cursor(), (0, "int main() {".len()));

    let _ = tab.apply_command(Command::InsertNewline, 0, &config);
    assert_eq!(tab.buffer.text(), "int main() {\n    \n}");
    assert_eq!(tab.buffer.cursor(), (1, 4));
}

#[test]
fn test_python_auto_pair_and_colon_indent() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(TabId::new(1), PathBuf::from("test.py"), "", &config);

    let _ = tab.apply_command(Command::InsertChar('('), 0, &config);
    assert_eq!(tab.buffer.text(), "()");
    assert_eq!(tab.buffer.cursor(), (0, 1));

    let mut tab = EditorTabState::from_file(
        TabId::new(2),
        PathBuf::from("test.py"),
        "if value:",
        &config,
    );
    tab.buffer.set_cursor(0, "if value:".len());

    let _ = tab.apply_command(Command::InsertNewline, 0, &config);
    assert_eq!(tab.buffer.text(), "if value:\n    ");
    assert_eq!(tab.buffer.cursor(), (1, 4));
}

#[test]
fn multi_cursor_insert_delete_and_undo_restore_all_cursors() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "abc\nabc\nabc",
        &config,
    );

    tab.buffer.set_cursor(0, 1);
    tab.secondary_cursors = vec![
        SecondaryCursor {
            pos: (1, 1),
            selection: None,
            goal_col: None,
        },
        SecondaryCursor {
            pos: (2, 1),
            selection: None,
            goal_col: None,
        },
    ];

    let (changed, _) = tab.apply_command(Command::InsertChar('X'), 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "aXbc\naXbc\naXbc");
    assert_eq!(tab.buffer.cursor(), (0, 2));
    assert_eq!(
        tab.secondary_cursors
            .iter()
            .map(|c| c.pos)
            .collect::<Vec<_>>(),
        vec![(1, 2), (2, 2)]
    );

    let (changed, _) = tab.apply_command(Command::DeleteBackward, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "abc\nabc\nabc");
    assert_eq!(tab.buffer.cursor(), (0, 1));
    assert_eq!(
        tab.secondary_cursors
            .iter()
            .map(|c| c.pos)
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 1)]
    );

    let (changed, _) = tab.apply_command(Command::Undo, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "aXbc\naXbc\naXbc");
    assert_eq!(tab.buffer.cursor(), (0, 2));
    assert_eq!(
        tab.secondary_cursors
            .iter()
            .map(|c| c.pos)
            .collect::<Vec<_>>(),
        vec![(1, 2), (2, 2)]
    );

    let (changed, _) = tab.apply_command(Command::Undo, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "abc\nabc\nabc");
    assert_eq!(tab.buffer.cursor(), (0, 1));
    assert_eq!(
        tab.secondary_cursors
            .iter()
            .map(|c| c.pos)
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 1)]
    );
}

#[test]
fn multi_cursor_insert_adjusts_later_cursor_on_same_line() {
    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.txt"), "abcdef", &config);

    tab.buffer.set_cursor(0, 1); // after 'a'
    tab.secondary_cursors = vec![SecondaryCursor {
        pos: (0, 4), // after 'd'
        selection: None,
        goal_col: None,
    }];

    let (changed, _) = tab.apply_command(Command::InsertChar('X'), 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "aXbcdXef");
    assert_eq!(tab.buffer.cursor(), (0, 2));
    assert_eq!(tab.secondary_cursors[0].pos, (0, 6));
}

#[test]
fn add_cursor_above_and_below_add_for_each_existing_cursor() {
    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.txt"), "a\nb\nc", &config);

    tab.buffer.set_cursor(1, 0);
    assert!(tab.secondary_cursors.is_empty());

    let (changed, _) = tab.apply_command(Command::AddCursorAbove, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.cursor(), (0, 0));
    assert_eq!(
        tab.secondary_cursors
            .iter()
            .map(|c| c.pos)
            .collect::<Vec<_>>(),
        vec![(1, 0)]
    );

    let (changed, _) = tab.apply_command(Command::AddCursorBelow, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.cursor(), (1, 0));
    assert_eq!(
        tab.secondary_cursors
            .iter()
            .map(|c| c.pos)
            .collect::<Vec<_>>(),
        vec![(0, 0), (2, 0)]
    );
}

#[test]
fn add_cursor_at_next_match_selects_word_then_adds_next_occurrence() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "foo foo foo",
        &config,
    );

    tab.buffer.set_cursor(0, 0);
    assert!(tab.buffer.selection().is_none());

    let (changed, _) = tab.apply_command(Command::AddCursorAtNextMatch, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.get_selection_text().as_deref(), Some("foo"));
    assert!(tab.secondary_cursors.is_empty());

    let (changed, _) = tab.apply_command(Command::AddCursorAtNextMatch, 0, &config);
    assert!(changed);
    assert_eq!(tab.secondary_cursors.len(), 1);
    assert_eq!(tab.buffer.get_selection_text().as_deref(), Some("foo"));
    assert_eq!(tab.buffer.cursor(), (0, 7)); // end of second match

    let secondary = &tab.secondary_cursors[0];
    assert_eq!(secondary.pos, (0, 3)); // end of first match
    assert_eq!(
        secondary.selection.as_ref().and_then(|s| {
            (!s.is_empty()).then(|| {
                tab.buffer
                    .rope()
                    .slice(tab.buffer.pos_to_char(s.range().0)..tab.buffer.pos_to_char(s.range().1))
                    .to_string()
            })
        }),
        Some("foo".to_string())
    );
}

#[test]
fn add_cursor_at_all_matches_selects_all_occurrences() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "foo foo foo",
        &config,
    );

    tab.buffer.set_cursor(0, 0);
    let (changed, _) = tab.apply_command(Command::AddCursorAtAllMatches, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.get_selection_text().as_deref(), Some("foo"));
    assert_eq!(tab.buffer.cursor(), (0, 3));
    assert_eq!(
        tab.secondary_cursors
            .iter()
            .map(|c| c.pos)
            .collect::<Vec<_>>(),
        vec![(0, 7), (0, 11)]
    );
}

#[test]
fn multi_cursor_copy_joins_selections_in_order() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "foo bar foo",
        &config,
    );

    let mut primary_sel = Selection::new((0, 0), Granularity::Char);
    primary_sel.update_cursor((0, 3), tab.buffer.rope());
    tab.buffer.set_selection(Some(primary_sel));
    tab.buffer.set_cursor(0, 3);

    let mut secondary_sel = Selection::new((0, 8), Granularity::Char);
    secondary_sel.update_cursor((0, 11), tab.buffer.rope());
    tab.secondary_cursors = vec![SecondaryCursor {
        pos: (0, 11),
        selection: Some(secondary_sel),
        goal_col: None,
    }];

    let (_changed, effects) = tab.apply_command(Command::Copy, 0, &config);
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        crate::kernel::Effect::SetClipboardText(text) => assert_eq!(text, "foo\nfoo"),
        other => panic!("unexpected effect: {other:?}"),
    }
}

#[test]
fn multi_cursor_cut_deletes_all_selections_single_undo() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "foo bar foo",
        &config,
    );

    let mut primary_sel = Selection::new((0, 0), Granularity::Char);
    primary_sel.update_cursor((0, 3), tab.buffer.rope());
    tab.buffer.set_selection(Some(primary_sel));
    tab.buffer.set_cursor(0, 3);

    let mut secondary_sel = Selection::new((0, 8), Granularity::Char);
    secondary_sel.update_cursor((0, 11), tab.buffer.rope());
    tab.secondary_cursors = vec![SecondaryCursor {
        pos: (0, 11),
        selection: Some(secondary_sel),
        goal_col: None,
    }];

    let (changed, effects) = tab.apply_command(Command::Cut, 0, &config);
    assert!(changed);
    assert_eq!(effects.len(), 1);
    assert_eq!(tab.buffer.text(), " bar ");

    let (changed, _) = tab.apply_command(Command::Undo, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "foo bar foo");
}

#[test]
fn multi_cursor_paste_distributes_lines() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "one\ntwo\nthree",
        &config,
    );

    tab.buffer.set_cursor(0, 0);
    tab.secondary_cursors = vec![
        SecondaryCursor {
            pos: (1, 0),
            selection: None,
            goal_col: None,
        },
        SecondaryCursor {
            pos: (2, 0),
            selection: None,
            goal_col: None,
        },
    ];

    assert!(tab.insert_text("A\nB\nC", config.tab_size));
    assert_eq!(tab.buffer.text(), "Aone\nBtwo\nCthree");

    let (changed, _) = tab.apply_command(Command::Undo, 0, &config);
    assert!(changed);
    assert_eq!(tab.buffer.text(), "one\ntwo\nthree");
}

#[test]
fn semantic_highlight_and_inlay_hints_do_not_flicker_on_edit() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "fn main() {}",
        &config,
    );

    tab.set_semantic_highlight(
        0,
        vec![vec![HighlightSpan {
            start: 0,
            end: 2,
            kind: HighlightKind::Keyword,
        }]],
    );
    tab.set_inlay_hints(0, 0, 1, vec![vec![": hint".to_string()]]);

    assert!(tab.semantic_highlight_line(0).is_some());
    assert!(tab.inlay_hint_line(0).is_some());

    let _ = tab.apply_command(Command::InsertChar('x'), 0, &config);

    assert!(tab.semantic_highlight_line(0).is_some());
    assert!(tab.inlay_hint_line(0).is_some());
}

#[test]
fn semantic_highlight_is_shifted_on_line_edit() {
    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "foo\nbar", &config);
    tab.set_semantic_highlight(
        0,
        vec![
            vec![HighlightSpan {
                start: 0,
                end: 3,
                kind: HighlightKind::Function,
            }],
            vec![HighlightSpan {
                start: 0,
                end: 3,
                kind: HighlightKind::Macro,
            }],
        ],
    );

    tab.buffer.set_cursor(0, tab.buffer.line_grapheme_len(0));
    let _ = tab.apply_command(Command::InsertChar('x'), 0, &config);

    assert_eq!(
        tab.semantic_highlight_line(0).unwrap_or_default(),
        &[HighlightSpan {
            start: 0,
            end: 3,
            kind: HighlightKind::Function
        }]
    );
    assert!(tab
        .semantic_highlight_line(1)
        .is_some_and(|spans| !spans.is_empty()));
}

#[test]
fn semantic_highlight_keeps_existing_lines_on_newline_edit() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "foo\nbar\nbaz",
        &config,
    );
    tab.set_semantic_highlight(
        0,
        vec![
            vec![HighlightSpan {
                start: 0,
                end: 3,
                kind: HighlightKind::Function,
            }],
            vec![HighlightSpan {
                start: 0,
                end: 3,
                kind: HighlightKind::Macro,
            }],
            vec![HighlightSpan {
                start: 0,
                end: 3,
                kind: HighlightKind::Type,
            }],
        ],
    );

    tab.buffer.set_cursor(0, tab.buffer.line_grapheme_len(0));
    let _ = tab.apply_command(Command::InsertNewline, 0, &config);

    assert!(tab
        .semantic_highlight_line(0)
        .is_some_and(|spans| !spans.is_empty()));
    assert!(tab
        .semantic_highlight_line(1)
        .is_some_and(|spans| spans.is_empty()));
    assert!(tab
        .semantic_highlight_line(2)
        .is_some_and(|spans| !spans.is_empty()));
}

#[test]
fn semantic_highlight_is_not_invalidated_when_appending_punctuation() {
    let config = EditorConfig::default();
    let mut tab =
        EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), "String", &config);
    tab.set_semantic_highlight(
        0,
        vec![vec![HighlightSpan {
            start: 0,
            end: 6,
            kind: HighlightKind::Type,
        }]],
    );

    tab.buffer.set_cursor(0, tab.buffer.line_grapheme_len(0));
    let _ = tab.apply_command(Command::InsertChar(':'), 0, &config);

    assert_eq!(
        tab.semantic_highlight_line(0).unwrap_or_default(),
        &[HighlightSpan {
            start: 0,
            end: 6,
            kind: HighlightKind::Type
        }]
    );
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }

    fn gen_range(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u32() as usize) % upper
    }
}

fn assert_cursor_invariants(tab: &EditorTabState) {
    let (row, col) = tab.buffer.cursor();
    let total_lines = tab.buffer.len_lines().max(1);
    assert!(row < total_lines);
    assert!(col <= tab.buffer.line_grapheme_len(row));
}

fn random_insert_char(rng: &mut Rng) -> char {
    match rng.gen_range(36) {
        0..=25 => (b'a' + rng.gen_range(26) as u8) as char,
        26 => ' ',
        27 => 'é',
        28 => '你',
        29 => '\u{301}',
        30 => '👍',
        31 => '🏽',
        _ => (b'a' + rng.gen_range(26) as u8) as char,
    }
}

#[test]
fn fuzz_editing_undo_redo_roundtrip() {
    let config = EditorConfig {
        auto_indent: false,
        ..Default::default()
    };

    let mut tab = EditorTabState::untitled(TabId::new(1), &config);
    let mut rng = Rng::new(0xC0FFEE);

    const STEPS: usize = 2000;
    for _ in 0..STEPS {
        let cmd = match rng.gen_range(10) {
            0 => Command::InsertChar(random_insert_char(&mut rng)),
            1 => Command::InsertNewline,
            2 => Command::InsertTab,
            3 => Command::DeleteBackward,
            4 => Command::DeleteForward,
            5 => Command::CursorLeft,
            6 => Command::CursorRight,
            7 => Command::CursorUp,
            8 => Command::CursorDown,
            _ => Command::InsertChar(random_insert_char(&mut rng)),
        };
        let _ = tab.apply_command(cmd, 0, &config);
        assert_cursor_invariants(&tab);
    }

    let final_text = tab.buffer.text();
    let final_cursor = tab.buffer.cursor();

    while tab.apply_command(Command::Undo, 0, &config).0 {
        assert_cursor_invariants(&tab);
    }

    while tab.apply_command(Command::Redo, 0, &config).0 {
        assert_cursor_invariants(&tab);
    }

    assert_eq!(tab.buffer.text(), final_text);
    assert_eq!(tab.buffer.cursor(), final_cursor);
}
