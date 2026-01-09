//! 工作台模块：统一管理视图和输入分发

use super::theme::UiTheme;
use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{AppMessage, AsyncRuntime};
use crate::kernel::services::adapters::{
    ClipboardService, GlobalSearchService, GlobalSearchTask, KeybindingContext, KeybindingService,
    PluginHost, PluginHostEvent, PluginHostHandle, SearchService, SearchTask,
};
use crate::kernel::services::ports::{EditorConfig, GlobalSearchMessage, SearchMessage};
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget, Store};
use crate::models::build_file_tree;
use crate::views::{ExplorerView, SearchView};
use ratatui::layout::Rect;
use ratatui::Frame;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

mod bridge;
mod input;
mod interaction;
mod mouse;
mod palette;
mod render;
#[cfg(test)]
mod tests;
mod tick;
mod util;

const HEADER_HEIGHT: u16 = 1;
const STATUS_HEIGHT: u16 = 1;
const ACTIVITY_BAR_WIDTH: u16 = 4;
const SIDEBAR_WIDTH_PERCENT: u16 = 20;
const SIDEBAR_MIN_WIDTH: u16 = 20;
const LOG_BUFFER_CAP: usize = 2000;
const MAX_LOG_DRAIN_PER_TICK: usize = 1024;
const MAX_PLUGIN_DRAIN_PER_TICK: usize = 256;
const SETTINGS_CHECK_INTERVAL: Duration = Duration::from_millis(500);

pub struct Workbench {
    store: Store,
    explorer: ExplorerView,
    search_view: SearchView,
    clipboard: ClipboardService,
    editor_search_service: SearchService,
    editor_search_tasks: Vec<Option<SearchTask>>,
    editor_search_rx: Vec<Option<Receiver<SearchMessage>>>,
    log_rx: Option<Receiver<String>>,
    logs: VecDeque<String>,
    settings_path: Option<PathBuf>,
    last_settings_check: Instant,
    last_settings_modified: Option<SystemTime>,
    keybindings: KeybindingService,
    theme: UiTheme,
    runtime: AsyncRuntime,
    plugin_host: Option<PluginHostHandle>,
    plugin_high_rx: Option<Receiver<PluginHostEvent>>,
    plugin_low_rx: Option<Receiver<PluginHostEvent>>,
    global_search_service: GlobalSearchService,
    global_search_task: Option<GlobalSearchTask>,
    global_search_rx: Option<Receiver<GlobalSearchMessage>>,
    last_render_area: Option<Rect>,
    last_activity_bar_area: Option<Rect>,
    last_sidebar_area: Option<Rect>,
    last_sidebar_tabs_area: Option<Rect>,
    last_sidebar_content_area: Option<Rect>,
    last_bottom_panel_area: Option<Rect>,
    last_editor_areas: Vec<Rect>,
    last_editor_inner_areas: Vec<Rect>,
    last_editor_content_sizes: Vec<(u16, u16)>,
    last_explorer_view_height: Option<u16>,
    last_search_sidebar_results_height: Option<u16>,
    last_search_panel_results_height: Option<u16>,
    last_editor_container_area: Option<Rect>,
    last_editor_splitter_area: Option<Rect>,
    editor_split_dragging: bool,
}

impl Workbench {
    pub fn new(
        root_path: &Path,
        runtime: AsyncRuntime,
        log_rx: Option<Receiver<String>>,
    ) -> std::io::Result<Self> {
        let file_tree = build_file_tree(root_path)?;
        let global_search_service = GlobalSearchService::new(runtime.tokio_handle().clone());
        let editor_search_service = SearchService::new(runtime.tokio_handle().clone());

        let mut keybindings = KeybindingService::new();
        let mut theme = UiTheme::default();
        let mut editor_config = EditorConfig::default();

        let settings_path = if cfg!(test) {
            None
        } else {
            crate::kernel::services::adapters::ensure_settings_file()
                .ok()
                .or_else(crate::kernel::services::adapters::get_settings_path)
        };
        let last_settings_modified = settings_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());

        let (plugin_host, plugin_high_rx, plugin_low_rx) = if cfg!(test) {
            (None, None, None)
        } else {
            let _ = crate::kernel::services::adapters::ensure_plugins_file();
            let config = crate::kernel::services::adapters::load_plugins_config().unwrap_or_default();
            let host = PluginHost::start(runtime.tokio_handle(), root_path.to_path_buf(), config);
            (Some(host.handle), Some(host.high_rx), Some(host.low_rx))
        };

        if !cfg!(test) {
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
            theme.apply_settings(&settings.theme);
            editor_config = settings.editor;
            }
        }

        let store = Store::new(crate::kernel::AppState::new(
            root_path.to_path_buf(),
            file_tree,
            editor_config,
        ));
        let panes = store.state().ui.editor_layout.panes.max(1);

        Ok(Self {
            store,
            explorer: ExplorerView::new(),
            search_view: SearchView::new(),
            clipboard: ClipboardService::new(),
            editor_search_service,
            editor_search_tasks: std::iter::repeat_with(|| None).take(panes).collect(),
            editor_search_rx: std::iter::repeat_with(|| None).take(panes).collect(),
            log_rx,
            logs: VecDeque::with_capacity(LOG_BUFFER_CAP.min(256)),
            settings_path,
            last_settings_check: Instant::now(),
            last_settings_modified,
            keybindings,
            theme,
            runtime,
            plugin_host,
            plugin_high_rx,
            plugin_low_rx,
            global_search_service,
            global_search_task: None,
            global_search_rx: None,
            last_render_area: None,
            last_activity_bar_area: None,
            last_sidebar_area: None,
            last_sidebar_tabs_area: None,
            last_sidebar_content_area: None,
            last_bottom_panel_area: None,
            last_editor_areas: Vec::new(),
            last_editor_inner_areas: Vec::new(),
            last_editor_content_sizes: vec![(0, 0); panes],
            last_explorer_view_height: None,
            last_search_sidebar_results_height: None,
            last_search_panel_results_height: None,
            last_editor_container_area: None,
            last_editor_splitter_area: None,
            editor_split_dragging: false,
        })
    }

    pub(super) fn open_settings(&mut self) {
        if cfg!(test) {
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
            AppMessage::FileError { path, error } => {
                tracing::error!(path = %path.display(), error = %error, "load_file failed");
            }
            AppMessage::PathCreated { path, is_dir } => {
                let _ = self.dispatch_kernel(KernelAction::ExplorerPathCreated { path, is_dir });
            }
            AppMessage::PathDeleted { path } => {
                let _ = self.dispatch_kernel(KernelAction::ExplorerPathDeleted { path });
            }
            AppMessage::FsOpError { op, path, error } => {
                self.logs
                    .push_back(format!("[fs:{op}] {}: {error}", path.display()));
                while self.logs.len() > LOG_BUFFER_CAP {
                    self.logs.pop_front();
                }
            }
        }
    }

    pub fn runtime(&self) -> &AsyncRuntime {
        &self.runtime
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

    fn active_editor_pane(&self) -> usize {
        self.store.state().ui.editor_layout.active_pane
    }
}

impl View for Workbench {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        let _scope = perf::scope("view.input");
        input::handle_input(self, event)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let _scope = perf::scope("view.render");
        render::render(self, frame, area);
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        render::cursor_position(self)
    }
}
