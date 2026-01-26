use super::super::theme::UiTheme;
use super::Workbench;
use crate::core::Command;
use crate::kernel::services::adapters::{ConfigService, KeybindingContext, KeybindingService};
use crate::kernel::services::ports::{GlobalSearchMessage, SearchMessage};
use crate::kernel::services::KernelMessage;
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget};
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

        let pane = self.store.state().ui.editor_layout.active_pane;
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
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return false;
        }

        let (line, column) = lsp_position_from_cursor(tab);
        let key = (path.clone(), line, column, tab.edit_version);
        if self.idle_hover_last_request.as_ref() == Some(&key) {
            return false;
        }
        self.idle_hover_last_request = Some(key);

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspHover));
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
        self.last_settings_modified = self
            .settings_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());

        true
    }
}

fn lsp_position_from_cursor(tab: &crate::kernel::editor::EditorTabState) -> (u32, u32) {
    let (row, col) = tab.buffer.cursor();
    let char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let line_start = rope.line_to_char(row);
    let col_chars = char_offset.saturating_sub(line_start);
    let utf16 = rope
        .line(row)
        .chars()
        .take(col_chars)
        .map(|ch| ch.len_utf16() as u32)
        .sum();
    (row as u32, utf16)
}
