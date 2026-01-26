use std::path::PathBuf;
use std::time::Instant;

use crate::core::Command;
use crate::kernel::services::ports::LspCodeAction;
use crate::kernel::editor::EditorAction;
use crate::kernel::locations::LocationItem;
use crate::kernel::problems::ProblemItem;
use crate::kernel::search::SearchViewport;
use crate::kernel::services::ports::LspCommand;
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::GlobalSearchMessage;
use crate::kernel::services::ports::LspCompletionItem;
use crate::kernel::services::ports::LspFoldingRange;
use crate::kernel::services::ports::LspInlayHint;
use crate::kernel::services::ports::LspSemanticToken;
use crate::kernel::services::ports::LspServerCapabilities;
use crate::kernel::services::ports::LspTextEdit;
use crate::kernel::services::ports::LspWorkspaceEdit;
use crate::kernel::symbols::SymbolItem;
use crate::kernel::state::BottomPanelTab;

#[derive(Debug, Clone)]
pub enum Action {
    RunCommand(Command),
    Editor(EditorAction),
    OpenPath(PathBuf),
    Tick,
    EditorConfigUpdated {
        config: EditorConfig,
    },
    InputDialogAppend(char),
    InputDialogBackspace,
    InputDialogCursorLeft,
    InputDialogCursorRight,
    InputDialogAccept,
    InputDialogCancel,
    PaletteAppend(char),
    PaletteBackspace,
    PaletteMoveSelection(isize),
    PaletteClose,
    EditorSetActivePane {
        pane: usize,
    },
    EditorSetSplitRatio {
        ratio: u16,
    },
    ExplorerSetViewHeight {
        height: usize,
    },
    ExplorerMoveSelection {
        delta: isize,
    },
    ExplorerScroll {
        delta: isize,
    },
    ExplorerActivate,
    ExplorerCollapse,
    ExplorerClickRow {
        row: usize,
        now: Instant,
    },
    ExplorerContextMenuOpen {
        tree_row: Option<usize>,
        x: u16,
        y: u16,
    },
    ExplorerContextMenuClose,
    ExplorerContextMenuMoveSelection {
        delta: isize,
    },
    ExplorerContextMenuSetSelected {
        index: usize,
    },
    ExplorerContextMenuConfirm,
    BottomPanelSetActiveTab {
        tab: BottomPanelTab,
    },
    SearchSetViewHeight {
        viewport: SearchViewport,
        height: usize,
    },
    SearchAppend(char),
    SearchBackspace,
    SearchCursorLeft,
    SearchCursorRight,
    SearchToggleCaseSensitive,
    SearchToggleRegex,
    SearchMoveSelection {
        delta: isize,
        viewport: SearchViewport,
    },
    SearchScroll {
        delta: isize,
        viewport: SearchViewport,
    },
    SearchClickRow {
        row: usize,
        viewport: SearchViewport,
    },
    SearchStart,
    SearchStarted {
        search_id: u64,
    },
    SearchMessage(GlobalSearchMessage),
    ProblemsClickRow {
        row: usize,
    },
    ProblemsSetViewHeight {
        height: usize,
    },
    CodeActionsClickRow {
        row: usize,
    },
    CodeActionsSetViewHeight {
        height: usize,
    },
    LocationsClickRow {
        row: usize,
    },
    LocationsSetViewHeight {
        height: usize,
    },
    SymbolsClickRow {
        row: usize,
    },
    SymbolsSetViewHeight {
        height: usize,
    },
    LspDiagnostics {
        path: PathBuf,
        items: Vec<ProblemItem>,
    },
    LspHover {
        text: String,
    },
    LspDefinition {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspReferences {
        items: Vec<LocationItem>,
    },
    LspCodeActions {
        items: Vec<LspCodeAction>,
    },
    LspSymbols {
        items: Vec<SymbolItem>,
    },
    LspServerCapabilities {
        capabilities: LspServerCapabilities,
    },
    LspSemanticTokens {
        path: PathBuf,
        version: u64,
        tokens: Vec<LspSemanticToken>,
    },
    LspInlayHints {
        path: PathBuf,
        version: u64,
        range: crate::kernel::services::ports::LspRange,
        hints: Vec<LspInlayHint>,
    },
    LspFoldingRanges {
        path: PathBuf,
        version: u64,
        ranges: Vec<LspFoldingRange>,
    },
    LspCompletion {
        items: Vec<LspCompletionItem>,
        is_incomplete: bool,
    },
    LspCompletionResolved {
        id: u64,
        detail: Option<String>,
        documentation: Option<String>,
        additional_text_edits: Vec<LspTextEdit>,
        command: Option<LspCommand>,
    },
    LspSignatureHelp {
        text: String,
    },
    LspApplyWorkspaceEdit {
        edit: LspWorkspaceEdit,
    },
    LspFormatCompleted {
        path: PathBuf,
    },
    CompletionClose,
    CompletionMoveSelection {
        delta: isize,
    },
    CompletionConfirm,
    DirLoaded {
        path: PathBuf,
        entries: Vec<DirEntryInfo>,
    },
    DirLoadError {
        path: PathBuf,
    },
    SetHoveredTab {
        pane: usize,
        index: usize,
    },
    ClearHoveredTab,
    ShowConfirmDialog {
        message: String,
        on_confirm: crate::kernel::state::PendingAction,
    },
    ConfirmDialogAccept,
    ConfirmDialogCancel,
    ExplorerPathCreated {
        path: PathBuf,
        is_dir: bool,
    },
    ExplorerPathDeleted {
        path: PathBuf,
    },
    ExplorerPathRenamed {
        from: PathBuf,
        to: PathBuf,
    },
}
