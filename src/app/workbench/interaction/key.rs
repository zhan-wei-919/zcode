use super::super::Workbench;
use super::terminal::terminal_bytes_for_key_event;
use crate::core::event::{Key, KeyCode, KeyEvent, KeyModifiers};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{KeybindingContext, KeybindingService};
use crate::kernel::{
    Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, SidebarTab,
};
use crate::tui::view::EventResult;
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn record_user_input(&mut self) {
        self.last_input_at = Instant::now();
        self.idle_hover_last_request = None;
        self.pending_completion_deadline = None;
        self.pending_inlay_hints_deadline = None;
        self.pending_folding_range_deadline = None;
        if self.store.state().ui.focus == FocusTarget::BottomPanel
            && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal
        {
            self.terminal_cursor_visible = true;
            self.terminal_cursor_last_blink = Instant::now();
        }

        if let Some(service) = self
            .kernel_services
            .get_mut::<crate::kernel::services::adapters::LspService>()
        {
            service.cancel_hover();
        }

        if self.store.state().ui.hover_message.is_some() {
            let _ = self.dispatch_kernel(KernelAction::LspHover {
                text: String::new(),
            });
        }
    }

    pub(in super::super) fn handle_key_event(&mut self, key_event: &KeyEvent) -> EventResult {
        let _scope = perf::scope("input.key");

        if self.store.state().ui.context_menu.visible {
            match key_event.code {
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::ContextMenuClose);
                    return EventResult::Consumed;
                }
                KeyCode::Up => {
                    let _ =
                        self.dispatch_kernel(KernelAction::ContextMenuMoveSelection { delta: -1 });
                    return EventResult::Consumed;
                }
                KeyCode::Down => {
                    let _ =
                        self.dispatch_kernel(KernelAction::ContextMenuMoveSelection { delta: 1 });
                    return EventResult::Consumed;
                }
                KeyCode::Enter => {
                    let _ = self.dispatch_kernel(KernelAction::ContextMenuConfirm);
                    return EventResult::Consumed;
                }
                _ => {
                    let _ = self.dispatch_kernel(KernelAction::ContextMenuClose);
                }
            }
        }

        if self.store.state().ui.input_dialog.visible {
            match (key_event.code, key_event.modifiers) {
                (KeyCode::Enter, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogAccept);
                    return EventResult::Consumed;
                }
                (KeyCode::Esc, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCancel);
                    return EventResult::Consumed;
                }
                (KeyCode::Backspace, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogBackspace);
                    return EventResult::Consumed;
                }
                (KeyCode::Left, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCursorLeft);
                    return EventResult::Consumed;
                }
                (KeyCode::Right, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCursorRight);
                    return EventResult::Consumed;
                }
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogAppend(ch));
                    return EventResult::Consumed;
                }
                _ => return EventResult::Consumed,
            }
        }

        if self.store.state().ui.confirm_dialog.visible {
            match key_event.code {
                KeyCode::Enter => {
                    let _ = self.dispatch_kernel(KernelAction::ConfirmDialogAccept);
                    return EventResult::Consumed;
                }
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::ConfirmDialogCancel);
                    return EventResult::Consumed;
                }
                _ => return EventResult::Consumed,
            }
        }

        if self.store.state().ui.completion.visible {
            match key_event.code {
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::CompletionClose);
                    return EventResult::Consumed;
                }
                KeyCode::Tab => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: 1 });
                    return EventResult::Consumed;
                }
                KeyCode::BackTab => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: -1 });
                    return EventResult::Consumed;
                }
                KeyCode::Enter => {
                    let pane = self.store.state().ui.editor_layout.active_pane;
                    let before_version = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                        .map(|tab| tab.edit_version);
                    let _ = self.dispatch_kernel(KernelAction::CompletionConfirm);
                    let after_version = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                        .map(|tab| tab.edit_version);
                    if before_version != after_version {
                        let refresh = Command::Paste;
                        self.maybe_schedule_semantic_tokens_debounce(&refresh);
                        self.maybe_schedule_inlay_hints_debounce(&refresh);
                        self.maybe_schedule_folding_range_debounce(&refresh);
                    }
                    return EventResult::Consumed;
                }
                _ => {}
            }
        }

        let context = self.keybinding_context();
        let key: Key = (*key_event).into();

        let cmd = self
            .kernel_services
            .get::<KeybindingService>()
            .and_then(|service| service.resolve(context, &key).cloned());

        let terminal_active = self.store.state().ui.focus == FocusTarget::BottomPanel
            && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal;

        if terminal_active
            && !matches!(
                cmd.as_ref(),
                Some(Command::ToggleBottomPanel | Command::FocusBottomPanel)
            )
        {
            if let Some(bytes) = terminal_bytes_for_key_event(key_event) {
                if let Some(id) = self.store.state().terminal.active_session().map(|s| s.id) {
                    let _ = self.dispatch_kernel(KernelAction::TerminalWrite { id, bytes });
                }
                return EventResult::Consumed;
            }
        }

        if let Some(cmd) = cmd {
            if cmd == Command::Copy
                && self.store.state().ui.focus == FocusTarget::BottomPanel
                && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Logs
            {
                self.copy_logs_to_clipboard();
                return EventResult::Consumed;
            }

            let cmd_for_schedule = cmd.clone();
            let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd));
            self.maybe_schedule_completion_debounce(&cmd_for_schedule);
            self.maybe_schedule_semantic_tokens_debounce(&cmd_for_schedule);
            self.maybe_schedule_inlay_hints_debounce(&cmd_for_schedule);
            self.maybe_schedule_folding_range_debounce(&cmd_for_schedule);
            if self.store.state().ui.should_quit {
                return EventResult::Quit;
            }
            return EventResult::Consumed;
        }

        match context {
            KeybindingContext::EditorSearchBar => {
                let pane = self.store.state().ui.editor_layout.active_pane;
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                        let _ = self.dispatch_kernel(KernelAction::Editor(
                            EditorAction::SearchBarAppend { pane, ch },
                        ));
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                }
            }
            KeybindingContext::Editor => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let cmd = Command::InsertChar(ch);
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd.clone()));
                    self.maybe_schedule_completion_debounce(&cmd);
                    self.maybe_schedule_semantic_tokens_debounce(&cmd);
                    self.maybe_schedule_inlay_hints_debounce(&cmd);
                    self.maybe_schedule_folding_range_debounce(&cmd);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            KeybindingContext::SidebarSearch => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::SearchAppend(ch));
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            KeybindingContext::CommandPalette => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::PaletteAppend(ch));
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            _ => EventResult::Ignored,
        }
    }

    pub(super) fn keybinding_context(&self) -> KeybindingContext {
        let ui = &self.store.state().ui;

        if ui.command_palette.visible && ui.focus == FocusTarget::CommandPalette {
            return KeybindingContext::CommandPalette;
        }

        match ui.focus {
            FocusTarget::Explorer => match ui.sidebar_tab {
                SidebarTab::Explorer => KeybindingContext::SidebarExplorer,
                SidebarTab::Search => KeybindingContext::SidebarSearch,
            },
            FocusTarget::BottomPanel => KeybindingContext::BottomPanel,
            FocusTarget::CommandPalette => KeybindingContext::CommandPalette,
            FocusTarget::Editor => {
                let pane = ui.editor_layout.active_pane;
                let visible = self
                    .store
                    .state()
                    .editor
                    .pane(pane)
                    .is_some_and(|p| p.search_bar.visible);
                if visible {
                    KeybindingContext::EditorSearchBar
                } else {
                    KeybindingContext::Editor
                }
            }
        }
    }
}
