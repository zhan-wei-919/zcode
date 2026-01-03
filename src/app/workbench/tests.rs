use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use std::sync::mpsc;
use tempfile::tempdir;

fn create_test_runtime() -> AsyncRuntime {
    let (tx, _rx) = mpsc::channel();
    AsyncRuntime::new(tx)
}

#[test]
fn test_workbench_new() {
    let dir = tempdir().unwrap();
    let runtime = create_test_runtime();
    let workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert_eq!(workbench.focus(), FocusTarget::Editor);
    assert!(workbench.sidebar_visible());
}

#[test]
fn test_toggle_sidebar() {
    let dir = tempdir().unwrap();
    let runtime = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert!(workbench.sidebar_visible());

    let key_event = KeyEvent {
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(!workbench.sidebar_visible());
}

#[test]
fn test_toggle_bottom_panel() {
    let dir = tempdir().unwrap();
    let runtime = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert!(!workbench.bottom_panel_visible());

    let key_event = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(workbench.bottom_panel_visible());
}

#[test]
fn test_focus_bottom_panel() {
    let dir = tempdir().unwrap();
    let runtime = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert_eq!(workbench.focus(), FocusTarget::Editor);

    let key_event = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(workbench.bottom_panel_visible());
    assert_eq!(workbench.focus(), FocusTarget::BottomPanel);
}
