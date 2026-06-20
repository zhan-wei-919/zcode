use crate::core::Command;
use crate::kernel::language::{adapter::adapter_for_tab, CompletionResolveState};
use crate::kernel::services::ports::LspCompletionTriggerContext;
use crate::kernel::state::{SignatureHelpPopupState, SignatureHelpRequestContext};
use crate::kernel::Effect;

use super::intel::completion::{completion_runtime_context, sync_completion_items_from_cache};
use super::intel::lsp::{lsp_request_target, lsp_server_capabilities_for_path};
use super::util::is_lsp_source_path;
use super::DispatchResult;

impl super::Store {
    pub(super) fn reduce_editor_command(&mut self, command: Command) -> DispatchResult {
        let mut state_changed = false;
        let effects = Vec::new();

        match command {
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
                // Compute this keystroke's `SyntaxFacts` ONCE (the single syntax-tree
                // descent). Every completion / signature-help policy check below reads
                // only `syntax`/`tab`/`language`, so each reuses this snapshot through a
                // lightweight runtime context instead of re-descending the tree (and
                // without the server-capability lookup a full context would perform).
                let key_syntax =
                    tab_with_adapter.map(|(t, adapter)| adapter.syntax().syntax_facts(t));
                let (should_complete, should_trigger_signature_help) = {
                    let signature_help_active = self.state.ui.signature_help.is_active();
                    let caps = tab
                        .and_then(|t| t.path.as_ref())
                        .and_then(|path| lsp_server_capabilities_for_path(&self.state, path));

                    let tab_supports_completion = caps.is_none_or(|caps| caps.completion);
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

                        let Some(syntax) = key_syntax.as_ref() else {
                            return false;
                        };
                        let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                            tab.language(),
                            tab,
                            syntax.clone(),
                        );
                        adapter.interaction().completion_triggered_by_insert(
                            &runtime,
                            ch,
                            completion_triggers,
                        )
                    });

                    let should_trigger_signature_help = tab_supports_signature_help
                        && tab_with_adapter.is_some_and(|(tab, adapter)| {
                            let Some(syntax) = key_syntax.as_ref() else {
                                return false;
                            };
                            let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                                tab.language(),
                                tab,
                                syntax.clone(),
                            );
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

                    (should_complete, should_trigger_signature_help)
                };

                if should_complete {
                    if let Some((pane, path, line, column, version)) =
                        lsp_request_target(&self.state)
                    {
                        self.state.ui.hover.clear();
                        self.state.ui.completion.close();
                        self.state.ui.completion.pending_request =
                            Some(self.completion_request_context(
                                pane,
                                path.clone(),
                                version,
                                key_syntax.clone(),
                            ));

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
                                key_syntax.clone().unwrap_or_default(),
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
                        self.state.ui.signature_help = SignatureHelpPopupState::default();
                        state_changed = true;
                    }
                }

                if should_trigger_signature_help {
                    if let Some((pane, path, line, column, version)) =
                        lsp_request_target(&self.state)
                    {
                        self.state.ui.signature_help.visible = false;
                        self.state.ui.signature_help.model = None;
                        self.state.ui.signature_help.request = Some(SignatureHelpRequestContext {
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
                        let Some(syntax) = key_syntax.as_ref() else {
                            return false;
                        };
                        let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                            t.language(),
                            t,
                            syntax.clone(),
                        );
                        adapter
                            .interaction()
                            .signature_help_should_keep_open(&runtime)
                    })
                {
                    self.state.ui.signature_help = SignatureHelpPopupState::default();
                    state_changed = true;
                }

                return DispatchResult {
                    effects,
                    state_changed,
                };
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

                // One syntax-tree descent for the whole keystroke; the keep-open / sync
                // policy checks below reuse it via lightweight runtime contexts.
                let key_syntax = self
                    .state
                    .editor
                    .pane(pane)
                    .and_then(|p| p.active_tab())
                    .map(|tab| adapter_for_tab(tab).syntax().syntax_facts(tab));

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
                    let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                        tab.language(),
                        tab,
                        key_syntax.clone().unwrap_or_default(),
                    );
                    let keep_completion_open =
                        adapter.interaction().completion_should_keep_open(&runtime);
                    let keep_signature_help_open = adapter
                        .interaction()
                        .signature_help_should_keep_open(&runtime);

                    if session_ok && !keep_completion_open {
                        if self.state.ui.completion.close() {
                            state_changed = true;
                        }

                        let had_signature_help = self.state.ui.signature_help.is_active();
                        if had_signature_help && !keep_signature_help_open {
                            self.state.ui.signature_help = SignatureHelpPopupState::default();
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
                            key_syntax.clone().unwrap_or_default(),
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
                            let Some(syntax) = key_syntax.as_ref() else {
                                return false;
                            };
                            let adapter = adapter_for_tab(t);
                            let runtime = crate::kernel::language::LanguageRuntimeContext::new(
                                t.language(),
                                t,
                                syntax.clone(),
                            );
                            adapter
                                .interaction()
                                .signature_help_should_keep_open(&runtime)
                        });
                    if !keep {
                        self.state.ui.signature_help = SignatureHelpPopupState::default();
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
                        self.state.ui.signature_help = SignatureHelpPopupState::default();
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
