use super::super::theme::UiTheme;
use super::Workbench;
use crate::core::Command;
use crate::kernel::services::adapters::{ConfigService, KeybindingContext, KeybindingService};
use crate::kernel::services::ports::{
    GlobalSearchMessage, LspPosition, LspPositionEncoding, SearchMessage,
};
use crate::kernel::services::KernelMessage;
use crate::kernel::{Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget};
use std::sync::mpsc;
use std::time::Instant;
use unicode_xid::UnicodeXID;

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

    fn poll_kernel_bus(&mut self) -> bool {
        let mut changed = false;
        let mut drained = 0usize;
        loop {
            if drained >= super::MAX_KERNEL_BUS_DRAIN_PER_TICK {
                break;
            }
            match self.kernel_services.try_recv() {
                Ok(msg) => match msg {
                    KernelMessage::Action(action) => {
                        drained += 1;
                        changed |= self.dispatch_kernel(action);
                    }
                },
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

        let char_offset = tab.buffer.pos_to_char(buf_pos);
        if tab.is_in_string_or_comment_at_char(char_offset) {
            return false;
        }
        if !buffer_pos_is_identifier(tab, buf_pos) {
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
        if Instant::now() < deadline {
            return false;
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

        let mut theme = UiTheme::default();
        theme.apply_settings(&settings.theme);
        let ui_theme = crate::app::theme::to_core_theme(&theme);

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
        self.theme = theme;
        self.ui_theme = ui_theme;
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
}

fn buffer_pos_is_identifier(
    tab: &crate::kernel::editor::EditorTabState,
    pos: (usize, usize),
) -> bool {
    let rope = tab.buffer.rope();
    let char_offset = tab.buffer.pos_to_char(pos).min(rope.len_chars());
    if char_offset >= rope.len_chars() {
        return false;
    }

    let ch = rope.char(char_offset);
    ch == '_' || UnicodeXID::is_xid_continue(ch)
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
