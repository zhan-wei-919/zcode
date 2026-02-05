//! 工作台模块：统一管理视图和输入分发

use super::theme::UiTheme;
use crate::core::event::InputEvent;
use crate::core::Command;
use crate::kernel::services::adapters::lsp::LspServerCommandOverride;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{AppMessage, AsyncRuntime};
use crate::kernel::services::adapters::{
    ClipboardService, ConfigService, FileService, GlobalSearchService, GlobalSearchTask,
    KeybindingContext, KeybindingService, LspService, SearchService, SearchTask,
};
use crate::kernel::services::ports::{
    EditorConfig, GlobalSearchMessage, LspServerKind, SearchMessage,
};
use crate::kernel::services::KernelServiceHost;
use crate::kernel::{Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, Store};
use crate::models::build_file_tree;
use crate::tui::view::{EventResult, View};
use crate::ui::backend::Backend;
use crate::ui::core::geom::Rect;
use crate::views::{ExplorerView, SearchView};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

mod bridge;
mod input;
mod interaction;
mod mouse;
mod paint;
mod palette;
mod render;
#[cfg(test)]
#[path = "../../../tests/unit/app/workbench.rs"]
mod tests;
mod tick;
mod util;

const STATUS_HEIGHT: u16 = 1;
const ACTIVITY_BAR_WIDTH: u16 = 3;
const SIDEBAR_WIDTH_PERCENT: u16 = 20;
const SIDEBAR_MIN_WIDTH: u16 = 20;
const LOG_BUFFER_CAP: usize = 2000;
const MAX_LOG_DRAIN_PER_TICK: usize = 1024;
const MAX_EDITOR_SEARCH_DRAIN_PER_TICK: usize = 256;
const MAX_GLOBAL_SEARCH_DRAIN_PER_TICK: usize = 256;
const MAX_KERNEL_BUS_DRAIN_PER_TICK: usize = 256;
const EDITOR_SEARCH_CHANNEL_CAP: usize = 64;
const GLOBAL_SEARCH_CHANNEL_CAP: usize = 64;
const SETTINGS_CHECK_INTERVAL: Duration = Duration::from_millis(500);
const HOVER_IDLE_DELAY: Duration = Duration::from_millis(500);
const COMPLETION_DEBOUNCE_DELAY: Duration = Duration::from_millis(60);
const SEMANTIC_TOKENS_DEBOUNCE_DELAY: Duration = Duration::from_millis(200);
const INLAY_HINTS_DEBOUNCE_DELAY: Duration = Duration::from_millis(200);
const FOLDING_RANGE_DEBOUNCE_DELAY: Duration = Duration::from_millis(250);
const TERMINAL_CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
struct PendingRestart {
    path: PathBuf,
    hard: bool,
}

