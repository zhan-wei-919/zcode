use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

#[derive(Debug, Clone)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    FocusGained,
    FocusLost,
    Paste(String),
}

impl InputEvent {
    pub fn is_key(&self) -> bool {
        matches!(self, InputEvent::Key(_))
    }

    pub fn is_mouse(&self) -> bool {
        matches!(self, InputEvent::Mouse(_))
    }

    pub fn as_key(&self) -> Option<&KeyEvent> {
        match self {
            InputEvent::Key(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_mouse(&self) -> Option<&MouseEvent> {
        match self {
            InputEvent::Mouse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crossterm::event::Event> for InputEvent {
    fn from(event: crossterm::event::Event) -> Self {
        match event {
            crossterm::event::Event::Key(e) => InputEvent::Key(e),
            crossterm::event::Event::Mouse(e) => InputEvent::Mouse(e),
            crossterm::event::Event::Resize(w, h) => InputEvent::Resize(w, h),
            crossterm::event::Event::FocusGained => InputEvent::FocusGained,
            crossterm::event::Event::FocusLost => InputEvent::FocusLost,
            crossterm::event::Event::Paste(s) => InputEvent::Paste(s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl Key {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn simple(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }

    pub fn ctrl(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::CONTROL)
    }

    pub fn alt(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::ALT)
    }

    pub fn shift(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::SHIFT)
    }

    pub fn ctrl_shift(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
    }
}

impl From<KeyEvent> for Key {
    fn from(event: KeyEvent) -> Self {
        let mut code = event.code;
        let mut modifiers = event.modifiers;

        if let KeyCode::Char(ch) = code {
            if ch.is_ascii_uppercase() {
                code = KeyCode::Char(ch.to_ascii_lowercase());
                modifiers |= KeyModifiers::SHIFT;
            }
        }

        Self::new(code, modifiers)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MousePosition {
    pub x: u16,
    pub y: u16,
}

impl MousePosition {
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    pub fn from_event(event: &MouseEvent) -> Self {
        Self::new(event.column, event.row)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

impl From<MouseEventKind> for MouseAction {
    fn from(kind: MouseEventKind) -> Self {
        match kind {
            MouseEventKind::Down(btn) => MouseAction::Down(btn),
            MouseEventKind::Up(btn) => MouseAction::Up(btn),
            MouseEventKind::Drag(btn) => MouseAction::Drag(btn),
            MouseEventKind::Moved => MouseAction::Moved,
            MouseEventKind::ScrollUp => MouseAction::ScrollUp,
            MouseEventKind::ScrollDown => MouseAction::ScrollDown,
            MouseEventKind::ScrollLeft => MouseAction::ScrollLeft,
            MouseEventKind::ScrollRight => MouseAction::ScrollRight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

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
            state: KeyEventState::NONE,
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
            state: KeyEventState::NONE,
        };
        let event = crossterm::event::Event::Key(key_event);
        let input: InputEvent = event.into();

        assert!(input.is_key());
        assert!(!input.is_mouse());
    }

    #[test]
    fn test_mouse_position() {
        let pos = MousePosition::new(10, 20);
        assert_eq!(pos.x, 10);
        assert_eq!(pos.y, 20);
    }
}
