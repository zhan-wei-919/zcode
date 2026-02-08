use super::super::theme::UiTheme;
use super::Workbench;
use crate::core::Command;
use crate::kernel::services::adapters::lsp::LspServerCommandOverride;
use crate::kernel::services::adapters::{
    ConfigService, KeybindingContext, KeybindingService, LspService,
};
use crate::kernel::services::ports::LspServerKind;
use crate::kernel::services::ports::{
    GlobalSearchMessage, LspPosition, LspPositionEncoding, SearchMessage,
};
use crate::kernel::services::KernelMessagePayload;
use crate::kernel::{Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget};
use rustc_hash::FxHashMap;
use std::sync::mpsc;
use std::time::Instant;

impl Workbench {
    /// 定时检查是否需要刷盘（由主循环调用）
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        changed |= self.poll_editor_search();
        changed |= self.poll_global_search();
        changed |= self.poll_kernel_bus();
        changed |= self.poll_logs();
        changed |= self.poll_settings();
        self.store.tick();
        changed |= self.poll_completion_debounce();
        changed |= self.poll_semantic_tokens_debounce();
        changed |= self.poll_inlay_hints_debounce();
        changed |= self.poll_folding_range_debounce();
        changed |= self.poll_idle_hover();
        changed |= self.poll_terminal_cursor_blink();
        changed |= self.poll_theme_save();
        self.poll_completion_rank_save();

