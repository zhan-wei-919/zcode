use crate::kernel::{Action, Effect};

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
            Action::ExplorerMovePath { from, to } => {
                let root = self.state.workspace_root.as_path();
                if self.state.ui.input_dialog.visible
                    || self.state.ui.confirm_dialog.visible
                    || from == to
                    || from.as_path() == root
                    || to.as_path() == root
                    || !from.starts_with(root)
                    || !to.starts_with(root)
                {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                super::DispatchResult {
                    effects: vec![Effect::RenamePath {
                        from,
                        to,
                        overwrite: false,
                    }],
                    state_changed: false,
                }
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
                    let clipboard_changed = self
                        .state
                        .explorer
                        .clear_clipboard_if_deleted_path(path.as_path());
                    let changed = self.state.explorer.apply_path_deleted(path);
                    let git_changed = if changed {
                        self.state
                            .explorer
                            .set_git_statuses(&self.state.git.file_status)
                    } else {
                        false
                    };
                    changed || git_changed || clipboard_changed
                },
            },
            Action::ExplorerPathRenamed { from, to } => {
                let mut state_changed = self
                    .state
                    .explorer
                    .clear_clipboard_if_cut_source_renamed(from.as_path());
                state_changed |= self
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
