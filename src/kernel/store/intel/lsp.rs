use crate::kernel::language::HoverSectionModel;
use crate::kernel::services::ports::lsp::{
    column_for_chars, line_len_chars, lsp_col_to_char_offset_in_line,
};
use crate::kernel::services::ports::{
    LspClientKey, LspHoverPayload, LspHoverPreviewPayload, LspPosition, LspPositionEncoding,
    LspRange, LspResourceOp, LspServerCapabilities, LspTextEdit, LspWorkspaceEdit,
    LspWorkspaceFileEdit,
};
use crate::kernel::state::{
    CompletionPopupState, PayloadStamp, RangePayloadStamp, SignatureHelpPopupState,
};
use crate::kernel::EditorAction;
use crate::kernel::{Action, Effect, FocusTarget, OverlayKind};
use rustc_hash::FxHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

use super::super::util::find_open_tab;
use super::super::util::{is_lsp_source_path, open_tabs_for_path, resolve_renamed_path};
use super::completion::{
    completion_runtime_context, filtered_completion_indices, language_runtime_context,
    normalize_completion_record, sort_completion_items,
};

// Test-only counter of `lsp_server_capabilities_for_path` calls (each resolves an
// `LspClientKey` via a filesystem marker-root walk). Thread-local — like the
// `SyntaxFacts` descent counter — so the parallel test runner's other tests can't
// pollute a measured per-keystroke delta. Compiles out entirely in non-test builds.
#[cfg(test)]
thread_local! {
    static LSP_CAPABILITY_LOOKUP_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn normalize_hover_preview_for_active_tab(
    state: &crate::kernel::AppState,
    payload: &LspHoverPreviewPayload,
) -> Option<HoverSectionModel> {
    if payload.title.trim().is_empty() && payload.blocks.is_empty() {
        return None;
    }

    let pane = state.ui.editor_layout.active_pane;
    let tab = state.editor.pane(pane).and_then(|pane| pane.active_tab())?;
    let adapter = crate::kernel::language::adapter::adapter_for_tab(tab);
    let runtime = language_runtime_context(state, tab, adapter);
    let body = adapter.hover_protocol().normalize_hover(
        &runtime,
        &LspHoverPayload {
            blocks: payload.blocks.clone(),
            range: None,
        },
    )?;

    Some(HoverSectionModel {
        title: payload.title.trim().to_string(),
        body,
    })
}

#[cfg(test)]
pub(in crate::kernel::store) fn reset_lsp_capability_lookup_perf_counter() {
    LSP_CAPABILITY_LOOKUP_CALLS.with(|count| count.set(0));
}

#[cfg(test)]
pub(in crate::kernel::store) fn lsp_capability_lookup_perf_counter() -> usize {
    LSP_CAPABILITY_LOOKUP_CALLS.with(|count| count.get())
}

pub(in crate::kernel::store) fn problem_byte_offset(
    tab: &crate::kernel::editor::EditorTabState,
    range: crate::kernel::panel::problems::ProblemRange,
    encoding: LspPositionEncoding,
) -> usize {
    lsp_position_to_byte_offset(tab, range.start_line, range.start_col, encoding)
}

pub(in crate::kernel::store) fn lsp_position_to_byte_offset(
    tab: &crate::kernel::editor::EditorTabState,
    line: u32,
    column: u32,
    encoding: LspPositionEncoding,
) -> usize {
    let rope = tab.buffer.rope();
    if rope.len_chars() == 0 {
        return 0;
    }

    let line_index = (line as usize).min(rope.len_lines().saturating_sub(1));
    let line_slice = rope.line(line_index);
    let col_chars = lsp_col_to_char_offset_in_line(line_slice, column, encoding);
    let line_start = rope.line_to_char(line_index);
    let line_len = line_len_chars(line_slice);
    let char_offset = (line_start + col_chars.min(line_len)).min(rope.len_chars());
    rope.char_to_byte(char_offset)
}

pub(in crate::kernel::store) fn lsp_request_target(
    state: &crate::kernel::AppState,
) -> Option<(usize, std::path::PathBuf, u32, u32, u64)> {
    let pane = state.ui.editor_layout.active_pane;
    let tab = state.editor.pane(pane)?.active_tab()?;
    let path = tab.path.as_ref()?.clone();
    if !is_lsp_source_path(&path) {
        return None;
    }
    let encoding = lsp_position_encoding_for_path(state, &path);
    let (line, column) = lsp_position_from_cursor(tab, encoding);
    Some((pane, path, line, column, tab.edit_version))
}

pub(in crate::kernel::store) fn lsp_server_capabilities_for_path<'a>(
    state: &'a crate::kernel::AppState,
    path: &Path,
) -> Option<&'a LspServerCapabilities> {
    #[cfg(test)]
    LSP_CAPABILITY_LOOKUP_CALLS.with(|count| count.set(count.get() + 1));

    let key = lsp_client_key_for_path(state, path)?;
    state.lsp.server_capabilities.get(&key)
}

