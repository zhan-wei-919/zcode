use crate::core::Command;
use crate::kernel::state::{PendingEditorNavigation, PendingEditorNavigationTarget};
use crate::kernel::{EditorAction, Effect, FocusTarget, OverlayKind};

use super::intel::lsp::{
    lsp_position_encoding_for_path, lsp_position_to_byte_offset, problem_byte_offset,
};
use super::search::search_open_target;
use super::util::{find_open_tab, search_viewport_for_focus};
use super::DispatchResult;

impl super::Store {
    pub(super) fn reduce_search_command(&mut self, command: Command) -> DispatchResult {
        let mut state_changed = false;
        let effects = Vec::new();

        match command {
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
                    self.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
                        pane,
                        path: path.clone(),
                        target: PendingEditorNavigationTarget::ByteOffset { byte_offset },
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
                    self.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
                        pane,
                        path: path.clone(),
                        target: PendingEditorNavigationTarget::LineColumn {
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
                    self.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
                        pane,
                        path: path.clone(),
                        target: PendingEditorNavigationTarget::LineColumn {
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
                    self.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
                        pane,
                        path: path.clone(),
                        target: PendingEditorNavigationTarget::LineColumn {
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
            _ => unreachable!("non-search command passed to reduce_search_command"),
        }

        DispatchResult {
            effects,
            state_changed,
        }
    }
}
