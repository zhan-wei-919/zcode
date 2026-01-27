use super::*;

#[test]
fn test_command_names() {
    assert_eq!(Command::CursorLeft.name(), "cursorLeft");
    assert_eq!(Command::InsertChar('a').name(), "insertChar");
    assert_eq!(Command::Quit.name(), "quit");
    assert_eq!(Command::Custom("myCommand".to_string()).name(), "myCommand");
}

#[test]
fn test_is_edit_command() {
    assert!(Command::InsertChar('a').is_edit_command());
    assert!(Command::DeleteBackward.is_edit_command());
    assert!(Command::Paste.is_edit_command());
    assert!(!Command::CursorLeft.is_edit_command());
    assert!(!Command::Save.is_edit_command());
}

#[test]
fn test_is_cursor_command() {
    assert!(Command::CursorLeft.is_cursor_command());
    assert!(Command::CursorFileEnd.is_cursor_command());
    assert!(!Command::InsertChar('a').is_cursor_command());
}

#[test]
fn test_is_selection_command() {
    assert!(Command::SelectAll.is_selection_command());
    assert!(Command::ClearSelection.is_selection_command());
    assert!(!Command::CursorLeft.is_selection_command());
}
