use crate::core::Command;
use crate::kernel::editor::EditorAction;
use crate::kernel::state::{ContextMenuItem, ContextMenuRequest, ContextMenuState, PendingAction};
use crate::kernel::{Action, FocusTarget, SidebarTab};

impl super::Store {
    pub(super) fn reduce_context_menu_action(&mut self, action: Action) -> super::DispatchResult {
        match action {
            Action::ContextMenuOpen { request, x, y } => {
                if self.state.ui.command_palette.visible
                    || self.state.ui.input_dialog.visible
                    || self.state.ui.confirm_dialog.visible
                {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let mut state_changed = false;
                match request {
                    ContextMenuRequest::Explorer { tree_row } => {
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
                            ContextMenuItem::ExplorerNewFile,
                            ContextMenuItem::ExplorerNewFolder,
                        ];
                        if !selected_is_root {
                            items.push(ContextMenuItem::ExplorerRename);
                            items.push(ContextMenuItem::ExplorerDelete);
                        }

                        let prev = self.state.ui.context_menu.clone();
                        self.state.ui.context_menu.visible = true;
                        self.state.ui.context_menu.anchor = (x, y);
                        self.state.ui.context_menu.selected = 0;
                        self.state.ui.context_menu.items = items;
                        self.state.ui.context_menu.request =
                            Some(ContextMenuRequest::Explorer { tree_row });
                        state_changed |= self.state.ui.context_menu != prev;

                        super::DispatchResult {
                            effects: Vec::new(),
                            state_changed,
                        }
                    }
                    ContextMenuRequest::Tab { pane, index } => {
                        if self.state.ui.focus != FocusTarget::Editor {
                            self.state.ui.focus = FocusTarget::Editor;
                            state_changed = true;
                        }

                        // Keep behavior minimal for now: do not change the active tab on right-click.
                        if self.state.ui.editor_layout.active_pane != pane {
                            self.state.ui.editor_layout.active_pane = pane;
                            state_changed = true;
                        }

                        let items = vec![ContextMenuItem::TabClose];

                        let prev = self.state.ui.context_menu.clone();
                        self.state.ui.context_menu.visible = true;
                        self.state.ui.context_menu.anchor = (x, y);
                        self.state.ui.context_menu.selected = 0;
                        self.state.ui.context_menu.items = items;
                        self.state.ui.context_menu.request =
                            Some(ContextMenuRequest::Tab { pane, index });
                        state_changed |= self.state.ui.context_menu != prev;

                        super::DispatchResult {
                            effects: Vec::new(),
                            state_changed,
                        }
                    }
                    ContextMenuRequest::EditorArea { pane } => {
                        if self.state.ui.focus != FocusTarget::Editor {
                            self.state.ui.focus = FocusTarget::Editor;
                            state_changed = true;
                        }

                        if self.state.ui.editor_layout.active_pane != pane {
                            self.state.ui.editor_layout.active_pane = pane;
                            state_changed = true;
                        }

                        let items = vec![ContextMenuItem::EditorCopy, ContextMenuItem::EditorPaste];

                        let prev = self.state.ui.context_menu.clone();
                        self.state.ui.context_menu.visible = true;
                        self.state.ui.context_menu.anchor = (x, y);
                        self.state.ui.context_menu.selected = 0;
                        self.state.ui.context_menu.items = items;
                        self.state.ui.context_menu.request =
                            Some(ContextMenuRequest::EditorArea { pane });
                        state_changed |= self.state.ui.context_menu != prev;

                        super::DispatchResult {
                            effects: Vec::new(),
                            state_changed,
                        }
                    }
                }
            }
            Action::ContextMenuClose => {
                if !self.state.ui.context_menu.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                self.state.ui.context_menu = ContextMenuState::default();
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ContextMenuMoveSelection { delta } => {
                if !self.state.ui.context_menu.visible || delta == 0 {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let len = self.state.ui.context_menu.items.len();
                if len == 0 {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let current = self.state.ui.context_menu.selected.min(len - 1) as isize;
                let len_i = len as isize;
                let mut next = (current + delta) % len_i;
                if next < 0 {
                    next += len_i;
                }
                let next = next as usize;
                let changed = next != self.state.ui.context_menu.selected;
                self.state.ui.context_menu.selected = next;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::ContextMenuSetSelected { index } => {
                if !self.state.ui.context_menu.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let len = self.state.ui.context_menu.items.len();
                if len == 0 {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let next = index.min(len - 1);
                let changed = next != self.state.ui.context_menu.selected;
                self.state.ui.context_menu.selected = next;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::ContextMenuConfirm => {
                if !self.state.ui.context_menu.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let selected = self.state.ui.context_menu.selected;
                let item = self.state.ui.context_menu.items.get(selected).copied();
                let request = self.state.ui.context_menu.request.clone();
                self.state.ui.context_menu = ContextMenuState::default();

                let Some(item) = item else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                match (item, request) {
                    (ContextMenuItem::ExplorerNewFile, _) => {
                        let mut result =
                            self.dispatch(Action::RunCommand(Command::ExplorerNewFile));
                        result.state_changed = true;
                        result
                    }
                    (ContextMenuItem::ExplorerNewFolder, _) => {
                        let mut result =
                            self.dispatch(Action::RunCommand(Command::ExplorerNewFolder));
                        result.state_changed = true;
                        result
                    }
                    (ContextMenuItem::ExplorerRename, _) => {
                        let mut result = self.dispatch(Action::RunCommand(Command::ExplorerRename));
                        result.state_changed = true;
                        result
                    }
                    (ContextMenuItem::ExplorerDelete, _) => {
                        let mut result = self.dispatch(Action::RunCommand(Command::ExplorerDelete));
                        result.state_changed = true;
                        result
                    }
                    (ContextMenuItem::TabClose, Some(ContextMenuRequest::Tab { pane, index })) => {
                        let is_dirty = self
                            .state
                            .editor
                            .pane(pane)
                            .is_some_and(|p| p.is_tab_dirty(index));

                        if is_dirty {
                            self.state.ui.confirm_dialog.visible = true;
                            self.state.ui.confirm_dialog.message =
                                "Unsaved changes. Close anyway?".to_string();
                            self.state.ui.confirm_dialog.on_confirm =
                                Some(PendingAction::CloseTab { pane, index });
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: true,
                            };
                        }

                        let mut result =
                            self.dispatch(Action::Editor(EditorAction::CloseTabAt { pane, index }));
                        result.state_changed = true;
                        result
                    }
                    (ContextMenuItem::EditorCopy, _) => {
                        let mut result = self.dispatch(Action::RunCommand(Command::Copy));
                        result.state_changed = true;
                        result
                    }
                    (ContextMenuItem::EditorPaste, _) => {
                        let mut result = self.dispatch(Action::RunCommand(Command::Paste));
                        result.state_changed = true;
                        result
                    }
                    _ => super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    },
                }
            }
            _ => super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            },
        }
    }
}
