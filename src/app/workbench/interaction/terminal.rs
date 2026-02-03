use crate::core::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn terminal_bytes_for_key_event(event: &KeyEvent) -> Option<Vec<u8>> {
    match (event.code, event.modifiers) {
        (KeyCode::Char(ch), KeyModifiers::CONTROL) => {
            let ch = ch.to_ascii_lowercase();
            Some(vec![(ch as u8) & 0x1f])
        }
        (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            Some(s.as_bytes().to_vec())
        }
        (KeyCode::Enter, _) => Some(vec![b'\r']),
        (KeyCode::Backspace, _) => Some(vec![0x7f]),
        (KeyCode::Tab, _) => Some(vec![b'\t']),
        (KeyCode::BackTab, _) => Some(b"\x1b[Z".to_vec()),
        (KeyCode::Esc, _) => Some(vec![0x1b]),
        (KeyCode::Up, _) => Some(b"\x1b[A".to_vec()),
        (KeyCode::Down, _) => Some(b"\x1b[B".to_vec()),
        (KeyCode::Right, _) => Some(b"\x1b[C".to_vec()),
        (KeyCode::Left, _) => Some(b"\x1b[D".to_vec()),
        (KeyCode::Home, _) => Some(b"\x1b[H".to_vec()),
        (KeyCode::End, _) => Some(b"\x1b[F".to_vec()),
        (KeyCode::Delete, _) => Some(b"\x1b[3~".to_vec()),
        (KeyCode::PageUp, _) => Some(b"\x1b[5~".to_vec()),
        (KeyCode::PageDown, _) => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}
