use super::super::Workbench;
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::KeybindingContext;
use crate::kernel::{Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget};
use crate::tui::view::EventResult;
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn handle_paste(&mut self, text: &str) -> EventResult {
        let _scope = perf::scope("input.paste");
        let context = self.keybinding_context();
        match context {
            KeybindingContext::Editor => {
                let pane = self.store.state().ui.editor_layout.active_pane;
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::InsertText {
                    pane,
                    text: text.to_string(),
                }));
                let refresh = Command::Paste;
                self.maybe_schedule_semantic_tokens_debounce(&refresh);
                self.maybe_schedule_inlay_hints_debounce(&refresh);
                self.maybe_schedule_folding_range_debounce(&refresh);
                EventResult::Consumed
            }
            KeybindingContext::EditorSearchBar => {
                let pane = self.store.state().ui.editor_layout.active_pane;
                for ch in text.chars() {
                    let _ =
                        self.dispatch_kernel(KernelAction::Editor(EditorAction::SearchBarAppend {
                            pane,
                            ch,
                        }));
                }
                EventResult::Consumed
            }
            KeybindingContext::SidebarSearch => {
                for ch in text.chars() {
                    let _ = self.dispatch_kernel(KernelAction::SearchAppend(ch));
                }
                EventResult::Consumed
            }
            KeybindingContext::CommandPalette => {
                for ch in text.chars() {
                    let _ = self.dispatch_kernel(KernelAction::PaletteAppend(ch));
                }
                EventResult::Consumed
            }
            KeybindingContext::BottomPanel => {
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    if let Some(id) = self.store.state().terminal.active_session().map(|s| s.id) {
                        let _ = self.dispatch_kernel(KernelAction::TerminalWrite {
                            id,
                            bytes: text.as_bytes().to_vec(),
                        });
                        return EventResult::Consumed;
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    pub(super) fn copy_logs_to_clipboard(&mut self) {
        let Some(clipboard) = self
            .kernel_services
            .get_mut::<crate::kernel::services::adapters::ClipboardService>()
        else {
            return;
        };

        if self.logs.is_empty() {
            return;
        }

        let mut text = String::new();
        for (idx, line) in self.logs.iter().enumerate() {
            if idx > 0 {
                text.push('\n');
            }
            text.push_str(line);
        }

        if let Err(err) = clipboard.set_text(&text) {
            self.logs.push_back(format!("[clipboard] {err}"));
            while self.logs.len() > super::super::LOG_BUFFER_CAP {
                self.logs.pop_front();
            }
        }
    }

    pub(super) fn maybe_schedule_semantic_tokens_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let edit_should_schedule = matches!(
            cmd,
            Command::InsertChar(_)
                | Command::InsertNewline
                | Command::InsertTab
                | Command::DeleteBackward
                | Command::DeleteForward
                | Command::DeleteLine
                | Command::DeleteToLineEnd
                | Command::DeleteSelection
                | Command::Undo
                | Command::Redo
                | Command::Paste
                | Command::Cut
        );

        let move_should_schedule = matches!(
            cmd,
            Command::CursorUp
                | Command::CursorDown
                | Command::ScrollUp
                | Command::ScrollDown
                | Command::PageUp
                | Command::PageDown
        ) && self
            .store
            .state()
            .lsp
            .server_capabilities
            .as_ref()
            .is_some_and(|c| c.semantic_tokens_range)
            && tab.buffer.len_lines().max(1) >= 2000;

        if edit_should_schedule || move_should_schedule {
            self.pending_semantic_tokens_deadline =
                Some(Instant::now() + super::super::SEMANTIC_TOKENS_DEBOUNCE_DELAY);
        }
    }

    pub(super) fn maybe_schedule_inlay_hints_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let should_schedule = matches!(
            cmd,
            Command::InsertChar(_)
                | Command::InsertNewline
                | Command::InsertTab
                | Command::DeleteBackward
                | Command::DeleteForward
                | Command::DeleteLine
                | Command::DeleteToLineEnd
                | Command::DeleteSelection
                | Command::Undo
                | Command::Redo
                | Command::Paste
                | Command::Cut
                | Command::CursorUp
                | Command::CursorDown
                | Command::ScrollUp
                | Command::ScrollDown
                | Command::PageUp
                | Command::PageDown
        );

        if should_schedule {
            self.pending_inlay_hints_deadline =
                Some(Instant::now() + super::super::INLAY_HINTS_DEBOUNCE_DELAY);
        }
    }

    pub(super) fn maybe_schedule_folding_range_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let should_schedule = matches!(
            cmd,
            Command::InsertChar(_)
                | Command::InsertNewline
                | Command::InsertTab
                | Command::DeleteBackward
                | Command::DeleteForward
                | Command::DeleteLine
                | Command::DeleteToLineEnd
                | Command::DeleteSelection
                | Command::Undo
                | Command::Redo
                | Command::Paste
                | Command::Cut
        );

        if should_schedule {
            self.pending_folding_range_deadline =
                Some(Instant::now() + super::super::FOLDING_RANGE_DEBOUNCE_DELAY);
        }
    }
}