pub(in crate::kernel::store) fn lsp_client_key_for_path(
    state: &crate::kernel::AppState,
    path: &Path,
) -> Option<LspClientKey> {
    crate::kernel::lsp_registry::client_key_for_path(&state.workspace_root, path)
        .map(|(_, key)| key)
}

pub(in crate::kernel::store) fn lsp_position_encoding_for_path(
    state: &crate::kernel::AppState,
    path: &Path,
) -> LspPositionEncoding {
    lsp_server_capabilities_for_path(state, path)
        .map(|c| c.position_encoding)
        .unwrap_or(LspPositionEncoding::Utf16)
}

fn hash_inlay_hints_payload(hints: &[crate::kernel::services::ports::LspInlayHint]) -> u64 {
    let mut hasher = FxHasher::default();
    for hint in hints {
        hint.position.line.hash(&mut hasher);
        hint.position.character.hash(&mut hasher);
        hint.padding_left.hash(&mut hasher);
        hint.padding_right.hash(&mut hasher);
        hint.label.hash(&mut hasher);
    }
    hasher.finish()
}

fn build_inlay_hint_lines_snapshot(
    hints: &[crate::kernel::services::ports::LspInlayHint],
    start_line: usize,
    end_line_exclusive: usize,
) -> Vec<Vec<String>> {
    let line_count = end_line_exclusive.saturating_sub(start_line);
    if line_count == 0 {
        return Vec::new();
    }

    let mut counts = vec![0usize; line_count];
    for hint in hints {
        let line = hint.position.line as usize;
        if line < start_line || line >= end_line_exclusive {
            continue;
        }
        counts[line - start_line] = counts[line - start_line].saturating_add(1);
    }

    let mut per_line = counts
        .into_iter()
        .map(Vec::<(u32, String)>::with_capacity)
        .collect::<Vec<_>>();
    let mut needs_sort = vec![false; line_count];

    for hint in hints {
        let line = hint.position.line as usize;
        if line < start_line || line >= end_line_exclusive {
            continue;
        }

        let mut text = String::new();
        if hint.padding_left {
            text.push(' ');
        }
        text.push_str(hint.label.as_str());
        if hint.padding_right {
            text.push(' ');
        }

        let line_idx = line - start_line;
        let row = &mut per_line[line_idx];
        if let Some((prev_col, prev_text)) = row.last() {
            let out_of_order = *prev_col > hint.position.character
                || (*prev_col == hint.position.character && prev_text.as_str() > text.as_str());
            if out_of_order {
                needs_sort[line_idx] = true;
            }
        }
        row.push((hint.position.character, text));
    }

    let mut lines = Vec::with_capacity(line_count);
    for (line_idx, mut row) in per_line.into_iter().enumerate() {
        if needs_sort[line_idx] && row.len() > 1 {
            row.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        }
        lines.push(row.into_iter().map(|(_, text)| text).collect());
    }
    lines
}

fn hash_folding_ranges_payload(ranges: &[crate::kernel::services::ports::LspFoldingRange]) -> u64 {
    let mut hasher = FxHasher::default();
    for range in ranges {
        range.start_line.hash(&mut hasher);
        range.end_line.hash(&mut hasher);
    }
    hasher.finish()
}

pub(in crate::kernel::store) fn lsp_position_encoding(
    state: &crate::kernel::AppState,
) -> LspPositionEncoding {
    let pane = state.ui.editor_layout.active_pane;
    let Some(tab) = state.editor.pane(pane).and_then(|pane| pane.active_tab()) else {
        return LspPositionEncoding::Utf16;
    };
    let Some(path) = tab.path.as_ref() else {
        return LspPositionEncoding::Utf16;
    };
    lsp_position_encoding_for_path(state, path)
}

pub(in crate::kernel::store) fn lsp_position_from_cursor(
    tab: &crate::kernel::editor::EditorTabState,
    encoding: LspPositionEncoding,
) -> (u32, u32) {
    lsp_position_from_buffer_pos(tab, tab.buffer.cursor(), encoding)
}

