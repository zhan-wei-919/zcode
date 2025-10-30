//zcode/src/editor/input/command.rs
//! 命令系统：语义命令定义和执行
//! 
//! 架构：
//! - Command: 语义命令枚举（不关心具体按键）
//! - Keybinding: 按键 → 命令的映射
//! - CommandExecutor: 命令执行器（在 Editor 上执行命令）
//!
//! 优势：
//! - 解耦按键和操作
//! - 支持自定义键位
//! - 支持命令历史（undo/redo）
//! - 支持宏录制

use crossterm::event::{KeyCode, KeyModifiers};

/// 编辑器命令（语义层）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    // ==================== 光标移动 ====================
    /// 光标左移
    CursorLeft,
    /// 光标右移
    CursorRight,
    /// 光标上移
    CursorUp,
    /// 光标下移
    CursorDown,
    
    /// 移动到行首
    CursorLineStart,
    /// 移动到行尾
    CursorLineEnd,
    
    /// 移动到文件开头
    CursorFileStart,
    /// 移动到文件结尾
    CursorFileEnd,
    
    /// 按词左移
    CursorWordLeft,
    /// 按词右移
    CursorWordRight,
    
    // ==================== 编辑操作 ====================
    /// 插入字符
    InsertChar(char),
    /// 插入换行
    InsertNewline,
    /// 插入制表符
    InsertTab,
    
    /// 删除前一个字符
    DeleteBackward,
    /// 删除后一个字符
    DeleteForward,
    /// 删除当前行
    DeleteLine,
    /// 删除到行尾
    DeleteToLineEnd,
    
    /// 删除选区内容
    DeleteSelection,
    
    // ==================== 选择操作 ====================
    /// 清除选区
    ClearSelection,
    /// 全选
    SelectAll,
    /// 选择当前词
    SelectWord,
    /// 选择当前行
    SelectLine,
    
    // ==================== 滚动操作 ====================
    /// 向上滚动
    ScrollUp,
    /// 向下滚动
    ScrollDown,
    /// 翻页向上
    PageUp,
    /// 翻页向下
    PageDown,
    
    // ==================== 系统操作 ====================
    /// 退出编辑器
    Quit,
    /// 保存文件
    Save,
    /// 撤销
    Undo,
    /// 重做
    Redo,
    
    // ==================== 扩展点 ====================
    /// 自定义命令（用于插件系统）
    Custom(String),
}

impl Command {
    /// 获取命令的显示名称
    pub fn name(&self) -> &str {
        match self {
            Command::CursorLeft => "cursorLeft",
            Command::CursorRight => "cursorRight",
            Command::CursorUp => "cursorUp",
            Command::CursorDown => "cursorDown",
            Command::CursorLineStart => "cursorLineStart",
            Command::CursorLineEnd => "cursorLineEnd",
            Command::CursorFileStart => "cursorFileStart",
            Command::CursorFileEnd => "cursorFileEnd",
            Command::CursorWordLeft => "cursorWordLeft",
            Command::CursorWordRight => "cursorWordRight",
            Command::InsertChar(_) => "insertChar",
            Command::InsertNewline => "insertNewline",
            Command::InsertTab => "insertTab",
            Command::DeleteBackward => "deleteBackward",
            Command::DeleteForward => "deleteForward",
            Command::DeleteLine => "deleteLine",
            Command::DeleteToLineEnd => "deleteToLineEnd",
            Command::DeleteSelection => "deleteSelection",
            Command::ClearSelection => "clearSelection",
            Command::SelectAll => "selectAll",
            Command::SelectWord => "selectWord",
            Command::SelectLine => "selectLine",
            Command::ScrollUp => "scrollUp",
            Command::ScrollDown => "scrollDown",
            Command::PageUp => "pageUp",
            Command::PageDown => "pageDown",
            Command::Quit => "quit",
            Command::Save => "save",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Custom(name) => name,
        }
    }
}

/// 按键描述（包含修饰键）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl Key {
    /// 创建新按键
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
    
    /// 创建无修饰键的按键
    pub fn simple(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::NONE,
        }
    }
    
    /// 创建 Ctrl + 按键
    pub fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::CONTROL,
        }
    }
    
    /// 创建 Alt + 按键
    pub fn alt(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::ALT,
        }
    }
    
    /// 创建 Shift + 按键
    pub fn shift(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: KeyModifiers::SHIFT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_command_names() {
        assert_eq!(Command::CursorLeft.name(), "cursorLeft");
        assert_eq!(Command::InsertChar('a').name(), "insertChar");
        assert_eq!(Command::Quit.name(), "quit");
    }
    
    #[test]
    fn test_key_creation() {
        let key = Key::simple(KeyCode::Left);
        assert_eq!(key.modifiers, KeyModifiers::NONE);
        
        let ctrl_s = Key::ctrl(KeyCode::Char('s'));
        assert_eq!(ctrl_s.modifiers, KeyModifiers::CONTROL);
    }
}

