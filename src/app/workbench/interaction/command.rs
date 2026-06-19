use super::super::Workbench;
use super::{
    classify_lsp_edit_trigger, lsp_debounce_duration, semantic_tokens_trigger, LspDebouncePipeline,
    LspDebounceTrigger,
};
use crate::core::Command;
use crate::kernel::lsp_registry;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::KeybindingContext;
use crate::kernel::{Action as KernelAction, EditorAction, FocusTarget};
use crate::tui::view::EventResult;
use std::path::PathBuf;
use std::time::Instant;

/// LSP 编辑前置守卫通过后的拥有式上下文：活动 pane 与文件路径。返回拥有式数据（path
/// 克隆）而非借用，使调用方在守卫后仍可自由 `&mut self`（dispatch / 改 debounce 字段）。
pub(super) struct LspEditContext {
    pub pane: usize,
    pub path: PathBuf,
}

impl Workbench {
    /// 四处 LSP-debounce / completion 调度共享的前置守卫：焦点在编辑器、LSP 服务存在、
    /// 有活动 tab 且其路径是 LSP 源文件。通过返回拥有式上下文，否则 None。
    pub(super) fn active_lsp_editing_context(&self) -> Option<LspEditContext> {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return None;
        }
        self.kernel_services
            .get::<crate::kernel::services::adapters::LspService>()?;
        let pane = self.store.state().ui.editor_layout.active_pane;
        let tab = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())?;
        let path = tab.path.as_ref()?;
        if !lsp_registry::is_lsp_source_path(path) {
            return None;
        }
        Some(LspEditContext {
            pane,
            path: path.clone(),
        })
    }

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
            KeybindingContext::CommandLine => {
                for ch in text.chars() {
                    let _ = self.dispatch_kernel(KernelAction::CommandLineAppend(ch));
                }
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
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
                        // This is a user-visible behavior change; keep a breadcrumb in the log file.
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

    /// 用户可见的诊断面包屑（剪贴板不可用、越界 fs 操作等）。Logs 视图已删除，
    /// 这些条目改写入日志文件，避免静默吞掉错误。
    pub(in super::super) fn push_log_line(&self, line: String) {
        tracing::warn!(target: "zcode::workbench", "{line}");
    }

    pub(super) fn maybe_schedule_semantic_tokens_debounce(&mut self, cmd: &Command) {
        let Some(ctx) = self.active_lsp_editing_context() else {
            return;
        };

        let timing = &self.store.state().editor.config.lsp_input_timing;
        let eager_refresh = self
            .store
            .state()
            .lsp
            .eager_semantic_refresh_paths
            .contains(&ctx.path);

        let edit_trigger = classify_lsp_edit_trigger(cmd, timing);

        let supports_semantic_tokens_range =
            lsp_registry::client_key_for_path(&self.store.state().workspace_root, &ctx.path)
                .map(|(_, key)| key)
                .and_then(|key| self.store.state().lsp.server_capabilities.get(&key))
                .is_some_and(|c| c.semantic_tokens_range);
        let buffer_lines = self
            .store
            .state()
            .editor
            .pane(ctx.pane)
            .and_then(|pane| pane.active_tab())
            .map(|tab| tab.buffer.len_lines().max(1))
            .unwrap_or(0);
        let move_should_schedule = matches!(
            cmd,
            Command::CursorUp
                | Command::CursorDown
                | Command::ScrollUp
                | Command::ScrollDown
                | Command::PageUp
                | Command::PageDown
        ) && supports_semantic_tokens_range
            && buffer_lines >= 2000;

        if let Some(trigger) = edit_trigger {
            let trigger = semantic_tokens_trigger(trigger, eager_refresh);
            let delay = lsp_debounce_duration(timing, LspDebouncePipeline::SemanticTokens, trigger);
            self.lsp_sync.debounce.semantic_tokens = Some(Instant::now() + delay);
            return;
        }

        if move_should_schedule {
            let trigger = semantic_tokens_trigger(LspDebounceTrigger::Identifier, eager_refresh);
            let delay = lsp_debounce_duration(timing, LspDebouncePipeline::SemanticTokens, trigger);
            self.lsp_sync.debounce.semantic_tokens = Some(Instant::now() + delay);
        }
    }

    pub(super) fn maybe_schedule_inlay_hints_debounce(&mut self, cmd: &Command) {
        if self.active_lsp_editing_context().is_none() {
            return;
        }

        let timing = &self.store.state().editor.config.lsp_input_timing;

        let edit_trigger = classify_lsp_edit_trigger(cmd, timing);
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
            let delay = lsp_debounce_duration(timing, LspDebouncePipeline::InlayHints, trigger);
            self.lsp_sync.debounce.inlay_hints = Some(Instant::now() + delay);
            return;
        }

        if move_should_schedule {
            let delay = lsp_debounce_duration(
                timing,
                LspDebouncePipeline::InlayHints,
                LspDebounceTrigger::Identifier,
            );
            self.lsp_sync.debounce.inlay_hints = Some(Instant::now() + delay);
        }
    }

    pub(super) fn maybe_schedule_folding_range_debounce(&mut self, cmd: &Command) {
        if self.active_lsp_editing_context().is_none() {
            return;
        }

        let timing = &self.store.state().editor.config.lsp_input_timing;

        let edit_trigger = classify_lsp_edit_trigger(cmd, timing);

        if let Some(trigger) = edit_trigger {
            let delay = lsp_debounce_duration(timing, LspDebouncePipeline::FoldingRange, trigger);
            self.lsp_sync.debounce.folding_range = Some(Instant::now() + delay);
        }
    }
}
