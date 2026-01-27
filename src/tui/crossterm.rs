use crate::core::event::{
    InputEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};

pub fn into_input_event(event: crossterm::event::Event) -> InputEvent {
    match event {
        crossterm::event::Event::Key(key) => InputEvent::Key(into_key_event(key)),
        crossterm::event::Event::Mouse(mouse) => InputEvent::Mouse(into_mouse_event(mouse)),
        crossterm::event::Event::Resize(w, h) => InputEvent::Resize(w, h),
        crossterm::event::Event::FocusGained => InputEvent::FocusGained,
        crossterm::event::Event::FocusLost => InputEvent::FocusLost,
        crossterm::event::Event::Paste(s) => InputEvent::Paste(s),
    }
}

pub fn into_key_event(event: crossterm::event::KeyEvent) -> KeyEvent {
    let mut modifiers = into_key_modifiers(event.modifiers);
    let code = into_key_code(event.code, &mut modifiers);
    KeyEvent {
        code,
        modifiers,
        kind: into_key_event_kind(event.kind),
    }
}

fn into_key_event_kind(kind: crossterm::event::KeyEventKind) -> KeyEventKind {
    match kind {
        crossterm::event::KeyEventKind::Press => KeyEventKind::Press,
        crossterm::event::KeyEventKind::Release => KeyEventKind::Release,
        crossterm::event::KeyEventKind::Repeat => KeyEventKind::Repeat,
    }
}

fn into_key_modifiers(mods: crossterm::event::KeyModifiers) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    if mods.contains(crossterm::event::KeyModifiers::SHIFT) {
        out |= KeyModifiers::SHIFT;
    }
    if mods.contains(crossterm::event::KeyModifiers::CONTROL) {
        out |= KeyModifiers::CONTROL;
    }
    if mods.contains(crossterm::event::KeyModifiers::ALT) {
        out |= KeyModifiers::ALT;
    }
    if mods.contains(crossterm::event::KeyModifiers::SUPER) {
        out |= KeyModifiers::SUPER;
    }
    out
}

fn into_key_code(code: crossterm::event::KeyCode, modifiers: &mut KeyModifiers) -> KeyCode {
    match code {
        crossterm::event::KeyCode::Char(ch) => KeyCode::Char(ch),
        crossterm::event::KeyCode::Enter => KeyCode::Enter,
        crossterm::event::KeyCode::Tab => KeyCode::Tab,
        crossterm::event::KeyCode::BackTab => KeyCode::BackTab,
        crossterm::event::KeyCode::Esc => KeyCode::Esc,
        crossterm::event::KeyCode::Backspace => KeyCode::Backspace,
        crossterm::event::KeyCode::Delete => KeyCode::Delete,
        crossterm::event::KeyCode::Up => KeyCode::Up,
        crossterm::event::KeyCode::Down => KeyCode::Down,
        crossterm::event::KeyCode::Left => KeyCode::Left,
        crossterm::event::KeyCode::Right => KeyCode::Right,
        crossterm::event::KeyCode::Home => KeyCode::Home,
        crossterm::event::KeyCode::End => KeyCode::End,
        crossterm::event::KeyCode::PageUp => KeyCode::PageUp,
        crossterm::event::KeyCode::PageDown => KeyCode::PageDown,
        crossterm::event::KeyCode::F(n) => KeyCode::F(n),
        crossterm::event::KeyCode::Null => {
            *modifiers |= KeyModifiers::CONTROL;
            KeyCode::Char(' ')
        }
        _ => KeyCode::Unknown,
    }
}

pub fn into_mouse_event(event: crossterm::event::MouseEvent) -> MouseEvent {
    MouseEvent {
        kind: into_mouse_event_kind(event.kind),
        column: event.column,
        row: event.row,
        modifiers: into_key_modifiers(event.modifiers),
    }
}

fn into_mouse_button(button: crossterm::event::MouseButton) -> MouseButton {
    match button {
        crossterm::event::MouseButton::Left => MouseButton::Left,
        crossterm::event::MouseButton::Right => MouseButton::Right,
        crossterm::event::MouseButton::Middle => MouseButton::Middle,
    }
}

fn into_mouse_event_kind(kind: crossterm::event::MouseEventKind) -> MouseEventKind {
    match kind {
        crossterm::event::MouseEventKind::Down(button) => {
            MouseEventKind::Down(into_mouse_button(button))
        }
        crossterm::event::MouseEventKind::Up(button) => {
            MouseEventKind::Up(into_mouse_button(button))
        }
        crossterm::event::MouseEventKind::Drag(button) => {
            MouseEventKind::Drag(into_mouse_button(button))
        }
        crossterm::event::MouseEventKind::Moved => MouseEventKind::Moved,
        crossterm::event::MouseEventKind::ScrollUp => MouseEventKind::ScrollUp,
        crossterm::event::MouseEventKind::ScrollDown => MouseEventKind::ScrollDown,
        crossterm::event::MouseEventKind::ScrollLeft => MouseEventKind::ScrollLeft,
        crossterm::event::MouseEventKind::ScrollRight => MouseEventKind::ScrollRight,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui/crossterm.rs"]
mod tests;
