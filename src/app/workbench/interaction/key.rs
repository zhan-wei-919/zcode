use super::super::Workbench;
use crate::core::event::{InputEvent, Key, KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{KeybindingContext, KeybindingService};
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget, OverlayKind, SidebarTab};
use crate::tui::view::EventResult;
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn record_user_input(&mut self, event: &InputEvent) {
        let preserve_hover = matches!(
            event,
            InputEvent::Mouse(me)
                if self.store.state().ui.hover.is_active()
                    && self.ui.hover_popup.last_area.is_some_and(|a| {
                        super::super::util::rect_contains(a, me.column, me.row)
                    })
                    && matches!(
                        me.kind,
                        MouseEventKind::Moved
                            | MouseEventKind::ScrollUp
                            | MouseEventKind::ScrollDown
                            | MouseEventKind::ScrollLeft
                            | MouseEventKind::ScrollRight
                    )
        );

        self.last_input_at = Instant::now();
        if !preserve_hover {
            self.ui.hover_popup.last_request = None;
            self.ui.hover_popup.last_anchor = None;
        }
        self.lsp_sync.debounce.inlay_hints = None;
        self.lsp_sync.debounce.folding_range = None;

        if !preserve_hover {
            if let Some(service) = self
                .kernel_services
                .get_mut::<crate::kernel::services::adapters::LspService>()
            {
                service.cancel_hover();
            }

            if self.store.state().ui.hover.is_active() {
                let _ = self.dispatch_kernel(KernelAction::LspHoverClear);
            }
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
            match (key_event.code, key_event.modifiers) {
                (KeyCode::Esc, _) => {
                    let _ = self.dispatch_kernel(KernelAction::CompletionClose);
                    self.ui.completion_doc.scroll = 0;
                    self.ui.completion_doc.total_lines = 0;
                    self.ui.completion_doc.key = None;
                    self.ui.completion_doc.last_area = None;
                    self.ui.completion_doc.render_cache.clear();
                    return EventResult::Consumed;
                }
                (KeyCode::Tab, _) => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: 1 });
                    self.reset_completion_doc_scroll();
                    return EventResult::Consumed;
                }
                (KeyCode::BackTab, _) => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: -1 });
                    self.reset_completion_doc_scroll();
                    return EventResult::Consumed;
                }
                (KeyCode::PageUp, _) => {
                    let step = self.completion_doc_view_height().max(1) as isize;
                    let _ = self.scroll_completion_doc_by(-step);
                    return EventResult::Consumed;
                }
                (KeyCode::PageDown, _) => {
                    let step = self.completion_doc_view_height().max(1) as isize;
                    let _ = self.scroll_completion_doc_by(step);
                    return EventResult::Consumed;
                }
                (KeyCode::Char('u' | 'U'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    let half = (self.completion_doc_view_height() / 2).max(1) as isize;
                    let _ = self.scroll_completion_doc_by(-half);
                    return EventResult::Consumed;
                }
                (KeyCode::Char('d' | 'D'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                    let half = (self.completion_doc_view_height() / 2).max(1) as isize;
                    let _ = self.scroll_completion_doc_by(half);
                    return EventResult::Consumed;
                }
                (KeyCode::Enter, _) => {
                    let pane = self.store.state().ui.editor_layout.active_pane;
                    let before_version = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                        .map(|tab| tab.edit_version);
                    let _ = self.dispatch_kernel(KernelAction::CompletionConfirm);

                    // Schedule debounced save of completion frequency data.
                    self.pending_completion_rank_save_deadline =
                        Some(Instant::now() + std::time::Duration::from_secs(2));

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

                    // Completion session ends; reset doc scroll for the next popup.
                    if !self.store.state().ui.completion.visible {
                        self.ui.completion_doc.scroll = 0;
                        self.ui.completion_doc.total_lines = 0;
                        self.ui.completion_doc.key = None;
                        self.ui.completion_doc.last_area = None;
                        self.ui.completion_doc.render_cache.clear();
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

        if let Some(cmd) = cmd {
            let cmd_for_schedule = cmd.clone();
            let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd));
            self.maybe_trigger_completion(&cmd_for_schedule);
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
                    self.maybe_trigger_completion(&cmd);
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
            KeybindingContext::CommandLine => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::CommandLineAppend(ch));
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            // 搜索浮层：顶部 query 行接收字符与退格（telescope 风格的即时过滤）。
            KeybindingContext::Overlay
                if self.store.state().ui.overlay.active == Some(OverlayKind::Search) =>
            {
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                        let _ = self.dispatch_kernel(KernelAction::SearchAppend(ch));
                        // telescope 风格：边输入边搜。
                        let _ = self
                            .dispatch_kernel(KernelAction::RunCommand(Command::GlobalSearchStart));
                        EventResult::Consumed
                    }
                    (KeyCode::Backspace, _) => {
                        let _ = self.dispatch_kernel(KernelAction::SearchBackspace);
                        let _ = self
                            .dispatch_kernel(KernelAction::RunCommand(Command::GlobalSearchStart));
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                }
            }
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
            FocusTarget::Overlay => KeybindingContext::Overlay,
            FocusTarget::CommandLine => KeybindingContext::CommandLine,
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
