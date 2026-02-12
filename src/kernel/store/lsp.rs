use crate::kernel::services::ports::{
    LspClientKey, LspPosition, LspPositionEncoding, LspRange, LspResourceOp, LspServerCapabilities,
    LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
};
use crate::kernel::state::{
    CompletionPopupState, PayloadStamp, RangePayloadStamp, SignatureHelpPopupState,
};
use crate::kernel::EditorAction;
use crate::kernel::{Action, BottomPanelTab, Effect, FocusTarget};
use rustc_hash::FxHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::completion::{filtered_completion_indices, sort_completion_items};
use super::semantic::{
    semantic_highlight_lines_from_tokens, semantic_highlight_lines_from_tokens_range,
};
use super::util::find_open_tab;
use super::util::{is_lsp_source_path, line_len_chars, open_tabs_for_path, resolve_renamed_path};

#[cfg(test)]
static LSP_CAPABILITY_LOOKUP_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn reset_lsp_capability_lookup_perf_counter() {
    LSP_CAPABILITY_LOOKUP_CALLS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn lsp_capability_lookup_perf_counter() -> usize {
    LSP_CAPABILITY_LOOKUP_CALLS.load(Ordering::Relaxed)
}

pub(super) fn problem_byte_offset(
    tab: &crate::kernel::editor::EditorTabState,
    range: crate::kernel::problems::ProblemRange,
    encoding: LspPositionEncoding,
) -> usize {
    lsp_position_to_byte_offset(tab, range.start_line, range.start_col, encoding)
}

pub(super) fn lsp_position_to_byte_offset(
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

pub(super) fn lsp_col_to_char_offset_in_line(
    line: ropey::RopeSlice<'_>,
    col: u32,
    encoding: LspPositionEncoding,
) -> usize {
    let mut units = 0u32;
    let mut chars = 0usize;
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            break;
        }
        if ch == '\r' && matches!(it.peek(), Some('\n')) {
            break;
        }
        let next = units
            + match encoding {
                LspPositionEncoding::Utf8 => ch.len_utf8() as u32,
                LspPositionEncoding::Utf16 => ch.len_utf16() as u32,
                LspPositionEncoding::Utf32 => 1,
            };
        if next > col {
            break;
        }
        units = next;
        chars += 1;
    }
    chars
}

pub(super) fn lsp_request_target(
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

pub(super) fn lsp_server_capabilities_for_path<'a>(
    state: &'a crate::kernel::AppState,
    path: &Path,
) -> Option<&'a LspServerCapabilities> {
    #[cfg(test)]
    {
        LSP_CAPABILITY_LOOKUP_CALLS.fetch_add(1, Ordering::Relaxed);
    }

    let key = lsp_client_key_for_path(state, path)?;
    state.lsp.server_capabilities.get(&key)
}

pub(super) fn lsp_client_key_for_path(
    state: &crate::kernel::AppState,
    path: &Path,
) -> Option<LspClientKey> {
    crate::kernel::lsp_registry::client_key_for_path(&state.workspace_root, path).map(|(_, key)| {
        // Ensure we key capabilities by (server kind + root) to support monorepos with multiple roots.
        key
    })
}

pub(super) fn lsp_position_encoding_for_path(
    state: &crate::kernel::AppState,
    path: &Path,
) -> LspPositionEncoding {
    lsp_server_capabilities_for_path(state, path)
        .map(|c| c.position_encoding)
        .unwrap_or(LspPositionEncoding::Utf16)
}

fn encoding_tag(encoding: LspPositionEncoding) -> u8 {
    match encoding {
        LspPositionEncoding::Utf8 => 8,
        LspPositionEncoding::Utf16 => 16,
        LspPositionEncoding::Utf32 => 32,
    }
}

fn hash_semantic_tokens_payload(
    tokens: &[crate::kernel::services::ports::LspSemanticToken],
    encoding: LspPositionEncoding,
    legend: &crate::kernel::services::ports::LspSemanticTokensLegend,
) -> u64 {
    let mut hasher = FxHasher::default();
    encoding_tag(encoding).hash(&mut hasher);
    legend.token_types.hash(&mut hasher);
    legend.token_modifiers.hash(&mut hasher);
    for token in tokens {
        token.line.hash(&mut hasher);
        token.start.hash(&mut hasher);
        token.length.hash(&mut hasher);
        token.token_type.hash(&mut hasher);
        token.modifiers.hash(&mut hasher);
    }
    hasher.finish()
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

pub(super) fn lsp_position_encoding(state: &crate::kernel::AppState) -> LspPositionEncoding {
    let pane = state.ui.editor_layout.active_pane;
    let Some(tab) = state.editor.pane(pane).and_then(|pane| pane.active_tab()) else {
        return LspPositionEncoding::Utf16;
    };
    let Some(path) = tab.path.as_ref() else {
        return LspPositionEncoding::Utf16;
    };
    lsp_position_encoding_for_path(state, path)
}

pub(super) fn lsp_position_from_cursor(
    tab: &crate::kernel::editor::EditorTabState,
    encoding: LspPositionEncoding,
) -> (u32, u32) {
    lsp_position_from_buffer_pos(tab, tab.buffer.cursor(), encoding)
}

pub(super) fn lsp_position_from_buffer_pos(
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
    let character = match encoding {
        LspPositionEncoding::Utf8 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf8() as u32)
            .sum(),
        LspPositionEncoding::Utf16 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf16() as u32)
            .sum(),
        LspPositionEncoding::Utf32 => col_chars as u32,
    };
    (row as u32, character)
}

