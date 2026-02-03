use crate::kernel::state::{ExplorerContextMenuItem, ExplorerContextMenuState};
use crate::kernel::{Action, Effect, FocusTarget, SidebarTab};

impl super::Store {
    pub(super) fn reduce_explorer_action(&mut self, action: Action) -> super::DispatchResult {
        match action {
            Action::ExplorerSetViewHeight { height } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.set_view_height(height),
            },
            Action::ExplorerMoveSelection { delta } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.move_selection(delta),
            },
            Action::ExplorerScroll { delta } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.scroll(delta),
            },
            Action::ExplorerActivate => {
                let prev_rows_len = self.state.explorer.rows.len();
                let (state_changed, effects) = self.state.explorer.activate_selected();
                let rows_changed = self.state.explorer.rows.len() != prev_rows_len;
                let explorer_git_changed = if rows_changed {
                    self.state
                        .explorer
                        .set_git_statuses(&self.state.git.file_status)
                } else {
                    false
                };
                super::DispatchResult {
                    effects,
                    state_changed: state_changed || explorer_git_changed,
                }
            }
            Action::ExplorerCollapse => {
                let prev_rows_len = self.state.explorer.rows.len();
                let state_changed = self.state.explorer.collapse_selected();
                let rows_changed = self.state.explorer.rows.len() != prev_rows_len;
                let explorer_git_changed = if rows_changed {
                    self.state
                        .explorer
                        .set_git_statuses(&self.state.git.file_status)
                } else {
                    false
                };
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: state_changed || explorer_git_changed,
                }
            }
            Action::ExplorerClickRow { row, now } => {
                let prev_rows_len = self.state.explorer.rows.len();
                let (state_changed, effects) = self.state.explorer.click_row(row, now);
                let rows_changed = self.state.explorer.rows.len() != prev_rows_len;
                let explorer_git_changed = if rows_changed {
                    self.state
                        .explorer
                        .set_git_statuses(&self.state.git.file_status)
                } else {
                    false
                };
                super::DispatchResult {
                    effects,
                    state_changed: state_changed || explorer_git_changed,
                }
            }
            Action::ExplorerContextMenuOpen { tree_row, x, y } => {
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
                    ExplorerContextMenuItem::NewFile,
                    ExplorerContextMenuItem::NewFolder,
                ];
                if !selected_is_root {
                    items.push(ExplorerContextMenuItem::Rename);
                    items.push(ExplorerContextMenuItem::Delete);
                }

                let prev = self.state.ui.explorer_context_menu.clone();
                self.state.ui.explorer_context_menu.visible = true;
                self.state.ui.explorer_context_menu.anchor = (x, y);
                self.state.ui.explorer_context_menu.selected = 0;
                self.state.ui.explorer_context_menu.items = items;
                state_changed |= self.state.ui.explorer_context_menu != prev;

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed,
                }
            }
            Action::ExplorerContextMenuClose => {
                if !self.state.ui.explorer_context_menu.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                self.state.ui.explorer_context_menu = ExplorerContextMenuState::default();
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::ExplorerContextMenuMoveSelection { delta } => {
                if !self.state.ui.explorer_context_menu.visible || delta == 0 {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let len = self.state.ui.explorer_context_menu.items.len();
                if len == 0 {
                    return super::DispatchResult {
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
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::ExplorerContextMenuSetSelected { index } => {
                if !self.state.ui.explorer_context_menu.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let len = self.state.ui.explorer_context_menu.items.len();
                if len == 0 {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let next = index.min(len - 1);
                let changed = next != self.state.ui.explorer_context_menu.selected;
                self.state.ui.explorer_context_menu.selected = next;
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::ExplorerContextMenuConfirm => {
                if !self.state.ui.explorer_context_menu.visible {
                    return super::DispatchResult {
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

                self.state.ui.explorer_context_menu = ExplorerContextMenuState::default();

                let Some(cmd) = cmd else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                let mut result = self.dispatch(Action::RunCommand(cmd));
                result.state_changed = true;
                result
            }
            Action::DirLoaded { path, entries } => super::DispatchResult {
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
            Action::DirLoadError { path } => super::DispatchResult {
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
            Action::ExplorerPathCreated { path, is_dir } => super::DispatchResult {
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
            Action::ExplorerPathDeleted { path } => super::DispatchResult {
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

                super::DispatchResult {
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
            _ => unreachable!("non-explorer action passed to reduce_explorer_action"),
        }
    }
}
