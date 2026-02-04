use crate::core::Command;
use crate::kernel::services::ports::{
    LspInsertTextFormat, LspPosition, LspRange, LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
};

#[cfg(test)]
use crate::kernel::services::ports::{LspCompletionItem, LspPositionEncoding};

mod completion;
mod context_menu;
mod explorer;
mod git;
mod lsp;
mod search;
mod semantic;
mod terminal;
mod util;

use completion::{
    apply_completion_insertion_cursor, completion_should_keep_open, completion_triggered_by_insert,
    should_close_completion_on_command, should_close_completion_on_editor_action,
    signature_help_closed_by_insert, signature_help_should_keep_open,
    signature_help_triggered_by_insert, sync_completion_items_from_cache, CompletionInsertion,
};

#[cfg(test)]
use completion::expand_snippet;
use lsp::{
    cursor_is_identifier, lsp_position_encoding, lsp_position_from_buffer_pos,
    lsp_position_from_char_offset, lsp_position_from_cursor, lsp_position_to_byte_offset,
    lsp_range_for_full_lines, lsp_request_target, problem_byte_offset,
};
use search::search_open_target;
use util::{
    bottom_panel_tabs, find_open_tab, is_rust_source_path, next_bottom_panel_tab,
    prev_bottom_panel_tab, search_viewport_for_focus,
};

use super::{
    Action, AppState, BottomPanelTab, EditorAction, Effect, FocusTarget, InputDialogKind,
    SidebarTab, SplitDirection,
};

pub struct DispatchResult {
    pub effects: Vec<Effect>,
    pub state_changed: bool,
}

pub struct Store {
    state: AppState,
}

impl Store {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn tick(&mut self) {
        for pane in &mut self.state.editor.panes {
            for tab in &mut pane.tabs {
                tab.history.tick();
            }
        }
    }

    fn maybe_close_empty_editor_split(&mut self, effects: &mut Vec<Effect>) -> bool {
        // Currently zcode supports at most a single editor split (2 panes).
        if self.state.ui.editor_layout.panes != 2 {
            return false;
        }
        if self.state.editor.panes.len() < 2 {
            return false;
        }

        let pane0_empty = self.state.editor.panes[0].tabs.is_empty();
        let pane1_empty = self.state.editor.panes[1].tabs.is_empty();
        if !pane0_empty && !pane1_empty {
            return false;
        }

        // Keep the non-empty pane in slot 0 before truncating.
        if pane0_empty && !pane1_empty {
            self.state.editor.panes.swap(0, 1);
        }

        self.state.ui.editor_layout.panes = 1;
        self.state.ui.editor_layout.active_pane = 0;
        self.state.ui.editor_layout.split_direction = SplitDirection::Vertical;
        self.state.ui.focus = FocusTarget::Editor;

        let _ = self.state.editor.ensure_panes(1);

        // Drop any UI state tied to the removed pane(s) to avoid stale indices.
        if self
            .state
            .ui
            .hovered_tab
            .is_some_and(|(pane, _)| pane >= 1)
        {
            self.state.ui.hovered_tab = None;
        }
        if self
            .state
            .ui
            .pending_editor_nav
            .as_ref()
            .is_some_and(|nav| nav.pane >= 1)
        {
            self.state.ui.pending_editor_nav = None;
        }

        let should_close_context_menu = self
            .state
            .ui
            .context_menu
            .request
            .as_ref()
            .is_some_and(|req| {
                matches!(
                    req,
                    super::state::ContextMenuRequest::Tab { pane, .. }
                        | super::state::ContextMenuRequest::EditorArea { pane }
                        if *pane >= 1
                )
            });
        if should_close_context_menu {
            self.state.ui.context_menu = super::state::ContextMenuState::default();
        }

        let should_close_confirm = self
            .state
            .ui
            .confirm_dialog
            .on_confirm
            .as_ref()
            .is_some_and(|pending| matches!(pending, super::state::PendingAction::CloseTab { pane, .. } if *pane >= 1));
        if should_close_confirm {
            self.state.ui.confirm_dialog = super::state::ConfirmDialogState::default();
        }

        // Popups are positioned relative to the editor pane; reset them after collapsing.
        self.state.ui.signature_help = super::state::SignatureHelpPopupState::default();
        self.state.ui.completion = super::state::CompletionPopupState::default();

        if let Some(repo_root) = self.state.git.repo_root.clone() {
            let path = self
                .state
                .editor
                .pane(0)
                .and_then(|pane_state| pane_state.active_tab())
                .and_then(|tab| tab.path.clone());
            if let Some(path) = path {
                effects.push(Effect::GitRefreshStatus {
                    repo_root: repo_root.clone(),
                });
                effects.push(Effect::GitRefreshDiff { repo_root, path });
            }
        }

        true
    }

