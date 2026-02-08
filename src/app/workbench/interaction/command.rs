use super::super::Workbench;
use super::{
    classify_lsp_edit_trigger, lsp_debounce_duration, LspDebouncePipeline, LspDebounceTrigger,
};
use crate::core::Command;
use crate::kernel::lsp_registry;
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
        self.set_clipboard_text(&text);
    }

    pub(in super::super) fn maybe_warn_clipboard_unavailable(&mut self) {
        if self.clipboard_unavailable_warned {
            return;
        }

        let available = self
            .kernel_services
            .get::<crate::kernel::services::adapters::ClipboardService>()
            .is_some_and(|svc| svc.is_available());
        if available {
            return;
        }

        self.push_log_line(
            "[clipboard] Clipboard unavailable. Linux: install wl-clipboard (wl-copy/wl-paste) or xclip/xsel. Paste via terminal (Ctrl+Shift+V).".to_string(),
        );
        self.clipboard_unavailable_warned = true;
    }

    pub(in super::super) fn set_clipboard_text(&mut self, text: &str) {
        let set_result = self
            .kernel_services
            .get_mut::<crate::kernel::services::adapters::ClipboardService>()
            .map(|svc| svc.set_text(text));

        match set_result {
            Some(Ok(())) => {}
            Some(Err(err)) => {
                self.maybe_warn_clipboard_unavailable();

                match crate::tui::osc52::copy_to_clipboard(text) {
                    Ok(()) => {
                        // This is a user-visible behavior change; keep a breadcrumb in Logs.
                        self.push_log_line("[clipboard] Copied via OSC52 fallback.".to_string());
                    }
                    Err(osc52_err) => {
                        self.push_log_line(format!("[clipboard] {err}"));
                        self.push_log_line(format!("[clipboard:osc52] {osc52_err}"));
                    }
                }
            }
            None => {
                self.maybe_warn_clipboard_unavailable();
            }
        }
    }

    pub(in super::super) fn push_log_line(&mut self, line: String) {
        self.logs.push_back(line);
        while self.logs.len() > super::super::LOG_BUFFER_CAP {
            self.logs.pop_front();
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
        if !lsp_registry::is_lsp_source_path(path) {
            return;
        }

        let timing = self.store.state().editor.config.lsp_input_timing.clone();

        let edit_trigger = classify_lsp_edit_trigger(cmd, &timing);

        let supports_semantic_tokens_range =
            lsp_registry::client_key_for_path(&self.store.state().workspace_root, path)
                .map(|(_, key)| key)
                .and_then(|key| self.store.state().lsp.server_capabilities.get(&key))
                .is_some_and(|c| c.semantic_tokens_range);
        let move_should_schedule = matches!(
            cmd,
            Command::CursorUp
                | Command::CursorDown
                | Command::ScrollUp
                | Command::ScrollDown
                | Command::PageUp
                | Command::PageDown
        ) && supports_semantic_tokens_range
            && tab.buffer.len_lines().max(1) >= 2000;

        if let Some(trigger) = edit_trigger {
            let delay =
                lsp_debounce_duration(&timing, LspDebouncePipeline::SemanticTokens, trigger);
            self.pending_semantic_tokens_deadline = Some(Instant::now() + delay);
            return;
        }

        if move_should_schedule {
            let delay = lsp_debounce_duration(
                &timing,
                LspDebouncePipeline::SemanticTokens,
                LspDebounceTrigger::Identifier,
            );
            self.pending_semantic_tokens_deadline = Some(Instant::now() + delay);
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
        if !lsp_registry::is_lsp_source_path(path) {
            return;
        }

        let timing = self.store.state().editor.config.lsp_input_timing.clone();

        let edit_trigger = classify_lsp_edit_trigger(cmd, &timing);
        let move_should_schedule = matches!(
            cmd,
            Command::CursorUp
                | Command::CursorDown
                | Command::ScrollUp
                | Command::ScrollDown
                | Command::PageUp
                | Command::PageDown
        );

        if let Some(trigger) = edit_trigger {
            let delay = lsp_debounce_duration(&timing, LspDebouncePipeline::InlayHints, trigger);
            self.pending_inlay_hints_deadline = Some(Instant::now() + delay);
            return;
        }

        if move_should_schedule {
            let delay = lsp_debounce_duration(
                &timing,
                LspDebouncePipeline::InlayHints,
                LspDebounceTrigger::Identifier,
            );
            self.pending_inlay_hints_deadline = Some(Instant::now() + delay);
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
        if !lsp_registry::is_lsp_source_path(path) {
            return;
        }

        let timing = self.store.state().editor.config.lsp_input_timing.clone();

        let edit_trigger = classify_lsp_edit_trigger(cmd, &timing);

        if let Some(trigger) = edit_trigger {
            let delay = lsp_debounce_duration(&timing, LspDebouncePipeline::FoldingRange, trigger);
            self.pending_folding_range_deadline = Some(Instant::now() + delay);
        }
    }
}
