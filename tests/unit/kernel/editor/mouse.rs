use crate::kernel::editor::{EditorTabState, TabId};
use crate::kernel::services::ports::EditorConfig;
use std::time::Instant;

fn tab_with_content(text: &str, viewport_height: usize) -> EditorTabState {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::untitled(TabId::new(1), &config);
    tab.buffer = crate::models::TextBuffer::from_text(text);
    tab.viewport.height = viewport_height;
    tab.viewport.width = 40;
    tab
}

fn start_drag(tab: &mut EditorTabState) {
    let tab_size = EditorConfig::default().tab_size;
    tab.mouse_down(0, 0, Instant::now(), tab_size, 2, 500);
}

#[test]
fn drag_not_dragging_returns_false() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::untitled(TabId::new(1), &config);
    assert!(!tab.mouse.dragging);
    assert!(!tab.mouse_drag(0, 0, config.tab_size, 0, false));
}

#[test]
fn drag_past_right_moves_cursor_to_line_end() {
    let mut tab = tab_with_content("Hello World\n", 10);
    start_drag(&mut tab);

    let tab_size = EditorConfig::default().tab_size;
    let result = tab.mouse_drag(0, 0, tab_size, 0, true);
    assert!(result);

    let (row, col) = tab.buffer.cursor();
    assert_eq!(row, 0);
    assert_eq!(col, tab.buffer.line_grapheme_len(0));
}

#[test]
fn drag_left_moves_cursor_to_col_zero() {
    let mut tab = tab_with_content("Hello World\n", 10);
    start_drag(&mut tab);

    let tab_size = EditorConfig::default().tab_size;
    // x=0 with past_right=false should map to column 0
    let result = tab.mouse_drag(0, 0, tab_size, 0, false);
    assert!(result);

    let (_row, col) = tab.buffer.cursor();
    assert_eq!(col, 0);
}

#[test]
fn drag_below_scrolls_viewport_down() {
    let text = (0..50)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut tab = tab_with_content(&text, 10);
    start_drag(&mut tab);

    let initial_offset = tab.viewport.line_offset;
    let tab_size = EditorConfig::default().tab_size;
    // overflow_y=3 means 3 rows below content area
    let result = tab.mouse_drag(0, 9, tab_size, 3, false);
    assert!(result);
    assert!(tab.viewport.line_offset > initial_offset);
}

#[test]
fn drag_above_scrolls_viewport_up() {
    let text = (0..50)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut tab = tab_with_content(&text, 10);
    tab.viewport.line_offset = 20;
    start_drag(&mut tab);

    let initial_offset = tab.viewport.line_offset;
    let tab_size = EditorConfig::default().tab_size;
    // overflow_y=-2 means 2 rows above content area
    let result = tab.mouse_drag(0, 0, tab_size, -2, false);
    assert!(result);
    assert!(tab.viewport.line_offset < initial_offset);
}

#[test]
fn drag_below_eof_stops_at_last_line() {
    let text = "line 0\nline 1\nline 2";
    let mut tab = tab_with_content(text, 10);
    start_drag(&mut tab);

    let tab_size = EditorConfig::default().tab_size;
    // With only 3 lines and viewport height 10, max_offset is 0.
    // overflow_y=100 tries to scroll far past the end, but gets clamped.
    // y=2 is the last visible line (only 3 lines exist).
    let result = tab.mouse_drag(0, 2, tab_size, 100, false);
    assert!(result);
    assert_eq!(tab.viewport.line_offset, 0);
}
