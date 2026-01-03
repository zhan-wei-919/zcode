use std::path::PathBuf;
use std::time::Instant;

use crate::core::Command;
use crate::kernel::editor::EditorAction;
use crate::kernel::state::BottomPanelTab;
use crate::runtime::DirEntryInfo;
use crate::kernel::services::ports::GlobalSearchMessage;
use crate::kernel::search::SearchViewport;

#[derive(Debug, Clone)]
pub enum Action {
    RunCommand(Command),
    Editor(EditorAction),
    OpenPath(PathBuf),
    Tick,
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
    DirLoaded {
        path: PathBuf,
        entries: Vec<DirEntryInfo>,
    },
    DirLoadError {
        path: PathBuf,
    },
}
