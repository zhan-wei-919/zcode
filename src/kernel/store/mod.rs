use crate::core::Command;
use crate::kernel::editor::{EditorTabState, HighlightKind, PendingSemanticLine, ReloadCause};
use std::path::Path;
use unicode_xid::UnicodeXID;

#[cfg(test)]
use crate::kernel::services::ports::{LspCompletionItem, LspPositionEncoding};

pub(crate) mod intel;
mod util;

#[path = "reducers/completion.rs"]
mod completion;
#[path = "reducers/confirm_dialog.rs"]
mod confirm_dialog;
#[path = "reducers/context_menu.rs"]
mod context_menu;
#[path = "reducers/editor_command.rs"]
mod editor_command;
#[path = "reducers/explorer.rs"]
mod explorer;
#[path = "reducers/explorer_command.rs"]
mod explorer_command;
#[path = "reducers/input_dialog.rs"]
mod input_dialog;
#[path = "reducers/lsp_command.rs"]
mod lsp_command;
#[path = "reducers/search.rs"]
mod search;
#[path = "reducers/search_command.rs"]
mod search_command;

#[cfg(test)]
use intel::completion::{apply_completion_insertion_cursor, CompletionInsertion};
use intel::completion::{
    completion_runtime_context, language_runtime_context_with_syntax,
    should_close_completion_on_editor_action,
};
pub use intel::completion_rank::CompletionRanker;

#[cfg(test)]
use intel::lsp::lsp_range_for_full_lines;
use intel::lsp::{lsp_position_encoding, lsp_position_to_byte_offset};
use intel::semantic::reconcile_pending_semantic_row;
use util::is_lsp_source_path;

#[cfg(test)]
use super::InputDialogKind;
use super::{Action, AppState, EditorAction, Effect, FocusTarget, OverlayKind};
use crate::kernel::language::{
    adapter::adapter_for_tab, adapter::SyntaxFacts, adapter_for, CompletionRecord,
    CompletionResolveState,
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
        syntax: Option<SyntaxFacts>,
    ) -> super::state::CompletionRequestContext {
        let normalization = self
            .state
            .editor
            .pane(pane)
            .and_then(|pane_state| pane_state.active_tab())
            .map(|tab| {
                let adapter = adapter_for_tab(tab);
                // Reuse the caller's per-keystroke `SyntaxFacts` when provided;
                // otherwise descend the syntax tree once here.
                let syntax = syntax.unwrap_or_else(|| adapter.syntax().syntax_facts(tab));
                language_runtime_context_with_syntax(&self.state, tab, adapter, syntax)
                    .completion_snapshot()
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
                    // `should_close_on_command` reads only `syntax`/`tab`, never server
                    // caps, so build a lightweight context: this drops the per-keystroke
                    // server-capability lookup (a filesystem marker-root walk) that the
                    // full context would do here purely to be ignored.
                    let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                        tab.language(),
                        tab,
                        adapter.syntax().syntax_facts(tab),
                    );
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

                let mut result =
                    match editor_action {
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

                                let (changed, cmd_effects) = self.state.editor.dispatch_action(
                                    EditorAction::GotoByteOffset { pane, byte_offset },
                                );
                                state_changed |= changed;
                                effects.extend(cmd_effects);
                                self.state.ui.pending_editor_nav = None;
                            }

                            self.schedule_lsp_requests_for_opened_file(
                                pane,
                                &opened_path,
                                &mut effects,
                            );

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
            action @ Action::CompletionClose
            | action @ Action::CompletionMoveSelection { .. }
            | action @ Action::CompletionConfirm => self.reduce_completion_action(action),
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
            action @ Action::ShowConfirmDialog { .. }
            | action @ Action::ConfirmDialogAccept
            | action @ Action::ConfirmDialogCancel => self.reduce_confirm_dialog_action(action),
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
            cmd @ Command::ExplorerUp
            | cmd @ Command::ExplorerDown
            | cmd @ Command::ExplorerActivate
            | cmd @ Command::ExplorerCollapse
            | cmd @ Command::ExplorerScrollUp
            | cmd @ Command::ExplorerScrollDown
            | cmd @ Command::ExplorerNewFile
            | cmd @ Command::ExplorerNewFolder
            | cmd @ Command::ExplorerRename
            | cmd @ Command::ExplorerDelete
            | cmd @ Command::ExplorerCut
            | cmd @ Command::ExplorerCopy
            | cmd @ Command::ExplorerPaste => return self.reduce_explorer_command(cmd),
            cmd @ Command::GlobalSearchStart
            | cmd @ Command::SearchResultsMoveUp
            | cmd @ Command::SearchResultsMoveDown
            | cmd @ Command::SearchResultsScrollUp
            | cmd @ Command::SearchResultsScrollDown
            | cmd @ Command::SearchResultsToggleExpand
            | cmd @ Command::SearchResultsOpenSelected => return self.reduce_search_command(cmd),
            cmd @ Command::LspHover
            | cmd @ Command::LspDefinition
            | cmd @ Command::LspCompletion
            | cmd @ Command::LspSignatureHelp
            | cmd @ Command::LspFormat
            | cmd @ Command::LspFormatSelection
            | cmd @ Command::LspRename
            | cmd @ Command::LspReferences
            | cmd @ Command::LspDocumentSymbols
            | cmd @ Command::LspWorkspaceSymbols
            | cmd @ Command::LspSemanticTokens
            | cmd @ Command::LspInlayHints
            | cmd @ Command::LspFoldingRange
            | cmd @ Command::LspCodeAction => return self.reduce_lsp_command(cmd),
            cmd => return self.reduce_editor_command(cmd),
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
