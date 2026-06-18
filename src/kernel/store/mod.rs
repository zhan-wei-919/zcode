use crate::core::Command;
use crate::kernel::editor::{EditorTabState, HighlightKind, PendingSemanticLine, ReloadCause};
use crate::kernel::services::ports::{
    LspCompletionTriggerContext, LspPosition, LspRange, LspTextEdit, LspWorkspaceEdit,
    LspWorkspaceFileEdit,
};
use std::path::Path;
use unicode_xid::UnicodeXID;

#[cfg(test)]
use crate::kernel::services::ports::{LspCompletionItem, LspPositionEncoding};

pub(crate) mod intel;
mod util;

#[path = "reducers/context_menu.rs"]
mod context_menu;
#[path = "reducers/explorer.rs"]
mod explorer;
#[path = "reducers/input_dialog.rs"]
mod input_dialog;
#[path = "reducers/search.rs"]
mod search;

use intel::completion::{
    adjust_completion_multiline_indentation, apply_completion_insertion_cursor,
    completion_replace_range, completion_runtime_context, should_close_completion_on_editor_action,
    sync_completion_items_from_cache, CompletionInsertion,
};
pub use intel::completion_rank::CompletionRanker;

#[cfg(test)]
use intel::completion::expand_snippet;
use intel::lsp::{
    lsp_position_encoding, lsp_position_encoding_for_path, lsp_position_from_buffer_pos,
    lsp_position_to_byte_offset, lsp_range_for_full_lines, lsp_request_target,
    lsp_server_capabilities_for_path, problem_byte_offset,
};
use intel::semantic::reconcile_pending_semantic_row;
use search::search_open_target;
use util::{find_open_tab, is_lsp_source_path, search_viewport_for_focus};

use super::{Action, AppState, EditorAction, Effect, FocusTarget, InputDialogKind, OverlayKind};
use crate::kernel::language::{
    adapter::adapter_for_tab, adapter_for, CompletionRecord, CompletionResolveState,
};

pub struct DispatchResult {
    pub effects: Vec<Effect>,
    pub state_changed: bool,
}

fn perf_action_label(action: &Action) -> &'static str {
    match action {
        Action::RunCommand(_) => "kernel.action.run_command",
        Action::Editor(_) => "kernel.action.editor",
        Action::Tick => "kernel.action.tick",
        Action::LspCompletion { .. } => "kernel.action.lsp_completion",
        Action::LspCompletionResolved { .. } => "kernel.action.lsp_completion_resolved",
        Action::LspSemanticTokens { .. } => "kernel.action.lsp_semantic_tokens",
        Action::LspSemanticTokensRange { .. } => "kernel.action.lsp_semantic_tokens_range",
        Action::LspInlayHints { .. } => "kernel.action.lsp_inlay_hints",
        Action::LspFoldingRanges { .. } => "kernel.action.lsp_folding_ranges",
        Action::LspDiagnostics { .. } => "kernel.action.lsp_diagnostics",
        Action::LspHoverClear => "kernel.action.lsp_hover_clear",
        Action::LspHoverResponse { .. } => "kernel.action.lsp_hover_response",
        Action::LspHoverImplementationPreview { .. } => {
            "kernel.action.lsp_hover_implementation_preview"
        }
        Action::LspHoverDefinitionPreview { .. } => "kernel.action.lsp_hover_definition_preview",
        Action::LspDefinition { .. } => "kernel.action.lsp_definition",
        Action::LspReferences { .. } => "kernel.action.lsp_references",
        Action::LspCodeActions { .. } => "kernel.action.lsp_code_actions",
        Action::LspSymbols { .. } => "kernel.action.lsp_symbols",
        Action::LspSignatureHelp { .. } => "kernel.action.lsp_signature_help",
        Action::LspApplyWorkspaceEdit { .. } => "kernel.action.lsp_apply_workspace_edit",
        Action::LspServerCapabilities { .. } => "kernel.action.lsp_server_capabilities",
        Action::LspProgressEnd => "kernel.action.lsp_progress_end",
        Action::SearchMessage(_) => "kernel.action.search_message",
        Action::SearchStarted { .. } => "kernel.action.search_started",
        Action::DirLoaded { .. } => "kernel.action.dir_loaded",
        Action::DirLoadError { .. } => "kernel.action.dir_load_error",
        _ => "kernel.action.other",
    }
}

fn perf_command_label(command: &Command) -> &'static str {
    match command {
        Command::InsertChar(_) => "kernel.command.insert_char",
        Command::DeleteBackward => "kernel.command.delete_backward",
        Command::DeleteForward => "kernel.command.delete_forward",
        Command::DeleteSelection => "kernel.command.delete_selection",
        Command::CursorLeft => "kernel.command.cursor_left",
        Command::CursorRight => "kernel.command.cursor_right",
        Command::CursorUp => "kernel.command.cursor_up",
        Command::CursorDown => "kernel.command.cursor_down",
        Command::LspCompletion => "kernel.command.lsp_completion",
        Command::LspSemanticTokens => "kernel.command.lsp_semantic_tokens",
        Command::LspInlayHints => "kernel.command.lsp_inlay_hints",
        Command::LspFoldingRange => "kernel.command.lsp_folding_range",
        Command::LspHover => "kernel.command.lsp_hover",
        Command::LspSignatureHelp => "kernel.command.lsp_signature_help",
        Command::EditorSearchBarBackspace => "kernel.command.editor_search_backspace",
        Command::Find => "kernel.command.find",
        Command::FindNext => "kernel.command.find_next",
        Command::FindPrev => "kernel.command.find_prev",
        Command::Save => "kernel.command.save",
        Command::OpenFile => "kernel.command.open_file",
        Command::CloseTab => "kernel.command.close_tab",
        Command::FocusEditor => "kernel.command.focus_editor",
        Command::FocusExplorer => "kernel.command.focus_explorer",
        Command::FocusSearch => "kernel.command.focus_search",
        Command::OpenDiagnostics => "kernel.command.open_diagnostics",
        Command::CloseOverlay => "kernel.command.close_overlay",
        Command::Escape => "kernel.command.escape",
        _ => "kernel.command.other",
    }
}

fn is_completion_identifier_start(ch: char) -> bool {
    ch == '_' || UnicodeXID::is_xid_start(ch)
}

fn is_completion_identifier_continue(ch: char) -> bool {
    ch == '_' || UnicodeXID::is_xid_continue(ch)
}