        changed
    }

    fn poll_editor_search(&mut self) -> bool {
        let panes = self.store.state().ui.editor_layout.panes.max(1);
        self.editor_search_tasks.resize_with(panes, || None);
        self.editor_search_rx.resize_with(panes, || None);

        let mut changed = false;

        for pane in 0..panes {
            let Some(rx) = self.editor_search_rx[pane].take() else {
                continue;
            };

            let mut done = false;
            let mut disconnected = false;
            let mut drained = 0usize;

            loop {
                if drained >= super::MAX_EDITOR_SEARCH_DRAIN_PER_TICK {
                    break;
                }
                match rx.try_recv() {
                    Ok(msg) => {
                        drained += 1;
                        done = matches!(
                            msg,
                            SearchMessage::Complete { .. }
                                | SearchMessage::Cancelled { .. }
                                | SearchMessage::Error { .. }
                        );

                        changed |= self.dispatch_kernel(KernelAction::Editor(
                            EditorAction::SearchMessage { pane, message: msg },
                        ));

                        if done {
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }

            if done || disconnected {
                self.editor_search_tasks[pane] = None;
            } else {
                self.editor_search_rx[pane] = Some(rx);
            }
        }

        changed
    }

    fn poll_global_search(&mut self) -> bool {
        let Some(rx) = self.global_search_rx.take() else {
            return false;
        };

        let mut changed = false;
        let mut done = false;
        let mut disconnected = false;
        let mut drained = 0usize;

        loop {
            if drained >= super::MAX_GLOBAL_SEARCH_DRAIN_PER_TICK {
                break;
            }
            match rx.try_recv() {
                Ok(msg) => {
                    drained += 1;
                    done = matches!(
                        msg,
                        GlobalSearchMessage::Complete { .. }
                            | GlobalSearchMessage::Cancelled { .. }
                            | GlobalSearchMessage::Error { .. }
                    );

                    changed |= self.dispatch_kernel(KernelAction::SearchMessage(msg));

                    if done {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if done || disconnected {
            self.global_search_task = None;
        } else {
            self.global_search_rx = Some(rx);
        }

        changed
    }

    pub fn poll_kernel_bus(&mut self) -> bool {
        let mut changed = false;
        let mut drained = 0usize;
        loop {
            if drained >= super::MAX_KERNEL_BUS_DRAIN_PER_TICK {
                break;
            }
            match self.kernel_services.try_recv() {
                Ok(msg) => {
                    let queue_wait = msg.enqueued_at.elapsed();
                    match msg.payload {
                        KernelMessagePayload::Action(action) => {
                            if queue_wait.as_millis() > 1 {
                                tracing::debug!(
                                    queue_wait_ms = queue_wait.as_millis() as u64,
                                    target = "lsp.pipeline",
                                    "kernel bus queue wait"
                                );
                            }
                            let is_progress_end = matches!(&action, KernelAction::LspProgressEnd);
                            drained += 1;
                            changed |= self.dispatch_kernel(action);
                            if is_progress_end {
                                self.pending_semantic_tokens_deadline = Some(Instant::now());
                                self.pending_inlay_hints_deadline = Some(Instant::now());
                                self.pending_folding_range_deadline = Some(Instant::now());
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    fn poll_logs(&mut self) -> bool {
        let Some(rx) = self.log_rx.take() else {
            return false;
        };

        let mut changed = false;
        let mut drained = 0usize;
        let mut disconnected = false;

        loop {
            match rx.try_recv() {
                Ok(line) => {
                    changed = true;
                    drained += 1;
                    self.logs.push_back(line);
                    while self.logs.len() > super::LOG_BUFFER_CAP {
                        self.logs.pop_front();
                    }
                    if drained >= super::MAX_LOG_DRAIN_PER_TICK {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if !disconnected {
            self.log_rx = Some(rx);
        }

        changed
    }

    fn poll_settings(&mut self) -> bool {
        if !super::settings_enabled() {
            return false;
        }

        let Some(path) = self.settings_path.as_ref() else {
            return false;
        };

        if self.last_settings_check.elapsed() < super::SETTINGS_CHECK_INTERVAL {
            return false;
        }
        self.last_settings_check = Instant::now();

        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if modified.is_some() && modified != self.last_settings_modified {
            self.last_settings_modified = modified;
            return self.reload_settings();
        }

        false
    }

    fn poll_idle_hover(&mut self) -> bool {
        if self.last_input_at.elapsed() < super::HOVER_IDLE_DELAY {
            return false;
        }

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }

        if self.store.state().ui.completion.visible
            || self.store.state().ui.completion.request.is_some()
            || self.store.state().ui.completion.pending_request.is_some()
            || self.store.state().ui.signature_help.visible
            || self.store.state().ui.command_palette.visible
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
            || self.store.state().ui.hover_message.is_some()
        {
            return false;
        }

        let active_pane = self.store.state().ui.editor_layout.active_pane;
        let (pane, pos, anchor) = self
            .idle_hover_target
            .filter(|t| self.store.state().editor.pane(t.pane).is_some())
            .map(|t| (t.pane, (t.row, t.col), Some(t.anchor)))
            .unwrap_or_else(|| (active_pane, (usize::MAX, usize::MAX), None));

        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return false;
        };

        let Some(path) = tab.path.as_ref() else {
            return false;
        };
        if !crate::kernel::lsp_registry::is_lsp_source_path(path) {
            return false;
        }

        // Resolve server capabilities/encoding for this path.
        let caps = crate::kernel::lsp_registry::client_key_for_path(
            &self.store.state().workspace_root,
            path,
        )
        .and_then(|(_, key)| self.store.state().lsp.server_capabilities.get(&key));
        let supports_hover = caps.map(|c| c.hover).unwrap_or(true);
        if !supports_hover {
            return false;
        }
        let encoding = caps
            .map(|c| c.position_encoding)
            .unwrap_or(LspPositionEncoding::Utf16);

        // Determine which buffer position we are hovering: mouse position (preferred), otherwise cursor.
        let buf_pos = if pos.0 == usize::MAX {
            tab.buffer.cursor()
        } else {
            let row = pos.0.min(tab.buffer.len_lines().saturating_sub(1));
            let col = pos.1.min(tab.buffer.line_grapheme_len(row));
            (row, col)
        };

        let Some(buf_pos) = tab.identifier_pos_at_or_before(buf_pos) else {
            return false;
        };

        let char_offset = tab.buffer.pos_to_char(buf_pos);
        if tab.is_in_string_or_comment_at_char(char_offset) {
            return false;
        }

        let (line, column) = lsp_position_from_buffer_pos(tab, buf_pos, encoding);
        let key = (path.clone(), line, column, tab.edit_version);
        if self.idle_hover_last_request.as_ref() == Some(&key) {
            return false;
        }
        self.idle_hover_last_request = Some(key);
        self.idle_hover_last_anchor = anchor;

        if let Some(service) = self
            .kernel_services
            .get_mut::<crate::kernel::services::adapters::LspService>()
        {
            service.request_hover(
                path,
                LspPosition {
                    line,
                    character: column,
                },
            );
        }
        false
    }

    fn poll_completion_debounce(&mut self) -> bool {
        let Some(deadline) = self.pending_completion_deadline else {
            return false;
        };
        let now = Instant::now();
        if now < deadline {
            return false;
        }

        let overshoot = now.duration_since(deadline);
        if overshoot.as_millis() > 5 {
            tracing::debug!(
                overshoot_ms = overshoot.as_millis() as u64,
                target = "lsp.pipeline",
                "completion debounce overshoot"
            );
        }

        self.pending_completion_deadline = None;

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }
        if self.store.state().ui.command_palette.visible
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
        {
            return false;
        }

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspCompletion));
        false
    }

    fn poll_semantic_tokens_debounce(&mut self) -> bool {
        let Some(deadline) = self.pending_semantic_tokens_deadline else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        self.pending_semantic_tokens_deadline = None;

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }
        if self.store.state().ui.command_palette.visible
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
        {
            return false;
        }

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspSemanticTokens));
        false
    }

    fn poll_inlay_hints_debounce(&mut self) -> bool {
        let Some(deadline) = self.pending_inlay_hints_deadline else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        self.pending_inlay_hints_deadline = None;

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }
        if self.store.state().ui.command_palette.visible
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
        {
            return false;
        }

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspInlayHints));
        false
    }

    fn poll_folding_range_debounce(&mut self) -> bool {
        let Some(deadline) = self.pending_folding_range_deadline else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        self.pending_folding_range_deadline = None;

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }
        if self.store.state().ui.command_palette.visible
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
        {
            return false;
        }

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspFoldingRange));
        false
    }

    pub(super) fn reload_settings(&mut self) -> bool {
        if !super::settings_enabled() {
            return false;
        }

        let Some(settings) = crate::kernel::services::adapters::settings::load_settings() else {
            return false;
        };

        let editor_config = settings.editor.clone();
        let mut keybindings = KeybindingService::new();
        let mut lsp_settings_override: Option<(String, Vec<String>, Option<serde_json::Value>)> =
            None;
        let mut lsp_server_overrides: FxHashMap<LspServerKind, LspServerCommandOverride> =
            FxHashMap::default();
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
                .map(str::to_string)
                .collect::<Vec<_>>();
            lsp_settings_override = Some((command.to_string(), args, None));
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
            if let Some(initialization_options) = cfg.initialization_options.clone() {
                entry.initialization_options = Some(initialization_options);
            }
        }

        let mut theme = UiTheme::default();
        let terminal_color_support =
            crate::ui::core::color_support::detect_terminal_color_support();
        theme.apply_settings(&settings.theme);
        let core_theme = crate::app::theme::to_core_theme(&theme);
        let ui_theme =
            crate::ui::core::theme_adapter::adapt_theme(&core_theme, terminal_color_support);

        let _ = self.store.dispatch(KernelAction::EditorConfigUpdated {
            config: editor_config.clone(),
        });

        if let Some(service) = self.kernel_services.get_mut::<KeybindingService>() {
            *service = keybindings;
        } else {
            let _ = self.kernel_services.register(keybindings);
        }
        if let Some(service) = self.kernel_services.get_mut::<ConfigService>() {
            *service.editor_mut() = editor_config.clone();
        } else {
            let _ = self
                .kernel_services
                .register(ConfigService::with_editor_config(editor_config));
        }

        if let Some(service) = self.kernel_services.get_mut::<LspService>() {
            let mut global_override = None;
            if super::lsp_command_override().is_none() {
                global_override = lsp_settings_override;
            }

            if service.reconfigure(global_override, lsp_server_overrides) {
                self.lsp_open_paths.clear();
                self.lsp_open_paths_version = 0;
            }
        }

        self.theme = theme;
        self.ui_theme = ui_theme;
        self.terminal_color_support = terminal_color_support;
        self.last_settings_modified = self
            .settings_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());

        true
    }

    fn poll_terminal_cursor_blink(&mut self) -> bool {
        if self.store.state().ui.focus != FocusTarget::BottomPanel
            || self.store.state().ui.bottom_panel.active_tab != BottomPanelTab::Terminal
        {
            if self.terminal_cursor_visible {
                self.terminal_cursor_visible = false;
                return true;
            }
            return false;
        }

        let Some(session) = self.store.state().terminal.active_session() else {
            return false;
        };

        if session.scroll_offset > 0 {
            if self.terminal_cursor_visible {
                self.terminal_cursor_visible = false;
                return true;
            }
            return false;
        }

        #[cfg(feature = "terminal")]
        {
            if session.parser.screen().hide_cursor() {
                if self.terminal_cursor_visible {
                    self.terminal_cursor_visible = false;
                    return true;
                }
                return false;
            }
        }

        if self.terminal_cursor_last_blink.elapsed() >= super::TERMINAL_CURSOR_BLINK_INTERVAL {
            self.terminal_cursor_last_blink = Instant::now();
            self.terminal_cursor_visible = !self.terminal_cursor_visible;
            return true;
        }

        false
    }

    fn poll_theme_save(&mut self) -> bool {
        let Some(deadline) = self.pending_theme_save_deadline else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }
        self.pending_theme_save_deadline = None;

        let Some(settings_path) = self.settings_path.as_ref() else {
            return false;
        };

        let settings_text = match std::fs::read_to_string(settings_path) {
            Ok(text) => text,
            Err(_) => return false,
        };
        let mut settings: crate::kernel::services::ports::Settings =
            match serde_json::from_str(&settings_text) {
                Ok(s) => s,
                Err(_) => return false,
            };

        // Build ThemeSettings from current theme
        settings.theme = self.build_theme_settings();

        let json = match serde_json::to_string_pretty(&settings) {
            Ok(j) => j,
            Err(_) => return false,
        };
        let _ = std::fs::write(settings_path, json);

        // Update last_settings_modified to avoid triggering reload_settings
        if let Ok(meta) = std::fs::metadata(settings_path) {
            if let Ok(modified) = meta.modified() {
                self.last_settings_modified = Some(modified);
            }
        }

        false
    }

    fn poll_completion_rank_save(&mut self) {
        let Some(deadline) = self.pending_completion_rank_save_deadline else {
            return;
        };
        if Instant::now() < deadline {
            return;
        }

        self.pending_completion_rank_save_deadline = None;
        if !self.store.completion_ranker_is_dirty() {
            return;
        }

        if crate::kernel::services::adapters::settings::save_completion_ranker(
            self.store.completion_ranker(),
        ) {
            self.store.clear_completion_ranker_dirty();
        }
    }

    fn build_theme_settings(&self) -> crate::kernel::services::ports::ThemeSettings {
        use crate::ui::core::theme_adapter::color_to_hex;
        let t = &self.theme;
        crate::kernel::services::ports::ThemeSettings {
            focus_border: color_to_hex(t.focus_border),
            inactive_border: color_to_hex(t.inactive_border),
            separator: color_to_hex(t.separator),
            accent_fg: color_to_hex(t.accent_fg),
            syntax_comment_fg: color_to_hex(t.syntax_comment_fg),
            syntax_keyword_fg: color_to_hex(t.syntax_keyword_fg),
            syntax_string_fg: color_to_hex(t.syntax_string_fg),
            syntax_number_fg: color_to_hex(t.syntax_number_fg),
            syntax_type_fg: color_to_hex(t.syntax_type_fg),
            syntax_attribute_fg: color_to_hex(t.syntax_attribute_fg),
            syntax_function_fg: color_to_hex(t.syntax_function_fg),
            syntax_variable_fg: color_to_hex(t.syntax_variable_fg),
            syntax_constant_fg: color_to_hex(t.syntax_constant_fg),
            syntax_regex_fg: color_to_hex(t.syntax_regex_fg),
            error_fg: color_to_hex(t.error_fg),
            warning_fg: color_to_hex(t.warning_fg),
            activity_bg: color_to_hex(t.activity_bg),
            activity_fg: color_to_hex(t.activity_fg),
            activity_active_bg: color_to_hex(t.activity_active_bg),
            activity_active_fg: color_to_hex(t.activity_active_fg),
            sidebar_tab_active_bg: color_to_hex(t.sidebar_tab_active_bg),
            sidebar_tab_active_fg: color_to_hex(t.sidebar_tab_active_fg),
            sidebar_tab_inactive_fg: color_to_hex(t.sidebar_tab_inactive_fg),
            header_fg: color_to_hex(t.header_fg),
            palette_border: color_to_hex(t.palette_border),
            palette_bg: color_to_hex(t.palette_bg),
            palette_fg: color_to_hex(t.palette_fg),
            palette_selected_bg: color_to_hex(t.palette_selected_bg),
            palette_selected_fg: color_to_hex(t.palette_selected_fg),
            palette_muted_fg: color_to_hex(t.palette_muted_fg),
            indent_guide_fg: color_to_hex(t.indent_guide_fg),
        }
    }
}

fn lsp_position_from_buffer_pos(
    tab: &crate::kernel::editor::EditorTabState,
    pos: (usize, usize),
    encoding: LspPositionEncoding,
) -> (u32, u32) {
    let (row, col) = pos;
    let char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let line_start = rope.line_to_char(row);
    let col_chars = char_offset.saturating_sub(line_start);

    let line_slice = rope.line(row);
    let character = match encoding {
        LspPositionEncoding::Utf8 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf8() as u32)
            .sum(),
        LspPositionEncoding::Utf16 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf16() as u32)
            .sum(),
        LspPositionEncoding::Utf32 => col_chars as u32,
    };

    (row as u32, character)
}
