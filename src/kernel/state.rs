use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::core::Command;
use crate::kernel::language::{
    CompletionEntry, CompletionNormalizationSnapshot, CompletionRecord, CompletionResolveState,
    HoverModel, HoverSectionModel, SignatureHelpModel,
};
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::LspClientKey;
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
    Overlay,
    CommandLine,
}

/// 居中 telescope 浮层承载的列表种类。条目 / 选择 / 滚动仍住在各自面板态里
/// （`ProblemsState` 等），这里只记录当前活动的是哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    Search,
    Problems,
    CodeActions,
    Locations,
    Symbols,
}

#[derive(Debug, Clone)]
pub struct EditorLayoutState {
    pub panes: usize,
    pub active_pane: usize,
}

impl Default for EditorLayoutState {
    fn default() -> Self {
        Self {
            panes: 1,
            active_pane: 0,
        }
    }
}

/// vim 风格 `:` 命令行：命令与搜索的输入载体，替代命令面板。
/// `input` 是 `:` 之后键入的内容；`selected` 指向命令名补全列表中的高亮项。
#[derive(Debug, Clone, Default)]
pub struct CommandLineState {
    pub active: bool,
    pub input: String,
    pub cursor: usize,
    pub selected: usize,
}

impl CommandLineState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
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

impl InputDialogState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// 居中浮层的可见性与当前种类。`None` 即不可见。
#[derive(Debug, Clone, Default)]
pub struct OverlayState {
    pub active: Option<OverlayKind>,
}

impl OverlayState {
    pub fn is_visible(&self) -> bool {
        self.active.is_some()
    }

    pub fn open(&mut self, kind: OverlayKind) -> bool {
        let changed = self.active != Some(kind);
        self.active = Some(kind);
        changed
    }

    pub fn close(&mut self) -> bool {
        let changed = self.active.is_some();
        self.active = None;
        changed
    }
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
    CloseTab {
        pane: usize,
        index: usize,
    },
    CloseTabsBatch {
        pane: usize,
        tab_ids: Vec<u64>,
    },
    DeletePath {
        path: PathBuf,
        is_dir: bool,
    },
    RenamePath {
        from: PathBuf,
        to: PathBuf,
        overwrite: bool,
    },
    CopyPath {
        from: PathBuf,
        to: PathBuf,
        overwrite: bool,
    },
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
    pub normalization: CompletionNormalizationSnapshot,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionPopupState {
    pub visible: bool,
    pub all_items: Vec<CompletionRecord>,
    pub index_by_id: FxHashMap<u64, usize>,
    pub visible_indices: Vec<usize>,
    pub selected: usize,
    pub selection_locked: bool,
    pub filter_cache_prefix: String,
    pub filter_cache_indices: Vec<usize>,
    pub filter_cache_source_len: usize,
    pub filter_cache_valid: bool,
    pub request: Option<CompletionRequestContext>,
    pub pending_request: Option<CompletionRequestContext>,
    pub is_incomplete: bool,
    pub resolve_inflight: Option<u64>,
    pub session_started_at: Option<Instant>,
}

impl CompletionPopupState {
    pub fn visible_len(&self) -> usize {
        self.visible_indices.len()
    }

    pub fn rebuild_index_by_id(&mut self) {
        self.index_by_id.clear();
        for (idx, item) in self.all_items.iter().enumerate() {
            self.index_by_id.insert(item.entry.id, idx);
        }
    }

    pub fn index_of_id(&mut self, id: u64) -> Option<usize> {
        if let Some(idx) = self.index_by_id.get(&id).copied() {
            if self
                .all_items
                .get(idx)
                .is_some_and(|item| item.entry.id == id)
            {
                return Some(idx);
            }
        }

        let found = self.all_items.iter().position(|item| item.entry.id == id)?;
        self.rebuild_index_by_id();
        Some(found)
    }

    pub fn visible_record(&self, visible_idx: usize) -> Option<&CompletionRecord> {
        let item_idx = *self.visible_indices.get(visible_idx)?;
        self.all_items.get(item_idx)
    }

