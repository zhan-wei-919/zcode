use crate::core::Command;
use crate::kernel::language::{adapter::adapter_for_tab, CompletionResolveState};
use crate::kernel::services::ports::{LspCompletionTriggerContext, LspPosition, LspRange};
use crate::kernel::{Effect, InputDialogKind, OverlayKind};

use super::intel::completion::sync_completion_items_from_cache;
use super::intel::lsp::{
    lsp_position_encoding_for_path, lsp_position_from_buffer_pos, lsp_range_for_full_lines,
    lsp_request_target, lsp_server_capabilities_for_path,
};
use super::util::is_lsp_source_path;
use super::DispatchResult;

impl super::Store {
    pub(super) fn reduce_lsp_command(&mut self, command: Command) -> DispatchResult {
        let mut state_changed = false;
        let effects = Vec::new();

        match command {
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
                        Some(self.completion_request_context(pane, path.clone(), version, None));

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
            _ => unreachable!("non-lsp command passed to reduce_lsp_command"),
        }

        DispatchResult {
            effects,
            state_changed,
        }
    }
}
