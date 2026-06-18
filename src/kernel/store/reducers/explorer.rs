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
                let (state_changed, effects) = self.state.explorer.activate_selected();
                super::DispatchResult {
                    effects,
                    state_changed,
                }
            }
            Action::ExplorerCollapse => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.collapse_selected(),
            },
            Action::ExplorerClickRow { row, now } => {
                let (state_changed, effects) = self.state.explorer.click_row(row, now);
                super::DispatchResult {
                    effects,
                    state_changed,
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
                state_changed: self.state.explorer.apply_dir_loaded(path, entries),
            },
            Action::DirLoadError { path } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_dir_load_error(path),
            },
            Action::ExplorerPathCreated { path, is_dir } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_path_created(path, is_dir),
            },
            Action::ExplorerPathDeleted { path } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: {
                    let clipboard_changed = self
                        .state
                        .explorer
                        .clear_clipboard_if_deleted_path(path.as_path());
                    let changed = self.state.explorer.apply_path_deleted(path);
                    changed || clipboard_changed
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

                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed,
                }
            }
            _ => unreachable!("non-explorer action passed to reduce_explorer_action"),
        }
    }
}
