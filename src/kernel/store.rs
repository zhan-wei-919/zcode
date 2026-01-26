use crate::core::Command;
use crate::kernel::services::ports::{
    LspCompletionItem, LspInsertTextFormat, LspPosition, LspPositionEncoding, LspRange,
    LspWorkspaceEdit,
};
use crate::kernel::services::ports::{LspResourceOp, LspTextEdit, LspWorkspaceFileEdit};
use crate::models::{Granularity, Selection};
use std::collections::HashMap;

use super::{
    Action, AppState, BottomPanelTab, EditorAction, Effect, FocusTarget, InputDialogKind,
    SearchResultItem, SearchViewport, SidebarTab, SplitDirection,
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
                            if (supports_semantic_tokens || supports_inlay_hints || supports_folding_range)
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

                                        let range = LspRange {
                                            start: LspPosition {
                                                line: start_line as u32,
                                                character: 0,
                                            },
                                            end: LspPosition {
                                                line: end_line_exclusive as u32,
                                                character: 0,
                                            },
                                        };

                                        effects.push(Effect::LspSemanticTokensRangeRequest {
                                            path: opened_path.clone(),
                                            version,
                                            range,
                                        });
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
                                    let range = LspRange {
                                        start: LspPosition {
                                            line: start_line as u32,
                                            character: 0,
                                        },
                                        end: LspPosition {
                                            line: end_line_exclusive as u32,
                                            character: 0,
                                        },
                                    };
                                    effects.push(Effect::LspInlayHintsRequest {
                                        path: opened_path.clone(),
                                        version,
                                        range,
                                    });
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
                        other => {
                            let (state_changed, effects) = self.state.editor.dispatch_action(other);
                            DispatchResult {
                                effects,
                                state_changed,
                            }
                        }
                    };

                result.state_changed |= completion_changed;
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
                        if value.contains('/') || value.contains('\\') || value == "." || value == ".."
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
                            let prev = dialog.error.replace("Name cannot contain spaces".to_string());
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

                DispatchResult {
                    effects: Vec::new(),
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
            Action::ExplorerSetViewHeight { height } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.set_view_height(height),
            },
            Action::ExplorerMoveSelection { delta } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.move_selection(delta),
            },
            Action::ExplorerScroll { delta } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.scroll(delta),
            },
            Action::ExplorerActivate => {
                let (state_changed, effects) = self.state.explorer.activate_selected();
                DispatchResult {
                    effects,
                    state_changed,
                }
            }
            Action::ExplorerCollapse => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.collapse_selected(),
            },
            Action::ExplorerClickRow { row, now } => {
                let (state_changed, effects) = self.state.explorer.click_row(row, now);
                DispatchResult {
                    effects,
                    state_changed,
                }
            }
            Action::ExplorerContextMenuOpen { tree_row, x, y } => {
                if self.state.ui.command_palette.visible
                    || self.state.ui.input_dialog.visible
                    || self.state.ui.confirm_dialog.visible
                {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let mut state_changed = false;
                if !self.state.ui.sidebar_visible {
                    self.state.ui.sidebar_visible = true;
                    state_changed = true;
                }
                if self.state.ui.sidebar_tab != SidebarTab::Explorer {
                    self.state.ui.sidebar_tab = SidebarTab::Explorer;
                    state_changed = true;
                }
                if self.state.ui.focus != FocusTarget::Explorer {
                    self.state.ui.focus = FocusTarget::Explorer;
                    state_changed = true;
                }

                if let Some(row) = tree_row {
                    state_changed |= self.state.explorer.select_row(row);
                }

                let selected_is_root = self
                    .state
                    .explorer
                    .selected_path_and_kind()
                    .map(|(path, _)| path == self.state.workspace_root)
                    .unwrap_or(true);

                let mut items = vec![
                    super::state::ExplorerContextMenuItem::NewFile,
                    super::state::ExplorerContextMenuItem::NewFolder,
                ];
                if !selected_is_root {
                    items.push(super::state::ExplorerContextMenuItem::Rename);
                    items.push(super::state::ExplorerContextMenuItem::Delete);
                }

                let prev = self.state.ui.explorer_context_menu.clone();
                self.state.ui.explorer_context_menu.visible = true;
                self.state.ui.explorer_context_menu.anchor = (x, y);
                self.state.ui.explorer_context_menu.selected = 0;
                self.state.ui.explorer_context_menu.items = items;
                state_changed |= self.state.ui.explorer_context_menu != prev;

                DispatchResult {
                    effects: Vec::new(),
                    state_changed,
                }
            }
            Action::ExplorerContextMenuClose => {
                if !self.state.ui.explorer_context_menu.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                self.state.ui.explorer_context_menu = super::state::ExplorerContextMenuState::default();
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ExplorerContextMenuMoveSelection { delta } => {
                if !self.state.ui.explorer_context_menu.visible || delta == 0 {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let len = self.state.ui.explorer_context_menu.items.len();
                if len == 0 {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let current = self.state.ui.explorer_context_menu.selected.min(len - 1) as isize;
                let len_i = len as isize;
                let mut next = (current + delta) % len_i;
                if next < 0 {
                    next += len_i;
                }
                let next = next as usize;
                let changed = next != self.state.ui.explorer_context_menu.selected;
                self.state.ui.explorer_context_menu.selected = next;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::ExplorerContextMenuSetSelected { index } => {
                if !self.state.ui.explorer_context_menu.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let len = self.state.ui.explorer_context_menu.items.len();
                if len == 0 {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let next = index.min(len - 1);
                let changed = next != self.state.ui.explorer_context_menu.selected;
                self.state.ui.explorer_context_menu.selected = next;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::ExplorerContextMenuConfirm => {
                if !self.state.ui.explorer_context_menu.visible {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let selected = self.state.ui.explorer_context_menu.selected;
                let cmd = self
                    .state
                    .ui
                    .explorer_context_menu
                    .items
                    .get(selected)
                    .copied()
                    .map(|item| item.command());

                self.state.ui.explorer_context_menu = super::state::ExplorerContextMenuState::default();

                let Some(cmd) = cmd else {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                let mut result = self.dispatch(Action::RunCommand(cmd));
                result.state_changed = true;
                result
            }
            Action::BottomPanelSetActiveTab { tab } => {
                let prev_visible = self.state.ui.bottom_panel.visible;
                let prev = self.state.ui.bottom_panel.active_tab.clone();
                let next = tab.clone();
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.bottom_panel.active_tab = tab;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: !prev_visible || prev != next,
                }
            }
            Action::SearchSetViewHeight { viewport, height } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.set_view_height(viewport, height),
            },
            Action::SearchAppend(ch) => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.append_query_char(ch),
            },
            Action::SearchBackspace => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.backspace_query(),
            },
            Action::SearchCursorLeft => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.cursor_left(),
            },
            Action::SearchCursorRight => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.cursor_right(),
            },
            Action::SearchToggleCaseSensitive => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.toggle_case_sensitive(),
            },
            Action::SearchToggleRegex => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.toggle_regex(),
            },
            Action::SearchMoveSelection { delta, viewport } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.move_selection(delta, viewport),
            },
            Action::SearchScroll { delta, viewport } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.scroll(delta, viewport),
            },
            Action::SearchClickRow { row, viewport } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.click_row(row, viewport),
            },
            Action::SearchStart => {
                if self.state.search.query.is_empty() {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let root = self.state.workspace_root.clone();
                let pattern = self.state.search.query.clone();
                let case_sensitive = self.state.search.case_sensitive;
                let use_regex = self.state.search.use_regex;

                let state_changed = self.state.search.begin_search();
                DispatchResult {
                    effects: vec![Effect::StartGlobalSearch {
                        root,
                        pattern,
                        case_sensitive,
                        use_regex,
                    }],
                    state_changed,
                }
            }
            Action::SearchStarted { search_id } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.set_active_search_id(search_id),
            },
            Action::SearchMessage(msg) => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.apply_message(msg),
            },
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
            Action::LspDiagnostics { path, items } => DispatchResult {
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
                DispatchResult {
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
                            lsp_position_to_byte_offset(
                                tab,
                                line,
                                column,
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
                self.state.ui.pending_editor_nav = Some(super::state::PendingEditorNavigation {
                    pane,
                    path: path.clone(),
                    target: super::state::PendingEditorNavigationTarget::LineColumn {
                        line,
                        column,
                    },
                });

                DispatchResult {
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

                DispatchResult {
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

                DispatchResult {
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

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspServerCapabilities { capabilities } => DispatchResult {
                effects: Vec::new(),
                state_changed: if self.state.lsp.server_capabilities.as_ref() == Some(&capabilities)
                {
                    false
                } else {
                    self.state.lsp.server_capabilities = Some(capabilities);
                    true
                },
            },
            Action::LspSemanticTokens {
                path,
                version,
                tokens,
            } => {
                let Some(legend) = self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .and_then(|c| c.semantic_tokens_legend.as_ref())
                else {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let encoding = lsp_position_encoding(&self.state);

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

                        let lines = match snapshot_lines.as_ref() {
                            Some(lines) => lines.clone(),
                            None => {
                                let lines = semantic_highlight_lines_from_tokens(
                                    tab.buffer.rope(),
                                    &tokens,
                                    legend,
                                    encoding,
                                );
                                snapshot_lines = Some(lines.clone());
                                lines
                            }
                        };

                        changed |= tab.set_semantic_highlight(version, lines);
                    }
                }

                DispatchResult {
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
                let Some(legend) = self
                    .state
                    .lsp
                    .server_capabilities
                    .as_ref()
                    .and_then(|c| c.semantic_tokens_legend.as_ref())
                else {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let start_line = range.start.line as usize;
                let end_line_exclusive = range.end.line as usize;
                if end_line_exclusive <= start_line {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let encoding = lsp_position_encoding(&self.state);

                let mut snapshot_lines: Option<Vec<Vec<crate::kernel::editor::HighlightSpan>>> =
                    None;
                let mut changed = false;

                for pane in &mut self.state.editor.panes {
                    for tab in &mut pane.tabs {
                        if tab.path.as_ref() != Some(&path) || tab.edit_version != version {
                            continue;
                        }

                        let lines = match snapshot_lines.as_ref() {
                            Some(lines) => lines.clone(),
                            None => {
                                let lines = semantic_highlight_lines_from_tokens_range(
                                    tab.buffer.rope(),
                                    &tokens,
                                    legend,
                                    encoding,
                                    start_line,
                                    end_line_exclusive,
                                );
                                snapshot_lines = Some(lines.clone());
                                lines
                            }
                        };

                        changed |= tab.set_semantic_highlight_range(version, start_line, lines);
                    }
                }

                DispatchResult {
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
                let end_line_exclusive = range.end.line as usize;
                if end_line_exclusive <= start_line {
                    return DispatchResult {
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

                        let lines = match snapshot.as_ref() {
                            Some(lines) => lines.clone(),
                            None => {
                                let mut per_line =
                                    vec![Vec::<(u32, String)>::new(); end_line_exclusive - start_line];

                                for hint in &hints {
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

                                    per_line[line - start_line]
                                        .push((hint.position.character, text));
                                }

                                let mut lines = Vec::with_capacity(per_line.len());
                                for mut row in per_line {
                                    row.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                                    lines.push(row.into_iter().map(|(_, s)| s).collect());
                                }

                                snapshot = Some(lines.clone());
                                lines
                            }
                        };

                        changed |=
                            tab.set_inlay_hints(version, start_line, end_line_exclusive, lines);
                    }
                }

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspFoldingRanges {
                path,
                version,
                ranges,
            } => {
                let mut snapshot: Option<Vec<crate::kernel::services::ports::LspFoldingRange>> =
                    None;
                let mut changed = false;

                for pane in &mut self.state.editor.panes {
                    for tab in &mut pane.tabs {
                        if tab.path.as_ref() != Some(&path) || tab.edit_version != version {
                            continue;
                        }

                        let ranges = match snapshot.as_ref() {
                            Some(ranges) => ranges.clone(),
                            None => {
                                snapshot = Some(ranges.clone());
                                ranges.clone()
                            }
                        };

                        changed |= tab.set_folding_ranges(version, ranges);
                    }
                }

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspCompletion {
                items,
                is_incomplete,
            } => {
                let Some(req) = self.state.ui.completion.pending_request.clone() else {
                    return DispatchResult {
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

                let valid = tab.path.as_ref() == Some(&req.path) && tab.edit_version >= req.version;

                if !valid || items.is_empty() {
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

                self.state.ui.hover_message = None;
                self.state.ui.completion.visible = true;
                let prev_selected = self
                    .state
                    .ui
                    .completion
                    .items
                    .get(self.state.ui.completion.selected)
                    .map(|item| item.id);

                let mut all_items = items;
                sort_completion_items(&mut all_items);
                self.state.ui.completion.all_items = all_items;
                self.state.ui.completion.items =
                    filtered_completion_items(tab, &self.state.ui.completion.all_items);
                self.state.ui.completion.selected = prev_selected
                    .and_then(|id| {
                        self.state
                            .ui
                            .completion
                            .items
                            .iter()
                            .position(|item| item.id == id)
                    })
                    .unwrap_or(0)
                    .min(self.state.ui.completion.items.len().saturating_sub(1));
                self.state.ui.completion.request = Some(req.clone());
                self.state.ui.completion.pending_request = None;
                self.state.ui.completion.is_incomplete = is_incomplete;
                self.state.ui.completion.resolve_inflight = None;
                self.state.ui.completion.session_started_at = Some(std::time::Instant::now());

                let mut effects = Vec::new();
                if let Some(item) = self
                    .state
                    .ui
                    .completion
                    .items
                    .get(self.state.ui.completion.selected)
                {
                    if self
                        .state
                        .lsp
                        .server_capabilities
                        .as_ref()
                        .is_none_or(|c| c.completion_resolve)
                        && item.data.is_some()
                        && item.documentation.as_ref().is_none_or(|d| d.trim().is_empty())
                    {
                        self.state.ui.completion.resolve_inflight = Some(item.id);
                        effects.push(Effect::LspCompletionResolveRequest { item: item.clone() });
                    }
                }
                DispatchResult {
                    effects,
                    state_changed: true,
                }
            }
            Action::LspCompletionResolved {
                id,
                detail,
                documentation,
                additional_text_edits,
                command,
            } => {
                let mut changed = false;

                if self.state.ui.completion.resolve_inflight == Some(id) {
                    self.state.ui.completion.resolve_inflight = None;
                    changed = true;
                }

                for item in self
                    .state
                    .ui
                    .completion
                    .all_items
                    .iter_mut()
                    .chain(self.state.ui.completion.items.iter_mut())
                {
                    if item.id != id {
                        continue;
                    }
                    if let Some(detail) = detail.as_ref() {
                        if item.detail.as_deref() != Some(detail) {
                            item.detail = Some(detail.clone());
                            changed = true;
                        }
                    }
                    if let Some(doc) = documentation.as_ref() {
                        if item.documentation.as_deref() != Some(doc) {
                            item.documentation = Some(doc.clone());
                            changed = true;
                        }
                    }
                    if !additional_text_edits.is_empty() && item.additional_text_edits.is_empty() {
                        item.additional_text_edits = additional_text_edits.clone();
                        changed = true;
                    }
                    if command.is_some() && item.command.is_none() {
                        item.command = command.clone();
                        changed = true;
                    }
                    item.data = None;
                }

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspSignatureHelp { text } => {
                let Some(req) = self.state.ui.signature_help.request.clone() else {
                    return DispatchResult {
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
                    self.state.ui.signature_help = super::state::SignatureHelpPopupState::default();
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: had,
                    };
                }

                let changed = !self.state.ui.signature_help.visible
                    || self.state.ui.signature_help.text != text;
                self.state.ui.signature_help.visible = true;
                self.state.ui.signature_help.text = text;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::LspApplyWorkspaceEdit { edit } => {
                let mut effects = Vec::new();
                let changed = self.apply_workspace_edit(edit, &mut effects);
                DispatchResult {
                    effects,
                    state_changed: changed,
                }
            }
            Action::LspFormatCompleted { path } => {
                if self.state.lsp.pending_format_on_save.as_ref() != Some(&path) {
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                self.state.lsp.pending_format_on_save = None;

                let preferred_pane = self.state.ui.editor_layout.active_pane;
                let Some((pane, tab_index)) = find_open_tab(&self.state.editor, preferred_pane, &path)
                else {
                    return DispatchResult {
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
                    return DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                DispatchResult {
                    effects: vec![Effect::WriteFile { pane, path, version }],
                    state_changed: false,
                }
            }
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
                            && item.documentation.as_ref().is_none_or(|d| d.trim().is_empty())
                            && self.state.ui.completion.resolve_inflight != Some(item.id)
                        {
                            self.state.ui.completion.resolve_inflight = Some(item.id);
                            effects.push(Effect::LspCompletionResolveRequest { item: item.clone() });
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
                    LspInsertTextFormat::Snippet => CompletionInsertion::from_snippet(&item.insert_text),
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
                let mut state_changed =
                    self.state.explorer.apply_path_renamed(from.clone(), to.clone());
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
                let should_complete = supports_completion && tab.is_some_and(|tab| {
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
                        let session_ok = self
                            .state
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
                                    && item.documentation.as_ref().is_none_or(|d| d.trim().is_empty())
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
            }
            Command::ToggleBottomPanel => {
                let visible = !self.state.ui.bottom_panel.visible;
                self.state.ui.bottom_panel.visible = visible;
                if !visible && self.state.ui.focus == FocusTarget::BottomPanel {
                    self.state.ui.focus = FocusTarget::Editor;
                }
                state_changed = true;
            }
            Command::FocusBottomPanel => {
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.focus = FocusTarget::BottomPanel;
                state_changed = true;
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
                self.state.ui.input_dialog.kind = Some(InputDialogKind::ExplorerRename { from: path });
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
                        .map(|tab| problem_byte_offset(tab, range, lsp_position_encoding(&self.state)))
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
                        let mut changed = sync_completion_items_from_cache(
                            &mut self.state.ui.completion,
                            tab,
                        );
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
                                && item.documentation.as_ref().is_none_or(|d| d.trim().is_empty())
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
                if let Some((_pane, path, _line, _column, _version)) = lsp_request_target(&self.state)
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
                self.state.ui.input_dialog.kind = Some(InputDialogKind::LspRename { path, line, column });
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
                    return DispatchResult { effects, state_changed };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult { effects, state_changed };
                };
                if !is_rust_source_path(&path) {
                    return DispatchResult { effects, state_changed };
                }

                let version = tab.edit_version;

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
                    let end_line_exclusive =
                        (viewport_top + height + overscan).min(total_lines);

                    let range = LspRange {
                        start: LspPosition {
                            line: start_line as u32,
                            character: 0,
                        },
                        end: LspPosition {
                            line: end_line_exclusive as u32,
                            character: 0,
                        },
                    };

                    return DispatchResult {
                        effects: vec![Effect::LspSemanticTokensRangeRequest {
                            path,
                            version,
                            range,
                        }],
                        state_changed,
                    };
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
                    return DispatchResult { effects, state_changed };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult { effects, state_changed };
                };
                if !is_rust_source_path(&path) {
                    return DispatchResult { effects, state_changed };
                }

                let total_lines = tab.buffer.len_lines().max(1);
                let start_line = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
                let end_line_exclusive =
                    (start_line + tab.viewport.height.max(1)).min(total_lines);

                let range = LspRange {
                    start: LspPosition {
                        line: start_line as u32,
                        character: 0,
                    },
                    end: LspPosition {
                        line: end_line_exclusive as u32,
                        character: 0,
                    },
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
                    return DispatchResult { effects, state_changed };
                };
                let Some(path) = tab.path.as_ref().cloned() else {
                    return DispatchResult { effects, state_changed };
                };
                if !is_rust_source_path(&path) {
                    return DispatchResult { effects, state_changed };
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
                    return DispatchResult { effects, state_changed };
                };
                if !is_rust_source_path(&path) {
                    return DispatchResult { effects, state_changed };
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
                    let session = self
                        .state
                        .ui
                        .completion
                        .request
                        .as_ref()
                        .or(self.state.ui.completion.pending_request.as_ref());

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
                                && item.documentation.as_ref().is_none_or(|d| d.trim().is_empty())
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

                return DispatchResult {
                    effects,
                    state_changed,
                };
            }
            other => {
                let pane = self.state.ui.editor_layout.active_pane;
                let (changed, cmd_effects) = self.state.editor.apply_command(pane, other);
                if changed {
                    state_changed = true;
                }
                // TODO: avoid allocation by using SmallVec if needed.
                let mut effects = effects;
                effects.extend(cmd_effects);
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

    fn apply_workspace_edit(&mut self, edit: LspWorkspaceEdit, effects: &mut Vec<Effect>) -> bool {
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
                old_path,
                new_path,
                ..
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

                    let (changed, _) = self.state.editor.dispatch_action(
                        EditorAction::ApplyTextEditToTab {
                            pane,
                            tab_index,
                            start_byte,
                            end_byte,
                            text: edit.new_text.clone(),
                        },
                    );
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
            self.state.editor.open_paths_version = self.state.editor.open_paths_version.saturating_add(1);
        }

        if !resource_ops.is_empty() || !pending_file_edits.is_empty() {
            effects.push(Effect::ApplyFileEdits {
                position_encoding: encoding,
                resource_ops,
                edits: pending_file_edits,
            });
        }

        any_changed || open_paths_changed
    }
}

fn resolve_renamed_path(
    mut path: std::path::PathBuf,
    renames: &HashMap<std::path::PathBuf, std::path::PathBuf>,
) -> std::path::PathBuf {
    let mut hops = 0usize;
    while let Some(next) = renames.get(&path).cloned() {
        if next == path {
            break;
        }
        path = next;
        hops += 1;
        if hops > 16 {
            break;
        }
    }
    path
}

fn search_viewport_for_focus(ui: &super::UiState) -> Option<SearchViewport> {
    match ui.focus {
        FocusTarget::Explorer if ui.sidebar_tab == SidebarTab::Search => {
            Some(SearchViewport::Sidebar)
        }
        FocusTarget::BottomPanel if ui.bottom_panel.active_tab == BottomPanelTab::SearchResults => {
            Some(SearchViewport::BottomPanel)
        }
        _ => None,
    }
}

fn bottom_panel_tabs() -> Vec<BottomPanelTab> {
    vec![
        BottomPanelTab::Problems,
        BottomPanelTab::CodeActions,
        BottomPanelTab::Locations,
        BottomPanelTab::Symbols,
        BottomPanelTab::SearchResults,
        BottomPanelTab::Logs,
    ]
}

fn next_bottom_panel_tab(
    tabs: &[BottomPanelTab],
    current: &BottomPanelTab,
) -> Option<BottomPanelTab> {
    if tabs.is_empty() {
        return None;
    }
    let idx = tabs.iter().position(|tab| tab == current).unwrap_or(0);
    Some(tabs[(idx + 1) % tabs.len()].clone())
}

fn prev_bottom_panel_tab(
    tabs: &[BottomPanelTab],
    current: &BottomPanelTab,
) -> Option<BottomPanelTab> {
    if tabs.is_empty() {
        return None;
    }
    let idx = tabs.iter().position(|tab| tab == current).unwrap_or(0);
    let prev = if idx == 0 { tabs.len() - 1 } else { idx - 1 };
    Some(tabs[prev].clone())
}

fn search_open_target(
    search: &super::SearchState,
    item: SearchResultItem,
) -> Option<(std::path::PathBuf, usize)> {
    match item {
        SearchResultItem::FileHeader { file_index } => {
            let file = search.files.get(file_index)?;
            let byte_offset = file.matches.first().map(|m| m.start).unwrap_or(0);
            Some((file.path.clone(), byte_offset))
        }
        SearchResultItem::MatchLine {
            file_index,
            match_index,
        } => {
            let file = search.files.get(file_index)?;
            let m = file.matches.get(match_index)?;
            Some((file.path.clone(), m.start))
        }
    }
}

fn problem_byte_offset(
    tab: &super::editor::EditorTabState,
    range: crate::kernel::problems::ProblemRange,
    encoding: LspPositionEncoding,
) -> usize {
    lsp_position_to_byte_offset(tab, range.start_line, range.start_col, encoding)
}

fn lsp_position_to_byte_offset(
    tab: &super::editor::EditorTabState,
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

fn lsp_col_to_char_offset_in_line(
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

fn line_len_chars(line: ropey::RopeSlice<'_>) -> usize {
    let mut len = 0usize;
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            break;
        }
        if ch == '\r' && matches!(it.peek(), Some('\n')) {
            break;
        }
        len += 1;
    }
    len
}

fn semantic_highlight_lines_from_tokens(
    rope: &ropey::Rope,
    tokens: &[crate::kernel::services::ports::LspSemanticToken],
    legend: &crate::kernel::services::ports::LspSemanticTokensLegend,
    encoding: LspPositionEncoding,
) -> Vec<Vec<crate::kernel::editor::HighlightSpan>> {
    let total_lines = rope.len_lines().max(1);
    let mut lines = vec![Vec::new(); total_lines];

    for token in tokens {
        let Some(token_type) = legend.token_types.get(token.token_type as usize) else {
            continue;
        };
        let Some(kind) = highlight_kind_for_semantic_token(token_type.as_str()) else {
            continue;
        };

        let line_index = token.line as usize;
        if line_index >= total_lines {
            continue;
        }

        let line_slice = rope.line(line_index);
        let start_chars = lsp_col_to_char_offset_in_line(line_slice, token.start, encoding);
        let end_units = token.start.saturating_add(token.length);
        let end_chars = lsp_col_to_char_offset_in_line(line_slice, end_units, encoding);

        let line_start_char = rope.line_to_char(line_index);
        let start_char = (line_start_char + start_chars).min(rope.len_chars());
        let end_char = (line_start_char + end_chars).min(rope.len_chars());

        let line_start_byte = rope.line_to_byte(line_index);
        let start_byte = rope.char_to_byte(start_char);
        let end_byte = rope.char_to_byte(end_char);

        let start = start_byte.saturating_sub(line_start_byte);
        let end = end_byte.saturating_sub(line_start_byte);
        if end <= start {
            continue;
        }

        lines[line_index].push(crate::kernel::editor::HighlightSpan { start, end, kind });
    }

    for line_spans in &mut lines {
        line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        merge_adjacent_highlight_spans(line_spans);
    }

    lines
}

fn semantic_highlight_lines_from_tokens_range(
    rope: &ropey::Rope,
    tokens: &[crate::kernel::services::ports::LspSemanticToken],
    legend: &crate::kernel::services::ports::LspSemanticTokensLegend,
    encoding: LspPositionEncoding,
    start_line: usize,
    end_line_exclusive: usize,
) -> Vec<Vec<crate::kernel::editor::HighlightSpan>> {
    if start_line >= end_line_exclusive {
        return Vec::new();
    }

    let total_lines = rope.len_lines().max(1);
    let start_line = start_line.min(total_lines.saturating_sub(1));
    let end_line_exclusive = end_line_exclusive.min(total_lines);
    if end_line_exclusive <= start_line {
        return Vec::new();
    }

    let mut lines = vec![Vec::new(); end_line_exclusive.saturating_sub(start_line)];

    for token in tokens {
        let Some(token_type) = legend.token_types.get(token.token_type as usize) else {
            continue;
        };
        let Some(kind) = highlight_kind_for_semantic_token(token_type.as_str()) else {
            continue;
        };

        let line_index = token.line as usize;
        if line_index < start_line || line_index >= end_line_exclusive {
            continue;
        }

        let line_slice = rope.line(line_index);
        let start_chars = lsp_col_to_char_offset_in_line(line_slice, token.start, encoding);
        let end_units = token.start.saturating_add(token.length);
        let end_chars = lsp_col_to_char_offset_in_line(line_slice, end_units, encoding);

        let line_start_char = rope.line_to_char(line_index);
        let start_char = (line_start_char + start_chars).min(rope.len_chars());
        let end_char = (line_start_char + end_chars).min(rope.len_chars());

        let line_start_byte = rope.line_to_byte(line_index);
        let start_byte = rope.char_to_byte(start_char);
        let end_byte = rope.char_to_byte(end_char);

        let start = start_byte.saturating_sub(line_start_byte);
        let end = end_byte.saturating_sub(line_start_byte);
        if end <= start {
            continue;
        }

        lines[line_index.saturating_sub(start_line)]
            .push(crate::kernel::editor::HighlightSpan { start, end, kind });
    }

    for line_spans in &mut lines {
        line_spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        merge_adjacent_highlight_spans(line_spans);
    }

    lines
}

fn highlight_kind_for_semantic_token(
    token_type: &str,
) -> Option<crate::kernel::editor::HighlightKind> {
    use crate::kernel::editor::HighlightKind;

    match token_type {
        "comment" => Some(HighlightKind::Comment),
        "string" => Some(HighlightKind::String),
        "keyword" => Some(HighlightKind::Keyword),
        "number" => Some(HighlightKind::Number),
        "type" | "struct" | "enum" | "interface" | "trait" | "typeParameter" => {
            Some(HighlightKind::Type)
        }
        "function" | "method" => Some(HighlightKind::Function),
        "macro" => Some(HighlightKind::Macro),
        "variable" | "parameter" | "property" | "enumMember" => Some(HighlightKind::Variable),
        "namespace" | "module" => Some(HighlightKind::Attribute),
        _ => None,
    }
}

fn merge_adjacent_highlight_spans(spans: &mut Vec<crate::kernel::editor::HighlightSpan>) {
    if spans.len() < 2 {
        return;
    }

    let mut write = 1usize;
    for read in 1..spans.len() {
        let span = spans[read];
        let prev = &mut spans[write - 1];
        if prev.kind == span.kind && prev.end >= span.start {
            prev.end = prev.end.max(span.end);
        } else {
            spans[write] = span;
            write += 1;
        }
    }
    spans.truncate(write);
}

fn is_rust_source_path(path: &std::path::Path) -> bool {
    matches!(path.extension().and_then(|s| s.to_str()), Some("rs"))
}

fn should_close_completion_on_editor_action(action: &EditorAction) -> bool {
    match action {
        EditorAction::SetViewportSize { .. } => false,
        EditorAction::SearchStarted { .. } | EditorAction::SearchMessage { .. } => false,
        _ => true,
    }
}

fn should_close_completion_on_command(cmd: &Command) -> bool {
    match cmd {
        Command::LspCompletion => false,
        Command::LspSemanticTokens | Command::LspInlayHints | Command::LspFoldingRange => false,
        Command::InsertChar(ch) => !completion_keeps_open_on_inserted_char(*ch),
        Command::DeleteBackward | Command::DeleteForward | Command::DeleteSelection => false,
        _ => true,
    }
}

fn completion_keeps_open_on_inserted_char(inserted: char) -> bool {
    inserted.is_alphanumeric() || inserted == '_' || inserted == '.'
}

fn sort_completion_items(items: &mut Vec<LspCompletionItem>) {
    items.sort_by(|a, b| {
        let a_key = a.sort_text.as_deref().unwrap_or(a.label.as_str());
        let b_key = b.sort_text.as_deref().unwrap_or(b.label.as_str());
        a_key
            .cmp(b_key)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.detail.cmp(&b.detail))
    });
}

fn filtered_completion_items(
    tab: &super::editor::EditorTabState,
    items: &[LspCompletionItem],
) -> Vec<LspCompletionItem> {
    if items.is_empty() {
        return Vec::new();
    }

    let prefix = completion_prefix_at_cursor(tab);
    if prefix.is_empty() {
        return items.to_vec();
    }

    if !items
        .iter()
        .any(|item| completion_item_matches_prefix(item, &prefix))
    {
        return items.to_vec();
    }

    items
        .iter()
        .filter(|item| completion_item_matches_prefix(item, &prefix))
        .cloned()
        .collect()
}

fn same_completion_item_ids(a: &[LspCompletionItem], b: &[LspCompletionItem]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (a, b) in a.iter().zip(b.iter()) {
        if a.id != b.id {
            return false;
        }
    }
    true
}

fn sync_completion_items_from_cache(
    completion: &mut super::state::CompletionPopupState,
    tab: &super::editor::EditorTabState,
) -> bool {
    if completion.all_items.is_empty() {
        return false;
    }

    let selected_id = completion
        .items
        .get(completion.selected)
        .map(|item| item.id);

    let new_items = filtered_completion_items(tab, &completion.all_items);
    if new_items.is_empty() {
        return false;
    }

    let changed = !same_completion_item_ids(&completion.items, &new_items);
    completion.items = new_items;
    completion.selected = selected_id
        .and_then(|id| completion.items.iter().position(|item| item.id == id))
        .unwrap_or(0)
        .min(completion.items.len().saturating_sub(1));
    completion.visible = true;
    changed
}

fn completion_item_matches_prefix(item: &LspCompletionItem, prefix: &str) -> bool {
    let candidate = item.filter_text.as_deref().unwrap_or(item.label.as_str());
    starts_with_ignore_ascii_case(candidate, prefix)
}

fn starts_with_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}

fn completion_prefix_at_cursor(tab: &super::editor::EditorTabState) -> String {
    let rope = tab.buffer.rope();
    let (start_char, end_char) = completion_prefix_bounds_at_cursor(tab);
    rope.slice(start_char..end_char).to_string()
}

fn completion_prefix_bounds_at_cursor(tab: &super::editor::EditorTabState) -> (usize, usize) {
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

    (start_char, end_char)
}

fn completion_should_keep_open(tab: &super::editor::EditorTabState) -> bool {
    if tab.is_in_string_or_comment_at_cursor() {
        return false;
    }

    let (start_char, end_char) = completion_prefix_bounds_at_cursor(tab);
    if start_char != end_char {
        return true;
    }

    let rope = tab.buffer.rope();
    if start_char > 0 && rope.char(start_char - 1) == '.' {
        return true;
    }
    if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
        return true;
    }

    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnippetExpansion {
    text: String,
    cursor: Option<usize>,
    selection: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletionInsertion {
    text: String,
    cursor: Option<usize>,
    selection: Option<(usize, usize)>,
}

impl CompletionInsertion {
    fn from_plain_text(text: String) -> Self {
        let cursor = text
            .strip_suffix("()")
            .map(|prefix| prefix.chars().count().saturating_add(1));
        Self {
            text,
            cursor,
            selection: None,
        }
    }

    fn from_snippet(snippet: &str) -> Self {
        let expanded = expand_snippet(snippet);
        Self {
            text: expanded.text,
            cursor: expanded.cursor,
            selection: expanded.selection,
        }
    }

    fn has_cursor_or_selection(&self) -> bool {
        self.cursor.is_some() || self.selection.is_some()
    }
}

fn apply_completion_insertion_cursor(
    tab: &mut super::editor::EditorTabState,
    insertion: &CompletionInsertion,
    tab_size: u8,
) {
    if !insertion.has_cursor_or_selection() {
        return;
    }

    let inserted_chars = insertion.text.chars().count();
    if inserted_chars == 0 {
        return;
    }

    let cursor_end = tab.buffer.cursor_char_offset();
    if cursor_end < inserted_chars {
        return;
    }

    let start_char = cursor_end.saturating_sub(inserted_chars);
    let rope = tab.buffer.rope();
    let end_char = cursor_end.min(rope.len_chars());
    let start_char = start_char.min(end_char);
    if rope.slice(start_char..end_char).to_string() != insertion.text {
        return;
    }

    tab.viewport.follow_cursor = true;

    if let Some((mut sel_start_rel, mut sel_end_rel)) = insertion.selection {
        if sel_start_rel > sel_end_rel {
            std::mem::swap(&mut sel_start_rel, &mut sel_end_rel);
        }
        let sel_start_char = start_char.saturating_add(sel_start_rel);
        let sel_end_char = start_char.saturating_add(sel_end_rel);
        let sel_start = tab.buffer.cursor_pos_from_char_offset(sel_start_char);
        let sel_end = tab.buffer.cursor_pos_from_char_offset(sel_end_char);

        tab.buffer
            .set_selection(Some(Selection::new(sel_start, Granularity::Char)));
        tab.buffer.update_selection_cursor(sel_end);
        tab.buffer.set_cursor(sel_end.0, sel_end.1);
    } else if let Some(cursor_rel) = insertion.cursor {
        let cursor_char = start_char.saturating_add(cursor_rel);
        let cursor = tab.buffer.cursor_pos_from_char_offset(cursor_char);
        tab.buffer.clear_selection();
        tab.buffer.set_cursor(cursor.0, cursor.1);
    }

    crate::kernel::editor::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
}

fn expand_snippet(snippet: &str) -> SnippetExpansion {
    let mut out = String::with_capacity(snippet.len());
    let mut out_chars = 0usize;

    let mut best_placeholder: Option<(u32, usize, usize)> = None;
    let mut best_tabstop: Option<(u32, usize)> = None;
    let mut final_cursor: Option<usize> = None;

    let mut it = snippet.chars().peekable();

    while let Some(ch) = it.next() {
        match ch {
            '\\' => match it.next() {
                Some(next) => {
                    out.push(next);
                    out_chars = out_chars.saturating_add(1);
                }
                None => {
                    out.push('\\');
                    out_chars = out_chars.saturating_add(1);
                }
            },
            '$' => match it.peek().copied() {
                Some('{') => {
                    let _ = it.next();
                    let mut content = String::new();
                    let mut depth = 0usize;
                    while let Some(c) = it.next() {
                        match c {
                            '{' => {
                                depth = depth.saturating_add(1);
                                content.push(c);
                            }
                            '}' => {
                                if depth == 0 {
                                    break;
                                }
                                depth = depth.saturating_sub(1);
                                content.push(c);
                            }
                            _ => content.push(c),
                        }
                    }

                    let digits = content
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>();
                    let index: Option<u32> = if digits.is_empty() {
                        None
                    } else {
                        digits.parse().ok()
                    };

                    if let Some(index) = index {
                        let rest = content.get(digits.len()..).unwrap_or_default();
                        let (inserted, inserted_is_placeholder) = if let Some((_, text)) = rest.split_once(':') {
                            (text.to_string(), true)
                        } else if let (Some(start), Some(end)) = (rest.find('|'), rest.rfind('|')) {
                            if end > start.saturating_add(1) {
                                let opts = &rest[start + 1..end];
                                let first = opts.split(',').next().unwrap_or_default().to_string();
                                (first, true)
                            } else {
                                (String::new(), false)
                            }
                        } else {
                            (String::new(), false)
                        };

                        if !inserted.is_empty() {
                            let start = out_chars;
                            out.push_str(&inserted);
                            let inserted_chars = inserted.chars().count();
                            out_chars = out_chars.saturating_add(inserted_chars);
                            let end = out_chars;

                            if inserted_is_placeholder && index > 0 {
                                let replace = best_placeholder
                                    .as_ref()
                                    .is_none_or(|(best_idx, _, _)| index < *best_idx);
                                if replace {
                                    best_placeholder = Some((index, start, end));
                                }
                            }
                        } else if index == 0 {
                            final_cursor = Some(out_chars);
                        } else if index > 0 {
                            let replace = best_tabstop
                                .as_ref()
                                .is_none_or(|(best_idx, _)| index < *best_idx);
                            if replace {
                                best_tabstop = Some((index, out_chars));
                            }
                        }

                        continue;
                    }
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num: u32 = 0;
                    while it.peek().is_some_and(|c| c.is_ascii_digit()) {
                        let digit = it.next().unwrap();
                        num = num.saturating_mul(10).saturating_add((digit as u32).saturating_sub('0' as u32));
                    }
                    if num == 0 {
                        final_cursor = Some(out_chars);
                    } else {
                        let replace = best_tabstop
                            .as_ref()
                            .is_none_or(|(best_idx, _)| num < *best_idx);
                        if replace {
                            best_tabstop = Some((num, out_chars));
                        }
                    }
                }
                _ => {
                    out.push('$');
                    out_chars = out_chars.saturating_add(1);
                }
            },
            _ => {
                out.push(ch);
                out_chars = out_chars.saturating_add(1);
            }
        }
    }

    let (selection, cursor) = if let Some((_idx, start, end)) = best_placeholder {
        (Some((start, end)), Some(end))
    } else if let Some((_idx, pos)) = best_tabstop {
        (None, Some(pos))
    } else {
        (None, final_cursor)
    };

    SnippetExpansion {
        text: out,
        cursor,
        selection,
    }
}

fn completion_triggered_by_insert(
    tab: &super::editor::EditorTabState,
    inserted: char,
    triggers: &[char],
) -> bool {
    if triggers.is_empty() {
        return match inserted {
            '.' => true,
            ':' => {
                let (row, col) = tab.buffer.cursor();
                let cursor_char_offset = tab.buffer.pos_to_char((row, col));
                let rope = tab.buffer.rope();
                let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
                if cursor_char_offset < 2 {
                    return false;
                }
                rope.char(cursor_char_offset - 1) == ':' && rope.char(cursor_char_offset - 2) == ':'
            }
            _ => false,
        };
    }

    match inserted {
        ':' => {
            if !triggers.contains(&':') {
                return false;
            }
            let (row, col) = tab.buffer.cursor();
            let cursor_char_offset = tab.buffer.pos_to_char((row, col));
            let rope = tab.buffer.rope();
            let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
            if cursor_char_offset < 2 {
                return false;
            }
            rope.char(cursor_char_offset - 1) == ':' && rope.char(cursor_char_offset - 2) == ':'
        }
        ch => triggers.contains(&ch),
    }
}

fn signature_help_triggered_by_insert(inserted: char, triggers: &[char]) -> bool {
    if triggers.is_empty() {
        matches!(inserted, '(' | ',')
    } else {
        triggers.contains(&inserted)
    }
}

fn signature_help_closed_by_insert(inserted: char) -> bool {
    matches!(inserted, ')')
}

fn lsp_request_target(
    state: &super::AppState,
) -> Option<(usize, std::path::PathBuf, u32, u32, u64)> {
    let pane = state.ui.editor_layout.active_pane;
    let tab = state.editor.pane(pane)?.active_tab()?;
    let path = tab.path.as_ref()?.clone();
    if !is_rust_source_path(&path) {
        return None;
    }
    let encoding = lsp_position_encoding(state);
    let (line, column) = lsp_position_from_cursor(tab, encoding);
    Some((pane, path, line, column, tab.edit_version))
}

fn lsp_position_encoding(state: &super::AppState) -> LspPositionEncoding {
    state
        .lsp
        .server_capabilities
        .as_ref()
        .map(|c| c.position_encoding)
        .unwrap_or(LspPositionEncoding::Utf16)
}

fn lsp_position_from_cursor(
    tab: &super::editor::EditorTabState,
    encoding: LspPositionEncoding,
) -> (u32, u32) {
    lsp_position_from_buffer_pos(tab, tab.buffer.cursor(), encoding)
}

fn cursor_is_identifier(tab: &super::editor::EditorTabState) -> bool {
    let (row, col) = tab.buffer.cursor();
    let char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let char_offset = char_offset.min(rope.len_chars());
    if char_offset >= rope.len_chars() {
        return false;
    }
    let ch = rope.char(char_offset);
    ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch)
}

fn lsp_position_from_buffer_pos(
    tab: &super::editor::EditorTabState,
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

fn lsp_position_from_char_offset(
    tab: &super::editor::EditorTabState,
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

fn find_open_tab(
    editor: &super::EditorState,
    preferred_pane: usize,
    path: &std::path::PathBuf,
) -> Option<(usize, usize)> {
    if let Some(pane_state) = editor.panes.get(preferred_pane) {
        if let Some(index) = pane_state
            .tabs
            .iter()
            .position(|t| t.path.as_ref() == Some(path))
        {
            return Some((preferred_pane, index));
        }
    }

    for (pane, pane_state) in editor.panes.iter().enumerate() {
        if pane == preferred_pane {
            continue;
        }
        if let Some(index) = pane_state
            .tabs
            .iter()
            .position(|t| t.path.as_ref() == Some(path))
        {
            return Some((pane, index));
        }
    }

    None
}

fn open_tabs_for_path(editor: &super::EditorState, path: &std::path::PathBuf) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (pane, pane_state) in editor.panes.iter().enumerate() {
        for (tab_index, tab) in pane_state.tabs.iter().enumerate() {
            if tab.path.as_ref() == Some(path) {
                out.push((pane, tab_index));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::services::ports::EditorConfig;
    use crate::kernel::services::ports::{
        LspPosition, LspRange, LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
    };
    use crate::kernel::state::{ExplorerContextMenuItem, PendingEditorNavigation, PendingEditorNavigationTarget};
    use crate::models::{FileTree, Granularity, Selection};
    use std::ffi::OsString;

    fn new_store() -> Store {
        let root = std::env::temp_dir();
        let tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
        Store::new(AppState::new(root, tree, EditorConfig::default()))
    }

    #[test]
    fn escape_opens_settings_when_idle_in_editor() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;

        let result = store.dispatch(Action::RunCommand(Command::Escape));

        assert!(matches!(result.effects.as_slice(), [Effect::OpenSettings]));
        assert!(!result.state_changed);
    }

    #[test]
    fn escape_closes_palette_first() {
        let mut store = new_store();
        store.state.ui.command_palette.visible = true;
        store.state.ui.command_palette.query = "x".to_string();
        store.state.ui.command_palette.selected = 1;
        store.state.ui.focus = FocusTarget::CommandPalette;

        let result = store.dispatch(Action::RunCommand(Command::Escape));

        assert!(result.effects.is_empty());
        assert!(result.state_changed);
        assert!(!store.state.ui.command_palette.visible);
        assert!(store.state.ui.command_palette.query.is_empty());
        assert_eq!(store.state.ui.command_palette.selected, 0);
        assert_eq!(store.state.ui.focus, FocusTarget::Editor);
    }

    #[test]
    fn escape_focuses_editor_when_in_other_panel() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Explorer;

        let result = store.dispatch(Action::RunCommand(Command::Escape));

        assert!(result.effects.is_empty());
        assert!(result.state_changed);
        assert_eq!(store.state.ui.focus, FocusTarget::Editor);
    }

    #[test]
    fn escape_closes_editor_search_bar() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        store.state.editor.pane_mut(0).unwrap().search_bar.visible = true;

        let result = store.dispatch(Action::RunCommand(Command::Escape));

        assert!(matches!(
            result.effects.as_slice(),
            [Effect::CancelEditorSearch { pane: 0 }]
        ));
        assert!(result.state_changed);
        assert!(!store.state.editor.pane(0).unwrap().search_bar.visible);
    }

    #[test]
    fn escape_clears_editor_selection_before_opening_settings() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join("test.txt");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path,
            content: "hello".to_string(),
        }));
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer
            .set_selection(Some(Selection::new((0, 0), Granularity::Char)));

        let result = store.dispatch(Action::RunCommand(Command::Escape));

        assert!(result.effects.is_empty());
        assert!(result.state_changed);
        assert!(store
            .state
            .editor
            .pane(0)
            .unwrap()
            .active_tab()
            .unwrap()
            .buffer
            .selection()
            .is_none());
    }

    #[test]
    fn explorer_new_file_flow_creates_effect() {
        let mut store = new_store();
        let result = store.dispatch(Action::RunCommand(Command::ExplorerNewFile));
        assert!(result.effects.is_empty());
        assert!(store.state.ui.input_dialog.visible);

        let _ = store.dispatch(Action::InputDialogAppend('x'));
        let result = store.dispatch(Action::InputDialogAccept);
        assert!(matches!(
            result.effects.as_slice(),
            [Effect::CreateFile(path)] if path.ends_with("x")
        ));
        assert!(!store.state.ui.input_dialog.visible);
    }

    #[test]
    fn explorer_context_menu_root_only_shows_create_items() {
        let mut store = new_store();

        let result = store.dispatch(Action::ExplorerContextMenuOpen {
            tree_row: None,
            x: 10,
            y: 5,
        });

        assert!(result.effects.is_empty());
        assert!(result.state_changed);
        assert!(store.state.ui.explorer_context_menu.visible);
        assert_eq!(
            store.state.ui.explorer_context_menu.items,
            vec![ExplorerContextMenuItem::NewFile, ExplorerContextMenuItem::NewFolder]
        );
    }

    #[test]
    fn explorer_context_menu_confirm_rename_opens_rename_dialog() {
        let root = std::env::temp_dir();
        let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
        let file_id = tree
            .insert_child(tree.root(), OsString::from("a.txt"), crate::models::NodeKind::File)
            .unwrap();

        let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));
        let tree_row = store
            .state
            .explorer
            .rows
            .iter()
            .position(|row| row.id == file_id)
            .unwrap();

        let _ = store.dispatch(Action::ExplorerContextMenuOpen {
            tree_row: Some(tree_row),
            x: 10,
            y: 5,
        });

        assert_eq!(
            store.state.ui.explorer_context_menu.items,
            vec![
                ExplorerContextMenuItem::NewFile,
                ExplorerContextMenuItem::NewFolder,
                ExplorerContextMenuItem::Rename,
                ExplorerContextMenuItem::Delete,
            ]
        );

        let _ = store.dispatch(Action::ExplorerContextMenuSetSelected { index: 2 });
        let result = store.dispatch(Action::ExplorerContextMenuConfirm);
        assert!(result.effects.is_empty());
        assert!(store.state.ui.input_dialog.visible);
        assert!(matches!(
            store.state.ui.input_dialog.kind,
            Some(InputDialogKind::ExplorerRename { .. })
        ));

        store.state.ui.input_dialog.value = "b.txt".to_string();
        store.state.ui.input_dialog.cursor = store.state.ui.input_dialog.value.len();
        let result = store.dispatch(Action::InputDialogAccept);
        assert!(matches!(
            result.effects.as_slice(),
            [Effect::RenamePath { from, to }]
                if from == &root.join("a.txt") && to == &root.join("b.txt")
        ));
    }

    #[test]
    fn explorer_delete_confirm_produces_delete_effect() {
        let root = std::env::temp_dir();
        let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
        let file_id = tree
            .insert_child(
                tree.root(),
                OsString::from("to_delete.txt"),
                crate::models::NodeKind::File,
            )
            .unwrap();
        tree.set_selected(Some(file_id));

        let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));
        let result = store.dispatch(Action::RunCommand(Command::ExplorerDelete));
        assert!(result.effects.is_empty());
        assert!(store.state.ui.confirm_dialog.visible);

        let result = store.dispatch(Action::ConfirmDialogAccept);
        assert!(matches!(
            result.effects.as_slice(),
            [Effect::DeletePath { path, is_dir: false }] if path.ends_with("to_delete.txt")
        ));
    }

    #[test]
    fn open_file_applies_pending_editor_nav_byte_offset_and_clears_it() {
        let mut store = new_store();
        let path = store.state.workspace_root.join("test.txt");
        let content = "a😀b".to_string();
        let byte_offset_after_emoji = "a😀".len();
        store.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
            pane: 0,
            path: path.clone(),
            target: PendingEditorNavigationTarget::ByteOffset {
                byte_offset: byte_offset_after_emoji,
            },
        });

        let result = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content,
        }));

        assert!(result.state_changed);
        assert!(result.effects.is_empty());
        assert!(store.state.ui.pending_editor_nav.is_none());
        assert_eq!(
            store
                .state
                .editor
                .pane(0)
                .unwrap()
                .active_tab()
                .unwrap()
                .buffer
                .cursor(),
            (0, 2)
        );
    }

    #[test]
    fn open_file_applies_pending_editor_nav_line_column_utf16_and_clears_it() {
        let mut store = new_store();
        let path = store.state.workspace_root.join("test.txt");
        store.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
            pane: 0,
            path: path.clone(),
            target: PendingEditorNavigationTarget::LineColumn { line: 0, column: 3 },
        });

        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path,
            content: "a😀b".to_string(),
        }));

        assert!(store.state.ui.pending_editor_nav.is_none());
        assert_eq!(
            store
                .state
                .editor
                .pane(0)
                .unwrap()
                .active_tab()
                .unwrap()
                .buffer
                .cursor(),
            (0, 2)
        );
    }

    #[test]
    fn open_file_does_not_consume_pending_nav_for_other_path() {
        let mut store = new_store();
        let pending_path = store.state.workspace_root.join("pending.txt");
        store.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
            pane: 0,
            path: pending_path,
            target: PendingEditorNavigationTarget::ByteOffset { byte_offset: 1 },
        });

        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: store.state.workspace_root.join("actual.txt"),
            content: "hello".to_string(),
        }));

        assert!(store.state.ui.pending_editor_nav.is_some());
        assert_eq!(
            store
                .state
                .editor
                .pane(0)
                .unwrap()
                .active_tab()
                .unwrap()
                .buffer
                .cursor(),
            (0, 0)
        );
    }

    #[test]
    fn insert_dot_in_rust_triggers_lsp_completion_request() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join("main.rs");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: "fn main() {}\n".to_string(),
        }));

        let result = store.dispatch(Action::RunCommand(Command::InsertChar('.')));

        assert!(result
            .effects
            .iter()
            .any(|e| { matches!(e, Effect::LspCompletionRequest { path: p, .. } if p == &path) }));
        let req = store
            .state
            .ui
            .completion
            .pending_request
            .as_ref()
            .expect("request set");
        assert_eq!(req.path, path);
    }

    #[test]
    fn insert_double_colon_in_rust_triggers_lsp_completion_request() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join("main.rs");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: "fn main() {}\n".to_string(),
        }));

        let first = store.dispatch(Action::RunCommand(Command::InsertChar(':')));
        assert!(!first
            .effects
            .iter()
            .any(|e| matches!(e, Effect::LspCompletionRequest { .. })));
        assert!(store.state.ui.completion.pending_request.is_none());
        assert!(store.state.ui.completion.request.is_none());

        let second = store.dispatch(Action::RunCommand(Command::InsertChar(':')));
        assert!(second
            .effects
            .iter()
            .any(|e| { matches!(e, Effect::LspCompletionRequest { path: p, .. } if p == &path) }));
        let req = store
            .state
            .ui
            .completion
            .pending_request
            .as_ref()
            .expect("request set");
        assert_eq!(req.path, path);
    }

    #[test]
    fn insert_dot_in_non_rust_does_not_trigger_lsp_completion_request() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join("main.txt");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path,
            content: "hello\n".to_string(),
        }));

        let result = store.dispatch(Action::RunCommand(Command::InsertChar('.')));

        assert!(!result
            .effects
            .iter()
            .any(|e| matches!(e, Effect::LspCompletionRequest { .. })));
        assert!(store.state.ui.completion.pending_request.is_none());
        assert!(store.state.ui.completion.request.is_none());
    }

    #[test]
    fn lsp_completion_items_are_filtered_and_sorted() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join("main.rs");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: "prin\n".to_string(),
        }));
        let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
        let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

        let items = vec![
            LspCompletionItem {
                id: 1,
                label: "self::".to_string(),
                detail: None,
                kind: None,
                documentation: None,
                insert_text: "self::".to_string(),
                insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
                insert_range: None,
                replace_range: None,
                sort_text: Some("2".to_string()),
                filter_text: None,
                additional_text_edits: Vec::new(),
                command: None,
                data: None,
            },
            LspCompletionItem {
                id: 2,
                label: "Alignment".to_string(),
                detail: None,
                kind: None,
                documentation: None,
                insert_text: "Alignment".to_string(),
                insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
                insert_range: None,
                replace_range: None,
                sort_text: Some("1".to_string()),
                filter_text: None,
                additional_text_edits: Vec::new(),
                command: None,
                data: None,
            },
            LspCompletionItem {
                id: 3,
                label: "println!".to_string(),
                detail: None,
                kind: None,
                documentation: None,
                insert_text: "println!".to_string(),
                insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
                insert_range: None,
                replace_range: None,
                sort_text: Some("0".to_string()),
                filter_text: None,
                additional_text_edits: Vec::new(),
                command: None,
                data: None,
            },
            LspCompletionItem {
                id: 4,
                label: "Print".to_string(),
                detail: None,
                kind: None,
                documentation: None,
                insert_text: "Print".to_string(),
                insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
                insert_range: None,
                replace_range: None,
                sort_text: Some("9".to_string()),
                filter_text: Some("Print".to_string()),
                additional_text_edits: Vec::new(),
                command: None,
                data: None,
            },
        ];

        let _ = store.dispatch(Action::LspCompletion {
            items,
            is_incomplete: false,
        });

        assert!(store.state.ui.completion.visible);
        assert_eq!(store.state.ui.completion.items.len(), 2);
        assert_eq!(store.state.ui.completion.items[0].label, "println!");
        assert_eq!(store.state.ui.completion.items[1].label, "Print");
    }

    #[test]
    fn completion_does_not_close_on_viewport_resize() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join("main.rs");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: "fn main() { pri }\n".to_string(),
        }));
        let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
        let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

        let items = vec![LspCompletionItem {
            id: 1,
            label: "println!".to_string(),
            detail: None,
            kind: None,
            documentation: None,
            insert_text: "println!".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        }];

        let _ = store.dispatch(Action::LspCompletion {
            items,
            is_incomplete: false,
        });

        assert!(store.state.ui.completion.visible);
        assert!(!store.state.ui.completion.items.is_empty());

        let _ = store.dispatch(Action::Editor(EditorAction::SetViewportSize {
            pane: 0,
            width: 80,
            height: 20,
        }));

        assert!(store.state.ui.completion.visible);
        assert!(!store.state.ui.completion.items.is_empty());
    }

    #[test]
    fn completion_does_not_close_on_editor_search_messages() {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join("main.rs");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: "fn main() { pri }\n".to_string(),
        }));
        let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
        let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

        let items = vec![LspCompletionItem {
            id: 1,
            label: "println!".to_string(),
            detail: None,
            kind: None,
            documentation: None,
            insert_text: "println!".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        }];

        let _ = store.dispatch(Action::LspCompletion {
            items,
            is_incomplete: false,
        });

        assert!(store.state.ui.completion.visible);
        assert!(!store.state.ui.completion.items.is_empty());

        let search_id = 7u64;
        let _ = store.dispatch(Action::Editor(EditorAction::SearchStarted { pane: 0, search_id }));
        let _ = store.dispatch(Action::Editor(EditorAction::SearchMessage {
            pane: 0,
            message: crate::kernel::services::ports::SearchMessage::Complete {
                search_id,
                total: 0,
            },
        }));

        assert!(store.state.ui.completion.visible);
        assert!(!store.state.ui.completion.items.is_empty());
    }

    #[test]
    fn lsp_workspace_edit_applies_to_all_open_tabs_for_path() {
        let mut store = new_store();
        store.state.editor.ensure_panes(2);

        let path = store.state.workspace_root.join("test.rs");
        let content = "hello\nworld\n".to_string();

        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: content.clone(),
        }));
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 1,
            path: path.clone(),
            content,
        }));

        let edit = LspWorkspaceEdit {
            changes: vec![LspWorkspaceFileEdit {
                path: path.clone(),
                edits: vec![LspTextEdit {
                    range: LspRange {
                        start: LspPosition {
                            line: 1,
                            character: 0,
                        },
                        end: LspPosition {
                            line: 1,
                            character: 5,
                        },
                    },
                    new_text: "rust".to_string(),
                }],
            }],
            ..Default::default()
        };

        let _ = store.dispatch(Action::LspApplyWorkspaceEdit { edit });

        assert_eq!(
            store
                .state
                .editor
                .pane(0)
                .unwrap()
                .active_tab()
                .unwrap()
                .buffer
                .text(),
            "hello\nrust\n"
        );
        assert_eq!(
            store
                .state
                .editor
                .pane(1)
                .unwrap()
                .active_tab()
                .unwrap()
                .buffer
                .text(),
            "hello\nrust\n"
        );
    }

    #[test]
    fn lsp_workspace_edit_schedules_file_edits_when_not_open() {
        let mut store = new_store();
        let path = store.state.workspace_root.join("test.rs");

        let edit = LspWorkspaceEdit {
            changes: vec![LspWorkspaceFileEdit {
                path: path.clone(),
                edits: vec![LspTextEdit {
                    range: LspRange {
                        start: LspPosition {
                            line: 0,
                            character: 0,
                        },
                        end: LspPosition {
                            line: 0,
                            character: 0,
                        },
                    },
                    new_text: "x".to_string(),
                }],
            }],
            ..Default::default()
        };

        let result = store.dispatch(Action::LspApplyWorkspaceEdit { edit });

        assert!(!result.state_changed);
        assert!(matches!(
            result.effects.as_slice(),
            [Effect::ApplyFileEdits { position_encoding, resource_ops, edits }]
                if *position_encoding == LspPositionEncoding::Utf16
                    && resource_ops.is_empty()
                    && edits.len() == 1
                    && edits[0].path == path
        ));
    }

    #[test]
    fn expand_snippet_strips_tabstops_and_keeps_placeholder_text() {
        let out = expand_snippet("foo$1bar$0");
        assert_eq!(out.text, "foobar");
        assert_eq!(out.cursor, Some(3));
        assert!(out.selection.is_none());

        let out = expand_snippet("fn ${1:name}($2)$0");
        assert_eq!(out.text, "fn name()");
        assert_eq!(out.selection, Some((3, 7)));
        assert_eq!(out.cursor, Some(7));

        let out = expand_snippet("x${1|a,b,c|}y");
        assert_eq!(out.text, "xay");
        assert_eq!(out.selection, Some((1, 2)));
        assert_eq!(out.cursor, Some(2));

        let out = expand_snippet("\\$\\{not_a_placeholder\\}");
        assert_eq!(out.text, "${not_a_placeholder}");
        assert!(out.cursor.is_none());
        assert!(out.selection.is_none());
    }

    #[test]
    fn completion_plain_text_moves_cursor_inside_trailing_parens() {
        let insertion = CompletionInsertion::from_plain_text("println!()".to_string());
        assert_eq!(insertion.text, "println!()");
        assert_eq!(insertion.cursor, Some("println!(".chars().count()));
        assert!(insertion.selection.is_none());

        let insertion = CompletionInsertion::from_plain_text("no_parens".to_string());
        assert_eq!(insertion.text, "no_parens");
        assert!(insertion.cursor.is_none());
        assert!(insertion.selection.is_none());
    }

    #[test]
    fn lsp_position_to_byte_offset_handles_emoji_crlf_and_out_of_bounds() {
        let mut store = new_store();
        let path = store.state.workspace_root.join("main.rs");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: "a😀b\r\nc".to_string(),
        }));
        let tab = store
            .state
            .editor
            .pane(0)
            .unwrap()
            .active_tab()
            .unwrap();

        let after_emoji = "a😀".len();
        assert_eq!(
            lsp_position_to_byte_offset(tab, 0, 3, LspPositionEncoding::Utf16),
            after_emoji
        );
        assert_eq!(
            lsp_position_to_byte_offset(tab, 0, 5, LspPositionEncoding::Utf8),
            after_emoji
        );

        assert_eq!(
            lsp_position_to_byte_offset(tab, 0, 1, LspPositionEncoding::Utf16),
            1
        );
        assert_eq!(
            lsp_position_to_byte_offset(tab, 1, 0, LspPositionEncoding::Utf16),
            "a😀b\r\n".len()
        );

        assert_eq!(
            lsp_position_to_byte_offset(tab, u32::MAX, u32::MAX, LspPositionEncoding::Utf16),
            "a😀b\r\nc".len()
        );
    }

    #[test]
    fn fuzz_workspace_edit_application_does_not_break_cursor_invariants() {
        struct Rng(u64);

        impl Rng {
            fn new(seed: u64) -> Self {
                Self(seed)
            }

            fn next_u32(&mut self) -> u32 {
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                (self.0 & 0xFFFF_FFFF) as u32
            }

            fn gen_range(&mut self, upper: u32) -> u32 {
                if upper == 0 {
                    return 0;
                }
                self.next_u32() % upper
            }
        }

        fn assert_cursor_invariants(tab: &crate::kernel::editor::EditorTabState) {
            let (row, col) = tab.buffer.cursor();
            let total_lines = tab.buffer.len_lines().max(1);
            assert!(row < total_lines);
            assert!(col <= tab.buffer.line_grapheme_len(row));
        }

        let mut store = new_store();
        let path = store.state.workspace_root.join("main.rs");
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: "line0\nline1\nline2\n".to_string(),
        }));

        let mut rng = Rng::new(0xBAD5_EED);

        const STEPS: usize = 300;
        for _ in 0..STEPS {
            let edits_len = 1 + rng.gen_range(3) as usize;
            let mut edits = Vec::with_capacity(edits_len);
            for _ in 0..edits_len {
                let huge = rng.gen_range(10) == 0;
                let start_line = if huge { u32::MAX } else { rng.gen_range(8) };
                let start_col = if huge { u32::MAX } else { rng.gen_range(32) };
                let end_line = if huge { 0 } else { rng.gen_range(8) };
                let end_col = if huge { 0 } else { rng.gen_range(32) };

                let new_text = match rng.gen_range(7) {
                    0 => String::new(),
                    1 => "x".to_string(),
                    2 => "😀".to_string(),
                    3 => "y\n".to_string(),
                    4 => "中".to_string(),
                    5 => "\r\n".to_string(),
                    _ => "_".to_string(),
                };

                edits.push(LspTextEdit {
                    range: LspRange {
                        start: LspPosition {
                            line: start_line,
                            character: start_col,
                        },
                        end: LspPosition {
                            line: end_line,
                            character: end_col,
                        },
                    },
                    new_text,
                });
            }

            let _ = store.dispatch(Action::LspApplyWorkspaceEdit {
                edit: LspWorkspaceEdit {
                    changes: vec![LspWorkspaceFileEdit {
                        path: path.clone(),
                        edits,
                    }],
                    ..Default::default()
                },
            });

            let tab = store
                .state
                .editor
                .pane(0)
                .unwrap()
                .active_tab()
                .unwrap();
            assert_cursor_invariants(tab);
        }
    }
}
