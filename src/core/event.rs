use std::ops::{BitOr, BitOrAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Enter,
    Tab,
    BackTab,
    Esc,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1 << 0);
    pub const CONTROL: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    pub const SUPER: Self = Self(1 << 3);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for KeyModifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for KeyModifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventKind {
    Press,
    Release,
    Repeat,
}

impl Default for KeyEventKind {
    fn default() -> Self {
        Self::Press
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub kind: KeyEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    Down(MouseButton),
    Up(MouseButton),
    Drag(MouseButton),
    Moved,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: KeyModifiers,
}

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
}