    pub fn dispatch(&mut self, action: Action) -> DispatchResult {
        match action {
            Action::RunCommand(cmd) => {
                let completion_changed = if should_close_completion_on_command(&cmd) {
                    let has_completion = self.state.ui.completion.visible
                        || self.state.ui.completion.request.is_some()
                        || self.state.ui.completion.pending_request.is_some()
                        || !self.state.ui.completion.all_items.is_empty()
                        || !self.state.ui.completion.items.is_empty();
                    if has_completion {
                        self.state.ui.completion = super::state::CompletionPopupState::default();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                let mut result = self.dispatch_command(cmd);
                result.state_changed |= completion_changed;
                result
            }
            Action::Editor(editor_action) => {
                let completion_changed = if should_close_completion_on_editor_action(&editor_action)
                {
                    let has_completion = self.state.ui.completion.visible
                        || self.state.ui.completion.request.is_some()
                        || self.state.ui.completion.pending_request.is_some()
                        || !self.state.ui.completion.all_items.is_empty()
                        || !self.state.ui.completion.items.is_empty();
                    if has_completion {
                        self.state.ui.completion = super::state::CompletionPopupState::default();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                let should_auto_close_editor_split = matches!(
                    &editor_action,
                    EditorAction::CloseTabAt { .. } | EditorAction::MoveTab { .. }
                );

                let mut result =
                    match editor_action {
                        EditorAction::OpenFile {
                            pane,
                            path,
                            content,
                        } => {
                            let opened_path = path.clone();
                            let opened_path_for_git = opened_path.clone();
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

                            let supports_semantic_tokens = self
                                .state
                                .lsp
                                .server_capabilities
                                .as_ref()
                                .is_none_or(|c| c.semantic_tokens);
                            let supports_inlay_hints = self
                                .state
                                .lsp
                                .server_capabilities
                                .as_ref()
                                .is_none_or(|c| c.inlay_hints);
                            let supports_folding_range = self
                                .state
                                .lsp
                                .server_capabilities
                                .as_ref()
                                .is_none_or(|c| c.folding_range);
                            if (supports_semantic_tokens
                                || supports_inlay_hints
                                || supports_folding_range)
                                && is_rust_source_path(&opened_path)
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
                                let encoding = lsp_position_encoding(&self.state);

                                if supports_semantic_tokens {
                                    let use_range = self
                                        .state
                                        .lsp
                                        .server_capabilities
                                        .as_ref()
                                        .is_none_or(|c| c.semantic_tokens_range)
                                        && tab.buffer.len_lines().max(1) >= 2000;

                                    if use_range {
                                        let total_lines = tab.buffer.len_lines().max(1);
                                        let viewport_top = tab
                                            .viewport
                                            .line_offset
                                            .min(total_lines.saturating_sub(1));
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
                                    } else {
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

                            if let Some(repo_root) = self.state.git.repo_root.clone() {
                                effects.push(Effect::GitRefreshStatus {
                                    repo_root: repo_root.clone(),
                                });
                                effects.push(Effect::GitRefreshDiff {
                                    repo_root,
                                    path: opened_path_for_git,
                                });
                            }

                            DispatchResult {
                                effects,
                                state_changed,
                            }
                        }
                        EditorAction::SetActiveTab { pane, index } => {
                            let (state_changed, mut effects) = self
                                .state
                                .editor
                                .dispatch_action(EditorAction::SetActiveTab { pane, index });

                            if let Some(repo_root) = self.state.git.repo_root.clone() {
                                let path = self
                                    .state
                                    .editor
                                    .pane(pane)
                                    .and_then(|pane_state| pane_state.active_tab())
                                    .and_then(|tab| tab.path.clone());
                                if let Some(path) = path {
                                    effects.push(Effect::GitRefreshStatus {
                                        repo_root: repo_root.clone(),
                                    });
                                    effects.push(Effect::GitRefreshDiff { repo_root, path });
                                }
                            }

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
                            let saved_path = path.clone();
                            let (state_changed, mut effects) =
                                self.state.editor.dispatch_action(EditorAction::Saved {
                                    pane,
                                    path,
                                    success,
                                    version,
                                });

                            if success {
                                if let Some(repo_root) = self.state.git.repo_root.clone() {
                                    effects.push(Effect::GitRefreshStatus {
                                        repo_root: repo_root.clone(),
                                    });
                                    effects.push(Effect::GitRefreshDiff {
                                        repo_root,
                                        path: saved_path,
                                    });
                                }
                            }

                            DispatchResult {
                                effects,
                                state_changed,
                            }
                        }
                        EditorAction::CloseTabAt { pane, index } => {
                            let (state_changed, mut effects) = self
                                .state
                                .editor
                                .dispatch_action(EditorAction::CloseTabAt { pane, index });

                            if let Some(repo_root) = self.state.git.repo_root.clone() {
                                let path = self
                                    .state
                                    .editor
                                    .pane(pane)
                                    .and_then(|pane_state| pane_state.active_tab())
                                    .and_then(|tab| tab.path.clone());
                                if let Some(path) = path {
                                    effects.push(Effect::GitRefreshStatus {
                                        repo_root: repo_root.clone(),
                                    });
                                    effects.push(Effect::GitRefreshDiff { repo_root, path });
                                }
                            }

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

                let editor_changed = result.state_changed;
                result.state_changed |= completion_changed;
                if should_auto_close_editor_split && editor_changed {
                    let collapsed = self.maybe_close_empty_editor_split(&mut result.effects);
                    result.state_changed |= collapsed;
                }
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
            action @ Action::GitInit
            | action @ Action::GitRepoDetected { .. }
            | action @ Action::GitRepoCleared
            | action @ Action::GitStatusUpdated { .. }
            | action @ Action::GitDiffUpdated { .. }
            | action @ Action::GitWorktreesUpdated { .. }
            | action @ Action::GitBranchesUpdated { .. }
            | action @ Action::GitWorktreeResolved { .. }
            | action @ Action::GitCheckoutBranch { .. } => self.reduce_git_action(action),
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
            Action::InputDialogAppend(ch) => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                dialog.error = None;
                if dialog.cursor > dialog.value.len() {
                    dialog.cursor = dialog.value.len();
                }
                dialog.value.insert(dialog.cursor, ch);
                dialog.cursor += ch.len_utf8();
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::InputDialogBackspace => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible || dialog.cursor == 0 {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                dialog.error = None;
                let prev = dialog.value[..dialog.cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                dialog.value.drain(prev..dialog.cursor);
                dialog.cursor = prev;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::InputDialogCursorLeft => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible || dialog.cursor == 0 {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let prev = dialog.value[..dialog.cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let changed = prev != dialog.cursor;
                dialog.cursor = prev;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::InputDialogCursorRight => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible || dialog.cursor >= dialog.value.len() {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let next = dialog.value[dialog.cursor..]
                    .chars()
                    .next()
                    .map(|ch| dialog.cursor + ch.len_utf8())
                    .unwrap_or(dialog.value.len());
                let changed = next != dialog.cursor;
                dialog.cursor = next;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::InputDialogAccept => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let Some(kind) = dialog.kind.as_ref() else {
                    dialog.visible = false;
                    dialog.title.clear();
                    dialog.value.clear();
                    dialog.cursor = 0;
                    dialog.error = None;
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                let value = dialog.value.trim();
                match kind {
                    InputDialogKind::NewFile { .. }
                    | InputDialogKind::NewFolder { .. }
                    | InputDialogKind::ExplorerRename { .. } => {
                        if value.is_empty() {
                            let prev = dialog.error.replace("Name required".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                        if value.contains('/')
                            || value.contains('\\')
                            || value == "."
                            || value == ".."
                        {
                            let prev = dialog.error.replace("Invalid name".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                    InputDialogKind::LspRename { .. } => {
                        if value.is_empty() {
                            let prev = dialog.error.replace("Name required".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                        if value.chars().any(|ch| ch.is_whitespace()) {
                            let prev = dialog
                                .error
                                .replace("Name cannot contain spaces".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                    InputDialogKind::LspWorkspaceSymbols => {
                        if value.is_empty() {
                            let prev = dialog.error.replace("Query required".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                    InputDialogKind::GitWorktreeAdd { .. } => {
                        if value.is_empty() {
                            let prev = dialog.error.replace("Branch required".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                        if value.chars().any(|ch| ch.is_whitespace()) {
                            let prev = dialog
                                .error
                                .replace("Branch cannot contain spaces".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                        if value.contains('\\') || value.contains("..") || value.starts_with('/') {
                            let prev = dialog.error.replace("Invalid branch".to_string());
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                }

                let value = value.to_string();
                let kind = dialog.kind.take();
                dialog.visible = false;
                dialog.title.clear();
                dialog.value.clear();
                dialog.cursor = 0;
                dialog.error = None;

                let Some(kind) = kind else {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                let effect = match kind {
                    InputDialogKind::NewFile { parent_dir } => {
                        Effect::CreateFile(parent_dir.join(&value))
                    }
                    InputDialogKind::NewFolder { parent_dir } => {
                        Effect::CreateDir(parent_dir.join(&value))
                    }
                    InputDialogKind::ExplorerRename { from } => {
                        let Some(parent) = from.parent() else {
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: true,
                            };
                        };
                        let to = parent.join(&value);
                        if to == from {
                            return DispatchResult {
                                effects: Vec::new(),
                                state_changed: true,
                            };
                        }
                        Effect::RenamePath { from, to }
                    }
                    InputDialogKind::LspRename { path, line, column } => Effect::LspRenameRequest {
                        path,
                        line,
                        column,
                        new_name: value,
                    },
                    InputDialogKind::LspWorkspaceSymbols => {
                        let _ = self.state.symbols.clear();
                        self.state.ui.bottom_panel.visible = true;
                        self.state.ui.bottom_panel.active_tab = BottomPanelTab::Symbols;
                        self.state.ui.focus = FocusTarget::BottomPanel;
                        Effect::LspWorkspaceSymbolsRequest { query: value }
                    }
                    InputDialogKind::GitWorktreeAdd { repo_root } => {
                        let branch = value
                            .strip_prefix("refs/heads/")
                            .unwrap_or(value.as_str())
                            .to_string();
                        Effect::GitWorktreeAdd { repo_root, branch }
                    }
                };

                DispatchResult {
                    effects: vec![effect],
                    state_changed: true,
                }
            }
            Action::InputDialogCancel => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                dialog.visible = false;
                dialog.title.clear();
                dialog.value.clear();
                dialog.cursor = 0;
                dialog.error = None;
                dialog.kind = None;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::EditorSetActivePane { pane } => {
                let completion_changed = {
                    let had = self.state.ui.completion.visible
                        || self.state.ui.completion.request.is_some()
                        || self.state.ui.completion.pending_request.is_some()
                        || !self.state.ui.completion.all_items.is_empty()
                        || !self.state.ui.completion.items.is_empty();
                    if had {
                        self.state.ui.completion = super::state::CompletionPopupState::default();
                    }
                    had
                };

                let panes = self.state.ui.editor_layout.panes.max(1);
                let pane = pane.min(panes - 1);
                let prev = self.state.ui.editor_layout.active_pane;
                let prev_focus = self.state.ui.focus;

                self.state.ui.editor_layout.active_pane = pane;
                self.state.ui.focus = FocusTarget::Editor;

                let mut effects = Vec::new();
                if pane != prev {
                    if let Some(repo_root) = self.state.git.repo_root.clone() {
                        let path = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.active_tab())
                            .and_then(|tab| tab.path.clone());
                        if let Some(path) = path {
                            effects.push(Effect::GitRefreshStatus {
                                repo_root: repo_root.clone(),
                            });
                            effects.push(Effect::GitRefreshDiff { repo_root, path });
                        }
                    }
                }

                DispatchResult {
                    effects,
                    state_changed: pane != prev
                        || prev_focus != FocusTarget::Editor
                        || completion_changed,
                }
            }
            Action::EditorSetSplitRatio { ratio } => {
                if self.state.ui.editor_layout.panes < 2 {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let ratio = ratio.clamp(100, 900);
                let prev = self.state.ui.editor_layout.split_ratio;
                self.state.ui.editor_layout.split_ratio = ratio;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: ratio != prev,
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
            Action::BottomPanelSetActiveTab { tab } => {
                let prev_visible = self.state.ui.bottom_panel.visible;
                let prev = self.state.ui.bottom_panel.active_tab.clone();
                let next = tab.clone();
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.bottom_panel.active_tab = tab;
                let mut effects = Vec::new();
                let mut state_changed = !prev_visible || prev != next;
                if next == BottomPanelTab::Terminal {
                    let (changed, terminal_effects) = self.ensure_terminal_session();
                    state_changed |= changed;
                    effects.extend(terminal_effects);
                }
                DispatchResult {
                    effects,
                    state_changed,
                }
            }
            action @ Action::SearchSetViewHeight { .. }
            | action @ Action::SearchAppend(_)
            | action @ Action::SearchBackspace
            | action @ Action::SearchCursorLeft
            | action @ Action::SearchCursorRight
            | action @ Action::SearchToggleCaseSensitive
            | action @ Action::SearchToggleRegex
            | action @ Action::SearchMoveSelection { .. }
            | action @ Action::SearchScroll { .. }
            | action @ Action::SearchClickRow { .. }
            | action @ Action::SearchStart
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
            action @ Action::TerminalWrite { .. }
            | action @ Action::TerminalResize { .. }
            | action @ Action::TerminalScroll { .. }
            | action @ Action::TerminalSpawned { .. }
            | action @ Action::TerminalOutput { .. }
            | action @ Action::TerminalExited { .. } => self.reduce_terminal_action(action),
            action @ Action::LspDiagnostics { .. }
            | action @ Action::LspHover { .. }
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
            Action::CompletionClose => {
                let had = self.state.ui.completion.visible
                    || self.state.ui.completion.request.is_some()
                    || self.state.ui.completion.pending_request.is_some()
                    || !self.state.ui.completion.all_items.is_empty()
                    || !self.state.ui.completion.items.is_empty();
                if had {
                    self.state.ui.completion = super::state::CompletionPopupState::default();
                }
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: had,
                }
            }
            Action::CompletionMoveSelection { delta } => {
                if !self.state.ui.completion.visible || self.state.ui.completion.items.is_empty() {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                let len = self.state.ui.completion.items.len();
                let prev = self.state.ui.completion.selected;
                let next = (prev as isize).wrapping_add(delta).rem_euclid(len as isize) as usize;
                self.state.ui.completion.selected = next;

                let mut effects = Vec::new();
                if next != prev {
                    if let Some(item) = self.state.ui.completion.items.get(next) {
                        if self
                            .state
                            .lsp
                            .server_capabilities
                            .as_ref()
                            .is_none_or(|c| c.completion_resolve)
                            && item.data.is_some()
                            && item
                                .documentation
                                .as_ref()
                                .is_none_or(|d| d.trim().is_empty())
                            && self.state.ui.completion.resolve_inflight != Some(item.id)
                        {
                            self.state.ui.completion.resolve_inflight = Some(item.id);
                            effects
                                .push(Effect::LspCompletionResolveRequest { item: item.clone() });
                        }
                    }
                }
                DispatchResult {
                    effects,
                    state_changed: next != prev,
                }
            }
            Action::CompletionConfirm => {
                if !self.state.ui.completion.visible || self.state.ui.completion.items.is_empty() {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let Some(req) = self.state.ui.completion.request.clone() else {
                    let had = self.state.ui.completion.visible
                        || self.state.ui.completion.request.is_some()
                        || self.state.ui.completion.pending_request.is_some()
                        || !self.state.ui.completion.all_items.is_empty()
                        || !self.state.ui.completion.items.is_empty();
                    self.state.ui.completion = super::state::CompletionPopupState::default();
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
                    let had = self.state.ui.completion.visible
                        || self.state.ui.completion.request.is_some()
                        || self.state.ui.completion.pending_request.is_some()
                        || !self.state.ui.completion.all_items.is_empty()
                        || !self.state.ui.completion.items.is_empty();
                    self.state.ui.completion = super::state::CompletionPopupState::default();
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                };

                let valid = tab.path.as_ref() == Some(&req.path);
                if !valid {
                    let had = self.state.ui.completion.visible
                        || self.state.ui.completion.request.is_some()
                        || self.state.ui.completion.pending_request.is_some()
                        || !self.state.ui.completion.all_items.is_empty()
                        || !self.state.ui.completion.items.is_empty();
                    self.state.ui.completion = super::state::CompletionPopupState::default();
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                }

                let selected = self
                    .state
                    .ui
                    .completion
                    .selected
                    .min(self.state.ui.completion.items.len().saturating_sub(1));
                let item = self.state.ui.completion.items[selected].clone();
                let insertion = match item.insert_text_format {
                    LspInsertTextFormat::PlainText => {
                        CompletionInsertion::from_plain_text(item.insert_text.clone())
                    }
                    LspInsertTextFormat::Snippet => {
                        CompletionInsertion::from_snippet(&item.insert_text)
                    }
                };

                let encoding = lsp_position_encoding(&self.state);
                let compute_range = || {
                    let (row, col) = tab.buffer.cursor();
                    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
                    let rope = tab.buffer.rope();
                    let end_char = cursor_char_offset.min(rope.len_chars());

                    let mut start_char = end_char;
                    while start_char > 0 {
                        let ch = rope.char(start_char - 1);
                        if ch.is_ascii_alphanumeric() || ch == '_' {
                            start_char = start_char.saturating_sub(1);
                        } else {
                            break;
                        }
                    }

                    LspRange {
                        start: lsp_position_from_char_offset(tab, start_char, encoding),
                        end: lsp_position_from_char_offset(tab, end_char, encoding),
                    }
                };
                let replace_range = if tab.edit_version == req.version {
                    item.replace_range.unwrap_or_else(compute_range)
                } else {
                    compute_range()
                };

                self.state.ui.completion = super::state::CompletionPopupState::default();

                let mut edits = item.additional_text_edits.clone();
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

                if insertion.has_cursor_or_selection() {
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
                                apply_completion_insertion_cursor(tab, &insertion, tab_size);
                            }
                        }
                    }
                }

                if let Some(cmd) = item.command {
                    effects.push(Effect::LspExecuteCommand {
                        command: cmd.command,
                        arguments: cmd.arguments,
                    });
                }

                DispatchResult {
                    effects,
                    state_changed: true,
                }
            }
            Action::DirLoaded { path, entries } => DispatchResult {
                effects: Vec::new(),
                state_changed: {
                    let changed = self.state.explorer.apply_dir_loaded(path, entries);
                    let git_changed = if changed {
                        self.state
                            .explorer
                            .set_git_statuses(&self.state.git.file_status)
                    } else {
                        false
                    };
                    changed || git_changed
                },
            },
            Action::DirLoadError { path } => DispatchResult {
                effects: Vec::new(),
                state_changed: {
                    let changed = self.state.explorer.apply_dir_load_error(path);
                    let git_changed = if changed {
                        self.state
                            .explorer
                            .set_git_statuses(&self.state.git.file_status)
                    } else {
                        false
                    };
                    changed || git_changed
                },
            },
            Action::ExplorerPathCreated { path, is_dir } => DispatchResult {
                effects: self
                    .state
                    .git
                    .repo_root
                    .clone()
                    .map(|repo_root| vec![Effect::GitRefreshStatus { repo_root }])
                    .unwrap_or_default(),
                state_changed: {
                    let changed = self.state.explorer.apply_path_created(path, is_dir);
                    let git_changed = if changed {
                        self.state
                            .explorer
                            .set_git_statuses(&self.state.git.file_status)
                    } else {
                        false
                    };
                    changed || git_changed
                },
            },
            Action::ExplorerPathDeleted { path } => DispatchResult {
                effects: self
                    .state
                    .git
                    .repo_root
                    .clone()
                    .map(|repo_root| vec![Effect::GitRefreshStatus { repo_root }])
                    .unwrap_or_default(),
                state_changed: {
                    let changed = self.state.explorer.apply_path_deleted(path);
                    let git_changed = if changed {
                        self.state
                            .explorer
                            .set_git_statuses(&self.state.git.file_status)
                    } else {
                        false
                    };
                    changed || git_changed
                },
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

                if state_changed {
                    state_changed |= self
                        .state
                        .explorer
                        .set_git_statuses(&self.state.git.file_status);
                }

                DispatchResult {
                    effects: if state_changed {
                        if let Some(repo_root) = self.state.git.repo_root.clone() {
                            vec![Effect::GitRefreshStatus { repo_root }]
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    },
                    state_changed,
                }
            }
            Action::PaletteAppend(ch) => {
                if !self.state.ui.command_palette.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                self.state.ui.command_palette.query.push(ch);
                self.state.ui.command_palette.selected = 0;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::PaletteBackspace => {
                if !self.state.ui.command_palette.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let removed = self.state.ui.command_palette.query.pop().is_some();
                if removed {
                    self.state.ui.command_palette.selected = 0;
                }
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: removed,
                }
            }
            Action::PaletteMoveSelection(delta) => {
                if !self.state.ui.command_palette.visible || delta == 0 {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let selected = &mut self.state.ui.command_palette.selected;
                if delta > 0 {
                    *selected = selected.saturating_add(delta as usize);
                } else {
                    *selected = selected.saturating_sub((-delta) as usize);
                }

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::PaletteClose => {
                if !self.state.ui.command_palette.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                self.state.ui.command_palette.visible = false;
                self.state.ui.command_palette.query.clear();
                self.state.ui.command_palette.selected = 0;
                if self.state.ui.focus == FocusTarget::CommandPalette {
                    self.state.ui.focus = FocusTarget::Editor;
                }

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
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
                            let (_changed, effects) = self.state.editor.close_tab_at(pane, index);
                            return DispatchResult {
                                effects,
                                state_changed: true,
                            };
                        }
                        super::PendingAction::DeletePath { path, is_dir } => {
                            return DispatchResult {
                                effects: vec![Effect::DeletePath { path, is_dir }],
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
                if self.state.ui.command_palette.visible {
                    self.state.ui.command_palette.visible = false;
                    self.state.ui.command_palette.query.clear();
                    self.state.ui.command_palette.selected = 0;
                    if self.state.ui.focus == FocusTarget::CommandPalette {
                        self.state.ui.focus = FocusTarget::Editor;
                    }

                    return DispatchResult {
                        effects,
                        state_changed: true,
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

                return DispatchResult {
                    effects: vec![Effect::OpenSettings],
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
            Command::GitWorktreeAdd => {
                if self.state.ui.input_dialog.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let Some(repo_root) = self.state.git.repo_root.clone() else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };

                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Git Worktree".to_string();
                self.state.ui.input_dialog.value.clear();
                self.state.ui.input_dialog.cursor = 0;
                self.state.ui.input_dialog.error = None;
                self.state.ui.input_dialog.kind =
                    Some(InputDialogKind::GitWorktreeAdd { repo_root });
                state_changed = true;
            }
            Command::GitTogglePanel => {
                let prev = self.state.ui.git_panel_expanded;
                self.state.ui.git_panel_expanded = !prev;

                if !self.state.ui.sidebar_visible {
                    self.state.ui.sidebar_visible = true;
                }
                if self.state.ui.sidebar_tab != SidebarTab::Explorer {
                    self.state.ui.sidebar_tab = SidebarTab::Explorer;
                }
                if self.state.ui.focus != FocusTarget::Explorer {
                    self.state.ui.focus = FocusTarget::Explorer;
                }

                state_changed = true;
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

                let supports_completion = self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.completion);
                let tab = self
                    .state
                    .editor
                    .pane(pane)
                    .and_then(|pane| pane.active_tab());
                let should_complete = supports_completion
                    && tab.is_some_and(|tab| {
                        let Some(path) = tab.path.as_ref() else {
                            return false;
                        };
                        if !is_rust_source_path(path) {
                            return false;
                        }

                        let triggers = self
                            .state
                            .lsp
                            .server_capabilities
                            .as_ref()
                            .map(|c| c.completion_triggers.as_slice())
                            .unwrap_or(&[]);
                        completion_triggered_by_insert(tab, ch, triggers)
                    });

                if should_complete {
                    if let Some((pane, path, line, column, version)) =
                        lsp_request_target(&self.state)
                    {
                        self.state.ui.hover_message = None;
                        self.state.ui.completion.visible = false;
                        self.state.ui.completion.items.clear();
                        self.state.ui.completion.all_items.clear();
                        self.state.ui.completion.selected = 0;
                        self.state.ui.completion.request = None;
                        self.state.ui.completion.is_incomplete = false;
                        self.state.ui.completion.resolve_inflight = None;
                        self.state.ui.completion.session_started_at = None;
                        self.state.ui.completion.pending_request =
                            Some(super::state::CompletionRequestContext {
                                pane,
                                path: path.clone(),
                                version,
                            });

                        effects.push(Effect::LspCompletionRequest { path, line, column });
                        state_changed = true;
                    }
                }
                if !should_complete
                    && self.state.ui.completion.visible
                    && !self.state.ui.completion.all_items.is_empty()
                {
                    if let Some(tab) = tab {
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
                            let mut changed = sync_completion_items_from_cache(
                                &mut self.state.ui.completion,
                                tab,
                            );

                            let selected = self
                                .state
                                .ui
                                .completion
                                .selected
                                .min(self.state.ui.completion.items.len().saturating_sub(1));
                            if let Some(item) = self.state.ui.completion.items.get(selected) {
                                if self
                                    .state
                                    .lsp
                                    .server_capabilities
                                    .as_ref()
                                    .is_none_or(|c| c.completion_resolve)
                                    && item.data.is_some()
                                    && item
                                        .documentation
                                        .as_ref()
                                        .is_none_or(|d| d.trim().is_empty())
                                    && self.state.ui.completion.resolve_inflight != Some(item.id)
                                {
                                    self.state.ui.completion.resolve_inflight = Some(item.id);
                                    effects.push(Effect::LspCompletionResolveRequest {
                                        item: item.clone(),
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

                if signature_help_closed_by_insert(ch) {
                    let had = self.state.ui.signature_help.visible
                        || self.state.ui.signature_help.request.is_some()
                        || !self.state.ui.signature_help.text.is_empty();
                    if had {
                        self.state.ui.signature_help =
                            super::state::SignatureHelpPopupState::default();
                        state_changed = true;
                    }
                }

                let supports_signature_help = self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.signature_help);
                if supports_signature_help {
                    let triggers = self
                        .state
                        .lsp
                        .server_capabilities
                        .as_ref()
                        .map(|c| c.signature_help_triggers.as_slice())
                        .unwrap_or(&[]);
                    if signature_help_triggered_by_insert(ch, triggers) {
                        if let Some((pane, path, line, column, version)) =
                            lsp_request_target(&self.state)
                        {
                            self.state.ui.signature_help.visible = false;
                            self.state.ui.signature_help.text.clear();
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
                }

                let had_signature_help = self.state.ui.signature_help.visible
                    || self.state.ui.signature_help.request.is_some()
                    || !self.state.ui.signature_help.text.is_empty();
                if had_signature_help && !tab.is_some_and(signature_help_should_keep_open) {
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
                self.state.ui.sidebar_tab = SidebarTab::Explorer;
                state_changed = true;
            }
            Command::FocusSearch => {
                self.state.ui.focus = FocusTarget::Explorer;
                self.state.ui.sidebar_visible = true;
                self.state.ui.sidebar_tab = SidebarTab::Search;
                state_changed = true;
            }
            Command::ToggleSidebarTab => {
                self.state.ui.focus = FocusTarget::Explorer;
                self.state.ui.sidebar_visible = true;
                self.state.ui.sidebar_tab = match self.state.ui.sidebar_tab {
                    SidebarTab::Explorer => SidebarTab::Search,
                    SidebarTab::Search => SidebarTab::Explorer,
                };
                state_changed = true;
            }
            Command::FocusEditor => {
                self.state.ui.focus = FocusTarget::Editor;
                state_changed = true;
            }
            Command::SplitEditorVertical => {
                let prev_dir = self.state.ui.editor_layout.split_direction;
                let prev_focus = self.state.ui.focus;
                self.state.ui.editor_layout.split_direction = SplitDirection::Vertical;
                if self.state.ui.editor_layout.panes < 2 {
                    self.state.ui.editor_layout.panes = 2;
                    self.state.ui.editor_layout.active_pane =
                        self.state.ui.editor_layout.active_pane.min(1);
                    self.state.ui.focus = FocusTarget::Editor;
                    let panes = self.state.ui.editor_layout.panes;
                    let _ = self.state.editor.ensure_panes(panes);
                    state_changed = true;
                } else {
                    self.state.ui.focus = FocusTarget::Editor;
                    state_changed =
                        prev_dir != SplitDirection::Vertical || prev_focus != FocusTarget::Editor;
                }

                if state_changed {
                    let pane = self.state.ui.editor_layout.active_pane;
                    let mut effects = effects;
                    if let Some(repo_root) = self.state.git.repo_root.clone() {
                        let path = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.active_tab())
                            .and_then(|tab| tab.path.clone());
                        if let Some(path) = path {
                            effects.push(Effect::GitRefreshStatus {
                                repo_root: repo_root.clone(),
                            });
                            effects.push(Effect::GitRefreshDiff { repo_root, path });
                        }
                    }
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
            }
            Command::SplitEditorHorizontal => {
                let prev_dir = self.state.ui.editor_layout.split_direction;
                let prev_focus = self.state.ui.focus;
                self.state.ui.editor_layout.split_direction = SplitDirection::Horizontal;
                if self.state.ui.editor_layout.panes < 2 {
                    self.state.ui.editor_layout.panes = 2;
                    self.state.ui.editor_layout.active_pane =
                        self.state.ui.editor_layout.active_pane.min(1);
                    self.state.ui.focus = FocusTarget::Editor;
                    let panes = self.state.ui.editor_layout.panes;
                    let _ = self.state.editor.ensure_panes(panes);
                    state_changed = true;
                } else {
                    self.state.ui.focus = FocusTarget::Editor;
                    state_changed =
                        prev_dir != SplitDirection::Horizontal || prev_focus != FocusTarget::Editor;
                }

                if state_changed {
                    let pane = self.state.ui.editor_layout.active_pane;
                    let mut effects = effects;
                    if let Some(repo_root) = self.state.git.repo_root.clone() {
                        let path = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.active_tab())
                            .and_then(|tab| tab.path.clone());
                        if let Some(path) = path {
                            effects.push(Effect::GitRefreshStatus {
                                repo_root: repo_root.clone(),
                            });
                            effects.push(Effect::GitRefreshDiff { repo_root, path });
                        }
                    }
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
            }
            Command::CloseEditorSplit => {
                if self.state.ui.editor_layout.panes > 1 {
                    self.state.ui.editor_layout.panes = 1;
                    self.state.ui.editor_layout.active_pane = 0;
                    self.state.ui.editor_layout.split_direction = SplitDirection::Vertical;
                    self.state.ui.focus = FocusTarget::Editor;
                    let panes = self.state.ui.editor_layout.panes;
                    let _ = self.state.editor.ensure_panes(panes);
                    state_changed = true;

                    let pane = self.state.ui.editor_layout.active_pane;
                    let mut effects = effects;
                    if let Some(repo_root) = self.state.git.repo_root.clone() {
                        let path = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.active_tab())
                            .and_then(|tab| tab.path.clone());
                        if let Some(path) = path {
                            effects.push(Effect::GitRefreshStatus {
                                repo_root: repo_root.clone(),
                            });
                            effects.push(Effect::GitRefreshDiff { repo_root, path });
                        }
                    }
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
            }
            Command::FocusNextEditorPane => {
                let panes = self.state.ui.editor_layout.panes.max(1);
                if panes > 1 {
                    self.state.ui.editor_layout.active_pane =
                        (self.state.ui.editor_layout.active_pane + 1) % panes;
                    self.state.ui.focus = FocusTarget::Editor;
                    state_changed = true;
                }

                if !state_changed {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let pane = self.state.ui.editor_layout.active_pane;
                let mut effects = effects;
                if let Some(repo_root) = self.state.git.repo_root.clone() {
                    let path = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|pane_state| pane_state.active_tab())
                        .and_then(|tab| tab.path.clone());
                    if let Some(path) = path {
                        effects.push(Effect::GitRefreshStatus {
                            repo_root: repo_root.clone(),
                        });
                        effects.push(Effect::GitRefreshDiff { repo_root, path });
                    }
                }

                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::FocusPrevEditorPane => {
                let panes = self.state.ui.editor_layout.panes.max(1);
                if panes > 1 {
                    self.state.ui.editor_layout.active_pane =
                        if self.state.ui.editor_layout.active_pane == 0 {
                            panes - 1
                        } else {
                            self.state.ui.editor_layout.active_pane - 1
                        };
                    self.state.ui.focus = FocusTarget::Editor;
                    state_changed = true;
                }

                if !state_changed {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let pane = self.state.ui.editor_layout.active_pane;
                let mut effects = effects;
                if let Some(repo_root) = self.state.git.repo_root.clone() {
                    let path = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|pane_state| pane_state.active_tab())
                        .and_then(|tab| tab.path.clone());
                    if let Some(path) = path {
                        effects.push(Effect::GitRefreshStatus {
                            repo_root: repo_root.clone(),
                        });
                        effects.push(Effect::GitRefreshDiff { repo_root, path });
                    }
                }

                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::ToggleBottomPanel => {
                let visible = !self.state.ui.bottom_panel.visible;
                self.state.ui.bottom_panel.visible = visible;
                if !visible && self.state.ui.focus == FocusTarget::BottomPanel {
                    self.state.ui.focus = FocusTarget::Editor;
                }
                state_changed = true;
                let mut effects = effects;
                if visible && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    let (changed, terminal_effects) = self.ensure_terminal_session();
                    state_changed |= changed;
                    effects.extend(terminal_effects);
                }
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::FocusBottomPanel => {
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.focus = FocusTarget::BottomPanel;
                state_changed = true;
                let mut effects = effects;
                if self.state.ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    let (changed, terminal_effects) = self.ensure_terminal_session();
                    state_changed |= changed;
                    effects.extend(terminal_effects);
                }
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::NextBottomPanelTab => {
                self.state.ui.bottom_panel.visible = true;
                let tabs = bottom_panel_tabs();
                if let Some(next) =
                    next_bottom_panel_tab(&tabs, &self.state.ui.bottom_panel.active_tab)
                {
                    if self.state.ui.bottom_panel.active_tab != next {
                        self.state.ui.bottom_panel.active_tab = next;
                        state_changed = true;
                    }
                }
                let mut effects = effects;
                if self.state.ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    let (changed, terminal_effects) = self.ensure_terminal_session();
                    state_changed |= changed;
                    effects.extend(terminal_effects);
                }
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::PrevBottomPanelTab => {
                self.state.ui.bottom_panel.visible = true;
                let tabs = bottom_panel_tabs();
                if let Some(prev) =
                    prev_bottom_panel_tab(&tabs, &self.state.ui.bottom_panel.active_tab)
                {
                    if self.state.ui.bottom_panel.active_tab != prev {
                        self.state.ui.bottom_panel.active_tab = prev;
                        state_changed = true;
                    }
                }
                let mut effects = effects;
                if self.state.ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    let (changed, terminal_effects) = self.ensure_terminal_session();
                    state_changed |= changed;
                    effects.extend(terminal_effects);
                }
                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            Command::CommandPalette => {
                let visible = !self.state.ui.command_palette.visible;
                self.state.ui.command_palette.visible = visible;
                if visible {
                    self.state.ui.focus = FocusTarget::CommandPalette;
                    self.state.ui.command_palette.query.clear();
                    self.state.ui.command_palette.selected = 0;
                } else if self.state.ui.focus == FocusTarget::CommandPalette {
                    self.state.ui.focus = FocusTarget::Editor;
                    self.state.ui.command_palette.query.clear();
                    self.state.ui.command_palette.selected = 0;
                }
                state_changed = true;
            }
            Command::PaletteClose => {
                if self.state.ui.command_palette.visible {
                    self.state.ui.command_palette.visible = false;
                    self.state.ui.command_palette.query.clear();
                    self.state.ui.command_palette.selected = 0;
                    if self.state.ui.focus == FocusTarget::CommandPalette {
                        self.state.ui.focus = FocusTarget::Editor;
                    }
                    state_changed = true;
                }
            }
            Command::PaletteBackspace => {
                if self.state.ui.command_palette.visible {
                    let removed = self.state.ui.command_palette.query.pop().is_some();
                    if removed {
                        self.state.ui.command_palette.selected = 0;
                        state_changed = true;
                    }
                }
            }
            Command::PaletteMoveUp => {
                if self.state.ui.command_palette.visible {
                    let prev = self.state.ui.command_palette.selected;
                    self.state.ui.command_palette.selected = prev.saturating_sub(1);
                    state_changed = self.state.ui.command_palette.selected != prev;
                }
            }
            Command::PaletteMoveDown => {
                if self.state.ui.command_palette.visible {
                    let prev = self.state.ui.command_palette.selected;
                    self.state.ui.command_palette.selected = prev.saturating_add(1);
                    state_changed = self.state.ui.command_palette.selected != prev;
                }
            }
            Command::PaletteConfirm => {
                if !self.state.ui.command_palette.visible {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

                let query = self.state.ui.command_palette.query.clone();
                let selected_raw = self.state.ui.command_palette.selected;
                let matches = crate::kernel::palette::match_items(&query);

                let palette_closed = true;
                self.state.ui.command_palette.visible = false;
                self.state.ui.command_palette.query.clear();
                self.state.ui.command_palette.selected = 0;
                if self.state.ui.focus == FocusTarget::CommandPalette {
                    self.state.ui.focus = FocusTarget::Editor;
                }

                if matches.is_empty() {
                    return DispatchResult {
                        effects,
                        state_changed: palette_closed,
                    };
                }

                let selected = selected_raw.min(matches.len().saturating_sub(1));
                let cmd = matches[selected].command.clone();

                let mut result = self.dispatch_command(cmd);
                result.state_changed |= palette_closed;
                return result;
            }
            Command::ExplorerUp => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Explorer
                {
                    state_changed = self.state.explorer.move_selection(-1);
                }
            }
            Command::ExplorerDown => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Explorer
                {
                    state_changed = self.state.explorer.move_selection(1);
                }
            }
            Command::ExplorerActivate => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Explorer
                {
                    let (changed, fx) = self.state.explorer.activate_selected();
                    return DispatchResult {
                        effects: fx,
                        state_changed: changed,
                    };
                }
            }
            Command::ExplorerCollapse => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Explorer
                {
                    state_changed = self.state.explorer.collapse_selected();
                }
            }
            Command::ExplorerScrollUp => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Explorer
                {
                    state_changed = self.state.explorer.scroll(-3);
                }
            }
            Command::ExplorerScrollDown => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Explorer
                {
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
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "New File".to_string();
                self.state.ui.input_dialog.value.clear();
                self.state.ui.input_dialog.cursor = 0;
                self.state.ui.input_dialog.error = None;
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
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "New Folder".to_string();
                self.state.ui.input_dialog.value.clear();
                self.state.ui.input_dialog.cursor = 0;
                self.state.ui.input_dialog.error = None;
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
                if path == self.state.workspace_root {
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

                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Rename".to_string();
                self.state.ui.input_dialog.value = file_name;
                self.state.ui.input_dialog.cursor = self.state.ui.input_dialog.value.len();
                self.state.ui.input_dialog.error = None;
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
                if path == self.state.workspace_root {
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
            Command::GlobalSearchStart => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Search
                    && !self.state.search.query.is_empty()
                {
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
            Command::GlobalSearchCursorLeft => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Search
                {
                    state_changed = self.state.search.cursor_left();
                }
            }
            Command::GlobalSearchCursorRight => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Search
                {
                    state_changed = self.state.search.cursor_right();
                }
            }
            Command::GlobalSearchBackspace => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Search
                {
                    state_changed = self.state.search.backspace_query();
                }
            }
            Command::GlobalSearchToggleCaseSensitive => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Search
                {
                    state_changed = self.state.search.toggle_case_sensitive();
                }
            }
            Command::GlobalSearchToggleRegex => {
                if self.state.ui.focus == FocusTarget::Explorer
                    && self.state.ui.sidebar_tab == SidebarTab::Search
                {
                    state_changed = self.state.search.toggle_regex();
                }
            }
            Command::SearchResultsMoveUp => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.move_selection(-1, viewport);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Problems
                {
                    state_changed = self.state.problems.move_selection(-1);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::CodeActions
                {
                    state_changed = self.state.code_actions.move_selection(-1);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Locations
                {
                    state_changed = self.state.locations.move_selection(-1);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Symbols
                {
                    state_changed = self.state.symbols.move_selection(-1);
                }
            }
            Command::SearchResultsMoveDown => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.move_selection(1, viewport);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Problems
                {
                    state_changed = self.state.problems.move_selection(1);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::CodeActions
                {
                    state_changed = self.state.code_actions.move_selection(1);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Locations
                {
                    state_changed = self.state.locations.move_selection(1);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Symbols
                {
                    state_changed = self.state.symbols.move_selection(1);
                }
            }
            Command::SearchResultsScrollUp => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.scroll(-3, viewport);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Problems
                {
                    state_changed = self.state.problems.scroll(-3);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::CodeActions
                {
                    state_changed = self.state.code_actions.scroll(-3);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Locations
                {
                    state_changed = self.state.locations.scroll(-3);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Symbols
                {
                    state_changed = self.state.symbols.scroll(-3);
                }
            }
            Command::SearchResultsScrollDown => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.scroll(3, viewport);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Problems
                {
                    state_changed = self.state.problems.scroll(3);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::CodeActions
                {
                    state_changed = self.state.code_actions.scroll(3);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Locations
                {
                    state_changed = self.state.locations.scroll(3);
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Symbols
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
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Problems
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

                        let byte_offset = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                            .map(|tab| {
                                problem_byte_offset(tab, range, lsp_position_encoding(&self.state))
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
                                line: range.start_line,
                                column: range.start_col,
                            },
                        });

                    return DispatchResult {
                        effects: vec![Effect::LoadFile(path)],
                        state_changed: true,
                    };
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::CodeActions
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
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Locations
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

                        let byte_offset = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                            .map(|tab| {
                                lsp_position_to_byte_offset(
                                    tab,
                                    item.line,
                                    item.column,
                                    lsp_position_encoding(&self.state),
                                )
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
                } else if self.state.ui.focus == FocusTarget::BottomPanel
                    && self.state.ui.bottom_panel.active_tab == BottomPanelTab::Symbols
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

                        let byte_offset = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.tabs.get(tab_index))
                            .map(|tab| {
                                lsp_position_to_byte_offset(
                                    tab,
                                    item.line,
                                    item.column,
                                    lsp_position_encoding(&self.state),
                                )
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
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.hover)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
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
                if !is_rust_source_path(path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }
                if tab.is_in_string_or_comment_at_cursor() || !cursor_is_identifier(tab) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let encoding = lsp_position_encoding(&self.state);
                let (line, column) = lsp_position_from_cursor(tab, encoding);
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
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.definition)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    return DispatchResult {
                        effects: vec![Effect::LspDefinitionRequest { path, line, column }],
                        state_changed,
                    };
                }
            }
            Command::LspCompletion => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.completion)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                if let Some((pane, path, line, column, version)) = lsp_request_target(&self.state) {
                    self.state.ui.hover_message = None;
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

                    let can_reuse = self.state.ui.completion.visible
                        && self
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
                        let mut changed =
                            sync_completion_items_from_cache(&mut self.state.ui.completion, tab);
                        let mut effects = Vec::new();

                        let selected = self
                            .state
                            .ui
                            .completion
                            .selected
                            .min(self.state.ui.completion.items.len().saturating_sub(1));
                        if let Some(item) = self.state.ui.completion.items.get(selected) {
                            if self
                                .state
                                .lsp
                                .server_capabilities
                                .as_ref()
                                .is_none_or(|c| c.completion_resolve)
                                && item.data.is_some()
                                && item
                                    .documentation
                                    .as_ref()
                                    .is_none_or(|d| d.trim().is_empty())
                                && self.state.ui.completion.resolve_inflight != Some(item.id)
                            {
                                self.state.ui.completion.resolve_inflight = Some(item.id);
                                effects.push(Effect::LspCompletionResolveRequest {
                                    item: item.clone(),
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
                        && !self.state.ui.completion.items.is_empty();
                    if !keep_open {
                        self.state.ui.completion.visible = false;
                        self.state.ui.completion.all_items.clear();
                        self.state.ui.completion.items.clear();
                        self.state.ui.completion.selected = 0;
                        self.state.ui.completion.request = None;
                        self.state.ui.completion.is_incomplete = false;
                        self.state.ui.completion.resolve_inflight = None;
                        self.state.ui.completion.session_started_at = None;
                    }
                    self.state.ui.completion.pending_request =
                        Some(super::state::CompletionRequestContext {
                            pane,
                            path: path.clone(),
                            version,
                        });

                    return DispatchResult {
                        effects: vec![Effect::LspCompletionRequest { path, line, column }],
                        state_changed: true,
                    };
                }
            }
            Command::LspSignatureHelp => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.signature_help)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    return DispatchResult {
                        effects: vec![Effect::LspSignatureHelpRequest { path, line, column }],
                        state_changed,
                    };
                }
            }
            Command::LspFormat => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.format)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                if let Some((_pane, path, _line, _column, _version)) =
                    lsp_request_target(&self.state)
                {
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
                if !is_rust_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let selection = tab.buffer.selection().filter(|sel| !sel.is_empty());
                if let Some(selection) = selection {
                    let encoding = lsp_position_encoding(&self.state);
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

                    let supports_range_format = self
                        .state
                        .lsp
                        .server_capabilities
                        .as_ref()
                        .is_none_or(|c| c.range_format);
                    if supports_range_format {
                        return DispatchResult {
                            effects: vec![Effect::LspRangeFormatRequest { path, range }],
                            state_changed,
                        };
                    }

                    let supports_format = self
                        .state
                        .lsp
                        .server_capabilities
                        .as_ref()
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

                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.format)
                {
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
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.rename)
                {
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

                let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };

                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Rename Symbol".to_string();
                self.state.ui.input_dialog.value.clear();
                self.state.ui.input_dialog.cursor = 0;
                self.state.ui.input_dialog.error = None;
                self.state.ui.input_dialog.kind =
                    Some(InputDialogKind::LspRename { path, line, column });
                state_changed = true;
            }
            Command::LspReferences => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.references)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    let mut changed = self.state.locations.clear();

                    let prev_visible = self.state.ui.bottom_panel.visible;
                    let prev_tab = self.state.ui.bottom_panel.active_tab.clone();
                    self.state.ui.bottom_panel.visible = true;
                    self.state.ui.bottom_panel.active_tab = BottomPanelTab::Locations;
                    if !prev_visible || prev_tab != BottomPanelTab::Locations {
                        changed = true;
                    }

                    return DispatchResult {
                        effects: vec![Effect::LspReferencesRequest { path, line, column }],
                        state_changed: state_changed || changed,
                    };
                }
            }
            Command::LspDocumentSymbols => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.document_symbols)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
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
                if !is_rust_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let mut changed = self.state.symbols.clear();

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

                return DispatchResult {
                    effects: vec![Effect::LspDocumentSymbolsRequest { path }],
                    state_changed: state_changed || changed,
                };
            }
            Command::LspWorkspaceSymbols => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.workspace_symbols)
                {
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

                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Workspace Symbols".to_string();
                self.state.ui.input_dialog.value.clear();
                self.state.ui.input_dialog.cursor = 0;
                self.state.ui.input_dialog.error = None;
                self.state.ui.input_dialog.kind = Some(InputDialogKind::LspWorkspaceSymbols);
                state_changed = true;
            }
            Command::LspSemanticTokens => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.semantic_tokens)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

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
                if !is_rust_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let version = tab.edit_version;
                let encoding = lsp_position_encoding(&self.state);

                let use_range = self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.semantic_tokens_range)
                    && tab.buffer.len_lines().max(1) >= 2000;

                if use_range {
                    let total_lines = tab.buffer.len_lines().max(1);
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

                return DispatchResult {
                    effects: vec![Effect::LspSemanticTokensRequest { path, version }],
                    state_changed,
                };
            }
            Command::LspInlayHints => {
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.inlay_hints)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

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
                if !is_rust_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
                    };
                }

                let total_lines = tab.buffer.len_lines().max(1);
                let start_line = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                let end_line_exclusive = (start_line + tab.viewport.height.max(1)).min(total_lines);
                let encoding = lsp_position_encoding(&self.state);
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
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.folding_range)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

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
                if !is_rust_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
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
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.folding_range)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }

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
                if !is_rust_source_path(&path) {
                    return DispatchResult {
                        effects,
                        state_changed,
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
                if !self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .is_none_or(|c| c.code_action)
                {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                }
                if let Some((_pane, path, line, column, _version)) = lsp_request_target(&self.state)
                {
                    let mut changed = self.state.code_actions.clear();

                    let prev_visible = self.state.ui.bottom_panel.visible;
                    let prev_tab = self.state.ui.bottom_panel.active_tab.clone();
                    let prev_focus = self.state.ui.focus;
                    self.state.ui.bottom_panel.visible = true;
                    self.state.ui.bottom_panel.active_tab = BottomPanelTab::CodeActions;
                    self.state.ui.focus = FocusTarget::BottomPanel;
                    if !prev_visible
                        || prev_tab != BottomPanelTab::CodeActions
                        || prev_focus != FocusTarget::BottomPanel
                    {
                        changed = true;
                    }

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
                    let supports_formatting = self
                        .state
                        .lsp
                        .server_capabilities
                        .as_ref()
                        .is_some_and(|c| c.format);
                    if supports_formatting {
                        let path = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.active_tab())
                            .and_then(|tab| tab.path.clone());
                        if let Some(path) = path {
                            if is_rust_source_path(&path) {
                                self.state.lsp.pending_format_on_save = Some(path.clone());
                                effects.push(Effect::LspFormatRequest { path });
                            }
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

                    if session_ok && !completion_should_keep_open(tab) {
                        let has_completion = self.state.ui.completion.visible
                            || self.state.ui.completion.request.is_some()
                            || self.state.ui.completion.pending_request.is_some()
                            || !self.state.ui.completion.all_items.is_empty()
                            || !self.state.ui.completion.items.is_empty();
                        if has_completion {
                            self.state.ui.completion =
                                super::state::CompletionPopupState::default();
                            state_changed = true;
                        }

                        let had_signature_help = self.state.ui.signature_help.visible
                            || self.state.ui.signature_help.request.is_some()
                            || !self.state.ui.signature_help.text.is_empty();
                        if had_signature_help && !signature_help_should_keep_open(tab) {
                            self.state.ui.signature_help =
                                super::state::SignatureHelpPopupState::default();
                            state_changed = true;
                        }
                        return DispatchResult {
                            effects,
                            state_changed,
                        };
                    }

                    if session_ok
                        && self.state.ui.completion.visible
                        && !self.state.ui.completion.all_items.is_empty()
                    {
                        let mut changed =
                            sync_completion_items_from_cache(&mut self.state.ui.completion, tab);

                        let selected = self
                            .state
                            .ui
                            .completion
                            .selected
                            .min(self.state.ui.completion.items.len().saturating_sub(1));
                        if let Some(item) = self.state.ui.completion.items.get(selected) {
                            if self
                                .state
                                .lsp
                                .server_capabilities
                                .as_ref()
                                .is_none_or(|c| c.completion_resolve)
                                && item.data.is_some()
                                && item
                                    .documentation
                                    .as_ref()
                                    .is_none_or(|d| d.trim().is_empty())
                                && self.state.ui.completion.resolve_inflight != Some(item.id)
                            {
                                self.state.ui.completion.resolve_inflight = Some(item.id);
                                effects.push(Effect::LspCompletionResolveRequest {
                                    item: item.clone(),
                                });
                                changed = true;
                            }
                        }

                        if changed {
                            state_changed = true;
                        }
                    }
                }

                let had_signature_help = self.state.ui.signature_help.visible
                    || self.state.ui.signature_help.request.is_some()
                    || !self.state.ui.signature_help.text.is_empty();
                if had_signature_help {
                    let keep = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|p| p.active_tab())
                        .is_some_and(signature_help_should_keep_open);
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
                let is_close_tab = matches!(other, Command::CloseTab);
                let should_git_refresh = matches!(
                    other,
                    Command::NextTab | Command::PrevTab | Command::CloseTab
                );
                let (changed, cmd_effects) = self.state.editor.apply_command(pane, other);
                if changed {
                    state_changed = true;
                }
                // TODO: avoid allocation by using SmallVec if needed.
                let mut effects = effects;
                effects.extend(cmd_effects);

                let mut collapsed = false;
                if changed && is_close_tab {
                    collapsed = self.maybe_close_empty_editor_split(&mut effects);
                    state_changed |= collapsed;
                }

                if should_git_refresh && !collapsed {
                    if let Some(repo_root) = self.state.git.repo_root.clone() {
                        let path = self
                            .state
                            .editor
                            .pane(pane)
                            .and_then(|pane_state| pane_state.active_tab())
                            .and_then(|tab| tab.path.clone());
                        if let Some(path) = path {
                            effects.push(Effect::GitRefreshStatus {
                                repo_root: repo_root.clone(),
                            });
                            effects.push(Effect::GitRefreshDiff { repo_root, path });
                        }
                    }
                }

                let had_signature_help = self.state.ui.signature_help.visible
                    || self.state.ui.signature_help.request.is_some()
                    || !self.state.ui.signature_help.text.is_empty();
                if had_signature_help {
                    let pane = self.state.ui.editor_layout.active_pane;
                    let keep = self
                        .state
                        .editor
                        .pane(pane)
                        .and_then(|p| p.active_tab())
                        .is_some_and(signature_help_should_keep_open);
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
#[path = "../../tests/unit/kernel/store.rs"]
mod tests;
