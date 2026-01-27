use super::*;

#[test]
fn test_key_creation() {
    let key = Key::ctrl(KeyCode::Char('s'));
    assert_eq!(key.code, KeyCode::Char('s'));
    assert_eq!(key.modifiers, KeyModifiers::CONTROL);
}

#[test]
fn test_key_from_event() {
    let event = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let key: Key = event.into();
    assert_eq!(key.code, KeyCode::Enter);
}

#[test]
fn test_input_event_conversion() {
    let key_event = KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let input = InputEvent::Key(key_event);

    assert!(input.is_key());
    assert!(!input.is_mouse());
}

#[test]
fn test_mouse_position() {
    let pos = MousePosition::new(10, 20);
    assert_eq!(pos.x, 10);
    assert_eq!(pos.y, 20);
}
