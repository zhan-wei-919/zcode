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