    pub fn selected_record(&self) -> Option<&CompletionRecord> {
        self.visible_record(self.selected.min(self.visible_len().saturating_sub(1)))
    }

    pub fn visible_item(&self, visible_idx: usize) -> Option<&CompletionEntry> {
        self.visible_record(visible_idx).map(|record| &record.entry)
    }

    pub fn selected_item(&self) -> Option<&CompletionEntry> {
        self.selected_record().map(|record| &record.entry)
    }

    pub fn set_resolve_state(&mut self, id: u64, state: CompletionResolveState) -> bool {
        let Some(idx) = self.index_of_id(id) else {
            return false;
        };
        let entry = &mut self.all_items[idx].entry;
        if entry.resolve_state == state {
            return false;
        }
        entry.resolve_state = state;
        true
    }

    pub fn invalidate_filter_cache(&mut self) {
        self.filter_cache_prefix.clear();
        self.filter_cache_indices.clear();
        self.filter_cache_source_len = self.all_items.len();
        self.filter_cache_valid = false;
    }

    pub fn reset_filter_cache_if_source_changed(&mut self) {
        if self.filter_cache_source_len != self.all_items.len() {
            self.invalidate_filter_cache();
        }
    }

    pub fn is_active(&self) -> bool {
        self.visible
            || self.request.is_some()
            || self.pending_request.is_some()
            || !self.all_items.is_empty()
            || !self.visible_indices.is_empty()
    }

