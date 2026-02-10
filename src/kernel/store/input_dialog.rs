use crate::kernel::{Action, BottomPanelTab, Effect, FocusTarget, InputDialogKind};

impl super::Store {
    pub(super) fn reduce_input_dialog_action(
        &mut self,
        action: Action,
    ) -> super::super::DispatchResult {
        match action {
            Action::InputDialogAppend(ch) => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible {
                    return super::DispatchResult {
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
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::InputDialogBackspace => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible || dialog.cursor == 0 {
                    return super::DispatchResult {
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
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            Action::InputDialogCursorLeft => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible || dialog.cursor == 0 {
                    return super::DispatchResult {
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
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::InputDialogCursorRight => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible || dialog.cursor >= dialog.value.len() {
                    return super::DispatchResult {
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
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: changed,
                }
            }
            Action::InputDialogAccept => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let Some(kind) = dialog.kind.as_ref() else {
                    dialog.reset();
                    return super::DispatchResult {
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
                            return super::DispatchResult {
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
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                    InputDialogKind::LspRename { .. } => {
                        if value.is_empty() {
                            let prev = dialog.error.replace("Name required".to_string());
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                        if value.chars().any(|ch| ch.is_whitespace()) {
                            let prev = dialog
                                .error
                                .replace("Name cannot contain spaces".to_string());
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                    InputDialogKind::LspWorkspaceSymbols => {
                        if value.is_empty() {
                            let prev = dialog.error.replace("Query required".to_string());
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                    InputDialogKind::GitWorktreeAdd { .. } => {
                        if value.is_empty() {
                            let prev = dialog.error.replace("Branch required".to_string());
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                        if value.chars().any(|ch| ch.is_whitespace()) {
                            let prev = dialog
                                .error
                                .replace("Branch cannot contain spaces".to_string());
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                        if value.contains('\\') || value.contains("..") || value.starts_with('/') {
                            let prev = dialog.error.replace("Invalid branch".to_string());
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: prev.as_deref() != dialog.error.as_deref(),
                            };
                        }
                    }
                }

                let value = value.to_string();
                let kind = dialog.kind.take();
                dialog.reset();

                let Some(kind) = kind else {
                    return super::DispatchResult {
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
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: true,
                            };
                        };
                        let to = parent.join(&value);
                        if to == from {
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: true,
                            };
                        }
                        let root = self.state.workspace_root.as_path();
                        if from.as_path() == root
                            || to.as_path() == root
                            || !from.starts_with(root)
                            || !to.starts_with(root)
                        {
                            return super::DispatchResult {
                                effects: Vec::new(),
                                state_changed: true,
                            };
                        }
                        Effect::RenamePath {
                            from,
                            to,
                            overwrite: false,
                        }
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

                super::DispatchResult {
                    effects: vec![effect],
                    state_changed: true,
                }
            }
            Action::InputDialogCancel => {
                let dialog = &mut self.state.ui.input_dialog;
                if !dialog.visible {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }
                dialog.reset();
                super::DispatchResult {
                    effects: Vec::new(),
                    state_changed: true,
                }
            }
            _ => unreachable!("non-input-dialog action passed to reduce_input_dialog_action"),
        }
    }
}
