use crate::kernel::{Action, EditorAction, Effect, PendingAction};

use super::DispatchResult;

impl super::Store {
    pub(super) fn reduce_confirm_dialog_action(&mut self, action: Action) -> DispatchResult {
        match action {
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
                        PendingAction::CloseTab { pane, index } => {
                            let mut result = self
                                .dispatch(Action::Editor(EditorAction::CloseTabAt { pane, index }));
                            result.state_changed = true;
                            return result;
                        }
                        PendingAction::CloseTabsBatch { pane, tab_ids } => {
                            let mut result =
                                self.dispatch(Action::Editor(EditorAction::CloseTabsById {
                                    pane,
                                    tab_ids,
                                }));
                            result.state_changed = true;
                            return result;
                        }
                        PendingAction::DeletePath { path, is_dir } => {
                            let root = self.state.workspace_root.as_path();
                            if path.as_path() == root || !path.starts_with(root) {
                                return DispatchResult {
                                    effects: Vec::new(),
                                    state_changed: true,
                                };
                            }
                            return DispatchResult {
                                effects: vec![Effect::DeletePath { path, is_dir }],
                                state_changed: true,
                            };
                        }
                        PendingAction::RenamePath {
                            from,
                            to,
                            overwrite,
                        } => {
                            let root = self.state.workspace_root.as_path();
                            if from.as_path() == root
                                || to.as_path() == root
                                || !from.starts_with(root)
                                || !to.starts_with(root)
                            {
                                return DispatchResult {
                                    effects: Vec::new(),
                                    state_changed: true,
                                };
                            }

                            return DispatchResult {
                                effects: vec![Effect::RenamePath {
                                    from,
                                    to,
                                    overwrite,
                                }],
                                state_changed: true,
                            };
                        }
                        PendingAction::CopyPath {
                            from,
                            to,
                            overwrite,
                        } => {
                            let root = self.state.workspace_root.as_path();
                            if from.as_path() == root
                                || to.as_path() == root
                                || !from.starts_with(root)
                                || !to.starts_with(root)
                            {
                                return DispatchResult {
                                    effects: Vec::new(),
                                    state_changed: true,
                                };
                            }

                            return DispatchResult {
                                effects: vec![Effect::CopyPath {
                                    from,
                                    to,
                                    overwrite,
                                }],
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
            _ => unreachable!("non-confirm-dialog action passed to reduce_confirm_dialog_action"),
        }
    }
}
