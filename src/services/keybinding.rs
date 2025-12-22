//! 快捷键服务：按键 → 命令映射
//!
//! 功能：
//! - 默认键位映射（类 VS Code）
//! - 支持自定义键位
//! - 支持键位冲突检测

use crate::core::event::Key;
use crate::core::Command;
use crate::core::Service;
use crossterm::event::KeyCode;
use std::collections::HashMap;

pub struct KeybindingService {
    bindings: HashMap<Key, Command>,
}

impl KeybindingService {
    pub fn new() -> Self {
        Self::with_defaults()
    }

    pub fn empty() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut bindings = HashMap::new();

        // ==================== 光标移动 ====================
        bindings.insert(Key::simple(KeyCode::Left), Command::CursorLeft);
        bindings.insert(Key::simple(KeyCode::Right), Command::CursorRight);
        bindings.insert(Key::simple(KeyCode::Up), Command::CursorUp);
        bindings.insert(Key::simple(KeyCode::Down), Command::CursorDown);

        bindings.insert(Key::simple(KeyCode::Home), Command::CursorLineStart);
        bindings.insert(Key::simple(KeyCode::End), Command::CursorLineEnd);

        bindings.insert(Key::ctrl(KeyCode::Home), Command::CursorFileStart);
        bindings.insert(Key::ctrl(KeyCode::End), Command::CursorFileEnd);

        bindings.insert(Key::ctrl(KeyCode::Left), Command::CursorWordLeft);
        bindings.insert(Key::ctrl(KeyCode::Right), Command::CursorWordRight);

        // ==================== 编辑操作 ====================
        bindings.insert(Key::simple(KeyCode::Enter), Command::InsertNewline);
        bindings.insert(Key::simple(KeyCode::Tab), Command::InsertTab);
        bindings.insert(Key::simple(KeyCode::Backspace), Command::DeleteBackward);
        bindings.insert(Key::simple(KeyCode::Delete), Command::DeleteForward);

        bindings.insert(Key::ctrl(KeyCode::Char('d')), Command::DeleteLine);
        bindings.insert(Key::ctrl(KeyCode::Char('k')), Command::DeleteToLineEnd);

        // ==================== 选择操作 ====================
        bindings.insert(Key::simple(KeyCode::Esc), Command::ClearSelection);
        bindings.insert(Key::ctrl(KeyCode::Char('a')), Command::SelectAll);

        // ==================== 滚动操作 ====================
        bindings.insert(Key::simple(KeyCode::PageUp), Command::PageUp);
        bindings.insert(Key::simple(KeyCode::PageDown), Command::PageDown);

        // ==================== 文件操作 ====================
        bindings.insert(Key::ctrl(KeyCode::Char('s')), Command::Save);
        bindings.insert(Key::ctrl(KeyCode::Char('w')), Command::CloseTab);
        bindings.insert(Key::ctrl(KeyCode::Tab), Command::NextTab);
        bindings.insert(Key::ctrl_shift(KeyCode::Tab), Command::PrevTab);

        // ==================== 系统操作 ====================
        bindings.insert(Key::ctrl(KeyCode::Char('q')), Command::Quit);
        bindings.insert(Key::ctrl(KeyCode::Char('z')), Command::Undo);
        bindings.insert(Key::ctrl(KeyCode::Char('y')), Command::Redo);
        bindings.insert(Key::ctrl(KeyCode::Char('c')), Command::Copy);
        bindings.insert(Key::ctrl(KeyCode::Char('x')), Command::Cut);
        bindings.insert(Key::ctrl(KeyCode::Char('v')), Command::Paste);

        // ==================== 查找替换 ====================
        bindings.insert(Key::ctrl(KeyCode::Char('f')), Command::Find);
        bindings.insert(Key::ctrl(KeyCode::Char('h')), Command::Replace);

        // ==================== 视图操作 ====================
        bindings.insert(Key::ctrl(KeyCode::Char('b')), Command::ToggleSidebar);
        bindings.insert(Key::ctrl_shift(KeyCode::Char('p')), Command::CommandPalette);
        bindings.insert(Key::ctrl_shift(KeyCode::Char('e')), Command::FocusExplorer);

        Self { bindings }
    }

    pub fn get(&self, key: &Key) -> Option<&Command> {
        self.bindings.get(key)
    }

    pub fn bind(&mut self, key: Key, command: Command) {
        self.bindings.insert(key, command);
    }

    pub fn unbind(&mut self, key: &Key) -> Option<Command> {
        self.bindings.remove(key)
    }

    pub fn keys_for_command(&self, command: &Command) -> Vec<Key> {
        self.bindings
            .iter()
            .filter(|(_, cmd)| *cmd == command)
            .map(|(key, _)| *key)
            .collect()
    }

    pub fn is_bound(&self, key: &Key) -> bool {
        self.bindings.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn all_bindings(&self) -> impl Iterator<Item = (&Key, &Command)> {
        self.bindings.iter()
    }
}

impl Default for KeybindingService {
    fn default() -> Self {
        Self::new()
    }
}

impl Service for KeybindingService {
    fn name(&self) -> &'static str {
        "KeybindingService"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings() {
        let service = KeybindingService::new();

        assert_eq!(
            service.get(&Key::simple(KeyCode::Left)),
            Some(&Command::CursorLeft)
        );

        assert_eq!(
            service.get(&Key::ctrl(KeyCode::Char('s'))),
            Some(&Command::Save)
        );

        assert!(service.len() > 20);
    }

    #[test]
    fn test_custom_bindings() {
        let mut service = KeybindingService::empty();

        service.bind(Key::ctrl(KeyCode::Char('w')), Command::DeleteLine);

        assert_eq!(
            service.get(&Key::ctrl(KeyCode::Char('w'))),
            Some(&Command::DeleteLine)
        );
    }

    #[test]
    fn test_unbind() {
        let mut service = KeybindingService::new();
        let key = Key::simple(KeyCode::Left);

        assert!(service.is_bound(&key));

        let removed = service.unbind(&key);
        assert_eq!(removed, Some(Command::CursorLeft));
        assert!(!service.is_bound(&key));
    }

    #[test]
    fn test_keys_for_command() {
        let service = KeybindingService::new();
        let keys = service.keys_for_command(&Command::CursorLeft);
        assert!(!keys.is_empty());
    }

    #[test]
    fn test_service_trait() {
        let service = KeybindingService::new();
        assert_eq!(service.name(), "KeybindingService");
    }
}
