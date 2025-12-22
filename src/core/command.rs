//! 命令系统：语义命令定义
//!
//! 架构：
//! - Command: 语义命令枚举（不关心具体按键）
//! - 支持命令历史（undo/redo）
//! - 支持宏录制
//! - 支持自定义命令扩展

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    // ==================== 光标移动 ====================
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
    CursorLineStart,
    CursorLineEnd,
    CursorFileStart,
    CursorFileEnd,
    CursorWordLeft,
    CursorWordRight,

    // ==================== 编辑操作 ====================
    InsertChar(char),
    InsertNewline,
    InsertTab,
    DeleteBackward,
    DeleteForward,
    DeleteLine,
    DeleteToLineEnd,
    DeleteSelection,

    // ==================== 选择操作 ====================
    ClearSelection,
    SelectAll,
    SelectWord,
    SelectLine,

    // ==================== 滚动操作 ====================
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,

    // ==================== 文件操作 ====================
    Save,
    SaveAs,
    OpenFile,
    CloseTab,
    NextTab,
    PrevTab,

    // ==================== 系统操作 ====================
    Quit,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,

    // ==================== 查找替换 ====================
    Find,
    FindNext,
    FindPrev,
    Replace,

    // ==================== 视图操作 ====================
    ToggleSidebar,
    FocusExplorer,
    FocusEditor,
    CommandPalette,

    // ==================== 扩展点 ====================
    Custom(String),
}

impl Command {
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
            Command::Save => "save",
            Command::SaveAs => "saveAs",
            Command::OpenFile => "openFile",
            Command::CloseTab => "closeTab",
            Command::NextTab => "nextTab",
            Command::PrevTab => "prevTab",
            Command::Quit => "quit",
            Command::Undo => "undo",
            Command::Redo => "redo",
            Command::Copy => "copy",
            Command::Cut => "cut",
            Command::Paste => "paste",
            Command::Find => "find",
            Command::FindNext => "findNext",
            Command::FindPrev => "findPrev",
            Command::Replace => "replace",
            Command::ToggleSidebar => "toggleSidebar",
            Command::FocusExplorer => "focusExplorer",
            Command::FocusEditor => "focusEditor",
            Command::CommandPalette => "commandPalette",
            Command::Custom(name) => name,
        }
    }

    pub fn is_edit_command(&self) -> bool {
        matches!(
            self,
            Command::InsertChar(_)
                | Command::InsertNewline
                | Command::InsertTab
                | Command::DeleteBackward
                | Command::DeleteForward
                | Command::DeleteLine
                | Command::DeleteToLineEnd
                | Command::DeleteSelection
                | Command::Paste
                | Command::Cut
        )
    }

    pub fn is_cursor_command(&self) -> bool {
        matches!(
            self,
            Command::CursorLeft
                | Command::CursorRight
                | Command::CursorUp
                | Command::CursorDown
                | Command::CursorLineStart
                | Command::CursorLineEnd
                | Command::CursorFileStart
                | Command::CursorFileEnd
                | Command::CursorWordLeft
                | Command::CursorWordRight
        )
    }

    pub fn is_selection_command(&self) -> bool {
        matches!(
            self,
            Command::ClearSelection
                | Command::SelectAll
                | Command::SelectWord
                | Command::SelectLine
        )
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
        assert_eq!(Command::Custom("myCommand".to_string()).name(), "myCommand");
    }

    #[test]
    fn test_is_edit_command() {
        assert!(Command::InsertChar('a').is_edit_command());
        assert!(Command::DeleteBackward.is_edit_command());
        assert!(Command::Paste.is_edit_command());
        assert!(!Command::CursorLeft.is_edit_command());
        assert!(!Command::Save.is_edit_command());
    }

    #[test]
    fn test_is_cursor_command() {
        assert!(Command::CursorLeft.is_cursor_command());
        assert!(Command::CursorFileEnd.is_cursor_command());
        assert!(!Command::InsertChar('a').is_cursor_command());
    }

    #[test]
    fn test_is_selection_command() {
        assert!(Command::SelectAll.is_selection_command());
        assert!(Command::ClearSelection.is_selection_command());
        assert!(!Command::CursorLeft.is_selection_command());
    }
}
