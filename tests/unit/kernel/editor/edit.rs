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