pub(in crate::kernel::store) fn lsp_position_from_buffer_pos(
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

pub(in crate::kernel::store) fn lsp_position_from_char_offset(
    tab: &crate::kernel::editor::EditorTabState,
    char_offset: usize,
    encoding: LspPositionEncoding,
) -> LspPosition {
    let rope = tab.buffer.rope();
    let char_offset = char_offset.min(rope.len_chars());
    let row = rope.char_to_line(char_offset);
    let line_start = rope.line_to_char(row);
    let col_chars = char_offset.saturating_sub(line_start);
    let line_slice = rope.line(row);
    let character = column_for_chars(line_slice, col_chars, encoding);

    LspPosition {
        line: row as u32,
        character,
    }
}

pub(in crate::kernel::store) fn lsp_range_for_full_lines(
    tab: &crate::kernel::editor::EditorTabState,
    start_line: usize,
    end_line_exclusive: usize,
    encoding: LspPositionEncoding,
) -> Option<LspRange> {
    if start_line >= end_line_exclusive {
        return None;
    }

    let total_lines = tab.buffer.len_lines().max(1);
    let start_line = start_line.min(total_lines.saturating_sub(1));
    let end_line_exclusive = end_line_exclusive
        .max(start_line.saturating_add(1))
        .min(total_lines);

    let start = LspPosition {
        line: start_line as u32,
        character: 0,
    };

    let end = if end_line_exclusive < total_lines {
        LspPosition {
            line: end_line_exclusive as u32,
            character: 0,
        }
    } else {
        let rope = tab.buffer.rope();
        let last_line = total_lines.saturating_sub(1);
        let line_start = rope.line_to_char(last_line);
        let slice = rope.line(last_line);
        let mut len = slice.len_chars();
        if len > 0 && slice.char(len.saturating_sub(1)) == '\n' {
            len = len.saturating_sub(1);
            if len > 0 && slice.char(len.saturating_sub(1)) == '\r' {
                len = len.saturating_sub(1);
            }
        }

        let end_char = (line_start + len).min(rope.len_chars());
        lsp_position_from_char_offset(tab, end_char, encoding)
    };

    Some(LspRange { start, end })
}

fn end_line_exclusive_from_range(range: &LspRange) -> usize {
    let end_line = range.end.line as usize;
    if range.end.character == 0 {
        end_line
    } else {
        end_line.saturating_add(1)
    }
}

impl super::super::Store {
    // 文件打开后，按服务器能力排期可选 LSP 请求（inlay / folding）。
    pub(in crate::kernel::store) fn schedule_lsp_requests_for_opened_file(
        &self,
        pane: usize,
        path: &Path,
        effects: &mut Vec<crate::kernel::Effect>,
    ) {
        let caps = lsp_server_capabilities_for_path(&self.state, path);
        let supports_inlay_hints = caps.is_some_and(|c| c.inlay_hints);
        let supports_folding_range = caps.is_some_and(|c| c.folding_range);
        if (supports_inlay_hints || supports_folding_range) && is_lsp_source_path(path) {
            let Some(tab) = self
                .state
                .editor
                .pane(pane)
                .and_then(|pane_state| pane_state.active_tab())
            else {
                return;
            };
            let version = tab.edit_version;
            let encoding = lsp_position_encoding_for_path(&self.state, path);

            if supports_inlay_hints {
                let total_lines = tab.buffer.len_lines().max(1);
                let start_line = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                let end_line_exclusive = (start_line + tab.viewport.height.max(1)).min(total_lines);
                if let Some(range) =
                    lsp_range_for_full_lines(tab, start_line, end_line_exclusive, encoding)
                {
                    effects.push(Effect::LspInlayHintsRequest {
                        path: path.to_path_buf(),
                        version,
                        range,
                    });
                }
            }

            if supports_folding_range {
                effects.push(Effect::LspFoldingRangeRequest {
                    path: path.to_path_buf(),
                    version,
                });
            }
        }
    }

    pub(in crate::kernel::store) fn apply_workspace_edit(
        &mut self,
        edit: LspWorkspaceEdit,
        effects: &mut Vec<crate::kernel::Effect>,
    ) -> bool {
        let LspWorkspaceEdit {
            changes,
            resource_ops,
        } = edit;
        let mut pending_file_edits: Vec<LspWorkspaceFileEdit> = Vec::new();
        let mut any_changed = false;
        let mut open_paths_changed = false;
        let encoding = lsp_position_encoding(&self.state);

        let mut rename_forward: HashMap<std::path::PathBuf, std::path::PathBuf> = HashMap::new();
        let mut rename_backward: HashMap<std::path::PathBuf, std::path::PathBuf> = HashMap::new();
        for op in &resource_ops {
            if let LspResourceOp::RenameFile {
                old_path, new_path, ..
            } = op
            {
                rename_forward.insert(old_path.clone(), new_path.clone());
                rename_backward.insert(new_path.clone(), old_path.clone());
            }
        }

        for mut file_edit in changes {
            if file_edit.edits.is_empty() {
                continue;
            }

            let mut targets = open_tabs_for_path(&self.state.editor, &file_edit.path);
            if let Some(old_path) = rename_backward.get(&file_edit.path) {
                targets.extend(open_tabs_for_path(&self.state.editor, old_path));
            }
            if let Some(new_path) = rename_forward.get(&file_edit.path) {
                targets.extend(open_tabs_for_path(&self.state.editor, new_path));
            }
            targets.sort_unstable();
            targets.dedup();
            if targets.is_empty() {
                file_edit.path = resolve_renamed_path(file_edit.path, &rename_forward);
                pending_file_edits.push(file_edit);
                continue;
            }

            let mut edits: Vec<&LspTextEdit> = file_edit.edits.iter().collect();
            edits.sort_by(|a, b| {
                b.range
                    .start
                    .line
                    .cmp(&a.range.start.line)
                    .then_with(|| b.range.start.character.cmp(&a.range.start.character))
                    .then_with(|| b.range.end.line.cmp(&a.range.end.line))
                    .then_with(|| b.range.end.character.cmp(&a.range.end.character))
            });

            for (pane, tab_index) in targets {
                for edit in &edits {
                    let (start_byte, end_byte) = {
                        let Some(tab) = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                        else {
                            continue;
                        };

                        let start = lsp_position_to_byte_offset(
                            tab,
                            edit.range.start.line,
                            edit.range.start.character,
                            encoding,
                        );
                        let end = lsp_position_to_byte_offset(
                            tab,
                            edit.range.end.line,
                            edit.range.end.character,
                            encoding,
                        );
                        (start, end)
                    };

                    let (changed, editor_effects) =
                        self.state
                            .editor
                            .dispatch_action(EditorAction::ApplyTextEditToTab {
                                pane,
                                tab_index,
                                start_byte,
                                end_byte,
                                text: edit.new_text.clone(),
                            });
                    effects.extend(editor_effects);
                    if changed {
                        any_changed = true;
                    }
                }
            }
        }

        for op in &resource_ops {
            let LspResourceOp::RenameFile {
                old_path, new_path, ..
            } = op
            else {
                continue;
            };

            for pane in &mut self.state.editor.panes {
                for tab in &mut pane.tabs {
                    if tab.path.as_ref() == Some(old_path) {
                        tab.set_path(new_path.clone());
                        open_paths_changed = true;
                    }
                }
            }
        }

        if open_paths_changed {
            self.state.editor.open_paths_version =
                self.state.editor.open_paths_version.saturating_add(1);
        }

        if !resource_ops.is_empty() || !pending_file_edits.is_empty() {
            effects.push(crate::kernel::Effect::ApplyFileEdits {
                position_encoding: encoding,
                resource_ops,
                edits: pending_file_edits,
            });
        }

        any_changed || open_paths_changed
    }

    fn handle_hover_response(
        &mut self,
        session: i32,
        payload: LspHoverPayload,
    ) -> super::super::DispatchResult {
        if session < self.state.ui.hover.session {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let previous_text = self.state.ui.hover.display_text();
        let next_model = {
            let pane = self.state.ui.editor_layout.active_pane;
            self.state
                .editor
                .pane(pane)
                .and_then(|pane| pane.active_tab())
                .and_then(|tab| {
                    let adapter = crate::kernel::language::adapter::adapter_for_tab(tab);
                    let runtime = language_runtime_context(&self.state, tab, adapter);
                    adapter.hover_protocol().normalize_hover(&runtime, &payload)
                })
        };
        let session_changed = session != self.state.ui.hover.session;
        let model_changed = self.state.ui.hover.model.as_ref() != next_model.as_ref();

        if session_changed {
            self.state.ui.hover.session = session;
            self.state.ui.hover.implementation_preview = None;
            self.state.ui.hover.definition_preview = None;
        }
        self.state.ui.hover.model = next_model;

        let updated =
            session_changed || model_changed || previous_text != self.state.ui.hover.display_text();
        super::super::DispatchResult {
            effects: Vec::new(),
            state_changed: updated,
        }
    }

    fn handle_hover_implementation_preview(
        &mut self,
        session: i32,
        payload: LspHoverPreviewPayload,
    ) -> super::super::DispatchResult {
        if session < self.state.ui.hover.session {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let previous_text = self.state.ui.hover.display_text();
        let session_changed = session != self.state.ui.hover.session;
        let model_changed = session_changed && self.state.ui.hover.model.is_some();
        if session_changed {
            self.state.ui.hover.session = session;
            self.state.ui.hover.model = None;
            self.state.ui.hover.definition_preview = None;
        }
        self.state.ui.hover.implementation_preview =
            normalize_hover_preview_for_active_tab(&self.state, &payload);

        let updated =
            session_changed || model_changed || previous_text != self.state.ui.hover.display_text();
        super::super::DispatchResult {
            effects: Vec::new(),
            state_changed: updated,
        }
    }

    fn handle_hover_definition_preview(
        &mut self,
        session: i32,
        payload: LspHoverPreviewPayload,
    ) -> super::super::DispatchResult {
        if session < self.state.ui.hover.session {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let previous_text = self.state.ui.hover.display_text();
        let session_changed = session != self.state.ui.hover.session;
        let model_changed = session_changed && self.state.ui.hover.model.is_some();
        if session_changed {
            self.state.ui.hover.session = session;
            self.state.ui.hover.model = None;
            self.state.ui.hover.implementation_preview = None;
        }
        self.state.ui.hover.definition_preview =
            normalize_hover_preview_for_active_tab(&self.state, &payload);

        let updated =
            session_changed || model_changed || previous_text != self.state.ui.hover.display_text();
        super::super::DispatchResult {
            effects: Vec::new(),
            state_changed: updated,
        }
    }

    fn handle_definition(
        &mut self,
        path: std::path::PathBuf,
        line: u32,
        column: u32,
    ) -> super::super::DispatchResult {
        let prev_focus = self.state.ui.focus;
        let prev_active_pane = self.state.ui.editor_layout.active_pane;
        let preferred_pane = self.state.ui.editor_layout.active_pane;

        if let Some((pane, tab_index)) = find_open_tab(&self.state.editor, preferred_pane, &path) {
            self.state.ui.editor_layout.active_pane = pane;
            self.state.ui.focus = FocusTarget::Editor;

            let (changed1, mut eff1) =
                self.state
                    .editor
                    .dispatch_action(EditorAction::SetActiveTab {
                        pane,
                        index: tab_index,
                    });

            let byte_offset = self
                .state
                .editor
                .pane(pane)
                .and_then(|pane_state| pane_state.tabs.get(tab_index))
                .map(|tab| {
                    let encoding = lsp_position_encoding_for_path(&self.state, &path);
                    lsp_position_to_byte_offset(tab, line, column, encoding)
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

            return super::super::DispatchResult {
                effects: eff1,
                state_changed,
            };
        }

        let pane = preferred_pane;
        self.state.ui.editor_layout.active_pane = pane;
        self.state.ui.focus = FocusTarget::Editor;
        self.state.ui.pending_editor_nav = Some(crate::kernel::state::PendingEditorNavigation {
            pane,
            path: path.clone(),
            target: crate::kernel::state::PendingEditorNavigationTarget::LineColumn {
                line,
                column,
            },
        });

        super::super::DispatchResult {
            effects: vec![Effect::LoadFile(path)],
            state_changed: true,
        }
    }

    fn handle_server_capabilities(
        &mut self,
        server: crate::kernel::services::ports::LspServerKind,
        root: std::path::PathBuf,
        capabilities: LspServerCapabilities,
    ) -> super::super::DispatchResult {
        let key = LspClientKey { server, root };
        let changed = match self.state.lsp.server_capabilities.get(&key) {
            Some(existing) if existing == &capabilities => false,
            _ => {
                self.state
                    .lsp
                    .server_capabilities
                    .insert(key.clone(), capabilities);
                true
            }
        };
        let mut effects = Vec::new();
        if changed {
            let Some(caps) = self.state.lsp.server_capabilities.get(&key) else {
                return super::super::DispatchResult {
                    effects,
                    state_changed: changed,
                };
            };

            // Request optional features once we know server capabilities; this avoids
            // queuing unsupported requests during initialization (common for pyright).
            for pane in &self.state.editor.panes {
                let Some(tab) = pane.active_tab() else {
                    continue;
                };
                let Some(path) = tab.path.as_ref() else {
                    continue;
                };
                if !is_lsp_source_path(path) {
                    continue;
                }

                let Some(tab_key) = lsp_client_key_for_path(&self.state, path) else {
                    continue;
                };
                if tab_key != key {
                    continue;
                }

                let version = tab.edit_version;
                let encoding = caps.position_encoding;

                if caps.inlay_hints {
                    let total_lines = tab.buffer.len_lines().max(1);
                    let start_line = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                    let end_line_exclusive =
                        (start_line + tab.viewport.height.max(1)).min(total_lines);
                    if let Some(range) =
                        lsp_range_for_full_lines(tab, start_line, end_line_exclusive, encoding)
                    {
                        effects.push(Effect::LspInlayHintsRequest {
                            path: path.clone(),
                            version,
                            range,
                        });
                    }
                }

                if caps.folding_range {
                    effects.push(Effect::LspFoldingRangeRequest {
                        path: path.clone(),
                        version,
                    });
                }
            }
        }
        super::super::DispatchResult {
            effects,
            state_changed: changed,
        }
    }

    fn handle_inlay_hints(
        &mut self,
        path: std::path::PathBuf,
        version: u64,
        range: LspRange,
        hints: Vec<crate::kernel::services::ports::LspInlayHint>,
    ) -> super::super::DispatchResult {
        let start_line = range.start.line as usize;
        let end_line_exclusive = end_line_exclusive_from_range(&range);
        if end_line_exclusive <= start_line {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let hint_count = hints.len();
        let stamp = RangePayloadStamp {
            version,
            item_count: hints.len(),
            start_line,
            end_line_exclusive,
            digest: hash_inlay_hints_payload(&hints),
        };
        if self
            .state
            .lsp
            .payload_fingerprints
            .inlay_range_by_path
            .get(&path)
            .copied()
            == Some(stamp)
        {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let matched_tabs = self
            .state
            .editor
            .panes
            .iter()
            .flat_map(|pane| pane.tabs.iter())
            .filter(|tab| tab.path.as_ref() == Some(&path) && tab.edit_version == version)
            .count();
        if matched_tabs == 0 {
            tracing::debug!(
                path = %path.display(),
                version,
                start_line,
                end_line_exclusive,
                hint_count,
                "drop inlay hints (no matching tab/version)"
            );
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let mut snapshot: Option<Vec<Vec<String>>> = None;
        let mut changed = false;

        for pane in &mut self.state.editor.panes {
            for tab in &mut pane.tabs {
                if tab.path.as_ref() != Some(&path) || tab.edit_version != version {
                    continue;
                }

                if snapshot.is_none() {
                    snapshot = Some(build_inlay_hint_lines_snapshot(
                        &hints,
                        start_line,
                        end_line_exclusive,
                    ));
                }

                if let Some(lines) = snapshot.as_ref() {
                    changed |= tab.set_inlay_hints_from_slice(
                        version,
                        start_line,
                        end_line_exclusive,
                        lines,
                    );
                }
            }
        }
        self.state
            .lsp
            .payload_fingerprints
            .inlay_range_by_path
            .insert(path, stamp);

        super::super::DispatchResult {
            effects: Vec::new(),
            state_changed: changed,
        }
    }

    fn handle_folding_ranges(
        &mut self,
        path: std::path::PathBuf,
        version: u64,
        ranges: Vec<crate::kernel::services::ports::LspFoldingRange>,
    ) -> super::super::DispatchResult {
        let stamp = PayloadStamp {
            version,
            item_count: ranges.len(),
            digest: hash_folding_ranges_payload(&ranges),
        };
        if self
            .state
            .lsp
            .payload_fingerprints
            .folding_by_path
            .get(&path)
            .copied()
            == Some(stamp)
        {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let matched_tabs = self
            .state
            .editor
            .panes
            .iter()
            .flat_map(|pane| pane.tabs.iter())
            .filter(|tab| tab.path.as_ref() == Some(&path) && tab.edit_version == version)
            .count();
        if matched_tabs == 0 {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let mut changed = false;

        for pane in &mut self.state.editor.panes {
            for tab in &mut pane.tabs {
                if tab.path.as_ref() != Some(&path) || tab.edit_version != version {
                    continue;
                }

                changed |= tab.set_folding_ranges_from_slice(version, &ranges);
            }
        }
        self.state
            .lsp
            .payload_fingerprints
            .folding_by_path
            .insert(path, stamp);

        super::super::DispatchResult {
            effects: Vec::new(),
            state_changed: changed,
        }
    }

    fn handle_completion(
        &mut self,
        items: Vec<crate::kernel::services::ports::LspCompletionItem>,
        is_incomplete: bool,
    ) -> super::super::DispatchResult {
        let Some(req) = self.state.ui.completion.pending_request.clone() else {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        };

        let Some(tab) = self
            .state
            .editor
            .pane(req.pane)
            .and_then(|pane| pane.active_tab())
        else {
            let had = self.state.ui.completion.is_active();
            self.state.ui.completion = CompletionPopupState::default();
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: had,
            };
        };

        let valid = tab.path.as_ref() == Some(&req.path) && tab.edit_version >= req.version;

        if !valid {
            let had = self.state.ui.completion.is_active();
            self.state.ui.completion = CompletionPopupState::default();
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: had,
            };
        }

        let adapter = crate::kernel::language::adapter::adapter_for_tab(tab);
        let items = if items.is_empty() {
            let runtime = completion_runtime_context(&self.state, tab, adapter);
            adapter
                .completion_protocol()
                .fallback_completion_items(&runtime)
        } else {
            items
        };

        if items.is_empty() {
            let had = self.state.ui.completion.is_active();
            self.state.ui.completion = CompletionPopupState::default();
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: had,
            };
        }

        self.state.ui.hover.clear();
        let prev_selected = if self.state.ui.completion.selection_locked {
            self.state.ui.completion.selected_item().map(|item| item.id)
        } else {
            None
        };

        let mut all_items = {
            let runtime = completion_runtime_context(&self.state, tab, adapter);
            items
                .into_iter()
                .map(|raw| {
                    normalize_completion_record(&runtime, adapter.completion_protocol(), raw)
                })
                .collect::<Vec<_>>()
        };
        let language = tab.language();
        sort_completion_items(&mut all_items, &self.completion_ranker, language);
        self.state.ui.completion.all_items = all_items;
        self.state.ui.completion.rebuild_index_by_id();
        self.state.ui.completion.invalidate_filter_cache();
        self.state.ui.completion.visible_indices = {
            let runtime = completion_runtime_context(&self.state, tab, adapter);
            filtered_completion_indices(
                &runtime,
                &self.state.ui.completion.all_items,
                adapter.interaction(),
            )
        };
        self.state.ui.completion.visible = self.state.ui.completion.visible_len() > 0;
        self.state.ui.completion.selected = prev_selected
            .and_then(|id| {
                self.state
                    .ui
                    .completion
                    .visible_indices
                    .iter()
                    .position(|idx| self.state.ui.completion.all_items[*idx].entry.id == id)
            })
            .unwrap_or(0)
            .min(self.state.ui.completion.visible_len().saturating_sub(1));
        self.state.ui.completion.request = Some(req.clone());
        self.state.ui.completion.pending_request = None;
        self.state.ui.completion.is_incomplete = is_incomplete;
        self.state.ui.completion.resolve_inflight = None;
        self.state.ui.completion.session_started_at = Some(Instant::now());

        let mut effects = Vec::new();
        if let Some(record) = self.state.ui.completion.selected_record().cloned() {
            let _ = self.maybe_request_completion_resolve_for_record(&record, &mut effects);
        }
        super::super::DispatchResult {
            effects,
            state_changed: true,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_completion_resolved(
        &mut self,
        id: u64,
        detail: Option<String>,
        documentation: Option<String>,
        insert_text: Option<String>,
        insert_text_format: Option<crate::kernel::services::ports::LspInsertTextFormat>,
        insert_range: Option<LspRange>,
        replace_range: Option<LspRange>,
        additional_text_edits: Vec<LspTextEdit>,
        command: Option<crate::kernel::services::ports::LspCommand>,
    ) -> super::super::DispatchResult {
        let mut changed = false;

        if self.state.ui.completion.resolve_inflight == Some(id) {
            self.state.ui.completion.resolve_inflight = None;
            changed = true;
        }

        let resolved_index = self.state.ui.completion.index_of_id(id);
        if let Some(idx) = resolved_index {
            let mut item_changed = false;
            {
                let record = &mut self.state.ui.completion.all_items[idx];
                let item = &mut record.raw;

                if let Some(detail) = detail.as_ref() {
                    if item.detail.as_deref() != Some(detail) {
                        item.detail = Some(detail.clone());
                        item_changed = true;
                    }
                }
                if let Some(doc) = documentation.as_ref() {
                    if item.documentation.as_deref() != Some(doc) {
                        item.documentation = Some(doc.clone());
                        item_changed = true;
                    }
                }
                if let Some(text) = insert_text.as_ref() {
                    if item.insert_text != *text {
                        item.insert_text = text.clone();
                        item_changed = true;
                    }
                }
                if let Some(format) = insert_text_format {
                    if item.insert_text_format != format {
                        item.insert_text_format = format;
                        item_changed = true;
                    }
                }
                if let Some(range) = insert_range {
                    let needs_update = item.insert_range.is_none_or(|current| {
                        current.start.line != range.start.line
                            || current.start.character != range.start.character
                            || current.end.line != range.end.line
                            || current.end.character != range.end.character
                    });
                    if needs_update {
                        item.insert_range = Some(range);
                        item_changed = true;
                    }
                }
                if let Some(range) = replace_range {
                    let needs_update = item.replace_range.is_none_or(|current| {
                        current.start.line != range.start.line
                            || current.start.character != range.start.character
                            || current.end.line != range.end.line
                            || current.end.character != range.end.character
                    });
                    if needs_update {
                        item.replace_range = Some(range);
                        item_changed = true;
                    }
                }
                if !additional_text_edits.is_empty() && item.additional_text_edits.is_empty() {
                    item.additional_text_edits = additional_text_edits.clone();
                    item_changed = true;
                }
                if command.is_some() && item.command.is_none() {
                    item.command = command.clone();
                    item_changed = true;
                }
                if item.data.is_some() {
                    item.data = None;
                    item_changed = true;
                }
            }

            if item_changed {
                let request = self.state.ui.completion.request.clone();
                let raw = self.state.ui.completion.all_items[idx].raw.clone();
                let next_entry = self
                    .state
                    .ui
                    .completion
                    .request
                    .as_ref()
                    .and_then(|req| find_open_tab(&self.state.editor, req.pane, &req.path))
                    .and_then(|(pane, tab_index)| {
                        self.state
                            .editor
                            .panes
                            .get(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                    })
                    .map(|tab| {
                        let adapter = crate::kernel::language::adapter::adapter_for_tab(tab);
                        let runtime = completion_runtime_context(&self.state, tab, adapter);
                        adapter.completion_protocol().normalize_completion(
                            &crate::kernel::language::CompletionContext::live(runtime, &raw),
                        )
                    })
                    .unwrap_or_else(|| {
                        let snapshot = request
                            .as_ref()
                            .map(|req| req.normalization.clone())
                            .unwrap_or_else(|| {
                                crate::kernel::language::CompletionNormalizationSnapshot::detached(
                                    None,
                                )
                            });
                        let adapter = crate::kernel::language::adapter_for(snapshot.language);
                        adapter.completion_protocol().normalize_completion(
                            &crate::kernel::language::CompletionContext::snapshot(snapshot, &raw),
                        )
                    });
                self.state.ui.completion.all_items[idx].entry = next_entry;
                if self
                    .state
                    .ui
                    .completion
                    .visible_indices
                    .binary_search(&idx)
                    .is_ok()
                {
                    changed = true;
                }
            }
        }

        super::super::DispatchResult {
            effects: Vec::new(),
            state_changed: changed,
        }
    }

    fn handle_signature_help(
        &mut self,
        payload: crate::kernel::services::ports::LspSignatureHelpPayload,
    ) -> super::super::DispatchResult {
        let Some(req) = self.state.ui.signature_help.request.clone() else {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        };

        let next_model = self
            .state
            .editor
            .pane(req.pane)
            .and_then(|pane| pane.active_tab())
            .filter(|tab| tab.path.as_ref() == Some(&req.path) && tab.edit_version >= req.version)
            .and_then(|tab| {
                let adapter = crate::kernel::language::adapter::adapter_for_tab(tab);
                let runtime = language_runtime_context(&self.state, tab, adapter);
                adapter
                    .signature_help_protocol()
                    .normalize_signature_help(&runtime, &payload)
            });

        let Some(next_model) = next_model else {
            let had = self.state.ui.signature_help.is_active();
            self.state.ui.signature_help = SignatureHelpPopupState::default();
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: had,
            };
        };

        let previous_text = self.state.ui.signature_help.display_text();
        let changed = !self.state.ui.signature_help.visible
            || self.state.ui.signature_help.model.as_ref() != Some(&next_model);
        self.state.ui.signature_help.visible = true;
        self.state.ui.signature_help.model = Some(next_model);
        let changed = changed || previous_text != self.state.ui.signature_help.display_text();
        super::super::DispatchResult {
            effects: Vec::new(),
            state_changed: changed,
        }
    }

    fn handle_format_completed(
        &mut self,
        path: std::path::PathBuf,
    ) -> super::super::DispatchResult {
        if self.state.lsp.pending_format_on_save.as_ref() != Some(&path) {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }
        self.state.lsp.pending_format_on_save = None;

        let preferred_pane = self.state.ui.editor_layout.active_pane;
        let Some((pane, tab_index)) = find_open_tab(&self.state.editor, preferred_pane, &path)
        else {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        };
        let Some(version) = self
            .state
            .editor
            .pane(pane)
            .and_then(|pane_state| pane_state.tabs.get(tab_index))
            .map(|tab| tab.edit_version)
        else {
            return super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        };

        super::super::DispatchResult {
            effects: vec![Effect::WriteFile {
                pane,
                path,
                version,
            }],
            state_changed: false,
        }
    }

    pub(in crate::kernel::store) fn reduce_lsp_action(
        &mut self,
        action: Action,
    ) -> super::super::DispatchResult {
        match action {
            Action::LspDiagnostics { path, items } => super::super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.problems.update_path(path, items),
            },
            Action::LspHoverClear => {
                let had = self.state.ui.hover.is_active();
                self.state.ui.hover.clear();
                super::super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: had,
                }
            }
            Action::LspHoverResponse { session, payload } => {
                self.handle_hover_response(session, payload)
            }
            Action::LspHoverImplementationPreview { session, payload } => {
                self.handle_hover_implementation_preview(session, payload)
            }
            Action::LspHoverDefinitionPreview { session, payload } => {
                self.handle_hover_definition_preview(session, payload)
            }
            Action::LspDefinition { path, line, column } => {
                self.handle_definition(path, line, column)
            }
            Action::LspReferences { items } => {
                let mut changed = self.state.locations.set_items(items);
                changed |= self.open_overlay(OverlayKind::Locations);

                super::super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspCodeActions { items } => {
                let mut changed = self.state.code_actions.set_items(items);
                changed |= self.open_overlay(OverlayKind::CodeActions);

                super::super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspSymbols { items } => {
                let mut changed = self.state.symbols.set_items(items);
                changed |= self.open_overlay(OverlayKind::Symbols);

                super::super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspServerCapabilities {
                server,
                root,
                capabilities,
            } => self.handle_server_capabilities(server, root, capabilities),
            Action::LspInlayHints {
                path,
                version,
                range,
                hints,
            } => self.handle_inlay_hints(path, version, range, hints),
            Action::LspFoldingRanges {
                path,
                version,
                ranges,
            } => self.handle_folding_ranges(path, version, ranges),
            Action::LspCompletion {
                items,
                is_incomplete,
            } => self.handle_completion(items, is_incomplete),
            Action::LspCompletionResolved {
                id,
                detail,
                documentation,
                insert_text,
                insert_text_format,
                insert_range,
                replace_range,
                additional_text_edits,
                command,
            } => self.handle_completion_resolved(
                id,
                detail,
                documentation,
                insert_text,
                insert_text_format,
                insert_range,
                replace_range,
                additional_text_edits,
                command,
            ),
            Action::LspSignatureHelp { payload } => self.handle_signature_help(payload),
            Action::LspApplyWorkspaceEdit { edit } => {
                let mut effects = Vec::new();
                let changed = self.apply_workspace_edit(edit, &mut effects);
                super::super::DispatchResult {
                    effects,
                    state_changed: changed,
                }
            }
            Action::LspFormatCompleted { path } => self.handle_format_completed(path),
            _ => unreachable!("non-lsp action passed to reduce_lsp_action"),
        }
    }
}
