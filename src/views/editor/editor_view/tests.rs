use super::*;
use crate::core::Command;

#[test]
fn test_editor_view_new() {
    let view = EditorView::new();
    assert_eq!(view.cursor(), (0, 0));
    assert!(!view.is_dirty());
}

#[test]
fn test_editor_view_from_text() {
    let view = EditorView::from_text("hello\nworld");
    assert_eq!(view.buffer().len_lines(), 2);
}

#[test]
fn test_cursor_movement() {
    let mut view = EditorView::from_text("hello\nworld");

    view.execute(Command::CursorRight);
    assert_eq!(view.cursor(), (0, 1));

    view.execute(Command::CursorDown);
    assert_eq!(view.cursor(), (1, 1));

    view.execute(Command::CursorLineEnd);
    assert_eq!(view.cursor(), (1, 5));
}

#[test]
fn test_insert_char() {
    let mut view = EditorView::new();
    view.execute(Command::InsertChar('a'));
    assert_eq!(view.buffer().text(), "a");
    assert!(view.is_dirty());
}

#[test]
fn test_undo_redo() {
    let mut view = EditorView::new();

    view.execute(Command::InsertChar('a'));
    view.execute(Command::InsertChar('b'));
    view.execute(Command::InsertChar('c'));
    assert_eq!(view.buffer().text(), "abc");
    assert_eq!(view.cursor(), (0, 3));

    view.undo();
    assert_eq!(view.buffer().text(), "ab");
    assert_eq!(view.cursor(), (0, 2));

    view.undo();
    assert_eq!(view.buffer().text(), "a");
    assert_eq!(view.cursor(), (0, 1));

    view.redo();
    assert_eq!(view.buffer().text(), "ab");
    assert_eq!(view.cursor(), (0, 2));

    view.redo();
    assert_eq!(view.buffer().text(), "abc");
    assert_eq!(view.cursor(), (0, 3));
}

#[test]
fn test_undo_redo_with_delete() {
    let mut view = EditorView::from_text("hello");
    view.execute(Command::CursorLineEnd);
    assert_eq!(view.cursor(), (0, 5));

    view.execute(Command::DeleteBackward);
    assert_eq!(view.buffer().text(), "hell");
    assert_eq!(view.cursor(), (0, 4));

    view.undo();
    assert_eq!(view.buffer().text(), "hello");
    assert_eq!(view.cursor(), (0, 5));

    view.redo();
    assert_eq!(view.buffer().text(), "hell");
    assert_eq!(view.cursor(), (0, 4));
}

#[test]
fn test_undo_branch() {
    let mut view = EditorView::new();

    view.execute(Command::InsertChar('a'));
    view.execute(Command::InsertChar('b'));
    assert_eq!(view.buffer().text(), "ab");

    view.undo();
    assert_eq!(view.buffer().text(), "a");

    view.execute(Command::InsertChar('c'));
    assert_eq!(view.buffer().text(), "ac");

    view.undo();
    assert_eq!(view.buffer().text(), "a");

    view.redo();
    assert_eq!(view.buffer().text(), "ac");
}