pub(super) fn lsp_position_from_char_offset(
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
    let character = match encoding {
        LspPositionEncoding::Utf8 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf8() as u32)
            .sum(),
        LspPositionEncoding::Utf16 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf16() as u32)
            .sum(),
        LspPositionEncoding::Utf32 => col_chars as u32,
    };

    LspPosition {
        line: row as u32,
        character,
    }
}

pub(super) fn lsp_range_for_full_lines(
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

impl super::Store {
    pub(super) fn apply_workspace_edit(
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

                    let (changed, _) =
                        self.state
                            .editor
                            .dispatch_action(EditorAction::ApplyTextEditToTab {
                                pane,
                                tab_index,
                                start_byte,
                                end_byte,
                                text: edit.new_text.clone(),
                            });
                    any_changed |= changed;
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

    pub(super) fn reduce_lsp_action(&mut self, action: Action) -> super::DispatchResult {
        match action {
            Action::LspDiagnostics { path, items } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.problems.update_path(path, items),
            },
            Action::LspHover { text } => {
                let text = text.trim().to_string();
                let updated = if text.is_empty() {
                    self.state.ui.hover_message.take().is_some()
                } else if self.state.ui.hover_message.as_deref() != Some(text.as_str()) {
                    self.state.ui.hover_message = Some(text);
                    true
                } else {
                    false
                };
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: updated,
                }
            }
            Action::LspDefinition { path, line, column } => {
                let prev_focus = self.state.ui.focus;
                let prev_active_pane = self.state.ui.editor_layout.active_pane;
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

                    return super::DispatchResult {
                        effects: eff1,
                        state_changed,
                    };
                }

                let pane = preferred_pane;
                self.state.ui.editor_layout.active_pane = pane;
                self.state.ui.focus = FocusTarget::Editor;
                self.state.ui.pending_editor_nav =
                    Some(crate::kernel::state::PendingEditorNavigation {
                        pane,
                        path: path.clone(),
                        target: crate::kernel::state::PendingEditorNavigationTarget::LineColumn {
                            line,
                            column,
                        },
                    });

                super::DispatchResult {
                    effects: vec![Effect::LoadFile(path)],
                    state_changed: true,
                }
            }
            Action::LspReferences { items } => {
                let mut changed = self.state.locations.set_items(items);

                let prev_visible = self.state.ui.bottom_panel.visible;
                let prev_tab = self.state.ui.bottom_panel.active_tab.clone();
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.bottom_panel.active_tab = BottomPanelTab::Locations;
                if !prev_visible || prev_tab != BottomPanelTab::Locations {
                    changed = true;
                }

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspCodeActions { items } => {
                let mut changed = self.state.code_actions.set_items(items);

                let prev_visible = self.state.ui.bottom_panel.visible;
                let prev_tab = self.state.ui.bottom_panel.active_tab.clone();
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.bottom_panel.active_tab = BottomPanelTab::CodeActions;
                if !prev_visible || prev_tab != BottomPanelTab::CodeActions {
                    changed = true;
                }

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspSymbols { items } => {
                let mut changed = self.state.symbols.set_items(items);

                let prev_visible = self.state.ui.bottom_panel.visible;
                let prev_tab = self.state.ui.bottom_panel.active_tab.clone();
                let prev_focus = self.state.ui.focus;
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.bottom_panel.active_tab = BottomPanelTab::Symbols;
                self.state.ui.focus = FocusTarget::BottomPanel;
                if !prev_visible
                    || prev_tab != BottomPanelTab::Symbols
                    || prev_focus != FocusTarget::BottomPanel
                {
                    changed = true;
                }

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspServerCapabilities {
                server,
                root,
                capabilities,
            } => {
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
                        return super::DispatchResult {
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

                        if caps.semantic_tokens
                            && (caps.semantic_tokens_full || caps.semantic_tokens_range)
                        {
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
                                        path: path.clone(),
                                        version,
                                        range,
                                    });
                                }
                            } else if can_full {
                                effects.push(Effect::LspSemanticTokensRequest {
                                    path: path.clone(),
                                    version,
                                });
                            }
                        }

                        if caps.inlay_hints {
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
                super::DispatchResult {
                    effects,
                    state_changed: changed,
                }
            }
            Action::LspSemanticTokens {
                path,
                version,
                tokens,
            } => {
                let Some(legend) = lsp_server_capabilities_for_path(&self.state, &path)
                    .and_then(|c| c.semantic_tokens_legend.as_ref())
                else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let encoding = lsp_position_encoding_for_path(&self.state, &path);
                let stamp = PayloadStamp {
                    version,
                    item_count: tokens.len(),
                    digest: hash_semantic_tokens_payload(&tokens, encoding, legend),
                };
                if self
                    .state
                    .lsp
                    .payload_fingerprints
                    .semantic_full_by_path
                    .get(&path)
                    .copied()
                    == Some(stamp)
                {
                    return super::DispatchResult {
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
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let Some(legend) = lsp_server_capabilities_for_path(&self.state, &path)
                    .and_then(|c| c.semantic_tokens_legend.clone())
                else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let mut snapshot_lines: Option<Vec<Vec<crate::kernel::editor::HighlightSpan>>> =
                    None;
                let mut changed = false;

                for pane in &mut self.state.editor.panes {
                    for tab in &mut pane.tabs {
                        if tab.path.as_ref() != Some(&path) || tab.edit_version != version {
                            continue;
                        }

                        if tokens.is_empty() {
                            changed |= tab.set_semantic_highlight(version, Vec::new());
                            continue;
                        }

                        if snapshot_lines.is_none() {
                            snapshot_lines = Some(semantic_highlight_lines_from_tokens(
                                tab.buffer.rope(),
                                &tokens,
                                &legend,
                                encoding,
                            ));
                        }

                        if let Some(lines) = snapshot_lines.as_ref() {
                            changed |= tab.set_semantic_highlight_from_slice(version, lines);
                        }
                    }
                }
                self.state
                    .lsp
                    .payload_fingerprints
                    .semantic_full_by_path
                    .insert(path, stamp);

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspSemanticTokensRange {
                path,
                version,
                range,
                tokens,
            } => {
                let Some(legend) = lsp_server_capabilities_for_path(&self.state, &path)
                    .and_then(|c| c.semantic_tokens_legend.as_ref())
                else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let start_line = range.start.line as usize;
                let end_line_exclusive = end_line_exclusive_from_range(&range);
                if end_line_exclusive <= start_line {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let encoding = lsp_position_encoding_for_path(&self.state, &path);
                let stamp = RangePayloadStamp {
                    version,
                    item_count: tokens.len(),
                    start_line,
                    end_line_exclusive,
                    digest: hash_semantic_tokens_payload(&tokens, encoding, legend),
                };
                if self
                    .state
                    .lsp
                    .payload_fingerprints
                    .semantic_range_by_path
                    .get(&path)
                    .copied()
                    == Some(stamp)
                {
                    return super::DispatchResult {
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
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let Some(legend) = lsp_server_capabilities_for_path(&self.state, &path)
                    .and_then(|c| c.semantic_tokens_legend.clone())
                else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let mut snapshot_lines: Option<Vec<Vec<crate::kernel::editor::HighlightSpan>>> =
                    None;
                let mut changed = false;

                for pane in &mut self.state.editor.panes {
                    for tab in &mut pane.tabs {
                        if tab.path.as_ref() != Some(&path) || tab.edit_version != version {
                            continue;
                        }

                        if snapshot_lines.is_none() {
                            snapshot_lines = Some(semantic_highlight_lines_from_tokens_range(
                                tab.buffer.rope(),
                                &tokens,
                                &legend,
                                encoding,
                                start_line,
                                end_line_exclusive,
                            ));
                        }

                        if let Some(lines) = snapshot_lines.as_ref() {
                            changed |= tab.set_semantic_highlight_range_from_slice(
                                version, start_line, lines,
                            );
                        }
                    }
                }
                self.state
                    .lsp
                    .payload_fingerprints
                    .semantic_range_by_path
                    .insert(path, stamp);

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspInlayHints {
                path,
                version,
                range,
                hints,
            } => {
                let start_line = range.start.line as usize;
                let end_line_exclusive = end_line_exclusive_from_range(&range);
                if end_line_exclusive <= start_line {
                    return super::DispatchResult {
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
                    return super::DispatchResult {
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
                    return super::DispatchResult {
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

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspFoldingRanges {
                path,
                version,
                ranges,
            } => {
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
                    return super::DispatchResult {
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
                    return super::DispatchResult {
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

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspCompletion {
                items,
                is_incomplete,
            } => {
                let Some(req) = self.state.ui.completion.pending_request.clone() else {
                    return super::DispatchResult {
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
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                };

                let valid = tab.path.as_ref() == Some(&req.path) && tab.edit_version >= req.version;

                if !valid || items.is_empty() {
                    let had = self.state.ui.completion.is_active();
                    self.state.ui.completion = CompletionPopupState::default();
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                }

                self.state.ui.hover_message = None;
                self.state.ui.completion.visible = true;
                let prev_selected = self.state.ui.completion.selected_item().map(|item| item.id);

                let mut all_items = items;
                let language = tab.language();
                sort_completion_items(&mut all_items, &self.completion_ranker, language);
                self.state.ui.completion.all_items = all_items;
                self.state.ui.completion.rebuild_index_by_id();
                self.state.ui.completion.invalidate_filter_cache();
                let strategy = super::completion_strategy::strategy_for_tab(tab);
                self.state.ui.completion.visible_indices =
                    filtered_completion_indices(tab, &self.state.ui.completion.all_items, strategy);
                self.state.ui.completion.selected = prev_selected
                    .and_then(|id| {
                        self.state
                            .ui
                            .completion
                            .visible_indices
                            .iter()
                            .position(|idx| self.state.ui.completion.all_items[*idx].id == id)
                    })
                    .unwrap_or(0)
                    .min(self.state.ui.completion.visible_len().saturating_sub(1));
                self.state.ui.completion.request = Some(req.clone());
                self.state.ui.completion.pending_request = None;
                self.state.ui.completion.is_incomplete = is_incomplete;
                self.state.ui.completion.resolve_inflight = None;
                self.state.ui.completion.session_started_at = Some(Instant::now());

                let mut effects = Vec::new();
                if let Some(item) = self.state.ui.completion.selected_item().cloned() {
                    if lsp_server_capabilities_for_path(&self.state, &req.path)
                        .is_none_or(|c| c.completion_resolve)
                        && item.data.is_some()
                        && item
                            .documentation
                            .as_ref()
                            .is_none_or(|d| d.trim().is_empty())
                    {
                        self.state.ui.completion.resolve_inflight = Some(item.id);
                        effects.push(Effect::LspCompletionResolveRequest {
                            item: Box::new(item),
                        });
                    }
                }
                super::DispatchResult {
                    effects,
                    state_changed: true,
                }
            }
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
            } => {
                let mut changed = false;

                if self.state.ui.completion.resolve_inflight == Some(id) {
                    self.state.ui.completion.resolve_inflight = None;
                    changed = true;
                }

                let resolved_index = self.state.ui.completion.index_of_id(id);
                if let Some(idx) = resolved_index {
                    let item = &mut self.state.ui.completion.all_items[idx];
                    let mut item_changed = false;

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

                    if item_changed
                        && self
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

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspSignatureHelp { text } => {
                let Some(req) = self.state.ui.signature_help.request.clone() else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let valid = self
                    .state
                    .editor
                    .pane(req.pane)
                    .and_then(|pane| pane.active_tab())
                    .is_some_and(|tab| {
                        tab.path.as_ref() == Some(&req.path) && tab.edit_version >= req.version
                    });

                let text = text.trim().to_string();

                if !valid || text.is_empty() {
                    let had = self.state.ui.signature_help.visible
                        || self.state.ui.signature_help.request.is_some()
                        || !self.state.ui.signature_help.text.is_empty();
                    self.state.ui.signature_help = SignatureHelpPopupState::default();
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                }

                let changed = !self.state.ui.signature_help.visible
                    || self.state.ui.signature_help.text != text;
                self.state.ui.signature_help.visible = true;
                self.state.ui.signature_help.text = text;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspApplyWorkspaceEdit { edit } => {
                let mut effects = Vec::new();
                let changed = self.apply_workspace_edit(edit, &mut effects);
                super::DispatchResult {
                    effects,
                    state_changed: changed,
                }
            }
            Action::LspFormatCompleted { path } => {
                if self.state.lsp.pending_format_on_save.as_ref() != Some(&path) {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                self.state.lsp.pending_format_on_save = None;

                let preferred_pane = self.state.ui.editor_layout.active_pane;
                let Some((pane, tab_index)) =
                    find_open_tab(&self.state.editor, preferred_pane, &path)
                else {
                    return super::DispatchResult {
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
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                super::DispatchResult {
                    effects: vec![Effect::WriteFile {
                        pane,
                        path,
                        version,
                    }],
                    state_changed: false,
                }
            }
            _ => unreachable!("non-lsp action passed to reduce_lsp_action"),
        }
    }
}