fn env_truthy(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

fn settings_enabled() -> bool {
    !cfg!(test) && !env_truthy("ZCODE_DISABLE_SETTINGS")
}

fn lsp_enabled() -> bool {
    if env_truthy("ZCODE_DISABLE_LSP") {
        return false;
    }

    if cfg!(test) {
        return std::env::var("ZCODE_LSP_COMMAND")
            .ok()
            .as_deref()
            .map(str::trim)
            .is_some_and(|v| !v.is_empty());
    }

    true
}

fn lsp_command_override() -> Option<(String, Vec<String>)> {
    let command = std::env::var("ZCODE_LSP_COMMAND")
        .ok()
        .map(|s| s.trim().to_string())?;
    if command.is_empty() {
        return None;
    }
    let args = std::env::var("ZCODE_LSP_ARGS")
        .ok()
        .unwrap_or_default()
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    Some((command, args))
}

fn git_enabled() -> bool {
    if env_truthy("ZCODE_DISABLE_GIT") {
        return false;
    }
    if cfg!(test) {
        return false;
    }
    true
}

pub struct Workbench {
    store: Store,
    explorer: ExplorerView,
    search_view: SearchView,
    editor_search_tasks: Vec<Option<SearchTask>>,
    editor_search_rx: Vec<Option<Receiver<SearchMessage>>>,
    log_rx: Option<Receiver<String>>,
    logs: VecDeque<String>,
    clipboard_unavailable_warned: bool,
    settings_path: Option<PathBuf>,
    last_settings_check: Instant,
    last_settings_modified: Option<SystemTime>,
    last_input_at: Instant,
    idle_hover_last_request: Option<(PathBuf, u32, u32, u64)>,
    pending_completion_deadline: Option<Instant>,
    pending_semantic_tokens_deadline: Option<Instant>,
    pending_inlay_hints_deadline: Option<Instant>,
    pending_folding_range_deadline: Option<Instant>,
    file_save_versions: FxHashMap<(usize, PathBuf), u64>,
    lsp_open_paths_version: u64,
    lsp_open_paths: FxHashSet<PathBuf>,
    theme: UiTheme,
    ui_theme: crate::ui::core::theme::Theme,
    ui_runtime: crate::ui::core::runtime::UiRuntime,
    ui_tree: crate::ui::core::tree::UiTree,
    runtime: AsyncRuntime,
    kernel_services: KernelServiceHost,
    global_search_task: Option<GlobalSearchTask>,
    global_search_rx: Option<Receiver<GlobalSearchMessage>>,
    last_render_area: Option<Rect>,
    last_activity_bar_area: Option<Rect>,
    last_sidebar_area: Option<Rect>,
    last_sidebar_tabs_area: Option<Rect>,
    last_sidebar_content_area: Option<Rect>,
    last_sidebar_container_area: Option<Rect>,
    last_git_panel_area: Option<Rect>,
    last_git_branch_areas: Vec<(String, Rect)>,
    last_bottom_panel_area: Option<Rect>,
    last_editor_areas: Vec<Rect>,
    last_editor_inner_areas: Vec<Rect>,
    last_editor_content_sizes: Vec<(u16, u16)>,
    last_explorer_view_height: Option<u16>,
    last_search_sidebar_results_height: Option<u16>,
    last_search_panel_results_height: Option<u16>,
    last_problems_panel_height: Option<u16>,
    last_locations_panel_height: Option<u16>,
    last_code_actions_panel_height: Option<u16>,
    last_symbols_panel_height: Option<u16>,
    last_terminal_panel_size: Option<(u16, u16)>,
    terminal_cursor_visible: bool,
    terminal_cursor_last_blink: Instant,
    last_editor_container_area: Option<Rect>,
    editor_split_dragging: bool,
    sidebar_split_dragging: bool,
    last_problems_click: Option<(Instant, usize)>,
    last_locations_click: Option<(Instant, usize)>,
    last_code_actions_click: Option<(Instant, usize)>,
    last_symbols_click: Option<(Instant, usize)>,
    pending_restart: Option<PendingRestart>,
}

impl Workbench {
    pub fn new(
        root_path: &Path,
        runtime: AsyncRuntime,
        log_rx: Option<Receiver<String>>,
    ) -> std::io::Result<Self> {
        let file_tree = build_file_tree(root_path)?;
        let absolute_root = file_tree.absolute_root().to_path_buf();
        let mut keybindings = KeybindingService::new();
        let mut theme = UiTheme::default();
        let mut editor_config = EditorConfig::default();
        let mut lsp_settings_override: Option<(String, Vec<String>)> = None;
        let mut lsp_server_overrides: FxHashMap<LspServerKind, LspServerCommandOverride> =
            FxHashMap::default();

        let settings_path = if !settings_enabled() {
            None
        } else {
            crate::kernel::services::adapters::ensure_settings_file()
                .ok()
                .or_else(crate::kernel::services::adapters::get_settings_path)
        };
        let last_settings_modified = settings_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());

        if settings_enabled() {
            if let Some(settings) = crate::kernel::services::adapters::settings::load_settings() {
                for rule in settings.keybindings {
                    if let Some(key) =
                        crate::kernel::services::adapters::settings::parse_keybinding(&rule.key)
                    {
                        let context = rule
                            .context
                            .as_deref()
                            .and_then(KeybindingContext::parse)
                            .unwrap_or(KeybindingContext::Global);
                        if rule.command.trim().is_empty() {
                            let _ = keybindings.unbind(context, &key);
                        } else {
                            keybindings.bind(context, key, Command::from_name(&rule.command));
                        }
                    }
                }
                if let Some(command) = settings
                    .lsp
                    .command
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    let args = settings
                        .lsp
                        .args
                        .iter()
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>();
                    lsp_settings_override = Some((command.to_string(), args));
                }
                for (name, cfg) in &settings.lsp.servers {
                    let Some(kind) = LspServerKind::from_settings_key(name) else {
                        continue;
                    };

                    let command = cfg
                        .command
                        .as_deref()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_string);
                    let args = cfg.args.as_ref().map(|args| {
                        args.iter()
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    });

                    let entry = lsp_server_overrides.entry(kind).or_default();
                    if let Some(command) = command {
                        entry.command = Some(command);
                    }
                    if let Some(args) = args {
                        entry.args = Some(args);
                    }
                }
                theme.apply_settings(&settings.theme);
                editor_config = settings.editor;
            }
        }

        let executor: Arc<dyn crate::kernel::services::ports::AsyncExecutor> =
            Arc::new(runtime.tokio_handle());
        let mut kernel_services = KernelServiceHost::new(executor);
        let _ = kernel_services.register(ClipboardService::new());
        let _ = kernel_services.register(SearchService::new(runtime.tokio_handle().clone()));
        let _ = kernel_services.register(GlobalSearchService::new(runtime.tokio_handle().clone()));
        let _ = kernel_services.register(ConfigService::with_editor_config(editor_config.clone()));
        let _ = kernel_services.register(FileService::new());
        let _ = kernel_services.register(keybindings);
        if lsp_enabled() {
            let ctx = kernel_services.context();
            let mut service = LspService::new(absolute_root.clone(), ctx);
            if let Some((command, args)) = lsp_command_override() {
                service = service.with_command(command, args);
            } else if let Some((command, args)) = lsp_settings_override {
                service = service.with_command(command, args);
            } else if !lsp_server_overrides.is_empty() {
                service = service.with_server_command_overrides(lsp_server_overrides);
            }
            let _ = kernel_services.register(service);
        }

        let store = Store::new(crate::kernel::AppState::new(
            absolute_root,
            file_tree,
            editor_config,
        ));
        let panes = store.state().ui.editor_layout.panes.max(1);
        let lsp_open_paths_version = store.state().editor.open_paths_version;

        let ui_theme = crate::app::theme::to_core_theme(&theme);

        let mut workbench = Self {
            store,
            explorer: ExplorerView::new(),
            search_view: SearchView::new(),
            editor_search_tasks: std::iter::repeat_with(|| None).take(panes).collect(),
            editor_search_rx: std::iter::repeat_with(|| None).take(panes).collect(),
            log_rx,
            logs: VecDeque::with_capacity(LOG_BUFFER_CAP.min(256)),
            clipboard_unavailable_warned: false,
            settings_path,
            last_settings_check: Instant::now(),
            last_settings_modified,
            last_input_at: Instant::now(),
            idle_hover_last_request: None,
            pending_completion_deadline: None,
            pending_semantic_tokens_deadline: None,
            pending_inlay_hints_deadline: None,
            pending_folding_range_deadline: None,
            file_save_versions: FxHashMap::default(),
            lsp_open_paths_version,
            lsp_open_paths: FxHashSet::default(),
            theme,
            ui_theme,
            ui_runtime: crate::ui::core::runtime::UiRuntime::new(),
            ui_tree: crate::ui::core::tree::UiTree::new(),
            runtime,
            kernel_services,
            global_search_task: None,
            global_search_rx: None,
            last_render_area: None,
            last_activity_bar_area: None,
            last_sidebar_area: None,
            last_sidebar_tabs_area: None,
            last_sidebar_content_area: None,
            last_sidebar_container_area: None,
            last_git_panel_area: None,
            last_git_branch_areas: Vec::new(),
            last_bottom_panel_area: None,
            last_editor_areas: Vec::new(),
            last_editor_inner_areas: Vec::new(),
            last_editor_content_sizes: vec![(0, 0); panes],
            last_explorer_view_height: None,
            last_search_sidebar_results_height: None,
            last_search_panel_results_height: None,
            last_problems_panel_height: None,
            last_locations_panel_height: None,
            last_code_actions_panel_height: None,
            last_symbols_panel_height: None,
            last_terminal_panel_size: None,
            terminal_cursor_visible: true,
            terminal_cursor_last_blink: Instant::now(),
            last_editor_container_area: None,
            editor_split_dragging: false,
            sidebar_split_dragging: false,
            last_problems_click: None,
            last_locations_click: None,
            last_code_actions_click: None,
            last_symbols_click: None,
            pending_restart: None,
        };

        if git_enabled() {
            let _ = workbench.dispatch_kernel(KernelAction::GitInit);
        }

        workbench.maybe_warn_clipboard_unavailable();
        Ok(workbench)
    }

    pub fn take_log_rx(&mut self) -> Option<Receiver<String>> {
        self.log_rx.take()
    }

    pub fn take_pending_restart(&mut self) -> Option<(PathBuf, bool)> {
        self.pending_restart.take().map(|req| (req.path, req.hard))
    }

    pub(super) fn open_settings(&mut self) {
        if !settings_enabled() {
            return;
        }

        let path = match crate::kernel::services::adapters::ensure_settings_file() {
            Ok(path) => path,
            Err(e) => {
                tracing::error!(error = %e, "ensure_settings_file failed");
                return;
            }
        };

        self.settings_path = Some(path.clone());
        self.last_settings_modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.runtime.load_file(path);
    }

    pub fn handle_message(&mut self, msg: AppMessage) {
        match msg {
            AppMessage::DirLoaded { path, entries } => {
                let _ = self.dispatch_kernel(KernelAction::DirLoaded { path, entries });
            }
            AppMessage::DirLoadError { path, error } => {
                tracing::warn!(path = %path.display(), error = %error, "load_dir failed");
                let _ = self.dispatch_kernel(KernelAction::DirLoadError { path });
            }
            AppMessage::FileLoaded { path, content } => {
                let pane = self
                    .store
                    .state()
                    .ui
                    .pending_editor_nav
                    .as_ref()
                    .filter(|p| p.path.as_path() == path.as_path())
                    .map(|p| p.pane)
                    .unwrap_or_else(|| self.active_editor_pane());
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
                    pane,
                    path,
                    content,
                }));
                let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::FocusEditor));
            }
            AppMessage::FileSaved {
                pane,
                path,
                success,
                version,
            } => {
                let save_key = (pane, path.clone());
                if self
                    .file_save_versions
                    .get(&save_key)
                    .is_some_and(|last| *last > version)
                {
                    return;
                }
                self.file_save_versions.insert(save_key, version);

                if success {
                    if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                        service.save_document(&path);
                    }

                    if self
                        .settings_path
                        .as_ref()
                        .is_some_and(|settings_path| settings_path.as_path() == path.as_path())
                    {
                        self.reload_settings();
                    }
                }

                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::Saved {
                    pane,
                    path,
                    success,
                    version,
                }));
            }
            AppMessage::FileError { path, error } => {
                tracing::error!(path = %path.display(), error = %error, "load_file failed");
            }
            AppMessage::PathCreated { path, is_dir } => {
                let _ = self.dispatch_kernel(KernelAction::ExplorerPathCreated { path, is_dir });
            }
            AppMessage::PathDeleted { path } => {
                let _ = self.dispatch_kernel(KernelAction::ExplorerPathDeleted { path });
            }
            AppMessage::PathRenamed { from, to } => {
                let _ = self.dispatch_kernel(KernelAction::ExplorerPathRenamed { from, to });
            }
            AppMessage::FsOpError {
                op,
                path,
                to,
                error,
            } => {
                let msg = if let Some(to) = to {
                    format!("[fs:{op}] {} -> {}: {error}", path.display(), to.display())
                } else {
                    format!("[fs:{op}] {}: {error}", path.display())
                };
                self.logs.push_back(msg);
                while self.logs.len() > LOG_BUFFER_CAP {
                    self.logs.pop_front();
                }
            }
            AppMessage::GitRepoDetected {
                repo_root,
                head,
                worktrees,
            } => {
                let _ = self.dispatch_kernel(KernelAction::GitRepoDetected {
                    repo_root,
                    head,
                    worktrees,
                });
            }
            AppMessage::GitRepoCleared => {
                let _ = self.dispatch_kernel(KernelAction::GitRepoCleared);
            }
            AppMessage::GitStatusUpdated { statuses } => {
                let _ = self.dispatch_kernel(KernelAction::GitStatusUpdated { statuses });
            }
            AppMessage::GitDiffUpdated { path, marks } => {
                let _ = self.dispatch_kernel(KernelAction::GitDiffUpdated { path, marks });
            }
            AppMessage::GitWorktreesUpdated { worktrees } => {
                let _ = self.dispatch_kernel(KernelAction::GitWorktreesUpdated { worktrees });
            }
            AppMessage::GitBranchesUpdated { branches } => {
                let _ = self.dispatch_kernel(KernelAction::GitBranchesUpdated { branches });
            }
            AppMessage::GitWorktreeResolved { path } => {
                let _ = self.dispatch_kernel(KernelAction::GitWorktreeResolved { path });
            }
            AppMessage::TerminalSpawned { id, title } => {
                let _ = self.dispatch_kernel(KernelAction::TerminalSpawned { id, title });
            }
            AppMessage::TerminalOutput { id, bytes } => {
                let _ = self.dispatch_kernel(KernelAction::TerminalOutput { id, bytes });
            }
            AppMessage::TerminalExited { id, code } => {
                let _ = self.dispatch_kernel(KernelAction::TerminalExited { id, code });
            }
        }
    }

    #[cfg(feature = "perf")]
    pub fn bench_run_command(&mut self, command: Command) {
        let _ = self.dispatch_kernel(KernelAction::RunCommand(command));
    }

    #[cfg(feature = "perf")]
    pub fn bench_set_active_pane(&mut self, pane: usize) {
        let _ = self.dispatch_kernel(KernelAction::EditorSetActivePane { pane });
    }

    #[cfg(feature = "perf")]
    pub fn bench_open_file(&mut self, pane: usize, path: PathBuf, content: String) {
        let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::OpenFile {
            pane,
            path,
            content,
        }));
    }

    pub fn runtime(&self) -> &AsyncRuntime {
        &self.runtime
    }

    pub fn state(&self) -> &crate::kernel::AppState {
        self.store.state()
    }

    pub fn has_lsp_service(&self) -> bool {
        self.kernel_services.get::<LspService>().is_some()
    }

    pub fn lsp_command_config(&self) -> Option<(String, Vec<String>)> {
        let service = self.kernel_services.get::<LspService>()?;
        let (command, args) = service.command_config();
        Some((command.to_string(), args.to_vec()))
    }

    pub fn focus(&self) -> FocusTarget {
        self.store.state().ui.focus
    }

    pub fn sidebar_visible(&self) -> bool {
        self.store.state().ui.sidebar_visible
    }

    pub fn bottom_panel_visible(&self) -> bool {
        self.store.state().ui.bottom_panel.visible
    }

    fn bottom_panel_tabs(&self) -> Vec<(BottomPanelTab, String)> {
        vec![
            (BottomPanelTab::Problems, " PROBLEMS ".to_string()),
            (BottomPanelTab::CodeActions, " ACTIONS ".to_string()),
            (BottomPanelTab::Locations, " LOCATIONS ".to_string()),
            (BottomPanelTab::Symbols, " SYMBOLS ".to_string()),
            (
                BottomPanelTab::SearchResults,
                " SEARCH RESULTS ".to_string(),
            ),
            (BottomPanelTab::Logs, " LOGS ".to_string()),
            (BottomPanelTab::Terminal, " TERMINAL ".to_string()),
        ]
    }

    fn active_editor_pane(&self) -> usize {
        self.store.state().ui.editor_layout.active_pane
    }
}

impl View for Workbench {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        let _scope = perf::scope("view.input");
        let result = input::handle_input(self, event);
        if matches!(result, EventResult::Quit) {
            return result;
        }
        if let Some((path, hard)) = self.take_pending_restart() {
            return EventResult::Restart { path, hard };
        }
        result
    }

    fn render(&mut self, backend: &mut dyn Backend, area: Rect) {
        let _scope = perf::scope("view.render");
        render::render(self, backend, area);
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        render::cursor_position(self)
    }
}