fn advance_identifier_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in text[start..].char_indices() {
        if !is_completion_identifier_continue(ch) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn completion_seed_head(text: &str) -> Option<&str> {
    let (mut start, _) = text
        .char_indices()
        .find(|(_, ch)| is_completion_identifier_start(*ch))?;
    let mut end = advance_identifier_end(text, start);
    let mut last = (start, end);

    loop {
        let rest = &text[end..];
        let sep_len = if rest.starts_with("::") || rest.starts_with("->") {
            2
        } else if rest.starts_with('.') {
            1
        } else {
            0
        };
        if sep_len == 0 {
            break;
        }

        let next_start = end + sep_len;
        let Some(next_ch) = text[next_start..].chars().next() else {
            break;
        };
        if !is_completion_identifier_start(next_ch) {
            break;
        }

        start = next_start;
        end = advance_identifier_end(text, start);
        last = (start, end);
    }

    text.get(last.0..last.1)
}

fn byte_offset_for_char_offset(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

fn completion_seed_matches_boundary(line: &str, start: usize, end: usize) -> bool {
    let prev_ok = line[..start]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_completion_identifier_continue(ch));
    let next_ok = line[end..]
        .chars()
        .next()
        .is_none_or(|ch| !is_completion_identifier_continue(ch));
    prev_ok && next_ok
}

fn seed_completion_semantic_highlight(
    tab: &mut EditorTabState,
    inserted_text: &str,
    kind: HighlightKind,
) -> bool {
    if inserted_text.contains('\n') {
        return false;
    }

    let Some(head) = completion_seed_head(inserted_text) else {
        return false;
    };

    let (row, col) = tab.buffer.cursor();
    let rope = tab.buffer.rope();
    let line_start_char = rope.line_to_char(row);
    let cursor_char = tab
        .buffer
        .pos_to_char((row, col))
        .saturating_sub(line_start_char);

    let Some(line_slice) = tab.buffer.line_slice(row) else {
        return false;
    };
    let line_owned = line_slice.to_string();
    let line = line_owned.strip_suffix('\n').unwrap_or(&line_owned);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let cursor_byte = byte_offset_for_char_offset(line, cursor_char);
    let search_end = cursor_byte.min(line.len());

    let start = line[..search_end]
        .rmatch_indices(head)
        .find_map(|(idx, _)| {
            let end = idx + head.len();
            completion_seed_matches_boundary(line, idx, end).then_some(idx)
        })
        .or_else(|| {
            line.rmatch_indices(head).find_map(|(idx, _)| {
                let end = idx + head.len();
                completion_seed_matches_boundary(line, idx, end).then_some(idx)
            })
        });
    let Some(start) = start else {
        return false;
    };

    let end = start + head.len();
    tab.seed_completion_token_semantic_kind(row, start, end, line.len(), kind)
}

pub struct Store {
    state: AppState,
    completion_ranker: CompletionRanker,
}

impl Store {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            completion_ranker: CompletionRanker::default(),
        }
    }

    pub fn new_with_ranker(state: AppState, completion_ranker: CompletionRanker) -> Self {
        Self {
            state,
            completion_ranker,
        }
    }

    /// 打开居中浮层并聚焦它，返回是否发生变化。所有 LSP 列表 / 诊断 / 搜索结果
    /// 都经此入口呈现，替代旧的常驻底部面板。
    fn open_overlay(&mut self, kind: OverlayKind) -> bool {
        let mut changed = self.state.ui.overlay.open(kind);
        if self.state.ui.focus != FocusTarget::Overlay {
            self.state.ui.focus = FocusTarget::Overlay;
            changed = true;
        }
        changed
    }

    /// 关闭浮层，焦点回到编辑区。
    fn close_overlay(&mut self) -> bool {
        let mut changed = self.state.ui.overlay.close();
        if self.state.ui.focus == FocusTarget::Overlay {
            self.state.ui.focus = FocusTarget::Editor;
            changed = true;
        }
        changed
    }

    fn active_tab_adapter(&self) -> &'static dyn crate::kernel::language::LanguageAdapter {
        let lang = self
            .state
            .editor
            .pane(self.state.ui.editor_layout.active_pane)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.language());
        adapter_for(lang)
    }

    fn completion_request_context(
        &self,
        pane: usize,
        path: std::path::PathBuf,
        version: u64,
    ) -> super::state::CompletionRequestContext {
        let normalization = self
            .state
            .editor
            .pane(pane)
            .and_then(|pane_state| pane_state.active_tab())
            .map(|tab| {
                let adapter = adapter_for_tab(tab);
                completion_runtime_context(&self.state, tab, adapter).completion_snapshot()
            })
            .unwrap_or_else(|| {
                crate::kernel::language::CompletionNormalizationSnapshot::detached(
                    crate::kernel::language::LanguageId::from_path(path.as_path()),
                )
            });

        super::state::CompletionRequestContext {
            pane,
            path,
            version,
            normalization,
        }
    }

    fn maybe_request_completion_resolve_for_record(
        &mut self,
        record: &CompletionRecord,
        effects: &mut Vec<Effect>,
    ) -> bool {
        if !matches!(
            record.entry.resolve_state,
            CompletionResolveState::Unresolved
        ) {
            return false;
        }
        if self.state.ui.completion.resolve_inflight == Some(record.entry.id) {
            return false;
        }

        self.state.ui.completion.resolve_inflight = Some(record.entry.id);
        let _ = self
            .state
            .ui
            .completion
            .set_resolve_state(record.entry.id, CompletionResolveState::Resolving);
        effects.push(Effect::LspCompletionResolveRequest {
            item: Box::new(record.raw.clone()),
        });
        true
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn completion_ranker(&self) -> &CompletionRanker {
        &self.completion_ranker
    }

    pub fn completion_ranker_is_dirty(&self) -> bool {
        self.completion_ranker.is_dirty()
    }

    pub fn clear_completion_ranker_dirty(&mut self) {
        self.completion_ranker.clear_dirty();
    }

    pub fn tick(&mut self) {
        let now = std::time::Instant::now();
        for pane in &mut self.state.editor.panes {
            for tab in &mut pane.tabs {
                tab.history.tick();
                if let crate::kernel::editor::DiskState::ReloadedFromDisk { at } = tab.disk_state {
                    if now.duration_since(at) >= std::time::Duration::from_secs(3) {
                        tab.disk_state = crate::kernel::editor::DiskState::InSync;
                    }
                }
            }
        }
    }

    fn reconcile_signature_help_visibility(&mut self) -> bool {
        if !self.state.ui.signature_help.is_active() {
            return false;
        }

        let keep_open = self
            .state
            .editor
            .pane(self.state.ui.editor_layout.active_pane)
            .and_then(|pane| pane.active_tab())
            .is_some_and(|tab| {
                let adapter = adapter_for_tab(tab);
                let runtime = completion_runtime_context(&self.state, tab, adapter);
                adapter
                    .interaction()
                    .signature_help_should_keep_open(&runtime)
            });

        if keep_open {
            return false;
        }

        self.state.ui.signature_help = super::state::SignatureHelpPopupState::default();
        true
    }

    fn active_editor_file_path(&self) -> Option<std::path::PathBuf> {
        let active_pane = self.state.ui.editor_layout.active_pane;
        self.state
            .editor
            .pane(active_pane)
            .and_then(|pane_state| pane_state.active_tab())
            .and_then(|tab| tab.path.clone())
    }

    fn active_editor_lsp_path_and_version(&self) -> Option<(std::path::PathBuf, u64)> {
        let active_pane = self.state.ui.editor_layout.active_pane;
        let tab = self
            .state
            .editor
            .pane(active_pane)
            .and_then(|pane_state| pane_state.active_tab())?;
        let path = tab.path.as_ref()?.clone();
        if !is_lsp_source_path(&path) {
            return None;
        }
        Some((path, tab.edit_version))
    }

    fn arm_semantic_flush_defer_for_path(&mut self, path: std::path::PathBuf, version: u64) {
        if self.state.lsp.eager_semantic_refresh_paths.contains(&path) {
            return;
        }
        self.state
            .lsp
            .defer_semantic_flush_by_path
            .insert(path, version);
    }

    fn arm_eager_semantic_refresh_for_path(&mut self, path: std::path::PathBuf) {
        self.state.lsp.eager_semantic_refresh_paths.insert(path);
    }

    fn clear_eager_semantic_refresh_for_path(&mut self, path: &Path) {
        self.state.lsp.eager_semantic_refresh_paths.remove(path);
    }

    fn clear_semantic_flush_defer_for_path(&mut self, path: &Path) {
        self.state.lsp.defer_semantic_flush_by_path.remove(path);
    }

    fn is_semantic_flush_deferred(&self, path: &Path, version: u64) -> bool {
        self.state
            .lsp
            .defer_semantic_flush_by_path
            .get(path)
            .is_some_and(|deferred_version| *deferred_version == version)
    }

    fn reconcile_pending_semantic_for_active_line(tab: &mut EditorTabState) {
        let (row, col) = tab.buffer.cursor();
        let Some(line_slice) = tab.buffer.line_slice(row) else {
            return;
        };

        let line_owned = line_slice.to_string();
        let line = line_owned.strip_suffix('\n').unwrap_or(&line_owned);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line_start_char = tab.buffer.rope().line_to_char(row);
        let cursor_char = tab
            .buffer
            .pos_to_char((row, col))
            .saturating_sub(line_start_char);
        let cursor_byte = byte_offset_for_char_offset(line, cursor_char);
        let merged_row = {
            let current = tab.semantic_segments_line(row);
            match tab.pending_semantic_line(row) {
                PendingSemanticLine::Uncovered => None,
                PendingSemanticLine::Covered(pending) => {
                    reconcile_pending_semantic_row(line, current, pending, cursor_byte)
                }
            }
        };
        let Some(merged_row) = merged_row else {
            return;
        };

        let merged_lines = [merged_row];
        let _ = tab.set_pending_semantic_highlight_range_from_slice(
            tab.edit_version,
            row,
            &merged_lines,
        );
    }

    fn flush_pending_semantic_highlight_for_tab(
        tab: &mut EditorTabState,
        is_active_tab: bool,
    ) -> bool {
        if is_active_tab {
            Self::reconcile_pending_semantic_for_active_line(tab);
        }
        tab.flush_pending_semantic_highlight()
    }

    fn flush_pending_semantic_highlights_for_path(&mut self, path: &Path) -> bool {
        self.clear_semantic_flush_defer_for_path(path);

        let active_pane_idx = self.state.ui.editor_layout.active_pane;
        let mut changed = false;
        for (pane_idx, pane) in self.state.editor.panes.iter_mut().enumerate() {
            let active_tab_idx = pane.active;
            for (tab_idx, tab) in pane.tabs.iter_mut().enumerate() {
                if tab.path.as_deref() != Some(path) {
                    continue;
                }
                let is_active_tab = pane_idx == active_pane_idx && tab_idx == active_tab_idx;
                changed |= Self::flush_pending_semantic_highlight_for_tab(tab, is_active_tab);
            }
        }
        changed
    }

    fn sync_explorer_selection_to_path(&mut self, path: &std::path::Path) -> bool {
        let Some(target_id) = self.state.explorer.node_id_for_path(path) else {
            return false;
        };
        if self.state.explorer.selected() == Some(target_id) {
            return false;
        }

        let Some(row) = self
            .state
            .explorer
            .rows
            .iter()
            .position(|row| row.id == target_id)
        else {
            return false;
        };

        self.state.explorer.select_row(row)
    }

    pub fn dispatch(&mut self, action: Action) -> DispatchResult {
        let _action_scope =
            crate::kernel::services::adapters::perf::scope(perf_action_label(&action));
        match action {
            Action::RunCommand(cmd) => {
                let _command_scope =
                    crate::kernel::services::adapters::perf::scope(perf_command_label(&cmd));
                let active_tab = self
                    .state
                    .editor
                    .pane(self.state.ui.editor_layout.active_pane)
                    .and_then(|p| p.active_tab());
                let completion_changed = if let Some(tab) = active_tab {
                    let adapter = self.active_tab_adapter();
                    let runtime = completion_runtime_context(&self.state, tab, adapter);
                    if adapter
                        .interaction()
                        .should_close_on_command(&cmd, Some(&runtime))
                    {
                        self.state.ui.completion.close()
                    } else {
                        false
                    }
                } else if self
                    .active_tab_adapter()
                    .interaction()
                    .should_close_on_command(&cmd, None)
                {
                    self.state.ui.completion.close()
                } else {
                    false
                };

                let mut result = self.dispatch_command(cmd);
                result.state_changed |= completion_changed;
                result
            }
            Action::Editor(editor_action) => {
                let prev_active_file = self.active_editor_file_path();
                let completion_changed = if should_close_completion_on_editor_action(&editor_action)
                {
                    self.state.ui.completion.close()
                } else {
                    false
                };

                let mut result = match editor_action {
                    EditorAction::OpenFile {
                        pane,
                        path,
                        content,
                    } => {
                        let opened_path = path.clone();
                        let pending = self
                            .state
                            .ui
                            .pending_editor_nav
                            .as_ref()
                            .filter(|p| p.pane == pane && p.path == path)
                            .map(|p| p.target.clone());

                        let (mut state_changed, mut effects) =
                            self.state.editor.dispatch_action(EditorAction::OpenFile {
                                pane,
                                path,
                                content,
                            });

                        if let Some(target) = pending {
                            let byte_offset = match target {
                                super::state::PendingEditorNavigationTarget::ByteOffset {
                                    byte_offset,
                                } => byte_offset,
                                super::state::PendingEditorNavigationTarget::LineColumn {
                                    line,
                                    column,
                                } => self
                                    .state
                                    .editor
                                    .pane(pane)
                                    .and_then(|pane_state| pane_state.active_tab())
                                    .map(|tab| {
                                        lsp_position_to_byte_offset(
                                            tab,
                                            line,
                                            column,
                                            lsp_position_encoding(&self.state),
                                        )
                                    })
                                    .unwrap_or(0),
                            };

                            let (changed, cmd_effects) =
                                self.state
                                    .editor
                                    .dispatch_action(EditorAction::GotoByteOffset {
                                        pane,
                                        byte_offset,
                                    });
                            state_changed |= changed;
                            effects.extend(cmd_effects);
                            self.state.ui.pending_editor_nav = None;
                        }

                        let caps = lsp_server_capabilities_for_path(&self.state, &opened_path);
                        let supports_semantic_tokens = caps.is_some_and(|c| {
                            c.semantic_tokens && (c.semantic_tokens_full || c.semantic_tokens_range)
                        });
                        let supports_inlay_hints = caps.is_some_and(|c| c.inlay_hints);
                        let supports_folding_range = caps.is_some_and(|c| c.folding_range);
                        if (supports_semantic_tokens
                            || supports_inlay_hints
                            || supports_folding_range)
                            && is_lsp_source_path(&opened_path)
                        {
                            let Some(tab) = self
                                .state
                                .editor
                                .pane(pane)
                                .and_then(|pane_state| pane_state.active_tab())
                            else {
                                return DispatchResult {
                                    effects,
                                    state_changed,
                                };
                            };
                            let version = tab.edit_version;
                            let encoding =
                                lsp_position_encoding_for_path(&self.state, &opened_path);

                            if supports_semantic_tokens {
                                let Some(caps) = caps else {
                                    return DispatchResult {
                                        effects,
                                        state_changed,
                                    };
                                };

                                let total_lines = tab.buffer.len_lines().max(1);
                                let can_range = caps.semantic_tokens_range;
                                let can_full = caps.semantic_tokens_full;
                                let prefer_range = can_range && (total_lines >= 2000 || !can_full);

                                if prefer_range {
                                    let viewport_top =
                                        tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                                    let height = tab.viewport.height.max(1);
                                    let overscan = 40usize.min(total_lines);
                                    let start_line = viewport_top.saturating_sub(overscan);
                                    let end_line_exclusive =
                                        (viewport_top + height + overscan).min(total_lines);
                                    if let Some(range) = lsp_range_for_full_lines(
                                        tab,
                                        start_line,
                                        end_line_exclusive,
                                        encoding,
                                    ) {
                                        effects.push(Effect::LspSemanticTokensRangeRequest {
                                            path: opened_path.clone(),
                                            version,
                                            range,
                                        });
                                    }
                                } else if can_full {
                                    effects.push(Effect::LspSemanticTokensRequest {
                                        path: opened_path.clone(),
                                        version,
                                    });
                                }
                            }

                            if supports_inlay_hints {
                                let total_lines = tab.buffer.len_lines().max(1);
                                let start_line =
                                    tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                                let end_line_exclusive =
                                    (start_line + tab.viewport.height.max(1)).min(total_lines);
                                if let Some(range) = lsp_range_for_full_lines(
                                    tab,
                                    start_line,
                                    end_line_exclusive,
                                    encoding,
                                ) {
                                    effects.push(Effect::LspInlayHintsRequest {
                                        path: opened_path.clone(),
                                        version,
                                        range,
                                    });
                                }
                            }

                            if supports_folding_range {
                                effects.push(Effect::LspFoldingRangeRequest {
                                    path: opened_path,
                                    version,
                                });
                            }
                        }

                        DispatchResult {
                            effects,
                            state_changed,
                        }
                    }
                    EditorAction::SetActiveTab { pane, index } => {
                        let (state_changed, effects) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::SetActiveTab { pane, index });

                        DispatchResult {
                            effects,
                            state_changed,
                        }
                    }
                    EditorAction::Saved {
                        pane,
                        path,
                        success,
                        version,
                    } => {
                        let (state_changed, effects) =
                            self.state.editor.dispatch_action(EditorAction::Saved {
                                pane,
                                path,
                                success,
                                version,
                            });

                        DispatchResult {
                            effects,
                            state_changed,
                        }
                    }
                    EditorAction::CloseTabAt { pane, index } => {
                        let (state_changed, effects) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::CloseTabAt { pane, index });

                        DispatchResult {
                            effects,
                            state_changed,
                        }
                    }
                    EditorAction::CloseTabsById { pane, tab_ids } => {
                        let (state_changed, effects) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::CloseTabsById { pane, tab_ids });

                        DispatchResult {
                            effects,
                            state_changed,
                        }
                    }
                    other => {
                        let (state_changed, effects) = self.state.editor.dispatch_action(other);
                        DispatchResult {
                            effects,
                            state_changed,
                        }
                    }
                };

                result.state_changed |= completion_changed;

                let next_active_file = self.active_editor_file_path();
                if next_active_file != prev_active_file {
                    if let Some(path) = next_active_file.as_deref() {
                        result.state_changed |= self.sync_explorer_selection_to_path(path);
                    }
                }
                result.state_changed |= self.reconcile_signature_help_visibility();
                result
            }
            Action::OpenPath(path) => DispatchResult {
                effects: vec![Effect::LoadFile(path)],
                state_changed: false,
            },
            Action::Tick => DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            },
            Action::EditorConfigUpdated { config } => {
                if self.state.editor.config == config {
                    DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    }
                } else {
                    self.state.editor.config = config;
                    DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    }
                }
            }
            action @ Action::InputDialogAppend(_)
            | action @ Action::InputDialogBackspace
            | action @ Action::InputDialogCursorLeft
            | action @ Action::InputDialogCursorRight
            | action @ Action::InputDialogAccept
            | action @ Action::InputDialogCancel => self.reduce_input_dialog_action(action),
            Action::EditorSetActivePane { pane } => {
                let completion_changed = { self.state.ui.completion.close() };

                let panes = self.state.ui.editor_layout.panes.max(1);
                let pane = pane.min(panes - 1);
                let prev = self.state.ui.editor_layout.active_pane;
                let prev_focus = self.state.ui.focus;

                self.state.ui.editor_layout.active_pane = pane;
                self.state.ui.focus = FocusTarget::Editor;

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: pane != prev
                        || prev_focus != FocusTarget::Editor
                        || completion_changed
                        || self.reconcile_signature_help_visibility(),
                }
            }
            Action::SidebarSetWidth { width } => {
                let width = width.max(1);
                let prev = self.state.ui.sidebar_width;
                let next = Some(width);
                self.state.ui.sidebar_width = next;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: prev != next,
                }
            }
            action @ Action::ContextMenuOpen { .. }
            | action @ Action::ContextMenuClose
            | action @ Action::ContextMenuMoveSelection { .. }
            | action @ Action::ContextMenuSetSelected { .. }
            | action @ Action::ContextMenuConfirm => self.reduce_context_menu_action(action),
            action @ Action::ExplorerSetViewHeight { .. }
            | action @ Action::ExplorerMoveSelection { .. }
            | action @ Action::ExplorerScroll { .. }
            | action @ Action::ExplorerActivate
            | action @ Action::ExplorerCollapse
            | action @ Action::ExplorerClickRow { .. }
            | action @ Action::ExplorerMovePath { .. } => self.reduce_explorer_action(action),
            action @ Action::SearchSetViewHeight { .. }
            | action @ Action::SearchAppend(_)
            | action @ Action::SearchBackspace
            | action @ Action::SearchClickRow { .. }
            | action @ Action::SearchStarted { .. }
            | action @ Action::SearchMessage(_) => self.reduce_search_action(action),
            Action::ProblemsClickRow { row } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.problems.click_row(row),
            },
            Action::ProblemsSetViewHeight { height } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.problems.set_view_height(height),
            },
            Action::CodeActionsClickRow { row } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.code_actions.click_row(row),
            },
            Action::CodeActionsSetViewHeight { height } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.code_actions.set_view_height(height),
            },
            Action::LocationsClickRow { row } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.locations.click_row(row),
            },
            Action::LocationsSetViewHeight { height } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.locations.set_view_height(height),
            },
            Action::SymbolsClickRow { row } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.symbols.click_row(row),
            },
            Action::SymbolsSetViewHeight { height } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.symbols.set_view_height(height),
            },
            action @ Action::LspDiagnostics { .. }
            | action @ Action::LspHoverClear
            | action @ Action::LspHoverResponse { .. }
            | action @ Action::LspHoverImplementationPreview { .. }
            | action @ Action::LspHoverDefinitionPreview { .. }
            | action @ Action::LspDefinition { .. }
            | action @ Action::LspReferences { .. }
            | action @ Action::LspCodeActions { .. }
            | action @ Action::LspSymbols { .. }
            | action @ Action::LspServerCapabilities { .. }
            | action @ Action::LspSemanticTokens { .. }
            | action @ Action::LspSemanticTokensRange { .. }
            | action @ Action::LspInlayHints { .. }
            | action @ Action::LspFoldingRanges { .. }
            | action @ Action::LspCompletion { .. }
            | action @ Action::LspCompletionResolved { .. }
            | action @ Action::LspSignatureHelp { .. }
            | action @ Action::LspApplyWorkspaceEdit { .. }
            | action @ Action::LspFormatCompleted { .. } => self.reduce_lsp_action(action),
            Action::LspProgressEnd => DispatchResult {
                effects: Vec::new(),
                state_changed: true,
            },
            Action::CompletionClose => {
                let had = self.state.ui.completion.close();
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: had,
                }
            }
            Action::CompletionMoveSelection { delta } => {
                if !self.state.ui.completion.visible || self.state.ui.completion.visible_len() == 0
                {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                let len = self.state.ui.completion.visible_len();
                let prev = self.state.ui.completion.selected;
                let next = (prev as isize).wrapping_add(delta).rem_euclid(len as isize) as usize;
                self.state.ui.completion.selected = next;

                let mut effects = Vec::new();
                if next != prev {
                    self.state.ui.completion.selection_locked = true;
                    if let Some(record) = self.state.ui.completion.visible_record(next).cloned() {
                        let _ =
                            self.maybe_request_completion_resolve_for_record(&record, &mut effects);
                    }
                }
                DispatchResult {
                    effects,
                    state_changed: next != prev,
                }
            }
            Action::CompletionConfirm => {
                if !self.state.ui.completion.visible || self.state.ui.completion.visible_len() == 0
                {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let Some(req) = self.state.ui.completion.request.clone() else {
                    let had = self.state.ui.completion.close();
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                };

                let Some(tab) = self
                    .state
                    .editor
                    .pane(req.pane)
                    .and_then(|pane| pane.active_tab())
                else {
                    let had = self.state.ui.completion.close();
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                };

                let valid = tab.path.as_ref() == Some(&req.path);
                if !valid {
                    let had = self.state.ui.completion.close();
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                }

                let Some(record) = self.state.ui.completion.selected_record().cloned() else {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };
                let completion_highlight_kind = {
                    let adapter = adapter_for_tab(tab);
                    adapter.completion_protocol().completion_highlight_kind(
                        &crate::kernel::language::CompletionContext::snapshot(
                            req.normalization.clone(),
                            &record.raw,
                        ),
                    )
                };
                let entry = record.entry;

                {
                    let language = self
                        .state
                        .editor
                        .pane(req.pane)
                        .and_then(|pane| pane.active_tab())
                        .and_then(|tab| tab.language());
                    self.completion_ranker
                        .record(language, &entry.label, entry.kind);
                }

                let mut insertion = CompletionInsertion::from_plan(entry.commit.insert.clone());

                let encoding = lsp_position_encoding_for_path(&self.state, &req.path);
                let replace_range =
                    completion_replace_range(tab, req.version, &entry.commit.replace, encoding);
                let insertion_start_byte = lsp_position_to_byte_offset(
                    tab,
                    replace_range.start.line,
                    replace_range.start.character,
                    encoding,
                );
                let insertion_start_char = tab
                    .buffer
                    .rope()
                    .byte_to_char(insertion_start_byte.min(tab.buffer.rope().len_bytes()));
                insertion =
                    adjust_completion_multiline_indentation(tab, insertion_start_char, insertion);

                let _ = self.state.ui.completion.close();

                let mut edits = entry.commit.additional_edits.clone();
                edits.push(LspTextEdit {
                    range: replace_range,
                    new_text: insertion.text.clone(),
                });

                let mut effects = Vec::new();
                let _changed = self.apply_workspace_edit(
                    LspWorkspaceEdit {
                        changes: vec![LspWorkspaceFileEdit {
                            path: req.path.clone(),
                            edits,
                        }],
                        ..Default::default()
                    },
                    &mut effects,
                );

                let tab_size = self.state.editor.config.tab_size;
                if let Some(pane_state) = self.state.editor.pane_mut(req.pane) {
                    let active = pane_state.active;
                    let target = if pane_state
                        .tabs
                        .get(active)
                        .is_some_and(|tab| tab.path.as_ref() == Some(&req.path))
                    {
                        Some(active)
                    } else {
                        pane_state
                            .tabs
                            .iter()
                            .position(|tab| tab.path.as_ref() == Some(&req.path))
                    };

                    if let Some(tab_index) = target {
                        if let Some(tab) = pane_state.tabs.get_mut(tab_index) {
                            if insertion.has_cursor_or_selection() {
                                apply_completion_insertion_cursor(tab, &insertion, tab_size);
                            }
                            if let Some(kind) = completion_highlight_kind {
                                let _ = seed_completion_semantic_highlight(
                                    tab,
                                    insertion.text.as_str(),
                                    kind,
                                );
                            }
                        }
                    }
                }

                if let Some(cmd) = entry.commit.command {
                    effects.push(Effect::LspExecuteCommand {
                        command: cmd.command,
                        arguments: cmd.arguments,
                    });
                }

                let requested_semantic_refresh = effects.iter().any(|effect| {
                    matches!(
                        effect,
                        Effect::LspSemanticTokensRequest { path, .. }
                            | Effect::LspSemanticTokensRangeRequest { path, .. }
                            if path == &req.path
                    )
                });
                if requested_semantic_refresh {
                    self.arm_eager_semantic_refresh_for_path(req.path.clone());
                }

                // Flush pending semantic highlights immediately on completion confirm.
                self.flush_pending_semantic_highlights_for_path(&req.path);

                DispatchResult {
                    effects,
                    state_changed: true,
                }
            }
            Action::DirLoaded { path, entries } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_dir_loaded(path, entries),
            },
            Action::DirLoadError { path } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_dir_load_error(path),
            },
            Action::ExplorerPathCreated { path, is_dir } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_path_created(path, is_dir),
            },
            Action::ExplorerPathDeleted { path } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_path_deleted(path),
            },
            Action::ExplorerPathRenamed { from, to } => {
                let mut state_changed = self
                    .state
                    .explorer
                    .apply_path_renamed(from.clone(), to.clone());
                let mut open_paths_changed = false;

                for pane in &mut self.state.editor.panes {
                    for tab in &mut pane.tabs {
                        let Some(path) = tab.path.as_ref() else {
                            continue;
                        };
                        if path == &from {
                            tab.set_path(to.clone());
                            open_paths_changed = true;
                            continue;
                        }
                        if path.starts_with(&from) {
                            if let Ok(rel) = path.strip_prefix(&from) {
                                tab.set_path(to.join(rel));
                                open_paths_changed = true;
                            }
                        }
                    }
                }

                if open_paths_changed {
                    self.state.editor.open_paths_version =
                        self.state.editor.open_paths_version.saturating_add(1);
                    state_changed = true;
                }

                DispatchResult {
                    effects: Vec::new(),
                    state_changed,
                }
            }
            Action::ExplorerDirChanged { path } => {
                let (state_changed, effects) = self.state.explorer.request_dir_reload(path);
                DispatchResult {
                    effects,
                    state_changed,
                }
            }
            Action::CommandLineAppend(ch) => {
                if !self.state.ui.command_line.active {
                    DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    }
                } else {
                    let line = &mut self.state.ui.command_line;
                    let cursor = line.cursor.min(line.input.len());
                    line.input.insert(cursor, ch);
                    line.cursor = cursor + ch.len_utf8();
                    line.selected = 0;
                    DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    }
                }
            }
            Action::SetHoveredTab { pane, index } => {
                let prev = self.state.ui.hovered_tab;
                self.state.ui.hovered_tab = Some((pane, index));
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: prev != self.state.ui.hovered_tab,
                }
            }
            Action::ClearHoveredTab => {
                let prev = self.state.ui.hovered_tab.take();
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: prev.is_some(),
                }
            }
            Action::ShowConfirmDialog {
                message,
                on_confirm,
            } => {
                self.state.ui.confirm_dialog.visible = true;
                self.state.ui.confirm_dialog.message = message;
                self.state.ui.confirm_dialog.on_confirm = Some(on_confirm);
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ConfirmDialogAccept => {
                if !self.state.ui.confirm_dialog.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let pending = self.state.ui.confirm_dialog.on_confirm.take();
                self.state.ui.confirm_dialog.visible = false;
                self.state.ui.confirm_dialog.message.clear();

                if let Some(action) = pending {
                    match action {
                        super::PendingAction::CloseTab { pane, index } => {
                            let mut result = self
                                .dispatch(Action::Editor(EditorAction::CloseTabAt { pane, index }));
                            result.state_changed = true;
                            return result;
                        }
                        super::PendingAction::CloseTabsBatch { pane, tab_ids } => {
                            let mut result =
                                self.dispatch(Action::Editor(EditorAction::CloseTabsById {
                                    pane,
                                    tab_ids,
                                }));
                            result.state_changed = true;
                            return result;
                        }
                        super::PendingAction::DeletePath { path, is_dir } => {
                            let root = self.state.workspace_root.as_path();
                            if path.as_path() == root || !path.starts_with(root) {
                                return DispatchResult {
                                    effects: Vec::new(),
                                    state_changed: true,
                                };
                            }
                            return DispatchResult {
                                effects: vec![Effect::DeletePath { path, is_dir }],
                                state_changed: true,
                            };
                        }
                        super::PendingAction::RenamePath {
                            from,
                            to,
                            overwrite,
                        } => {
                            let root = self.state.workspace_root.as_path();
                            if from.as_path() == root
                                || to.as_path() == root
                                || !from.starts_with(root)
                                || !to.starts_with(root)
                            {
                                return DispatchResult {
                                    effects: Vec::new(),
                                    state_changed: true,
                                };
                            }

                            return DispatchResult {
                                effects: vec![Effect::RenamePath {
                                    from,
                                    to,
                                    overwrite,
                                }],
                                state_changed: true,
                            };
                        }
                        super::PendingAction::CopyPath {
                            from,
                            to,
                            overwrite,
                        } => {
                            let root = self.state.workspace_root.as_path();
                            if from.as_path() == root
                                || to.as_path() == root
                                || !from.starts_with(root)
                                || !to.starts_with(root)
                            {
                                return DispatchResult {
                                    effects: Vec::new(),
                                    state_changed: true,
                                };
                            }

                            return DispatchResult {
                                effects: vec![Effect::CopyPath {
                                    from,
                                    to,
                                    overwrite,
                                }],
                                state_changed: true,
                            };
                        }
                    }
                }

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ConfirmDialogCancel => {
                if !self.state.ui.confirm_dialog.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                self.state.ui.confirm_dialog.visible = false;
                self.state.ui.confirm_dialog.message.clear();
                self.state.ui.confirm_dialog.on_confirm = None;

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
        }
    }

    fn dispatch_command(&mut self, command: Command) -> DispatchResult {
        let mut state_changed = false;
        let effects = Vec::new();

        match command {
            Command::Escape => {
                // 命令行 / 浮层各自的 Esc 已由其键位上下文处理；此处兜底关闭它们，
                // 防止 Escape 经全局回退时残留活动状态。
                if self.state.ui.command_line.active {
                    self.state.ui.command_line.reset();
                    if self.state.ui.focus == FocusTarget::CommandLine {
                        self.state.ui.focus = FocusTarget::Editor;
                    }
                    return DispatchResult {
                        effects,
                        state_changed: true,
                    };
                }
                if self.state.ui.overlay.is_visible() {
                    return DispatchResult {
                        effects,
                        state_changed: self.close_overlay(),
                    };
                }

                if self.state.ui.focus != FocusTarget::Editor {
                    self.state.ui.focus = FocusTarget::Editor;
                    return DispatchResult {
                        effects,
                        state_changed: true,
                    };
                }

                let pane = self.state.ui.editor_layout.active_pane;
                let search_bar_visible = self
                    .state
                    .editor
                    .pane(pane)
                    .is_some_and(|p| p.search_bar.visible);
                if search_bar_visible {
                    let (changed, eff) = self
                        .state
                        .editor
                        .apply_command(pane, Command::EditorSearchBarClose);
                    return DispatchResult {
                        effects: eff,
                        state_changed: changed,
                    };
                }

                let is_multi_cursor = self
                    .state
                    .editor
                    .pane(pane)
                    .and_then(|p| p.active_tab())
                    .is_some_and(|t| t.is_multi_cursor());
                if is_multi_cursor {
                    let (changed, eff) = self
                        .state
                        .editor
                        .apply_command(pane, Command::RemoveSecondaryCursors);
                    return DispatchResult {
                        effects: eff,
                        state_changed: changed,
                    };
                }

                let has_selection = self
                    .state
                    .editor
                    .pane(pane)
                    .and_then(|p| p.active_tab())
                    .is_some_and(|t| t.buffer.selection().is_some());
                if has_selection {
                    let (changed, eff) = self
                        .state
                        .editor
                        .apply_command(pane, Command::ClearSelection);
                    return DispatchResult {
                        effects: eff,
                        state_changed: changed,
                    };
                }

                // 没有任何可关闭的东西时，Esc 是无操作——绝不打开设置文件。
                return DispatchResult {
                    effects,
                    state_changed: false,
                };
            }
            Command::Quit => {
                self.state.ui.should_quit = true;
                return DispatchResult {
                    effects: vec![Effect::LspShutdown],
                    state_changed: true,
                };
            }
            Command::ReloadSettings => {
                return DispatchResult {
                    effects: vec![Effect::ReloadSettings],
                    state_changed: false,
                };
            }
            Command::OpenSettings => {
                return DispatchResult {
                    effects: vec![Effect::OpenSettings],
                    state_changed: false,
                };
            }
            Command::HardReload => {
                return DispatchResult {
                    effects: vec![Effect::Restart {
                        path: self.state.workspace_root.clone(),
                        hard: true,
                    }],
                    state_changed: false,
                };
            }
            Command::ReloadFromDisk => {
                let pane = self.state.ui.editor_layout.active_pane;
                if let Some(request) = self
                    .state
                    .editor
                    .pane_mut(pane)
                    .and_then(|p| p.active_tab_mut())
                    .and_then(|t| t.issue_reload_request(pane, ReloadCause::ManualCommand))
                {
                    return DispatchResult {
                        effects: vec![Effect::ReloadFile(request)],
                        state_changed: false,
                    };
                }
            }
            Command::InsertChar(ch) => {
                let pane = self.state.ui.editor_layout.active_pane;
                let (changed, cmd_effects) = self
                    .state
                    .editor
                    .apply_command(pane, Command::InsertChar(ch));
                if changed {
                    state_changed = true;
                }

                let mut effects = effects;
                effects.extend(cmd_effects);

                let boundary_chars = self
                    .state
                    .editor
                    .config
                    .lsp_input_timing
                    .boundary_chars
                    .as_str();
                if let Some((path, version)) = self.active_editor_lsp_path_and_version() {
                    if boundary_chars.contains(ch) {
                        state_changed |= self.flush_pending_semantic_highlights_for_path(&path);
                    } else if changed {
                        self.arm_semantic_flush_defer_for_path(path, version);
                    }
                }

                let tab = self
                    .state
                    .editor
                    .pane(pane)
                    .and_then(|pane| pane.active_tab());
                let tab_with_adapter = tab.map(|t| (t, adapter_for_tab(t)));
                let (
                    _tab_supports_completion_resolve,
                    should_complete,
                    should_trigger_signature_help,
                ) = {
                    let signature_help_active = self.state.ui.signature_help.is_active();
                    let caps = tab
                        .and_then(|t| t.path.as_ref())
                        .and_then(|path| lsp_server_capabilities_for_path(&self.state, path));

                    let tab_supports_completion = caps.is_none_or(|caps| caps.completion);
                    let tab_supports_completion_resolve =
                        caps.is_none_or(|caps| caps.completion_resolve);
                    let tab_supports_signature_help = caps.is_none_or(|caps| caps.signature_help);

                    let completion_triggers: &[char] = caps
                        .map(|caps| caps.completion_triggers.as_slice())
                        .unwrap_or(&[]);
                    let signature_help_triggers: &[char] = caps
                        .map(|caps| caps.signature_help_triggers.as_slice())
                        .unwrap_or(&[]);

                    let should_complete = tab_with_adapter.is_some_and(|(tab, adapter)| {
                        let Some(path) = tab.path.as_ref() else {
                            return false;
                        };
                        if !is_lsp_source_path(path) {
                            return false;
                        }

                        if !tab_supports_completion {
                            return false;
                        }

                        let runtime = completion_runtime_context(&self.state, tab, adapter);
                        adapter.interaction().completion_triggered_by_insert(
                            &runtime,
                            ch,
                            completion_triggers,
                        )
                    });

                    let should_trigger_signature_help = tab_supports_signature_help
                        && tab_with_adapter.is_some_and(|(tab, adapter)| {
                            let runtime = completion_runtime_context(&self.state, tab, adapter);
                            adapter.interaction().signature_help_triggered(
                                &runtime,
                                ch,
                                signature_help_triggers,
                            )
                        });
                    // Avoid popping up signature help on `,` when it isn't already active.
                    // This is a common editing gesture inside existing calls (e.g. building
                    // variadic argument lists) where a persistent popup is distracting.
                    let should_trigger_signature_help =
                        should_trigger_signature_help && (ch != ',' || signature_help_active);

                    (
                        tab_supports_completion_resolve,
                        should_complete,
                        should_trigger_signature_help,
                    )
                };

                if should_complete {
                    if let Some((pane, path, line, column, version)) =
                        lsp_request_target(&self.state)
                    {
                        self.state.ui.hover.clear();
                        self.state.ui.completion.close();
                        self.state.ui.completion.pending_request =
                            Some(self.completion_request_context(pane, path.clone(), version));

                        effects.push(Effect::LspCompletionRequest {
                            path,
                            line,
                            column,
                            trigger: LspCompletionTriggerContext::trigger_character(ch),
                        });
                        state_changed = true;
                    }
                }
                if !should_complete && !self.state.ui.completion.all_items.is_empty() {
                    if let Some((tab, adapter)) = tab_with_adapter {
                        let session_ok =
                            self.state
                                .ui
                                .completion
                                .request
                                .as_ref()
                                .is_some_and(|session| {
                                    session.pane == pane && tab.path.as_ref() == Some(&session.path)
                                });
                        if session_ok {
                            let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                                tab.language(),
                                tab,
                                adapter.syntax().syntax_facts(tab),
                            );
                            let mut changed = sync_completion_items_from_cache(
                                &mut self.state.ui.completion,
                                &runtime,
                                adapter.interaction(),
                            );

                            if let Some(record) =
                                self.state.ui.completion.selected_record().cloned()
                            {
                                if matches!(
                                    record.entry.resolve_state,
                                    CompletionResolveState::Unresolved
                                ) && record
                                    .entry
                                    .documentation
                                    .as_ref()
                                    .is_none_or(|d| d.trim().is_empty())
                                    && self.state.ui.completion.resolve_inflight
                                        != Some(record.entry.id)
                                {
                                    self.state.ui.completion.resolve_inflight =
                                        Some(record.entry.id);
                                    let _ = self.state.ui.completion.set_resolve_state(
                                        record.entry.id,
                                        CompletionResolveState::Resolving,
                                    );
                                    effects.push(Effect::LspCompletionResolveRequest {
                                        item: Box::new(record.raw),
                                    });
                                    changed = true;
                                }
                            }

                            if changed {
                                state_changed = true;
                            }
                        }
                    }
                }

                if tab_with_adapter
                    .map(|(_, adapter)| adapter.interaction())
                    .unwrap_or_else(|| self.active_tab_adapter().interaction())
                    .signature_help_closed_by(ch)
                {
                    let had = self.state.ui.signature_help.is_active();
                    if had {
                        self.state.ui.signature_help =
                            super::state::SignatureHelpPopupState::default();
                        state_changed = true;
                    }
                }

                if should_trigger_signature_help {
                    if let Some((pane, path, line, column, version)) =
                        lsp_request_target(&self.state)
                    {
                        self.state.ui.signature_help.visible = false;
                        self.state.ui.signature_help.model = None;
                        self.state.ui.signature_help.request =
                            Some(super::state::SignatureHelpRequestContext {
                                pane,
                                path: path.clone(),
                                version,
                            });
                        effects.push(Effect::LspSignatureHelpRequest { path, line, column });
                        state_changed = true;
                    }
                }

                let had_signature_help = self.state.ui.signature_help.is_active();
                if had_signature_help
                    && !tab_with_adapter.is_some_and(|(t, adapter)| {
                        let runtime = completion_runtime_context(&self.state, t, adapter);
                        adapter
                            .interaction()
                            .signature_help_should_keep_open(&runtime)
                    })
                {
                    self.state.ui.signature_help = super::state::SignatureHelpPopupState::default();
                    state_changed = true;
                }

                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::ToggleSidebar => {
                self.state.ui.sidebar_visible = !self.state.ui.sidebar_visible;
                if !self.state.ui.sidebar_visible && self.state.ui.focus == FocusTarget::Explorer {
                    self.state.ui.focus = FocusTarget::Editor;
                }
                state_changed = true;
            }
            Command::FocusExplorer => {
                self.state.ui.focus = FocusTarget::Explorer;
                self.state.ui.sidebar_visible = true;
                state_changed = true;
            }
            Command::FocusSearch => {
                // 全局搜索改为居中浮层（telescope 风格）。
                state_changed = self.open_overlay(OverlayKind::Search);
            }
            Command::FocusEditor => {
                self.state.ui.focus = FocusTarget::Editor;
                state_changed = true;
            }
            Command::OpenDiagnostics => {
                state_changed = self.open_overlay(OverlayKind::Problems);
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::CloseOverlay => {
                state_changed = self.close_overlay();
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::OpenCommandLine => {
                self.state.ui.command_line.reset();
                self.state.ui.command_line.active = true;
                self.state.ui.focus = FocusTarget::CommandLine;
                state_changed = true;
            }
            Command::CommandLineClose => {
                if self.state.ui.command_line.active {
                    self.state.ui.command_line.reset();
                    if self.state.ui.focus == FocusTarget::CommandLine {
                        self.state.ui.focus = FocusTarget::Editor;
                    }
                    state_changed = true;
                }
            }
            Command::CommandLineBackspace => {
                if self.state.ui.command_line.active {
                    let line = &mut self.state.ui.command_line;
                    if line.cursor > 0 && !line.input.is_empty() {
                        let prev = line.input[..line.cursor]
                            .char_indices()
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        line.input.drain(prev..line.cursor);
                        line.cursor = prev;
                        line.selected = 0;
                        state_changed = true;
                    } else {
                        // 空行退格关闭命令行（vim 习惯）。
                        self.state.ui.command_line.reset();
                        if self.state.ui.focus == FocusTarget::CommandLine {
                            self.state.ui.focus = FocusTarget::Editor;
                        }
                        state_changed = true;
                    }
                }
            }
            Command::CommandLineMoveUp => {
                if self.state.ui.command_line.active {
                    let prev = self.state.ui.command_line.selected;
                    self.state.ui.command_line.selected = prev.saturating_sub(1);
                    state_changed = self.state.ui.command_line.selected != prev;
                }
            }
            Command::CommandLineMoveDown => {
                if self.state.ui.command_line.active {
                    let prev = self.state.ui.command_line.selected;
                    self.state.ui.command_line.selected = prev.saturating_add(1);
                    state_changed = self.state.ui.command_line.selected != prev;
                }
            }
            Command::CommandLineConfirm => {
                if !self.state.ui.command_line.active {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let input = self.state.ui.command_line.input.trim().to_string();
                let selected_raw = self.state.ui.command_line.selected;
                let matches = crate::kernel::palette::match_items(&input);

                self.state.ui.command_line.reset();
                if self.state.ui.focus == FocusTarget::CommandLine {
                    self.state.ui.focus = FocusTarget::Editor;
                }

                // 优先取补全列表的选中项；否则按命令名解析输入。
                let cmd = if !matches.is_empty() {
                    let selected = selected_raw.min(matches.len().saturating_sub(1));
                    Some(matches[selected].command.clone())
                } else if !input.is_empty() {
                    Some(Command::from_name(&input))
                } else {
                    None
                };

                let Some(cmd) = cmd else {
                    return DispatchResult {
                        effects,
                        state_changed: true,
                    };
                };

                let mut result = self.dispatch_command(cmd);
                result.state_changed = true;
                return result;
            }
            Command::ExplorerUp => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.move_selection(-1);
                }
            }
            Command::ExplorerDown => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.move_selection(1);
                }
            }
            Command::ExplorerActivate => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    let (changed, fx) = self.state.explorer.activate_selected();
                    return DispatchResult {
                        effects: fx,
                        state_changed: changed,
                    };
                }
            }
            Command::ExplorerCollapse => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.collapse_selected();
                }
            }
            Command::ExplorerScrollUp => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.scroll(-3);
                }
            }
            Command::ExplorerScrollDown => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.scroll(3);
                }
            }
            Command::ExplorerNewFile => {
                if self.state.ui.input_dialog.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let parent_dir = self.state.explorer.selected_create_parent_dir();
                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "New File".to_string();
                self.state.ui.input_dialog.kind = Some(InputDialogKind::NewFile { parent_dir });
                state_changed = true;
            }
            Command::ExplorerNewFolder => {
                if self.state.ui.input_dialog.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let parent_dir = self.state.explorer.selected_create_parent_dir();
                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "New Folder".to_string();
                self.state.ui.input_dialog.kind = Some(InputDialogKind::NewFolder { parent_dir });
                state_changed = true;
            }
            Command::ExplorerRename => {
                if self.state.ui.input_dialog.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let Some((path, _is_dir)) = self.state.explorer.selected_path_and_kind() else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };
                let root = self.state.workspace_root.as_path();
                if path.as_path() == root || !path.starts_with(root) {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let file_name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if file_name.is_empty() {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Rename".to_string();
                self.state.ui.input_dialog.value = file_name;
                self.state.ui.input_dialog.cursor = self.state.ui.input_dialog.value.len();
                self.state.ui.input_dialog.kind =
                    Some(InputDialogKind::ExplorerRename { from: path });
                state_changed = true;
            }
            Command::ExplorerDelete => {
                if self.state.ui.confirm_dialog.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let Some((path, is_dir)) = self.state.explorer.selected_path_and_kind() else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };
                let root = self.state.workspace_root.as_path();
                if path.as_path() == root || !path.starts_with(root) {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let rel = path
                    .strip_prefix(&self.state.workspace_root)
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                let message = if is_dir {
                    format!("Delete folder \"{}\" and all contents?", rel)
                } else {
                    format!("Delete file \"{}\"?", rel)
                };

                self.state.ui.confirm_dialog.visible = true;
                self.state.ui.confirm_dialog.message = message;
                self.state.ui.confirm_dialog.on_confirm =
                    Some(super::PendingAction::DeletePath { path, is_dir });
                state_changed = true;
            }
            Command::ExplorerCut => {
                state_changed = self.set_explorer_clipboard_from_selection(
                    super::state::ExplorerClipboardMode::Cut,
                );
            }
            Command::ExplorerCopy => {
                state_changed = self.set_explorer_clipboard_from_selection(
                    super::state::ExplorerClipboardMode::Copy,
                );
            }
            Command::ExplorerPaste => {
                let Some(effect) = self.explorer_paste_effect() else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };
                return DispatchResult {
                    effects: vec![effect],
                    state_changed: false,
                };
            }
            Command::GlobalSearchStart => {
                let search_focused = self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Search);
                if search_focused && !self.state.search.query.is_empty() {
                    let root = self.state.workspace_root.clone();
                    let pattern = self.state.search.query.clone();
                    let case_sensitive = self.state.search.case_sensitive;
                    let use_regex = self.state.search.use_regex;
                    let changed = self.state.search.begin_search();
                    return DispatchResult {
                        effects: vec![Effect::StartGlobalSearch {
                            root,
                            pattern,
                            case_sensitive,
                            use_regex,
                        }],
                        state_changed: changed,
                    };
                }
            }
            Command::SearchResultsMoveUp => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.move_selection(-1, viewport);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Problems)
                {
                    state_changed = self.state.problems.move_selection(-1);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::CodeActions)
                {
                    state_changed = self.state.code_actions.move_selection(-1);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Locations)
                {
                    state_changed = self.state.locations.move_selection(-1);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Symbols)
                {
                    state_changed = self.state.symbols.move_selection(-1);
                }
            }
            Command::SearchResultsMoveDown => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.move_selection(1, viewport);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Problems)
                {
                    state_changed = self.state.problems.move_selection(1);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::CodeActions)
                {
                    state_changed = self.state.code_actions.move_selection(1);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Locations)
                {
                    state_changed = self.state.locations.move_selection(1);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Symbols)
                {
                    state_changed = self.state.symbols.move_selection(1);
                }
            }
            Command::SearchResultsScrollUp => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.scroll(-3, viewport);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Problems)
                {
                    state_changed = self.state.problems.scroll(-3);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::CodeActions)
                {
                    state_changed = self.state.code_actions.scroll(-3);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Locations)
                {
                    state_changed = self.state.locations.scroll(-3);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Symbols)
                {
                    state_changed = self.state.symbols.scroll(-3);
                }
            }
            Command::SearchResultsScrollDown => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.scroll(3, viewport);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Problems)
                {
                    state_changed = self.state.problems.scroll(3);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::CodeActions)
                {
                    state_changed = self.state.code_actions.scroll(3);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Locations)
                {
                    state_changed = self.state.locations.scroll(3);
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Symbols)
                {
                    state_changed = self.state.symbols.scroll(3);
                }
            }
            Command::SearchResultsToggleExpand => {
                if search_viewport_for_focus(&self.state.ui).is_some() {
                    state_changed = self.state.search.toggle_selected_file_expanded();
                }
            }
            Command::SearchResultsOpenSelected => {
                if search_viewport_for_focus(&self.state.ui).is_some() {
                    let prev_focus = self.state.ui.focus;
                    let prev_active_pane = self.state.ui.editor_layout.active_pane;

                    let Some(item) = self
                        .state
                        .search
                        .items
                        .get(self.state.search.selected_index)
                        .copied()
                    else {
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    };

                    let Some((path, byte_offset)) = search_open_target(&self.state.search, item)
                    else {
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    };

                    let preferred_pane = self.state.ui.editor_layout.active_pane;
                    if let Some((pane, tab_index)) =
                        find_open_tab(&self.state.editor, preferred_pane, &path)
                    {
                        self.state.ui.editor_layout.active_pane = pane;
                        self.state.ui.focus = FocusTarget::Editor;

                        let (changed1, mut eff1) =
                            self.state
                                .editor
                                .dispatch_action(EditorAction::SetActiveTab {
                                    pane,
                                    index: tab_index,
                                });
                        let (changed2, eff2) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::GotoByteOffset { pane, byte_offset });
                        eff1.extend(eff2);

                        let ui_changed = prev_focus != FocusTarget::Editor
                            || prev_active_pane != self.state.ui.editor_layout.active_pane;
                        let state_changed = ui_changed || changed1 || changed2;

                        return DispatchResult {
                            effects: eff1,
                            state_changed,
                        };
                    }

                    let pane = preferred_pane;
                    self.state.ui.editor_layout.active_pane = pane;
                    self.state.ui.focus = FocusTarget::Editor;
                    self.state.ui.pending_editor_nav =
                        Some(super::state::PendingEditorNavigation {
                            pane,
                            path: path.clone(),
                            target: super::state::PendingEditorNavigationTarget::ByteOffset {
                                byte_offset,
                            },
                        });

                    return DispatchResult {
                        effects: vec![Effect::LoadFile(path)],
                        state_changed: true,
                    };
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Problems)
                {
                    let prev_focus = self.state.ui.focus;
                    let prev_active_pane = self.state.ui.editor_layout.active_pane;

                    let Some(item) = self
                        .state
                        .problems
                        .items()
                        .get(self.state.problems.selected_index())
                        .cloned()
                    else {
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    };

                    let path = item.path.clone();
                    let range = item.range;
                    let preferred_pane = self.state.ui.editor_layout.active_pane;

                    if let Some((pane, tab_index)) =
                        find_open_tab(&self.state.editor, preferred_pane, &path)
                    {
                        self.state.ui.editor_layout.active_pane = pane;
                        self.state.ui.focus = FocusTarget::Editor;

                        let (changed1, mut eff1) =
                            self.state
                                .editor
                                .dispatch_action(EditorAction::SetActiveTab {
                                    pane,
                                    index: tab_index,
                                });

                        let encoding = lsp_position_encoding_for_path(&self.state, &path);
                        let byte_offset = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                            .map(|tab| problem_byte_offset(tab, range, encoding))
                            .unwrap_or(0);

                        let (changed2, eff2) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::GotoByteOffset { pane, byte_offset });
                        eff1.extend(eff2);

                        let ui_changed = prev_focus != FocusTarget::Editor
                            || prev_active_pane != self.state.ui.editor_layout.active_pane;
                        let state_changed = ui_changed || changed1 || changed2;

                        return DispatchResult {
                            effects: eff1,
                            state_changed,
                        };
                    }

                    let pane = preferred_pane;
                    self.state.ui.editor_layout.active_pane = pane;
                    self.state.ui.focus = FocusTarget::Editor;
                    self.state.ui.pending_editor_nav =
                        Some(super::state::PendingEditorNavigation {
                            pane,
                            path: path.clone(),
                            target: super::state::PendingEditorNavigationTarget::LineColumn {
                                line: range.start_line,
                                column: range.start_col,
                            },
                        });

                    return DispatchResult {
                        effects: vec![Effect::LoadFile(path)],
                        state_changed: true,
                    };
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::CodeActions)
                {
                    let Some(action) = self.state.code_actions.selected().cloned() else {
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    };

                    let mut effects = effects;
                    let mut changed = self.state.code_actions.clear();

                    if let Some(edit) = action.edit {
                        changed |= self.apply_workspace_edit(edit, &mut effects);
                    }

                    if let Some(command) = action.command {
                        effects.push(Effect::LspExecuteCommand {
                            command: command.command,
                            arguments: command.arguments,
                        });
                        changed = true;
                    }

                    let prev_focus = self.state.ui.focus;
                    self.state.ui.focus = FocusTarget::Editor;
                    if prev_focus != FocusTarget::Editor {
                        changed = true;
                    }

                    return DispatchResult {
                        effects,
                        state_changed: state_changed || changed,
                    };
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Locations)
                {
                    let prev_focus = self.state.ui.focus;
                    let prev_active_pane = self.state.ui.editor_layout.active_pane;

                    let Some(item) = self
                        .state
                        .locations
                        .items()
                        .get(self.state.locations.selected_index())
                        .cloned()
                    else {
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    };

                    let path = item.path.clone();
                    let preferred_pane = self.state.ui.editor_layout.active_pane;

                    if let Some((pane, tab_index)) =
                        find_open_tab(&self.state.editor, preferred_pane, &path)
                    {
                        self.state.ui.editor_layout.active_pane = pane;
                        self.state.ui.focus = FocusTarget::Editor;

                        let (changed1, mut eff1) =
                            self.state
                                .editor
                                .dispatch_action(EditorAction::SetActiveTab {
                                    pane,
                                    index: tab_index,
                                });

                        let encoding = lsp_position_encoding_for_path(&self.state, &path);
                        let byte_offset = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                            .map(|tab| {
                                lsp_position_to_byte_offset(tab, item.line, item.column, encoding)
                            })
                            .unwrap_or(0);

                        let (changed2, eff2) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::GotoByteOffset { pane, byte_offset });
                        eff1.extend(eff2);

                        let ui_changed = prev_focus != FocusTarget::Editor
                            || prev_active_pane != self.state.ui.editor_layout.active_pane;
                        let state_changed = ui_changed || changed1 || changed2;

                        return DispatchResult {
                            effects: eff1,
                            state_changed,
                        };
                    }

                    let pane = preferred_pane;
                    self.state.ui.editor_layout.active_pane = pane;
                    self.state.ui.focus = FocusTarget::Editor;
                    self.state.ui.pending_editor_nav =
                        Some(super::state::PendingEditorNavigation {
                            pane,
                            path: path.clone(),
                            target: super::state::PendingEditorNavigationTarget::LineColumn {
                                line: item.line,
                                column: item.column,
                            },
                        });

                    return DispatchResult {
                        effects: vec![Effect::LoadFile(path)],
                        state_changed: true,
                    };
                } else if self.state.ui.focus == FocusTarget::Overlay
                    && self.state.ui.overlay.active == Some(OverlayKind::Symbols)
                {
                    let prev_focus = self.state.ui.focus;
                    let prev_active_pane = self.state.ui.editor_layout.active_pane;

                    let Some(item) = self.state.symbols.selected().cloned() else {
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    };

                    let path = item.path.clone();
                    let preferred_pane = self.state.ui.editor_layout.active_pane;

                    if let Some((pane, tab_index)) =
                        find_open_tab(&self.state.editor, preferred_pane, &path)
                    {
                        self.state.ui.editor_layout.active_pane = pane;
                        self.state.ui.focus = FocusTarget::Editor;

                        let (changed1, mut eff1) =
                            self.state
                                .editor
                                .dispatch_action(EditorAction::SetActiveTab {
                                    pane,
                                    index: tab_index,
                                });

                        let encoding = lsp_position_encoding_for_path(&self.state, &path);
                        let byte_offset = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                            .map(|tab| {
                                lsp_position_to_byte_offset(tab, item.line, item.column, encoding)
                            })
                            .unwrap_or(0);

                        let (changed2, eff2) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::GotoByteOffset { pane, byte_offset });
                        eff1.extend(eff2);

                        let ui_changed = prev_focus != FocusTarget::Editor
                            || prev_active_pane != self.state.ui.editor_layout.active_pane;
                        let state_changed = ui_changed || changed1 || changed2;

                        return DispatchResult {
                            effects: eff1,
                            state_changed,
                        };
                    }

                    let pane = preferred_pane;
                    self.state.ui.editor_layout.active_pane = pane;
                    self.state.ui.focus = FocusTarget::Editor;
                    self.state.ui.pending_editor_nav =
                        Some(super::state::PendingEditorNavigation {
                            pane,
                            path: path.clone(),
                            target: super::state::PendingEditorNavigationTarget::LineColumn {
                                line: item.line,
                                column: item.column,
                            },
                        });

                    return DispatchResult {
                        effects: vec![Effect::LoadFile(path)],
                        state_changed: true,
                    };
                }
            }
            Command::LspHover => {
                let pane = self.state.ui.editor_layout.active_pane;
                let Some(tab) = self
                    .state
                    .editor
                    .pane(pane)
                    .and_then(|pane| pane.active_tab())
                else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                let Some(path) = tab.path.as_ref() else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                if !is_lsp_source_path(path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
                let supports_hover =
                    lsp_server_capabilities_for_path(&self.state, path).is_none_or(|c| c.hover);
                if !supports_hover {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                let cursor = tab.buffer.cursor();
                let pos = tab.identifier_pos_at_or_before(cursor).unwrap_or(cursor);
                let char_offset = tab.buffer.pos_to_char(pos);
                if tab.is_in_string_or_comment_at_char(char_offset) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let encoding = lsp_position_encoding_for_path(&self.state, path);
                let (line, column) = lsp_position_from_buffer_pos(tab, pos, encoding);
                return DispatchResult {
                    effects: vec![Effect::LspHoverRequest {
                        path: path.clone(),
                        line,
                        column,
                    }],
                    state_changed,
                };
            }
            Command::LspDefinition => {
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    let supports_definition = lsp_server_capabilities_for_path(&self.state, &path)
                        .is_none_or(|c| c.definition);
                    if !supports_definition {
                        return DispatchResult {
                            effects,
                            state_changed: false,
                        };
                    }
                    return DispatchResult {
                        effects: vec![Effect::LspDefinitionRequest { path, line, column }],
                        state_changed,
                    };
                }
            }
            Command::LspCompletion => {
                if let Some((pane, path, line, column, version)) = lsp_request_target(&self.state) {
                    let supports_completion = lsp_server_capabilities_for_path(&self.state, &path)
                        .is_none_or(|c| c.completion);
                    if !supports_completion {
                        return DispatchResult {
                            effects,
                            state_changed: false,
                        };
                    }

                    self.state.ui.hover.clear();
                    let Some(tab) = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                    else {
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    };

                    let can_reuse = self
                        .state
                        .ui
                        .completion
                        .request
                        .as_ref()
                        .is_some_and(|session| session.pane == pane && session.path == path)
                        && self.state.ui.completion.pending_request.is_none()
                        && !self.state.ui.completion.all_items.is_empty()
                        && !self.state.ui.completion.is_incomplete
                        && self
                            .state
                            .ui
                            .completion
                            .session_started_at
                            .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(2));

                    if can_reuse {
                        let adapter = adapter_for_tab(tab);
                        let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                            tab.language(),
                            tab,
                            adapter.syntax().syntax_facts(tab),
                        );
                        let mut changed = sync_completion_items_from_cache(
                            &mut self.state.ui.completion,
                            &runtime,
                            adapter.interaction(),
                        );
                        let mut effects = Vec::new();

                        if let Some(record) = self.state.ui.completion.selected_record().cloned() {
                            if matches!(
                                record.entry.resolve_state,
                                CompletionResolveState::Unresolved
                            ) && record
                                .entry
                                .documentation
                                .as_ref()
                                .is_none_or(|d| d.trim().is_empty())
                                && self.state.ui.completion.resolve_inflight
                                    != Some(record.entry.id)
                            {
                                self.state.ui.completion.resolve_inflight = Some(record.entry.id);
                                let _ = self.state.ui.completion.set_resolve_state(
                                    record.entry.id,
                                    CompletionResolveState::Resolving,
                                );
                                effects.push(Effect::LspCompletionResolveRequest {
                                    item: Box::new(record.raw),
                                });
                                changed = true;
                            }
                        }

                        return DispatchResult {
                            effects,
                            state_changed: changed,
                        };
                    }

                    let keep_open = self.state.ui.completion.visible
                        && self.state.ui.completion.visible_len() > 0;
                    if !keep_open {
                        self.state.ui.completion.close();
                    }
                    self.state.ui.completion.pending_request =
                        Some(self.completion_request_context(pane, path.clone(), version));

                    return DispatchResult {
                        effects: vec![Effect::LspCompletionRequest {
                            path,
                            line,
                            column,
                            trigger: LspCompletionTriggerContext::invoked(),
                        }],
                        state_changed: true,
                    };
                }
            }
            Command::LspSignatureHelp => {
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    let supports_signature_help =
                        lsp_server_capabilities_for_path(&self.state, &path)
                            .is_none_or(|c| c.signature_help);
                    if !supports_signature_help {
                        return DispatchResult {
                            effects,
                            state_changed: false,
                        };
                    }
                    return DispatchResult {
                        effects: vec![Effect::LspSignatureHelpRequest { path, line, column }],
                        state_changed,
                    };
                }
            }
            Command::LspFormat => {
                if let Some((_pane, path, _line, _column, _version)) =
                    lsp_request_target(&self.state)
                {
                    let supports_format = lsp_server_capabilities_for_path(&self.state, &path)
                        .is_none_or(|c| c.format);
                    if !supports_format {
                        return DispatchResult {
                            effects,
                            state_changed: false,
                        };
                    }
                    return DispatchResult {
                        effects: vec![Effect::LspFormatRequest { path }],
                        state_changed,
                    };
                }
            }
            Command::LspFormatSelection => {
                let pane = self.state.ui.editor_layout.active_pane;
                let Some(tab) = self.state.editor.pane(pane).and_then(|p| p.active_tab()) else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                if !is_lsp_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let selection = tab.buffer.selection().filter(|sel| !sel.is_empty());
                if let Some(selection) = selection {
                    let encoding = lsp_position_encoding_for_path(&self.state, &path);
                    let (start_pos, end_pos) = selection.range();
                    let (start_line, start_col) =
                        lsp_position_from_buffer_pos(tab, start_pos, encoding);
                    let (end_line, end_col) = lsp_position_from_buffer_pos(tab, end_pos, encoding);
                    let range = LspRange {
                        start: LspPosition {
                            line: start_line,
                            character: start_col,
                        },
                        end: LspPosition {
                            line: end_line,
                            character: end_col,
                        },
                    };

                    let supports_range_format =
                        lsp_server_capabilities_for_path(&self.state, &path)
                            .is_none_or(|c| c.range_format);
                    if supports_range_format {
                        return DispatchResult {
                            effects: vec![Effect::LspRangeFormatRequest { path, range }],
                            state_changed,
                        };
                    }

                    let supports_format = lsp_server_capabilities_for_path(&self.state, &path)
                        .is_none_or(|c| c.format);
                    if supports_format {
                        return DispatchResult {
                            effects: vec![Effect::LspFormatRequest { path }],
                            state_changed,
                        };
                    }

                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                if !lsp_server_capabilities_for_path(&self.state, &path).is_none_or(|c| c.format) {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                return DispatchResult {
                    effects: vec![Effect::LspFormatRequest { path }],
                    state_changed,
                };
            }
            Command::LspRename => {
                if self.state.ui.input_dialog.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };

                let supports_rename =
                    lsp_server_capabilities_for_path(&self.state, &path).is_none_or(|c| c.rename);
                if !supports_rename {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Rename Symbol".to_string();
                self.state.ui.input_dialog.kind =
                    Some(InputDialogKind::LspRename { path, line, column });
                state_changed = true;
            }
            Command::LspReferences => {
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    let supports_references = lsp_server_capabilities_for_path(&self.state, &path)
                        .is_none_or(|c| c.references);
                    if !supports_references {
                        return DispatchResult {
                            effects,
                            state_changed: false,
                        };
                    }

                    let mut changed = self.state.locations.clear();
                    changed |= self.open_overlay(OverlayKind::Locations);

                    return DispatchResult {
                        effects: vec![Effect::LspReferencesRequest { path, line, column }],
                        state_changed: state_changed || changed,
                    };
                }
            }
            Command::LspDocumentSymbols => {
                let pane = self.state.ui.editor_layout.active_pane;
                let Some(tab) = self.state.editor.pane(pane).and_then(|p| p.active_tab()) else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                if !is_lsp_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
                let supports_symbols = lsp_server_capabilities_for_path(&self.state, &path)
                    .is_none_or(|c| c.document_symbols);
                if !supports_symbols {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let mut changed = self.state.symbols.clear();
                changed |= self.open_overlay(OverlayKind::Symbols);

                return DispatchResult {
                    effects: vec![Effect::LspDocumentSymbolsRequest { path }],
                    state_changed: state_changed || changed,
                };
            }
            Command::LspWorkspaceSymbols => {
                let supports_workspace_symbols = self.state.lsp.server_capabilities.is_empty()
                    || self
                        .state
                        .lsp
                        .server_capabilities
                        .values()
                        .any(|c| c.workspace_symbols);
                if !supports_workspace_symbols {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                if self.state.ui.input_dialog.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Workspace Symbols".to_string();
                self.state.ui.input_dialog.kind = Some(InputDialogKind::LspWorkspaceSymbols);
                state_changed = true;
            }
            Command::LspSemanticTokens => {
                let pane = self.state.ui.editor_layout.active_pane;
                let Some(tab) = self.state.editor.pane(pane).and_then(|p| p.active_tab()) else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                if !is_lsp_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let version = tab.edit_version;
                let caps = lsp_server_capabilities_for_path(&self.state, &path);
                let supports_semantic_tokens = caps.is_some_and(|c| {
                    c.semantic_tokens && (c.semantic_tokens_full || c.semantic_tokens_range)
                });
                if !supports_semantic_tokens {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let encoding = lsp_position_encoding_for_path(&self.state, &path);

                let Some(caps) = caps else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };

                let total_lines = tab.buffer.len_lines().max(1);
                let can_range = caps.semantic_tokens_range;
                let can_full = caps.semantic_tokens_full;
                let prefer_range = can_range && (total_lines >= 2000 || !can_full);

                if prefer_range {
                    let viewport_top = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                    let height = tab.viewport.height.max(1);
                    let overscan = 40usize.min(total_lines);
                    let start_line = viewport_top.saturating_sub(overscan);
                    let end_line_exclusive = (viewport_top + height + overscan).min(total_lines);
                    if let Some(range) =
                        lsp_range_for_full_lines(tab, start_line, end_line_exclusive, encoding)
                    {
                        return DispatchResult {
                            effects: vec![Effect::LspSemanticTokensRangeRequest {
                                path,
                                version,
                                range,
                            }],
                            state_changed,
                        };
                    }
                }

                if can_full {
                    return DispatchResult {
                        effects: vec![Effect::LspSemanticTokensRequest { path, version }],
                        state_changed,
                    };
                }

                return DispatchResult {
                    effects: Vec::new(),
                    state_changed: false,
                };
            }
            Command::LspInlayHints => {
                let pane = self.state.ui.editor_layout.active_pane;
                let Some(tab) = self.state.editor.pane(pane).and_then(|p| p.active_tab()) else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                if !is_lsp_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
                let supports_inlay_hints = lsp_server_capabilities_for_path(&self.state, &path)
                    .is_some_and(|c| c.inlay_hints);
                if !supports_inlay_hints {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let total_lines = tab.buffer.len_lines().max(1);
                let start_line = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                let end_line_exclusive = (start_line + tab.viewport.height.max(1)).min(total_lines);
                let encoding = lsp_position_encoding_for_path(&self.state, &path);
                let Some(range) =
                    lsp_range_for_full_lines(tab, start_line, end_line_exclusive, encoding)
                else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };

                return DispatchResult {
                    effects: vec![Effect::LspInlayHintsRequest {
                        path,
                        version: tab.edit_version,
                        range,
                    }],
                    state_changed,
                };
            }
            Command::LspFoldingRange => {
                let pane = self.state.ui.editor_layout.active_pane;
                let Some(tab) = self.state.editor.pane(pane).and_then(|p| p.active_tab()) else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                if !is_lsp_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
                let supports_folding_range = lsp_server_capabilities_for_path(&self.state, &path)
                    .is_some_and(|c| c.folding_range);
                if !supports_folding_range {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                return DispatchResult {
                    effects: vec![Effect::LspFoldingRangeRequest {
                        path,
                        version: tab.edit_version,
                    }],
                    state_changed,
                };
            }
            cmd @ (Command::EditorFoldToggle | Command::EditorFold | Command::EditorUnfold) => {
                let pane = self.state.ui.editor_layout.active_pane;
                let Some((path, version, needs_request)) = self
                    .state
                    .editor
                    .pane(pane)
                    .and_then(|pane_state| pane_state.active_tab())
                    .and_then(|tab| {
                        let path = tab.path.as_ref().cloned()?;
                        let version = tab.edit_version;
                        let folding_version = tab.folding_version().unwrap_or(0);
                        let needs_request = !tab.has_folding_ranges() || folding_version < version;
                        Some((path, version, needs_request))
                    })
                else {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                };
                if !is_lsp_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
                let supports_folding_range = lsp_server_capabilities_for_path(&self.state, &path)
                    .is_some_and(|c| c.folding_range);
                if !supports_folding_range {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let (changed, cmd_effects) = self.state.editor.apply_command(pane, cmd);
                let mut effects = effects;
                effects.extend(cmd_effects);
                if needs_request {
                    effects.push(Effect::LspFoldingRangeRequest { path, version });
                }

                return DispatchResult {
                    effects,
                    state_changed: state_changed || changed,
                };
            }
            Command::LspCodeAction => {
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    let supports_code_action = lsp_server_capabilities_for_path(&self.state, &path)
                        .is_none_or(|c| c.code_action);
                    if !supports_code_action {
                        return DispatchResult {
                            effects,
                            state_changed: false,
                        };
                    }

                    let mut changed = self.state.code_actions.clear();
                    changed |= self.open_overlay(OverlayKind::CodeActions);

                    return DispatchResult {
                        effects: vec![Effect::LspCodeActionRequest { path, line, column }],
                        state_changed: state_changed || changed,
                    };
                }
            }
            Command::Save => {
                let pane = self.state.ui.editor_layout.active_pane;
                let (changed, cmd_effects) = self.state.editor.apply_command(pane, Command::Save);
                if changed {
                    state_changed = true;
                }

                let mut effects = effects;
                effects.extend(cmd_effects);

                if self.state.editor.config.format_on_save {
                    let path = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|pane_state| pane_state.active_tab())
                        .and_then(|tab| tab.path.clone());
                    if let Some(path) = path {
                        if is_lsp_source_path(&path)
                            && lsp_server_capabilities_for_path(&self.state, &path)
                                .is_some_and(|c| c.format)
                        {
                            self.state.lsp.pending_format_on_save = Some(path.clone());
                            effects.push(Effect::LspFormatRequest { path });
                        }
                    }
                }

                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::OpenFile => {
                // UI should translate selection -> path and dispatch Action::OpenPath.
            }
            Command::Custom(name) => {
                let pane = self.state.ui.editor_layout.active_pane;
                let (changed, cmd_effects) =
                    self.state.editor.apply_command(pane, Command::Custom(name));
                if changed {
                    state_changed = true;
                }
                let mut effects = effects;
                effects.extend(cmd_effects);
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            cmd @ (Command::DeleteBackward | Command::DeleteForward | Command::DeleteSelection) => {
                let pane = self.state.ui.editor_layout.active_pane;
                let (changed, cmd_effects) = self.state.editor.apply_command(pane, cmd);
                if changed {
                    state_changed = true;
                }

                let mut effects = effects;
                effects.extend(cmd_effects);

                if let Some(tab) = self.state.editor.pane(pane).and_then(|p| p.active_tab()) {
                    let session = self.state.ui.completion.request.as_ref().or(self
                        .state
                        .ui
                        .completion
                        .pending_request
                        .as_ref());

                    let session_ok = session.is_some_and(|session| {
                        session.pane == pane && tab.path.as_ref() == Some(&session.path)
                    });
                    let adapter = adapter_for_tab(tab);
                    let keep_completion_open = {
                        let runtime = completion_runtime_context(&self.state, tab, adapter);
                        adapter.interaction().completion_should_keep_open(&runtime)
                    };
                    let keep_signature_help_open = {
                        let runtime = completion_runtime_context(&self.state, tab, adapter);
                        adapter
                            .interaction()
                            .signature_help_should_keep_open(&runtime)
                    };

                    if session_ok && !keep_completion_open {
                        if self.state.ui.completion.close() {
                            state_changed = true;
                        }

                        let had_signature_help = self.state.ui.signature_help.is_active();
                        if had_signature_help && !keep_signature_help_open {
                            self.state.ui.signature_help =
                                super::state::SignatureHelpPopupState::default();
                            state_changed = true;
                        }
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    }

                    if session_ok && !self.state.ui.completion.all_items.is_empty() {
                        let adapter = adapter_for_tab(tab);
                        let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                            tab.language(),
                            tab,
                            adapter.syntax().syntax_facts(tab),
                        );
                        let mut changed = sync_completion_items_from_cache(
                            &mut self.state.ui.completion,
                            &runtime,
                            adapter.interaction(),
                        );

                        if let Some(record) = self.state.ui.completion.selected_record().cloned() {
                            if matches!(
                                record.entry.resolve_state,
                                CompletionResolveState::Unresolved
                            ) && record
                                .entry
                                .documentation
                                .as_ref()
                                .is_none_or(|d| d.trim().is_empty())
                                && self.state.ui.completion.resolve_inflight
                                    != Some(record.entry.id)
                            {
                                self.state.ui.completion.resolve_inflight = Some(record.entry.id);
                                let _ = self.state.ui.completion.set_resolve_state(
                                    record.entry.id,
                                    CompletionResolveState::Resolving,
                                );
                                effects.push(Effect::LspCompletionResolveRequest {
                                    item: Box::new(record.raw),
                                });
                                changed = true;
                            }
                        }

                        if changed {
                            state_changed = true;
                        }
                    }
                }

                let had_signature_help = self.state.ui.signature_help.is_active();
                if had_signature_help {
                    let keep = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|p| p.active_tab())
                        .is_some_and(|t| {
                            let adapter = adapter_for_tab(t);
                            let runtime = completion_runtime_context(&self.state, t, adapter);
                            adapter
                                .interaction()
                                .signature_help_should_keep_open(&runtime)
                        });
                    if !keep {
                        self.state.ui.signature_help =
                            super::state::SignatureHelpPopupState::default();
                        state_changed = true;
                    }
                }

                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            other => {
                let pane = self.state.ui.editor_layout.active_pane;
                let should_flush_newline = matches!(other, Command::InsertNewline);
                let should_flush_tab = matches!(other, Command::InsertTab);
                let should_flush_cursor_move = other.is_cursor_command();
                let should_defer_semantic_after_edit = matches!(
                    other,
                    Command::DeleteBackward
                        | Command::DeleteForward
                        | Command::DeleteLine
                        | Command::DeleteToLineEnd
                        | Command::DeleteSelection
                        | Command::Undo
                        | Command::Redo
                        | Command::Paste
                        | Command::Cut
                );
                let (changed, cmd_effects) = self.state.editor.apply_command(pane, other);
                if changed {
                    state_changed = true;
                }
                // TODO: avoid allocation by using SmallVec if needed.
                let mut effects = effects;
                effects.extend(cmd_effects);

                if should_flush_newline || should_flush_tab {
                    let boundary_chars = self
                        .state
                        .editor
                        .config
                        .lsp_input_timing
                        .boundary_chars
                        .as_str();
                    let should_flush = (should_flush_newline && boundary_chars.contains('\n'))
                        || (should_flush_tab && boundary_chars.contains('\t'));
                    if should_flush {
                        if let Some(path) = self.active_editor_file_path() {
                            state_changed |= self.flush_pending_semantic_highlights_for_path(&path);
                        }
                    } else if changed {
                        if let Some((path, version)) = self.active_editor_lsp_path_and_version() {
                            self.arm_semantic_flush_defer_for_path(path, version);
                        }
                    }
                }

                if should_flush_cursor_move {
                    if let Some(path) = self.active_editor_file_path() {
                        state_changed |= self.flush_pending_semantic_highlights_for_path(&path);
                    }
                }

                if should_defer_semantic_after_edit && changed {
                    if let Some((path, version)) = self.active_editor_lsp_path_and_version() {
                        self.arm_semantic_flush_defer_for_path(path, version);
                    }
                }

                let had_signature_help = self.state.ui.signature_help.is_active();
                if had_signature_help {
                    let pane = self.state.ui.editor_layout.active_pane;
                    let keep = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|p| p.active_tab())
                        .is_some_and(|t| {
                            let adapter = adapter_for_tab(t);
                            let runtime = completion_runtime_context(&self.state, t, adapter);
                            adapter
                                .interaction()
                                .signature_help_should_keep_open(&runtime)
                        });
                    if !keep {
                        self.state.ui.signature_help =
                            super::state::SignatureHelpPopupState::default();
                        state_changed = true;
                    }
                }
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
        }

        DispatchResult {
            effects,
            state_changed,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/store.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/kernel/store_lsp_perf.rs"]
mod tests_lsp_perf;
