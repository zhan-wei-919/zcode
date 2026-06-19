use crate::kernel::language::adapter::adapter_for_tab;
use crate::kernel::services::ports::{LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit};
use crate::kernel::{Action, Effect};

use super::intel::completion::{
    adjust_completion_multiline_indentation, apply_completion_insertion_cursor,
    completion_replace_range, CompletionInsertion,
};
use super::intel::lsp::{lsp_position_encoding_for_path, lsp_position_to_byte_offset};
use super::seed_completion_semantic_highlight;
use super::DispatchResult;

impl super::Store {
    pub(super) fn reduce_completion_action(&mut self, action: Action) -> DispatchResult {
        match action {
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
            _ => unreachable!("non-completion action passed to reduce_completion_action"),
        }
    }
}
