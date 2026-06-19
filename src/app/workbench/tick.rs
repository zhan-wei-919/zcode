use super::settings_parse::{parse_settings, ParsedSettings};
use super::Workbench;
use crate::core::Command;
use crate::kernel::services::adapters::{
    ConfigService, FileWatchEvent, KeybindingService, LspService,
};
use crate::kernel::services::ports::lsp::column_for_chars;
use crate::kernel::services::ports::{
    GlobalSearchMessage, LspPosition, LspPositionEncoding, SearchMessage,
};
use crate::kernel::services::KernelMessagePayload;
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget};
use std::sync::mpsc;
use std::time::Instant;

impl Workbench {
    /// 定时检查是否需要刷盘（由主循环调用）
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        changed |= self.poll_file_watcher();
        changed |= self.poll_editor_search();
        changed |= self.poll_global_search();
        changed |= self.poll_kernel_bus();
        changed |= self.poll_settings();
        self.store.tick();
        changed |= self.poll_semantic_tokens_debounce();
        changed |= self.poll_inlay_hints_debounce();
        changed |= self.poll_folding_range_debounce();
        changed |= self.poll_idle_hover();
        changed |= self.poll_definition_jump_highlight();
        self.poll_completion_rank_save();

        changed
    }

    fn poll_file_watcher(&mut self) -> bool {
        let Some(watcher) = self.file_watcher.as_mut() else {
            return false;
        };
        let events = watcher.drain_events();
        if events.is_empty() {
            return false;
        }
        let mut changed = false;
        for event in events {
            match event {
                FileWatchEvent::EditorModified(path) => {
                    changed |= self.dispatch_kernel(KernelAction::Editor(
                        EditorAction::FileExternallyModified { path },
                    ));
                }
                FileWatchEvent::EditorRemoved(path) => {
                    changed |= self.dispatch_kernel(KernelAction::Editor(
                        EditorAction::FileExternallyDeleted { path },
                    ));
                }
                FileWatchEvent::WorkspaceCreated { path, is_dir } => {
                    changed |=
                        self.dispatch_kernel(KernelAction::ExplorerPathCreated { path, is_dir });
                }
                FileWatchEvent::WorkspaceDeleted { path } => {
                    changed |= self.dispatch_kernel(KernelAction::ExplorerPathDeleted { path });
                }
                FileWatchEvent::WorkspaceRenamed { from, to } => {
                    changed |= self.dispatch_kernel(KernelAction::ExplorerPathRenamed { from, to });
                }
                FileWatchEvent::WorkspaceDirChanged { path } => {
                    changed |= self.dispatch_kernel(KernelAction::ExplorerDirChanged { path });
                }
            }
        }
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
                                self.lsp_sync.debounce.semantic_tokens = Some(Instant::now());
                                self.lsp_sync.debounce.inlay_hints = Some(Instant::now());
                                self.lsp_sync.debounce.folding_range = Some(Instant::now());
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

        // Idle hover should be driven by the last mouse position within the editor content.
        // Cursor-based hover remains available via the explicit `LspHover` command.
        let Some(target) = self
            .ui
            .hover_popup
            .target
            .filter(|t| self.store.state().editor.pane(t.pane).is_some())
        else {
            return false;
        };

        if self.store.state().ui.completion.visible
            || self.store.state().ui.completion.request.is_some()
            || self.store.state().ui.completion.pending_request.is_some()
            || self.store.state().ui.signature_help.visible
            || self.store.state().ui.command_line.active
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
            || self.store.state().ui.hover.is_active()
        {
            return false;
        }

        let pane = target.pane;
        let pos = (target.row, target.col);
        let anchor = Some(target.anchor);
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

        // Determine which buffer position we are hovering.
        let row = pos.0.min(tab.buffer.len_lines().saturating_sub(1));
        let col = pos.1.min(tab.buffer.line_grapheme_len(row));
        let buf_pos = (row, col);

        let Some(buf_pos) = tab.identifier_pos_at(buf_pos) else {
            return false;
        };

        let char_offset = tab.buffer.pos_to_char(buf_pos);
        if tab.is_in_string_or_comment_at_char(char_offset) {
            return false;
        }

        let (line, column) = lsp_position_from_buffer_pos(tab, buf_pos, encoding);
        let key = (path.clone(), line, column, tab.edit_version);
        if self.ui.hover_popup.last_request.as_ref() == Some(&key) {
            return false;
        }
        self.ui.hover_popup.last_request = Some(key);
        self.ui.hover_popup.last_anchor = anchor;

        if let Some(service) = self
            .kernel_services
            .get_mut::<crate::kernel::services::adapters::LspService>()
        {
            let hover = &self.store.state().editor.config.lsp_hover;
            service.request_hover(
                path,
                LspPosition {
                    line,
                    character: column,
                },
                crate::kernel::services::adapters::lsp::HoverRequestOptions {
                    include_definition_source: hover.show_definition_source,
                    definition_max_lines: hover.definition_max_lines_clamped(),
                },
            );
        }
        false
    }

    fn poll_semantic_tokens_debounce(&mut self) -> bool {
        let Some(deadline) = self.lsp_sync.debounce.semantic_tokens else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        self.lsp_sync.debounce.semantic_tokens = None;

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }
        if self.store.state().ui.command_line.active
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
        {
            return false;
        }

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspSemanticTokens));
        false
    }

    fn poll_inlay_hints_debounce(&mut self) -> bool {
        let Some(deadline) = self.lsp_sync.debounce.inlay_hints else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        self.lsp_sync.debounce.inlay_hints = None;

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }
        if self.store.state().ui.command_line.active
            || self.store.state().ui.input_dialog.visible
            || self.store.state().ui.confirm_dialog.visible
        {
            return false;
        }

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspInlayHints));
        false
    }

    fn poll_folding_range_debounce(&mut self) -> bool {
        let Some(deadline) = self.lsp_sync.debounce.folding_range else {
            return false;
        };
        if Instant::now() < deadline {
            return false;
        }

        self.lsp_sync.debounce.folding_range = None;

        if self.store.state().ui.focus != FocusTarget::Editor {
            return false;
        }
        if self.store.state().ui.command_line.active
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

        let ParsedSettings {
            keybindings,
            editor_config,
            lsp_settings_override,
            lsp_server_overrides,
        } = parse_settings(settings);

        let _ = self.store.dispatch(KernelAction::EditorConfigUpdated {
            config: editor_config.clone(),
        });

        if let Some(service) = self.kernel_services.get_mut::<KeybindingService>() {
            *service = keybindings;
        } else {
            let _ = self.kernel_services.register(keybindings);
        }
        if let Some(service) = self.kernel_services.get_mut::<ConfigService>() {
            *service.editor_mut() = editor_config;
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
                self.lsp_sync.open_paths.clear();
                self.lsp_sync.open_paths_version = 0;
            }
        }

        self.last_settings_modified = self
            .settings_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());

        true
    }

    fn poll_definition_jump_highlight(&mut self) -> bool {
        if self
            .pending_definition_highlight
            .as_ref()
            .is_some_and(|pending| {
                pending.armed_at.elapsed() >= super::DEFINITION_JUMP_PENDING_TIMEOUT
            })
        {
            self.pending_definition_highlight = None;
        }

        let Some(highlight) = self.definition_jump_highlight else {
            return false;
        };
        if highlight.started_at.elapsed() >= super::DEFINITION_JUMP_HIGHLIGHT_DURATION {
            self.definition_jump_highlight = None;
            return true;
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
    let character = column_for_chars(line_slice, col_chars, encoding);

    (row as u32, character)
}
