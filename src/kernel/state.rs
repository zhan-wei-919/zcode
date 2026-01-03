use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::models::{FileTree, FileTreeRow, LoadState, NodeId, NodeKind};
use crate::runtime::DirEntryInfo;
use crate::kernel::services::ports::EditorConfig;

use super::effect::Effect;
use super::editor::EditorState;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomPanelTab {
    Problems,
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
pub struct BottomPanelState {
    pub visible: bool,
    pub active_tab: BottomPanelTab,
}

#[derive(Debug, Clone)]
pub struct PendingEditorNavigation {
    pub pane: usize,
    pub path: PathBuf,
    pub byte_offset: usize,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub sidebar_visible: bool,
    pub sidebar_tab: SidebarTab,
    pub bottom_panel: BottomPanelState,
    pub focus: FocusTarget,
    pub editor_layout: EditorLayoutState,
    pub command_palette: CommandPaletteState,
    pub pending_editor_nav: Option<PendingEditorNavigation>,
    pub should_quit: bool,
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
            pending_editor_nav: None,
            should_quit: false,
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub workspace_root: PathBuf,
    pub ui: UiState,
    pub explorer: ExplorerState,
    pub search: SearchState,
    pub editor: EditorState,
}

impl AppState {
    pub fn new(workspace_root: PathBuf, file_tree: FileTree) -> Self {
        let editor = EditorState::new(EditorConfig::default());
        Self {
            workspace_root,
            ui: UiState::default(),
            explorer: ExplorerState::new(file_tree),
            search: SearchState::default(),
            editor,
        }
    }
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

        let current_index = self
            .tree
            .selected()
            .and_then(|id| self.index_by_id.get(&id).copied())
            .unwrap_or(0);

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

        let max_scroll = self
            .rows
            .len()
            .saturating_sub(self.view_height.max(1));
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

        (false, Vec::new())
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

    pub fn click_row(
        &mut self,
        row: usize,
        now: Instant,
    ) -> (bool, Vec<Effect>) {
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
