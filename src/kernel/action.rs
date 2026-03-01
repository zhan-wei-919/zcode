use std::path::PathBuf;
use std::time::Instant;

use crate::core::Command;
use crate::kernel::editor::EditorAction;
use crate::kernel::panel::locations::LocationItem;
use crate::kernel::panel::problems::ProblemItem;
use crate::kernel::panel::symbols::SymbolItem;
use crate::kernel::search::SearchViewport;
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::GlobalSearchMessage;
use crate::kernel::services::ports::LspCodeAction;
use crate::kernel::services::ports::LspCommand;
use crate::kernel::services::ports::LspCompletionItem;
use crate::kernel::services::ports::LspFoldingRange;
use crate::kernel::services::ports::LspInlayHint;
use crate::kernel::services::ports::LspSemanticToken;
use crate::kernel::services::ports::LspServerCapabilities;
use crate::kernel::services::ports::LspServerKind;
use crate::kernel::services::ports::LspTextEdit;
use crate::kernel::services::ports::LspWorkspaceEdit;
use crate::kernel::state::{BottomPanelTab, PreviewLanguage, ThemeEditorFocus};
use crate::kernel::{GitFileStatus, GitGutterMarks, GitHead, GitWorktreeItem, TerminalId};

#[derive(Debug, Clone)]
pub enum Action {
    RunCommand(Command),
    Editor(EditorAction),
    OpenPath(PathBuf),
    Tick,
    GitInit,
    GitRepoDetected {
        repo_root: PathBuf,
        head: GitHead,
        worktrees: Vec<GitWorktreeItem>,
    },
    GitRepoCleared,
    GitStatusUpdated {
        statuses: Vec<(PathBuf, GitFileStatus)>,
    },
    GitDiffUpdated {
        path: PathBuf,
        marks: GitGutterMarks,
    },
    GitWorktreesUpdated {
        worktrees: Vec<GitWorktreeItem>,
    },
    GitBranchesUpdated {
        branches: Vec<String>,
    },
    GitWorktreeResolved {
        path: PathBuf,
    },
    GitCheckoutBranch {
        branch: String,
    },
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
    SidebarSetWidth {
        width: u16,
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
    ContextMenuOpen {
        request: crate::kernel::state::ContextMenuRequest,
        x: u16,
        y: u16,
    },
    ContextMenuClose,
    ContextMenuMoveSelection {
        delta: isize,
    },
    ContextMenuSetSelected {
        index: usize,
    },
    ContextMenuConfirm,
    ExplorerMovePath {
        from: PathBuf,
        to: PathBuf,
    },
    BottomPanelSetActiveTab {
        tab: BottomPanelTab,
    },
    BottomPanelSetHeightRatio {
        ratio: u16,
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
    TerminalWrite {
        id: TerminalId,
        bytes: Vec<u8>,
    },
    TerminalResize {
        id: TerminalId,
        cols: u16,
        rows: u16,
    },
    TerminalScroll {
        id: TerminalId,
        delta: isize,
    },
    TerminalSpawned {
        id: TerminalId,
        title: String,
    },
    TerminalOutput {
        id: TerminalId,
        bytes: Vec<u8>,
    },
    TerminalExited {
        id: TerminalId,
        code: Option<i32>,
    },
    LspDiagnostics {
        path: PathBuf,
        items: Vec<ProblemItem>,
    },
    LspHover {
        text: String,
    },
    LspHoverResponse {
        session: i32,
        text: String,
    },
    LspHoverDefinitionPreview {
        session: i32,
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
        server: LspServerKind,
        root: PathBuf,
        capabilities: LspServerCapabilities,
    },
    LspSemanticTokens {
        path: PathBuf,
        version: u64,
        tokens: Vec<LspSemanticToken>,
    },
    LspSemanticTokensRange {
        path: PathBuf,
        version: u64,
        range: crate::kernel::services::ports::LspRange,
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
        insert_text: Option<String>,
        insert_text_format: Option<crate::kernel::services::ports::LspInsertTextFormat>,
        insert_range: Option<crate::kernel::services::ports::LspRange>,
        replace_range: Option<crate::kernel::services::ports::LspRange>,
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
    LspProgressEnd,
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
    ExplorerDirChanged {
        path: PathBuf,
    },
    ThemeEditorOpen,
    ThemeEditorClose,
    ThemeEditorMoveTokenSelection {
        delta: isize,
    },
    ThemeEditorSetFocus {
        focus: ThemeEditorFocus,
    },
    ThemeEditorAdjustHue {
        delta: i16,
    },
    ThemeEditorSetHue {
        hue: u16,
    },
    ThemeEditorAdjustSaturation {
        delta: i8,
    },
    ThemeEditorAdjustLightness {
        delta: i8,
    },
    ThemeEditorSetSaturationLightness {
        saturation: u8,
        lightness: u8,
    },
    ThemeEditorSetAnsiIndex {
        index: u8,
    },
    ThemeEditorCycleLanguage,
    ThemeEditorSetLanguage {
        language: PreviewLanguage,
    },
    ThemeEditorResetToken,
}
