//! 命令系统：语义命令定义
//!
//! 架构：
//! - Command: 语义命令枚举（不关心具体按键）
//! - 支持命令历史（undo/redo）
//! - 支持宏录制
//! - 支持自定义命令扩展

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    Escape,

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
    ExtendSelectionLeft,
    ExtendSelectionRight,
    ExtendSelectionUp,
    ExtendSelectionDown,
    ExtendSelectionLineStart,
    ExtendSelectionLineEnd,
    ExtendSelectionWordLeft,
    ExtendSelectionWordRight,

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

    // ==================== Explorer（侧边栏） ====================
    ExplorerUp,
    ExplorerDown,
    ExplorerActivate,
    ExplorerCollapse,
    ExplorerScrollUp,
    ExplorerScrollDown,
    ExplorerNewFile,
    ExplorerNewFolder,
    ExplorerRename,
    ExplorerDelete,

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

    // ==================== Editor Search Bar ====================
    EditorSearchBarClose,
    EditorSearchBarSwitchField,
    EditorSearchBarToggleCaseSensitive,
    EditorSearchBarToggleRegex,
    EditorSearchBarToggleReplaceMode,
    EditorSearchBarCursorLeft,
    EditorSearchBarCursorRight,
    EditorSearchBarCursorHome,
    EditorSearchBarCursorEnd,
    EditorSearchBarBackspace,
    EditorSearchBarDeleteForward,
    EditorSearchBarReplaceCurrent,
    EditorSearchBarReplaceAll,

    // ==================== Global Search（侧边栏 Search） ====================
    GlobalSearchStart,
    GlobalSearchCursorLeft,
    GlobalSearchCursorRight,
    GlobalSearchBackspace,
    GlobalSearchToggleCaseSensitive,
    GlobalSearchToggleRegex,

    // ==================== Search Results（侧边栏/底部面板共享） ====================
    SearchResultsMoveUp,
    SearchResultsMoveDown,
    SearchResultsScrollUp,
    SearchResultsScrollDown,
    SearchResultsToggleExpand,
    SearchResultsOpenSelected,

    // ==================== LSP ====================
    LspHover,
    LspDefinition,
    LspCompletion,
    LspSignatureHelp,
    LspFormat,
    LspFormatSelection,
    LspRename,
    LspReferences,
    LspCodeAction,
    LspDocumentSymbols,
    LspWorkspaceSymbols,
    LspSemanticTokens,
    LspInlayHints,
    LspFoldingRange,

    // ==================== Folding ====================
    EditorFoldToggle,
    EditorFold,
    EditorUnfold,

    // ==================== Command Palette（面板内部） ====================
    PaletteClose,
    PaletteMoveUp,
    PaletteMoveDown,
    PaletteBackspace,
    PaletteConfirm,

    // ==================== 视图操作 ====================
    ToggleSidebar,
    FocusExplorer,
    FocusSearch,
    ToggleSidebarTab,
    FocusEditor,
    SplitEditorVertical,
    SplitEditorHorizontal,
    CloseEditorSplit,
    FocusNextEditorPane,
    FocusPrevEditorPane,
    ToggleBottomPanel,
    FocusBottomPanel,
    NextBottomPanelTab,
    PrevBottomPanelTab,
    CommandPalette,
    ReloadSettings,
    OpenSettings,

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
            Command::ExtendSelectionLeft => "extendSelectionLeft",
            Command::ExtendSelectionRight => "extendSelectionRight",
            Command::ExtendSelectionUp => "extendSelectionUp",
            Command::ExtendSelectionDown => "extendSelectionDown",
            Command::ExtendSelectionLineStart => "extendSelectionLineStart",
            Command::ExtendSelectionLineEnd => "extendSelectionLineEnd",
            Command::ExtendSelectionWordLeft => "extendSelectionWordLeft",
            Command::ExtendSelectionWordRight => "extendSelectionWordRight",
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
            Command::ExplorerUp => "explorerUp",
            Command::ExplorerDown => "explorerDown",
            Command::ExplorerActivate => "explorerActivate",
            Command::ExplorerCollapse => "explorerCollapse",
            Command::ExplorerScrollUp => "explorerScrollUp",
            Command::ExplorerScrollDown => "explorerScrollDown",
            Command::ExplorerNewFile => "explorerNewFile",
            Command::ExplorerNewFolder => "explorerNewFolder",
            Command::ExplorerRename => "explorerRename",
            Command::ExplorerDelete => "explorerDelete",
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
            Command::EditorSearchBarClose => "editorSearchBarClose",
            Command::EditorSearchBarSwitchField => "editorSearchBarSwitchField",
            Command::EditorSearchBarToggleCaseSensitive => "editorSearchBarToggleCaseSensitive",
            Command::EditorSearchBarToggleRegex => "editorSearchBarToggleRegex",
            Command::EditorSearchBarToggleReplaceMode => "editorSearchBarToggleReplaceMode",
            Command::EditorSearchBarCursorLeft => "editorSearchBarCursorLeft",
            Command::EditorSearchBarCursorRight => "editorSearchBarCursorRight",
            Command::EditorSearchBarCursorHome => "editorSearchBarCursorHome",
            Command::EditorSearchBarCursorEnd => "editorSearchBarCursorEnd",
            Command::EditorSearchBarBackspace => "editorSearchBarBackspace",
            Command::EditorSearchBarDeleteForward => "editorSearchBarDeleteForward",
            Command::EditorSearchBarReplaceCurrent => "editorSearchBarReplaceCurrent",
            Command::EditorSearchBarReplaceAll => "editorSearchBarReplaceAll",
            Command::GlobalSearchStart => "globalSearchStart",
            Command::GlobalSearchCursorLeft => "globalSearchCursorLeft",
            Command::GlobalSearchCursorRight => "globalSearchCursorRight",
            Command::GlobalSearchBackspace => "globalSearchBackspace",
            Command::GlobalSearchToggleCaseSensitive => "globalSearchToggleCaseSensitive",
            Command::GlobalSearchToggleRegex => "globalSearchToggleRegex",
            Command::SearchResultsMoveUp => "searchResultsMoveUp",
            Command::SearchResultsMoveDown => "searchResultsMoveDown",
            Command::SearchResultsScrollUp => "searchResultsScrollUp",
            Command::SearchResultsScrollDown => "searchResultsScrollDown",
            Command::SearchResultsToggleExpand => "searchResultsToggleExpand",
            Command::SearchResultsOpenSelected => "searchResultsOpenSelected",
            Command::LspHover => "lspHover",
            Command::LspDefinition => "lspDefinition",
            Command::LspCompletion => "lspCompletion",
            Command::LspSignatureHelp => "lspSignatureHelp",
            Command::LspFormat => "lspFormat",
            Command::LspFormatSelection => "lspFormatSelection",
            Command::LspRename => "lspRename",
            Command::LspReferences => "lspReferences",
            Command::LspCodeAction => "lspCodeAction",
            Command::LspDocumentSymbols => "lspDocumentSymbols",
            Command::LspWorkspaceSymbols => "lspWorkspaceSymbols",
            Command::LspSemanticTokens => "lspSemanticTokens",
            Command::LspInlayHints => "lspInlayHints",
            Command::LspFoldingRange => "lspFoldingRange",
            Command::EditorFoldToggle => "editorFoldToggle",
            Command::EditorFold => "editorFold",
            Command::EditorUnfold => "editorUnfold",
            Command::PaletteClose => "paletteClose",
            Command::PaletteMoveUp => "paletteMoveUp",
            Command::PaletteMoveDown => "paletteMoveDown",
            Command::PaletteBackspace => "paletteBackspace",
            Command::PaletteConfirm => "paletteConfirm",
            Command::ToggleSidebar => "toggleSidebar",
            Command::FocusExplorer => "focusExplorer",
            Command::FocusSearch => "focusSearch",
            Command::ToggleSidebarTab => "toggleSidebarTab",
            Command::FocusEditor => "focusEditor",
            Command::SplitEditorVertical => "splitEditorVertical",
            Command::SplitEditorHorizontal => "splitEditorHorizontal",
            Command::CloseEditorSplit => "closeEditorSplit",
            Command::FocusNextEditorPane => "focusNextEditorPane",
            Command::FocusPrevEditorPane => "focusPrevEditorPane",
            Command::ToggleBottomPanel => "toggleBottomPanel",
            Command::FocusBottomPanel => "focusBottomPanel",
            Command::NextBottomPanelTab => "nextBottomPanelTab",
            Command::PrevBottomPanelTab => "prevBottomPanelTab",
            Command::CommandPalette => "commandPalette",
            Command::ReloadSettings => "reloadSettings",
            Command::OpenSettings => "openSettings",
            Command::Escape => "escape",
            Command::Custom(name) => name,
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "escape" => Command::Escape,
            "cursorLeft" => Command::CursorLeft,
            "cursorRight" => Command::CursorRight,
            "cursorUp" => Command::CursorUp,
            "cursorDown" => Command::CursorDown,
            "cursorLineStart" => Command::CursorLineStart,
            "cursorLineEnd" => Command::CursorLineEnd,
            "cursorFileStart" => Command::CursorFileStart,
            "cursorFileEnd" => Command::CursorFileEnd,
            "cursorWordLeft" => Command::CursorWordLeft,
            "cursorWordRight" => Command::CursorWordRight,
            "insertNewline" => Command::InsertNewline,
            "insertTab" => Command::InsertTab,
            "deleteBackward" => Command::DeleteBackward,
            "deleteForward" => Command::DeleteForward,
            "deleteLine" => Command::DeleteLine,
            "deleteToLineEnd" => Command::DeleteToLineEnd,
            "deleteSelection" => Command::DeleteSelection,
            "clearSelection" => Command::ClearSelection,
            "selectAll" => Command::SelectAll,
            "selectWord" => Command::SelectWord,
            "selectLine" => Command::SelectLine,
            "extendSelectionLeft" => Command::ExtendSelectionLeft,
            "extendSelectionRight" => Command::ExtendSelectionRight,
            "extendSelectionUp" => Command::ExtendSelectionUp,
            "extendSelectionDown" => Command::ExtendSelectionDown,
            "extendSelectionLineStart" => Command::ExtendSelectionLineStart,
            "extendSelectionLineEnd" => Command::ExtendSelectionLineEnd,
            "extendSelectionWordLeft" => Command::ExtendSelectionWordLeft,
            "extendSelectionWordRight" => Command::ExtendSelectionWordRight,
            "scrollUp" => Command::ScrollUp,
            "scrollDown" => Command::ScrollDown,
            "pageUp" => Command::PageUp,
            "pageDown" => Command::PageDown,
            "save" => Command::Save,
            "saveAs" => Command::SaveAs,
            "openFile" => Command::OpenFile,
            "closeTab" => Command::CloseTab,
            "nextTab" => Command::NextTab,
            "prevTab" => Command::PrevTab,
            "explorerUp" => Command::ExplorerUp,
            "explorerDown" => Command::ExplorerDown,
            "explorerActivate" => Command::ExplorerActivate,
            "explorerCollapse" => Command::ExplorerCollapse,
            "explorerScrollUp" => Command::ExplorerScrollUp,
            "explorerScrollDown" => Command::ExplorerScrollDown,
            "explorerNewFile" => Command::ExplorerNewFile,
            "explorerNewFolder" => Command::ExplorerNewFolder,
            "explorerRename" => Command::ExplorerRename,
            "explorerDelete" => Command::ExplorerDelete,
            "quit" => Command::Quit,
            "undo" => Command::Undo,
            "redo" => Command::Redo,
            "copy" => Command::Copy,
            "cut" => Command::Cut,
            "paste" => Command::Paste,
            "find" => Command::Find,
            "findNext" => Command::FindNext,
            "findPrev" => Command::FindPrev,
            "replace" => Command::Replace,
            "editorSearchBarClose" => Command::EditorSearchBarClose,
            "editorSearchBarSwitchField" => Command::EditorSearchBarSwitchField,
            "editorSearchBarToggleCaseSensitive" => Command::EditorSearchBarToggleCaseSensitive,
            "editorSearchBarToggleRegex" => Command::EditorSearchBarToggleRegex,
            "editorSearchBarToggleReplaceMode" => Command::EditorSearchBarToggleReplaceMode,
            "editorSearchBarCursorLeft" => Command::EditorSearchBarCursorLeft,
            "editorSearchBarCursorRight" => Command::EditorSearchBarCursorRight,
            "editorSearchBarCursorHome" => Command::EditorSearchBarCursorHome,
            "editorSearchBarCursorEnd" => Command::EditorSearchBarCursorEnd,
            "editorSearchBarBackspace" => Command::EditorSearchBarBackspace,
            "editorSearchBarDeleteForward" => Command::EditorSearchBarDeleteForward,
            "editorSearchBarReplaceCurrent" => Command::EditorSearchBarReplaceCurrent,
            "editorSearchBarReplaceAll" => Command::EditorSearchBarReplaceAll,
            "globalSearchStart" => Command::GlobalSearchStart,
            "globalSearchCursorLeft" => Command::GlobalSearchCursorLeft,
            "globalSearchCursorRight" => Command::GlobalSearchCursorRight,
            "globalSearchBackspace" => Command::GlobalSearchBackspace,
            "globalSearchToggleCaseSensitive" => Command::GlobalSearchToggleCaseSensitive,
            "globalSearchToggleRegex" => Command::GlobalSearchToggleRegex,
            "searchResultsMoveUp" => Command::SearchResultsMoveUp,
            "searchResultsMoveDown" => Command::SearchResultsMoveDown,
            "searchResultsScrollUp" => Command::SearchResultsScrollUp,
            "searchResultsScrollDown" => Command::SearchResultsScrollDown,
            "searchResultsToggleExpand" => Command::SearchResultsToggleExpand,
            "searchResultsOpenSelected" => Command::SearchResultsOpenSelected,
            "lspHover" => Command::LspHover,
            "lspDefinition" => Command::LspDefinition,
            "lspCompletion" => Command::LspCompletion,
            "lspSignatureHelp" => Command::LspSignatureHelp,
            "lspFormat" => Command::LspFormat,
            "lspFormatSelection" => Command::LspFormatSelection,
            "lspRename" => Command::LspRename,
            "lspReferences" => Command::LspReferences,
            "lspCodeAction" => Command::LspCodeAction,
            "lspDocumentSymbols" => Command::LspDocumentSymbols,
            "lspWorkspaceSymbols" => Command::LspWorkspaceSymbols,
            "lspSemanticTokens" => Command::LspSemanticTokens,
            "lspInlayHints" => Command::LspInlayHints,
            "lspFoldingRange" => Command::LspFoldingRange,
            "editorFoldToggle" => Command::EditorFoldToggle,
            "editorFold" => Command::EditorFold,
            "editorUnfold" => Command::EditorUnfold,
            "paletteClose" => Command::PaletteClose,
            "paletteMoveUp" => Command::PaletteMoveUp,
            "paletteMoveDown" => Command::PaletteMoveDown,
            "paletteBackspace" => Command::PaletteBackspace,
            "paletteConfirm" => Command::PaletteConfirm,
            "toggleSidebar" => Command::ToggleSidebar,
            "focusExplorer" => Command::FocusExplorer,
            "focusSearch" => Command::FocusSearch,
            "toggleSidebarTab" => Command::ToggleSidebarTab,
            "focusEditor" => Command::FocusEditor,
            "splitEditorVertical" => Command::SplitEditorVertical,
            "splitEditorHorizontal" => Command::SplitEditorHorizontal,
            "closeEditorSplit" => Command::CloseEditorSplit,
            "focusNextEditorPane" => Command::FocusNextEditorPane,
            "focusPrevEditorPane" => Command::FocusPrevEditorPane,
            "toggleBottomPanel" => Command::ToggleBottomPanel,
            "focusBottomPanel" => Command::FocusBottomPanel,
            "nextBottomPanelTab" => Command::NextBottomPanelTab,
            "prevBottomPanelTab" => Command::PrevBottomPanelTab,
            "commandPalette" => Command::CommandPalette,
            "reloadSettings" => Command::ReloadSettings,
            "openSettings" => Command::OpenSettings,
            other => Command::Custom(other.to_string()),
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
                | Command::ExtendSelectionLeft
                | Command::ExtendSelectionRight
                | Command::ExtendSelectionUp
                | Command::ExtendSelectionDown
                | Command::ExtendSelectionLineStart
                | Command::ExtendSelectionLineEnd
                | Command::ExtendSelectionWordLeft
                | Command::ExtendSelectionWordRight
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/core/command.rs"]
mod tests;
