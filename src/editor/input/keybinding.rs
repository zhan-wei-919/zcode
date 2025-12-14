//zcode/src/editor/input/keybinding.rs
//! 键位绑定系统：按键 → 命令映射
//! 
//! 功能：
//! - 默认键位映射（类 VS Code）
//! - 支持自定义键位
//! - 支持键位冲突检测
//! - 支持从配置文件加载（未来）

use std::collections::HashMap;
use crossterm::event::KeyCode;
use super::command::{Command, Key};

/// 键位绑定表
pub struct Keybindings {
    /// 按键 → 命令的映射
    bindings: HashMap<Key, Command>,
}

impl Keybindings {
    /// 创建默认键位绑定（类 VS Code/Emacs）
    pub fn default() -> Self {
        let mut bindings = HashMap::new();
        
        // ==================== 光标移动 ====================
        bindings.insert(Key::simple(KeyCode::Left), Command::CursorLeft);
        bindings.insert(Key::simple(KeyCode::Right), Command::CursorRight);
        bindings.insert(Key::simple(KeyCode::Up), Command::CursorUp);
        bindings.insert(Key::simple(KeyCode::Down), Command::CursorDown);
        
        // Home/End
        bindings.insert(Key::simple(KeyCode::Home), Command::CursorLineStart);
        bindings.insert(Key::simple(KeyCode::End), Command::CursorLineEnd);
        
        // Ctrl + Home/End: 文件首尾
        bindings.insert(Key::ctrl(KeyCode::Home), Command::CursorFileStart);
        bindings.insert(Key::ctrl(KeyCode::End), Command::CursorFileEnd);
        
        // Ctrl + Left/Right: 按词移动
        bindings.insert(Key::ctrl(KeyCode::Left), Command::CursorWordLeft);
        bindings.insert(Key::ctrl(KeyCode::Right), Command::CursorWordRight);
        
        // ==================== 编辑操作 ====================
        bindings.insert(Key::simple(KeyCode::Enter), Command::InsertNewline);
        bindings.insert(Key::simple(KeyCode::Tab), Command::InsertTab);
        bindings.insert(Key::simple(KeyCode::Backspace), Command::DeleteBackward);
        bindings.insert(Key::simple(KeyCode::Delete), Command::DeleteForward);
        
        // Ctrl + D: 删除当前行
        bindings.insert(Key::ctrl(KeyCode::Char('d')), Command::DeleteLine);
        
        // Ctrl + K: 删除到行尾
        bindings.insert(Key::ctrl(KeyCode::Char('k')), Command::DeleteToLineEnd);
        
        // ==================== 选择操作 ====================
        // Esc: 清除选区
        bindings.insert(Key::simple(KeyCode::Esc), Command::ClearSelection);
        
        // Ctrl + A: 全选
        bindings.insert(Key::ctrl(KeyCode::Char('a')), Command::SelectAll);
        
        // ==================== 滚动操作 ====================
        bindings.insert(Key::simple(KeyCode::PageUp), Command::PageUp);
        bindings.insert(Key::simple(KeyCode::PageDown), Command::PageDown);
        
        // ==================== 系统操作 ====================
        // Ctrl + Q: 退出
        bindings.insert(Key::ctrl(KeyCode::Char('q')), Command::Quit);
        
        // Ctrl + S: 保存
        bindings.insert(Key::ctrl(KeyCode::Char('s')), Command::Save);
        
        // Ctrl + Z: 撤销
        bindings.insert(Key::ctrl(KeyCode::Char('z')), Command::Undo);
        
        // Ctrl + Y: 重做
        bindings.insert(Key::ctrl(KeyCode::Char('y')), Command::Redo);
        
        Self { bindings }
    }
    
    /// 创建空的键位绑定表
    pub fn empty() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
    
    /// 获取按键对应的命令
    pub fn get(&self, key: &Key) -> Option<&Command> {
        self.bindings.get(key)
    }
    
    /// 绑定按键到命令
    pub fn bind(&mut self, key: Key, command: Command) {
        self.bindings.insert(key, command);
    }
    
    /// 解除按键绑定
    pub fn unbind(&mut self, key: &Key) -> Option<Command> {
        self.bindings.remove(key)
    }
    
    /// 获取命令对应的所有按键
    pub fn keys_for_command(&self, command: &Command) -> Vec<Key> {
        self.bindings
            .iter()
            .filter(|(_, cmd)| *cmd == command)
            .map(|(key, _)| *key)
            .collect()
    }
    
    /// 检查按键是否已绑定
    pub fn is_bound(&self, key: &Key) -> bool {
        self.bindings.contains_key(key)
    }
    
    /// 获取所有绑定数量
    pub fn len(&self) -> usize {
        self.bindings.len()
    }
    
    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_bindings() {
        let bindings = Keybindings::default();
        
        // 验证基本移动键
        assert_eq!(
            bindings.get(&Key::simple(KeyCode::Left)),
            Some(&Command::CursorLeft)
        );
        
        // 验证 Ctrl 组合键
        assert_eq!(
            bindings.get(&Key::ctrl(KeyCode::Char('s'))),
            Some(&Command::Save)
        );
        
        // 验证绑定数量
        assert!(bindings.len() > 10, "Should have multiple default bindings");
    }
    
    #[test]
    fn test_custom_bindings() {
        let mut bindings = Keybindings::empty();
        
        // 添加自定义绑定
        bindings.bind(
            Key::ctrl(KeyCode::Char('w')),
            Command::DeleteLine,
        );
        
        assert_eq!(
            bindings.get(&Key::ctrl(KeyCode::Char('w'))),
            Some(&Command::DeleteLine)
        );
    }
    
    #[test]
    fn test_unbind() {
        let mut bindings = Keybindings::default();
        let key = Key::simple(KeyCode::Left);
        
        assert!(bindings.is_bound(&key));
        
        let removed = bindings.unbind(&key);
        assert_eq!(removed, Some(Command::CursorLeft));
        assert!(!bindings.is_bound(&key));
    }
    
    #[test]
    fn test_keys_for_command() {
        let bindings = Keybindings::default();
        
        let keys = bindings.keys_for_command(&Command::CursorLeft);
        assert!(!keys.is_empty());
    }
}

