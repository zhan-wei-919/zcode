use super::EditorGroup;
use crate::core::event::InputEvent;
use crate::core::view::View;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

#[test]
fn test_editor_group_new() {
    let group = EditorGroup::new();
    assert_eq!(group.tab_count(), 1);
    assert_eq!(group.active_index(), 0);
}

#[test]
fn test_open_file() {
    let mut group = EditorGroup::new();
    group.open_file(PathBuf::from("/test/file.txt"), "hello");

    assert_eq!(group.tab_count(), 2);
    assert_eq!(group.active_index(), 1);
}

#[test]
fn test_close_tab() {
    let mut group = EditorGroup::new();
    group.open_file(PathBuf::from("/test/file.txt"), "hello");

    assert!(group.close_active_tab());
    assert_eq!(group.tab_count(), 1);
}

#[test]
fn test_tab_navigation() {
    let mut group = EditorGroup::new();
    group.open_file(PathBuf::from("/test/a.txt"), "a");
    group.open_file(PathBuf::from("/test/b.txt"), "b");

    assert_eq!(group.active_index(), 2);

    group.prev_tab();
    assert_eq!(group.active_index(), 1);

    group.next_tab();
    assert_eq!(group.active_index(), 2);
}

#[test]
fn test_mouse_select_tab() {
    let mut group = EditorGroup::new();
    group.open_file(PathBuf::from("/test/a.txt"), "a");
    group.open_file(PathBuf::from("/test/b.txt"), "b");
    group.active_index = 0;
    group.area = Some(Rect::new(0, 0, 80, 10));

    let first_width = 1 + (UnicodeWidthStr::width("Untitled").saturating_add(2) as u16) + 1 + 1;
    let click_col = first_width + 1;

    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click_col,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    let result = group.handle_input(&InputEvent::Mouse(ev));
    assert!(result.is_consumed());
    assert_eq!(group.active_index(), 1);
}

#[test]
fn test_cannot_close_last_tab() {
    let mut group = EditorGroup::new();
    assert!(!group.close_active_tab());
    assert_eq!(group.tab_count(), 1);
}
