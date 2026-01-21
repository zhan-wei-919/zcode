use std::path::PathBuf;
use std::time::Instant;

use crate::core::Command;
use crate::kernel::editor::EditorAction;
use crate::kernel::problems::ProblemItem;
use crate::kernel::search::SearchViewport;
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::GlobalSearchMessage;
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
}