    pub fn close(&mut self) -> bool {
        if self.is_active() {
            *self = Self::default();
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignatureHelpRequestContext {
    pub pane: usize,
    pub path: PathBuf,
    pub version: u64,
}

#[derive(Debug, Clone, Default)]
pub struct HoverPopupState {
    pub session: i32,
    pub model: Option<HoverModel>,
    pub implementation_preview: Option<HoverSectionModel>,
    pub definition_preview: Option<HoverSectionModel>,
}

impl HoverPopupState {
    pub fn is_active(&self) -> bool {
        self.model.is_some()
            || self.implementation_preview.is_some()
            || self.definition_preview.is_some()
    }

    pub fn clear(&mut self) {
        self.session = 0;
        self.model = None;
        self.implementation_preview = None;
        self.definition_preview = None;
    }

    pub fn display_text(&self) -> Option<String> {
        let base = self
            .model
            .as_ref()
            .map(HoverModel::to_display_text)
            .unwrap_or_default();
        let definition = self
            .definition_preview
            .as_ref()
            .map(HoverSectionModel::to_display_text)
            .unwrap_or_default();
        let implementation = self
            .implementation_preview
            .as_ref()
            .map(HoverSectionModel::to_display_text)
            .unwrap_or_default();

        let mut sections = Vec::with_capacity(3);
        if !base.trim().is_empty() {
            sections.push(base);
        }
        if !definition.trim().is_empty() {
            sections.push(definition);
        }
        if !implementation.trim().is_empty() {
            sections.push(implementation);
        }

        match sections.len() {
            0 => None,
            1 => sections.into_iter().next(),
            _ => Some(sections.join("\n\n---\n\n")),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SignatureHelpPopupState {
    pub visible: bool,
    pub model: Option<SignatureHelpModel>,
    pub request: Option<SignatureHelpRequestContext>,
}

impl SignatureHelpPopupState {
    pub fn is_active(&self) -> bool {
        self.visible || self.model.is_some() || self.request.is_some()
    }

    pub fn display_text(&self) -> Option<String> {
        if !self.visible {
            return None;
        }

        let text = self.model.as_ref()?.to_display_text();
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuRequest {
    Explorer { tree_row: Option<usize> },
    Tab { pane: usize, index: usize },
    TabBar { pane: usize },
    EditorArea { pane: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabMenuAction {
    Close,
    CloseOthers,
    CloseToRight,
    CloseAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerMenuAction {
    NewFile,
    NewFolder,
    Rename,
    Delete,
    CopyPath,
    CopyRelativePath,
    Cut,
    Copy,
    Paste,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuAction {
    RunCommand(Command),
    Tab(TabMenuAction),
    Explorer(ExplorerMenuAction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuEntryKind {
    Action(ContextMenuAction),
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuEntry {
    pub label: &'static str,
    pub kind: ContextMenuEntryKind,
    pub enabled: bool,
}

impl ContextMenuEntry {
    pub fn action(label: &'static str, action: ContextMenuAction) -> Self {
        Self {
            label,
            kind: ContextMenuEntryKind::Action(action),
            enabled: true,
        }
    }

    pub fn disabled_action(label: &'static str, action: ContextMenuAction) -> Self {
        Self {
            label,
            kind: ContextMenuEntryKind::Action(action),
            enabled: false,
        }
    }

    pub fn separator() -> Self {
        Self {
            label: "",
            kind: ContextMenuEntryKind::Separator,
            enabled: false,
        }
    }

    pub fn is_selectable(&self) -> bool {
        self.enabled && matches!(self.kind, ContextMenuEntryKind::Action(_))
    }

    pub fn enabled_action(&self) -> Option<&ContextMenuAction> {
        match (&self.kind, self.enabled) {
            (ContextMenuEntryKind::Action(action), true) => Some(action),
            _ => None,
        }
    }

    pub fn is_separator(&self) -> bool {
        matches!(self.kind, ContextMenuEntryKind::Separator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextMenuState {
    pub visible: bool,
    pub anchor: (u16, u16),
    pub selected: usize,
    pub items: Vec<ContextMenuEntry>,
    pub request: Option<ContextMenuRequest>,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub sidebar_visible: bool,
    pub sidebar_width: Option<u16>,
    pub overlay: OverlayState,
    pub focus: FocusTarget,
    pub editor_layout: EditorLayoutState,
    pub command_line: CommandLineState,
    pub input_dialog: InputDialogState,
    pub context_menu: ContextMenuState,
    pub pending_editor_nav: Option<PendingEditorNavigation>,
    pub should_quit: bool,
    pub hovered_tab: Option<(usize, usize)>,
    pub confirm_dialog: ConfirmDialogState,
    pub hover: HoverPopupState,
    pub signature_help: SignatureHelpPopupState,
    pub completion: CompletionPopupState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            sidebar_visible: true,
            sidebar_width: None,
            overlay: OverlayState::default(),
            focus: FocusTarget::Editor,
            editor_layout: EditorLayoutState::default(),
            command_line: CommandLineState::default(),
            input_dialog: InputDialogState::default(),
            context_menu: ContextMenuState::default(),
            pending_editor_nav: None,
            should_quit: false,
            hovered_tab: None,
            confirm_dialog: ConfirmDialogState::default(),
            hover: HoverPopupState::default(),
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
    pub server_capabilities: FxHashMap<LspClientKey, LspServerCapabilities>,
    pub payload_fingerprints: LspPayloadFingerprints,
    pub pending_format_on_save: Option<PathBuf>,
    pub pending_second_semantic_pass_by_path: FxHashMap<PathBuf, u64>,
    pub defer_semantic_flush_by_path: FxHashMap<PathBuf, u64>,
    pub eager_semantic_refresh_paths: FxHashSet<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadStamp {
    pub version: u64,
    pub item_count: usize,
    pub digest: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangePayloadStamp {
    pub version: u64,
    pub item_count: usize,
    pub start_line: usize,
    pub end_line_exclusive: usize,
    pub digest: u64,
}

#[derive(Debug, Clone, Default)]
pub struct LspPayloadFingerprints {
    pub semantic_full_by_path: FxHashMap<PathBuf, PayloadStamp>,
    pub semantic_range_by_path: FxHashMap<PathBuf, RangePayloadStamp>,
    pub inlay_range_by_path: FxHashMap<PathBuf, RangePayloadStamp>,
    pub folding_by_path: FxHashMap<PathBuf, PayloadStamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerClipboardMode {
    Cut,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerClipboardPayload {
    pub path: PathBuf,
    pub is_dir: bool,
    pub mode: ExplorerClipboardMode,
}

pub struct ExplorerState {
    tree: FileTree,
    pub view_height: usize,
    pub scroll_offset: usize,
    pub rows: Vec<FileTreeRow>,
    index_by_id: FxHashMap<NodeId, usize>,
    last_click: Option<(Instant, NodeId)>,
    clipboard: Option<ExplorerClipboardPayload>,
}

impl std::fmt::Debug for ExplorerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExplorerState")
            .field("scroll_offset", &self.scroll_offset)
            .field("rows_len", &self.rows.len())
            .field("selected", &self.tree.selected())
            .field("clipboard", &self.clipboard)
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
            clipboard: None,
        };
        state.refresh_rows();
        state
    }

    pub fn clipboard(&self) -> Option<&ExplorerClipboardPayload> {
        self.clipboard.as_ref()
    }

    pub fn set_clipboard(
        &mut self,
        path: PathBuf,
        is_dir: bool,
        mode: ExplorerClipboardMode,
    ) -> bool {
        let next = Some(ExplorerClipboardPayload { path, is_dir, mode });
        if self.clipboard == next {
            return false;
        }
        self.clipboard = next;
        true
    }

    pub fn clear_clipboard(&mut self) -> bool {
        self.clipboard.take().is_some()
    }

    pub fn clear_clipboard_if_deleted_path(&mut self, path: &Path) -> bool {
        let should_clear = self.clipboard.as_ref().is_some_and(|payload| {
            payload.path.as_path() == path || payload.path.starts_with(path)
        });
        if should_clear {
            self.clipboard = None;
        }
        should_clear
    }

    pub fn clear_clipboard_if_cut_source_renamed(&mut self, from: &Path) -> bool {
        let should_clear = self.clipboard.as_ref().is_some_and(|payload| {
            payload.mode == ExplorerClipboardMode::Cut
                && (payload.path.as_path() == from || payload.path.starts_with(from))
        });
        if should_clear {
            self.clipboard = None;
        }
        should_clear
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.tree.selected()
    }

    pub fn root_id(&self) -> NodeId {
        self.tree.root()
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

    pub fn path_and_kind_for(&self, id: NodeId) -> Option<(PathBuf, bool)> {
        let path = self.tree.full_path_ro(id)?;
        Some((path, self.tree.is_dir(id)))
    }

    pub fn node_id_for_path(&self, path: &Path) -> Option<NodeId> {
        self.tree.find_node_by_path_ro(path)
    }

    pub fn selected_create_parent_dir(&self) -> PathBuf {
        let root = self.tree.absolute_root().to_path_buf();
        let Some(id) = self.tree.selected() else {
            return root;
        };

        let Some((path, is_dir)) = self.path_and_kind_for(id) else {
            return root;
        };
        if is_dir {
            return path;
        }
        path.parent().unwrap_or(&root).to_path_buf()
    }

    pub fn selected_path_and_kind(&self) -> Option<(PathBuf, bool)> {
        let id = self.tree.selected()?;
        self.path_and_kind_for(id)
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

    pub fn request_dir_reload(&mut self, path: PathBuf) -> (bool, Vec<Effect>) {
        let Some(node_id) = self.tree.find_node_by_path(&path) else {
            return (false, Vec::new());
        };
        if !self.tree.is_dir(node_id) {
            return (false, Vec::new());
        }

        match self.tree.load_state(node_id) {
            Some(LoadState::Loaded) => {
                self.tree.set_load_state(node_id, LoadState::Loading);
                self.refresh_rows();
                (true, vec![Effect::LoadDir(path)])
            }
            Some(LoadState::Loading) | Some(LoadState::NotLoaded) | None => (false, Vec::new()),
        }
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
