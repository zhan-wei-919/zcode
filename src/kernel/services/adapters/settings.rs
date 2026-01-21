use crate::core::event::Key;
use crate::core::Command;
use crate::kernel::services::ports::settings::Settings;
use crossterm::event::{KeyCode, KeyModifiers};
use std::path::PathBuf;

const SETTINGS_DIR: &str = ".zcode";
const SETTINGS_FILE: &str = "setting.json";

pub fn get_settings_path() -> Option<PathBuf> {
    get_cache_dir().map(|dir| dir.join(SETTINGS_DIR).join(SETTINGS_FILE))
}

pub fn ensure_settings_file() -> std::io::Result<PathBuf> {
    let path = get_settings_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Cannot determine settings directory",
        )
    })?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    if !path.exists() {
        let content =
            serde_json::to_string_pretty(&Settings::default()).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(&path, content)?;
    }
    Ok(path)
}

pub fn load_settings() -> Option<Settings> {
    let path = get_settings_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn parse_keybinding(value: &str) -> Option<Key> {
    let mut modifiers = KeyModifiers::NONE;
    let mut key_part: Option<&str> = None;
    for part in value.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "alt" | "option" => modifiers |= KeyModifiers::ALT,
            "super" | "meta" | "cmd" | "command" => modifiers |= KeyModifiers::SUPER,
            _ => key_part = Some(part),
        }
    }
    let key_part = key_part?;
    let mut code = parse_key_code(key_part)?;
    if let KeyCode::Char(ch) = code {
        if ch.is_ascii_uppercase() {
            code = KeyCode::Char(ch.to_ascii_lowercase());
            modifiers |= KeyModifiers::SHIFT;
        }
    }
    Some(Key::new(code, modifiers))
}

pub fn parse_command(value: &str) -> Command {
    Command::from_name(value)
}

fn parse_key_code(value: &str) -> Option<KeyCode> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }

    let v_lc = v.to_ascii_lowercase();
    let code = match v_lc.as_str() {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        _ if v_lc.starts_with('f') => {
            let n = v_lc.strip_prefix('f')?.parse::<u8>().ok()?;
            KeyCode::F(n)
        }
        _ => {
            let mut chars = v.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(ch)
        }
    };

    Some(code)
}

fn get_cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        return std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join("Library/Caches"));
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            return Some(PathBuf::from(xdg));
        }
        return std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".cache"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return Some(PathBuf::from(local));
        }
        return std::env::var("APPDATA").ok().map(PathBuf::from);
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}
