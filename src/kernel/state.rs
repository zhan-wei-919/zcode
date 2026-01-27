use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::core::Command;
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::LspCompletionItem;
use crate::kernel::services::ports::LspServerCapabilities;
use crate::kernel::{CodeActionsState, LocationsState, ProblemsState, SymbolsState};
use crate::models::{should_ignore, FileTree, FileTreeRow, LoadState, NodeId, NodeKind};

use super::editor::EditorState;
use super::effect::Effect;
use super::search::SearchState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Explorer,
    Editor,
    BottomPanel,
    CommandPalette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Explorer,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BottomPanelTab {
    Problems,
    CodeActions,
    Locations,
    Symbols,
    SearchResults,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone)]
pub struct EditorLayoutState {
    pub panes: usize,
    pub active_pane: usize,
    pub split_ratio: u16,
    pub split_direction: SplitDirection,
}

impl Default for EditorLayoutState {
    fn default() -> Self {
        Self {
            panes: 1,
            active_pane: 0,
            split_ratio: 500,
            split_direction: SplitDirection::Vertical,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandPaletteState {
    pub visible: bool,
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub enum InputDialogKind {
    NewFile {
        parent_dir: PathBuf,
    },
    NewFolder {
        parent_dir: PathBuf,
    },
    ExplorerRename {
        from: PathBuf,
    },
    LspRename {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspWorkspaceSymbols,
}

#[derive(Debug, Clone, Default)]
pub struct InputDialogState {
    pub visible: bool,
    pub title: String,
    pub value: String,
    pub cursor: usize,
    pub error: Option<String>,
    pub kind: Option<InputDialogKind>,
}

#[derive(Debug, Clone)]
pub struct BottomPanelState {
    pub visible: bool,
    pub active_tab: BottomPanelTab,
}

#[derive(Debug, Clone)]
pub enum PendingEditorNavigationTarget {
    ByteOffset { byte_offset: usize },
    LineColumn { line: u32, column: u32 },
}

#[derive(Debug, Clone)]
pub struct PendingEditorNavigation {
    pub pane: usize,
    pub path: PathBuf,
    pub target: PendingEditorNavigationTarget,
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    CloseTab { pane: usize, index: usize },
    DeletePath { path: PathBuf, is_dir: bool },
}

#[derive(Debug, Clone, Default)]
pub struct ConfirmDialogState {
    pub visible: bool,
    pub message: String,
    pub on_confirm: Option<PendingAction>,
}

#[derive(Debug, Clone)]
pub struct CompletionRequestContext {
    pub pane: usize,
    pub path: PathBuf,
    pub version: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionPopupState {
    pub visible: bool,
    pub all_items: Vec<LspCompletionItem>,
    pub items: Vec<LspCompletionItem>,
    pub selected: usize,
    pub request: Option<CompletionRequestContext>,
    pub pending_request: Option<CompletionRequestContext>,
    pub is_incomplete: bool,
    pub resolve_inflight: Option<u64>,
    pub session_started_at: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct SignatureHelpRequestContext {
    pub pane: usize,
    pub path: PathBuf,
    pub version: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SignatureHelpPopupState {
    pub visible: bool,
    pub text: String,
    pub request: Option<SignatureHelpRequestContext>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerContextMenuItem {
    NewFile,
    NewFolder,
    Rename,
    Delete,
}

impl ExplorerContextMenuItem {
    pub fn label(self) -> &'static str {
        match self {
            ExplorerContextMenuItem::NewFile => "New File",
            ExplorerContextMenuItem::NewFolder => "New Folder",
            ExplorerContextMenuItem::Rename => "Rename",
            ExplorerContextMenuItem::Delete => "Delete",
        }
    }

    pub fn command(self) -> Command {
        match self {
            ExplorerContextMenuItem::NewFile => Command::ExplorerNewFile,
            ExplorerContextMenuItem::NewFolder => Command::ExplorerNewFolder,
            ExplorerContextMenuItem::Rename => Command::ExplorerRename,
            ExplorerContextMenuItem::Delete => Command::ExplorerDelete,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExplorerContextMenuState {
    pub visible: bool,
    pub anchor: (u16, u16),
    pub selected: usize,
    pub items: Vec<ExplorerContextMenuItem>,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub sidebar_visible: bool,
    pub sidebar_tab: SidebarTab,
    pub bottom_panel: BottomPanelState,
    pub focus: FocusTarget,
    pub editor_layout: EditorLayoutState,
    pub command_palette: CommandPaletteState,
    pub input_dialog: InputDialogState,
    pub explorer_context_menu: ExplorerContextMenuState,
    pub pending_editor_nav: Option<PendingEditorNavigation>,
    pub should_quit: bool,
    pub hovered_tab: Option<(usize, usize)>,
    pub confirm_dialog: ConfirmDialogState,
    pub hover_message: Option<String>,
    pub signature_help: SignatureHelpPopupState,
    pub completion: CompletionPopupState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            sidebar_tab: SidebarTab::Explorer,
            bottom_panel: BottomPanelState {
                visible: false,
                active_tab: BottomPanelTab::Problems,
            },
            focus: FocusTarget::Editor,
            editor_layout: EditorLayoutState::default(),
            command_palette: CommandPaletteState {
                visible: false,
                query: String::new(),
                selected: 0,
            },
            input_dialog: InputDialogState::default(),
            explorer_context_menu: ExplorerContextMenuState::default(),
            pending_editor_nav: None,
            should_quit: false,
            hovered_tab: None,
            confirm_dialog: ConfirmDialogState::default(),
            hover_message: None,
            signature_help: SignatureHelpPopupState::default(),
            completion: CompletionPopupState::default(),
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub workspace_root: PathBuf,
    pub ui: UiState,
    pub lsp: LspState,
    pub explorer: ExplorerState,
    pub search: SearchState,
    pub editor: EditorState,
    pub problems: ProblemsState,
    pub code_actions: CodeActionsState,
    pub locations: LocationsState,
    pub symbols: SymbolsState,
}

impl AppState {
    pub fn new(workspace_root: PathBuf, file_tree: FileTree, editor_config: EditorConfig) -> Self {
        let editor = EditorState::new(editor_config);
        Self {
            workspace_root,
            ui: UiState::default(),
            lsp: LspState::default(),
            explorer: ExplorerState::new(file_tree),
            search: SearchState::default(),
            editor,
            problems: ProblemsState::default(),
            code_actions: CodeActionsState::default(),
            locations: LocationsState::default(),
            symbols: SymbolsState::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LspState {
    pub server_capabilities: Option<LspServerCapabilities>,
    pub pending_format_on_save: Option<PathBuf>,
}

pub struct ExplorerState {
    tree: FileTree,
    pub view_height: usize,
    pub scroll_offset: usize,
    pub rows: Vec<FileTreeRow>,
    index_by_id: FxHashMap<NodeId, usize>,
    last_click: Option<(Instant, NodeId)>,
}

impl std::fmt::Debug for ExplorerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExplorerState")
            .field("scroll_offset", &self.scroll_offset)
            .field("rows_len", &self.rows.len())
            .field("selected", &self.tree.selected())
            .finish()
    }
}

impl ExplorerState {
    const DOUBLE_CLICK_MS: u64 = 300;

    pub fn new(tree: FileTree) -> Self {
        let mut state = Self {
            tree,
            view_height: 10,
            scroll_offset: 0,
            rows: Vec::new(),
            index_by_id: FxHashMap::default(),
            last_click: None,
        };
        state.refresh_rows();
        state
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.tree.selected()
    }

    pub fn set_view_height(&mut self, height: usize) -> bool {
        let height = height.max(1);
        if self.view_height == height {
            return false;
        }
        self.view_height = height;

        if let Some(selected) = self.tree.selected() {
            if let Some(index) = self.index_by_id.get(&selected).copied() {
                self.keep_row_visible(index);
            }
        } else {
            self.clamp_scroll();
        }

        true
    }

    pub fn move_selection(&mut self, delta: isize) -> bool {
        if self.rows.is_empty() || delta == 0 {
            return false;
        }

        let current_index = match self
            .tree
            .selected()
            .and_then(|id| self.index_by_id.get(&id).copied())
        {
            Some(index) => index,
            None => {
                let new_index = if delta < 0 { self.rows.len() - 1 } else { 0 };
                let new_id = self.rows[new_index].id;
                self.tree.set_selected(Some(new_id));
                self.keep_row_visible(new_index);
                return true;
            }
        };

        let new_index = if delta < 0 {
            current_index.saturating_sub((-delta) as usize)
        } else {
            (current_index + delta as usize).min(self.rows.len() - 1)
        };

        if new_index == current_index {
            return false;
        }

        let new_id = self.rows[new_index].id;
        self.tree.set_selected(Some(new_id));
        self.keep_row_visible(new_index);
        true
    }

    pub fn scroll(&mut self, delta: isize) -> bool {
        if self.rows.is_empty() || delta == 0 {
            return false;
        }

        let max_scroll = self.rows.len().saturating_sub(self.view_height.max(1));
        let prev = self.scroll_offset;

        if delta > 0 {
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max_scroll);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        }

        self.scroll_offset != prev
    }

    pub fn activate_selected(&mut self) -> (bool, Vec<Effect>) {
        let Some(id) = self.tree.selected() else {
            return (false, Vec::new());
        };

        if self.tree.is_dir(id) {
            return self.toggle_dir(id);
        }

        let path = self.tree.full_path(id);
        (false, vec![Effect::LoadFile(path)])
    }

    pub fn collapse_selected(&mut self) -> bool {
        let Some(id) = self.tree.selected() else {
            return false;
        };
        if self.tree.is_dir(id) && self.tree.is_expanded(id) {
            self.tree.collapse(id);
            self.refresh_rows();
            return true;
        }
        false
    }

    pub fn click_row(&mut self, row: usize, now: Instant) -> (bool, Vec<Effect>) {
        if row >= self.rows.len() {
            return (false, Vec::new());
        }

        let node_id = self.rows[row].id;

        let is_double_click = self
            .last_click
            .map(|(last_time, last_id)| {
                last_id == node_id
                    && now.duration_since(last_time).as_millis() as u64 <= Self::DOUBLE_CLICK_MS
            })
            .unwrap_or(false);

        if is_double_click {
            self.last_click = None;
            if self.tree.is_dir(node_id) {
                return self.toggle_dir(node_id);
            }
            let path = self.tree.full_path(node_id);
            return (false, vec![Effect::LoadFile(path)]);
        }

        self.last_click = Some((now, node_id));

        let prev_selected = self.tree.selected();
        self.tree.set_selected(Some(node_id));
        if let Some(index) = self.index_by_id.get(&node_id).copied() {
            self.keep_row_visible(index);
        }

        (prev_selected != Some(node_id), Vec::new())
    }

    pub fn select_row(&mut self, row: usize) -> bool {
        if row >= self.rows.len() {
            return false;
        }

        let node_id = self.rows[row].id;
        let prev_selected = self.tree.selected();
        self.tree.set_selected(Some(node_id));
        if let Some(index) = self.index_by_id.get(&node_id).copied() {
            self.keep_row_visible(index);
        }
        prev_selected != Some(node_id)
    }

    pub fn selected_create_parent_dir(&mut self) -> PathBuf {
        let root = self.tree.absolute_root().to_path_buf();
        let Some(id) = self.tree.selected() else {
            return root;
        };

        let path = self.tree.full_path(id);
        if self.tree.is_dir(id) {
            return path;
        }
        path.parent().unwrap_or(&root).to_path_buf()
    }

    pub fn selected_path_and_kind(&mut self) -> Option<(PathBuf, bool)> {
        let id = self.tree.selected()?;
        let path = self.tree.full_path(id);
        Some((path, self.tree.is_dir(id)))
    }

    pub fn apply_path_created(&mut self, path: PathBuf, is_dir: bool) -> bool {
        let Some(parent) = path.parent() else {
            return false;
        };
        let Some(name) = path.file_name() else {
            return false;
        };
        if should_ignore(&name.to_string_lossy()) {
            return false;
        }

        let Some(parent_id) = self.tree.find_node_by_path(parent) else {
            return false;
        };
        if !self.tree.is_dir(parent_id) {
            return false;
        }

        let kind = if is_dir {
            NodeKind::Dir
        } else {
            NodeKind::File
        };
        if self
            .tree
            .insert_child(parent_id, name.to_os_string(), kind)
            .is_ok()
        {
            self.refresh_rows();
            return true;
        }

        false
    }

    pub fn apply_path_deleted(&mut self, path: PathBuf) -> bool {
        let Some(id) = self.tree.find_node_by_path(&path) else {
            return false;
        };
        if self.tree.delete(id).is_ok() {
            self.refresh_rows();
            return true;
        }
        false
    }

    pub fn apply_path_renamed(&mut self, from: PathBuf, to: PathBuf) -> bool {
        if from == to {
            return false;
        }

        let Some(id) = self.tree.find_node_by_path(&from) else {
            return false;
        };
        let is_dir = self.tree.is_dir(id);

        let same_parent = from
            .parent()
            .and_then(|a| to.parent().map(|b| a == b))
            .unwrap_or(false);
        if same_parent {
            let Some(name) = to.file_name() else {
                return false;
            };
            if self.tree.rename(id, name.to_os_string()).is_ok() {
                self.refresh_rows();
                return true;
            }
            return false;
        }

        let mut changed = false;
        if let Some(parent) = to.parent().and_then(|p| self.tree.find_node_by_path(p)) {
            if self.tree.is_dir(parent) {
                if self.tree.move_to(id, parent).is_ok() {
                    changed = true;
                }
                if let Some(name) = to.file_name() {
                    if self.tree.rename(id, name.to_os_string()).is_ok() {
                        changed = true;
                    }
                }
            }
        }

        if changed {
            self.refresh_rows();
            return true;
        }

        let deleted = self.apply_path_deleted(from);
        let created = self.apply_path_created(to, is_dir);
        deleted || created
    }

    pub fn apply_dir_loaded(&mut self, path: PathBuf, entries: Vec<DirEntryInfo>) -> bool {
        let Some(node_id) = self.tree.find_node_by_path(&path) else {
            return false;
        };

        for entry in entries {
            let kind = if entry.is_dir {
                NodeKind::Dir
            } else {
                NodeKind::File
            };
            let _ = self.tree.insert_child(node_id, entry.name.into(), kind);
        }

        self.tree.set_load_state(node_id, LoadState::Loaded);
        self.refresh_rows();
        true
    }

    pub fn apply_dir_load_error(&mut self, path: PathBuf) -> bool {
        let Some(node_id) = self.tree.find_node_by_path(&path) else {
            return false;
        };

        self.tree.set_load_state(node_id, LoadState::NotLoaded);
        self.tree.collapse(node_id);
        self.refresh_rows();
        true
    }

    fn toggle_dir(&mut self, id: NodeId) -> (bool, Vec<Effect>) {
        if self.tree.is_expanded(id) {
            self.tree.collapse(id);
            self.refresh_rows();
            return (true, Vec::new());
        }

        match self.tree.load_state(id) {
            Some(LoadState::NotLoaded) => {
                self.tree.set_load_state(id, LoadState::Loading);
                self.tree.expand(id);
                self.refresh_rows();
                let path = self.tree.full_path(id);
                (true, vec![Effect::LoadDir(path)])
            }
            Some(LoadState::Loading) => (false, Vec::new()),
            Some(LoadState::Loaded) | None => {
                self.tree.expand(id);
                self.refresh_rows();
                (true, Vec::new())
            }
        }
    }

    fn refresh_rows(&mut self) {
        self.rows = self.tree.flatten_for_view();

        self.index_by_id.clear();
        self.index_by_id.reserve(self.rows.len());
        for (i, row) in self.rows.iter().enumerate() {
            self.index_by_id.insert(row.id, i);
        }

        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let view_height = self.view_height.max(1);
        let max_scroll = self.rows.len().saturating_sub(view_height);
        self.scroll_offset = self.scroll_offset.min(max_scroll);
    }

    fn keep_row_visible(&mut self, row_index: usize) {
        let view_height = self.view_height.max(1);

        if row_index < self.scroll_offset {
            self.scroll_offset = row_index;
            self.clamp_scroll();
            return;
        }

        if row_index >= self.scroll_offset + view_height {
            self.scroll_offset = row_index.saturating_sub(view_height - 1);
        }

        self.clamp_scroll();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/state.rs"]
mod tests;
