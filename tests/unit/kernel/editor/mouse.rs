use crate::kernel::editor::{EditorTabState, TabId};
use crate::kernel::services::ports::EditorConfig;
use crate::models::Granularity;

fn tab_with_content(text: &str, viewport_height: usize) -> EditorTabState {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::untitled(TabId::new(1), &config);
    tab.buffer = crate::models::TextBuffer::from_text(text);
    tab.viewport.height = viewport_height;
    tab.viewport.width = 40;
    tab
}

#[test]
fn place_cursor_sets_position_and_selection() {
    let mut tab = tab_with_content("Hello World\n", 10);
    let tab_size = EditorConfig::default().tab_size;

    tab.place_cursor(0, 5, Granularity::Char, tab_size);
    let (row, col) = tab.buffer.cursor();
    assert_eq!(row, 0);
    assert_eq!(col, 5);
    assert!(tab.buffer.selection().is_some());
}

#[test]
fn place_cursor_word_selects_word() {
    let mut tab = tab_with_content("Hello World\n", 10);
    let tab_size = EditorConfig::default().tab_size;

    tab.place_cursor(0, 1, Granularity::Word, tab_size);
    assert_eq!(tab.buffer.get_selection_text().as_deref(), Some("Hello"));
}

#[test]
fn extend_selection_updates_cursor_and_selection() {
    let mut tab = tab_with_content("Hello World\n", 10);
    let tab_size = EditorConfig::default().tab_size;

    tab.place_cursor(0, 0, Granularity::Char, tab_size);
    tab.extend_selection(0, 5, tab_size);

    let (row, col) = tab.buffer.cursor();
    assert_eq!(row, 0);
    assert_eq!(col, 5);
    assert_eq!(tab.buffer.get_selection_text().as_deref(), Some("Hello"));
}

#[test]
fn end_selection_gesture_clears_empty_char_selection() {
    let mut tab = tab_with_content("Hello World\n", 10);
    let tab_size = EditorConfig::default().tab_size;

    // Place cursor creates a Char selection at (0,0) which is empty
    tab.place_cursor(0, 0, Granularity::Char, tab_size);
    assert!(tab.buffer.selection().is_some());

    tab.end_selection_gesture();
    assert!(tab.buffer.selection().is_none());
}

#[test]
fn end_selection_gesture_keeps_nonempty_selection() {
    let mut tab = tab_with_content("Hello World\n", 10);
    let tab_size = EditorConfig::default().tab_size;

    tab.place_cursor(0, 0, Granularity::Char, tab_size);
    tab.extend_selection(0, 5, tab_size);

    tab.end_selection_gesture();
    // Selection is non-empty, should be kept
    assert!(tab.buffer.selection().is_some());
    assert_eq!(tab.buffer.get_selection_text().as_deref(), Some("Hello"));
}

#[test]
fn end_selection_gesture_keeps_word_selection() {
    let mut tab = tab_with_content("Hello World\n", 10);
    let tab_size = EditorConfig::default().tab_size;

    tab.place_cursor(0, 1, Granularity::Word, tab_size);
    // Word selection is non-empty even without extend
    tab.end_selection_gesture();
    assert!(tab.buffer.selection().is_some());
}

#[test]
fn place_cursor_line_selects_full_line() {
    let mut tab = tab_with_content("Hello World\nSecond line\n", 10);
    let tab_size = EditorConfig::default().tab_size;

    tab.place_cursor(0, 3, Granularity::Line, tab_size);
    let text = tab.buffer.get_selection_text();
    assert!(text.is_some());
    let text = text.unwrap();
    assert!(text.contains("Hello World"));
}

#[test]
fn add_cursor_at_demotes_old_primary() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::untitled(TabId::new(1), &config);
    tab.buffer = crate::models::TextBuffer::from_text("abc\ndef\n");
    tab.viewport.height = 10;
    tab.viewport.width = 40;

    tab.buffer.set_cursor(0, 1);
    assert!(tab.secondary_cursors.is_empty());

    let changed = tab.add_cursor_at(1, 2, config.tab_size);
    assert!(changed);
    assert_eq!(tab.buffer.cursor(), (1, 2));
    assert_eq!(tab.secondary_cursors.len(), 1);
    assert_eq!(tab.secondary_cursors[0].pos, (0, 1));
}
