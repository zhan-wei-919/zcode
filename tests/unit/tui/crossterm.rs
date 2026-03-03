use super::*;

#[test]
fn ctrl_space_normalizes_from_null() {
    let event = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Null,
        crossterm::event::KeyModifiers::NONE,
    );
    let converted = into_key_event(event);
    assert_eq!(converted.code, KeyCode::Char(' '));
    assert!(converted.modifiers.contains(KeyModifiers::CONTROL));
}

#[test]
fn ctrl_c_normalizes_from_control_character() {
    let event = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('\u{3}'),
        crossterm::event::KeyModifiers::NONE,
    );
    let converted = into_key_event(event);
    assert_eq!(converted.code, KeyCode::Char('c'));
    assert!(converted.modifiers.contains(KeyModifiers::CONTROL));
}
