use crate::core::Command;
use crate::kernel::state::ExplorerClipboardMode;
use crate::kernel::{FocusTarget, InputDialogKind, PendingAction};

use super::DispatchResult;

impl super::Store {
    pub(super) fn reduce_explorer_command(&mut self, command: Command) -> DispatchResult {
        let mut state_changed = false;
        let effects = Vec::new();

        match command {
            Command::ExplorerUp => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.move_selection(-1);
                }
            }
            Command::ExplorerDown => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.move_selection(1);
                }
            }
            Command::ExplorerActivate => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    let (changed, fx) = self.state.explorer.activate_selected();
                    return DispatchResult {
                        effects: fx,
                        state_changed: changed,
                    };
                }
            }
            Command::ExplorerCollapse => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.collapse_selected();
                }
            }
            Command::ExplorerScrollUp => {
                if self.state.ui.focus == FocusTarget::Explorer {
                    state_changed = self.state.explorer.scroll(-3);
                }
            }
            Command::ExplorerScrollDown => {
                if self.state.ui.focus == FocusTarget::Explorer {
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
                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "New File".to_string();
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
                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "New Folder".to_string();
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
                let root = self.state.workspace_root.as_path();
                if path.as_path() == root || !path.starts_with(root) {
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

                self.state.ui.input_dialog.reset();
                self.state.ui.input_dialog.visible = true;
                self.state.ui.input_dialog.title = "Rename".to_string();
                self.state.ui.input_dialog.value = file_name;
                self.state.ui.input_dialog.cursor = self.state.ui.input_dialog.value.len();
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
                let root = self.state.workspace_root.as_path();
                if path.as_path() == root || !path.starts_with(root) {
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
                    Some(PendingAction::DeletePath { path, is_dir });
                state_changed = true;
            }
            Command::ExplorerCut => {
                state_changed =
                    self.set_explorer_clipboard_from_selection(ExplorerClipboardMode::Cut);
            }
            Command::ExplorerCopy => {
                state_changed =
                    self.set_explorer_clipboard_from_selection(ExplorerClipboardMode::Copy);
            }
            Command::ExplorerPaste => {
                let Some(effect) = self.explorer_paste_effect() else {
                    return DispatchResult {
                        effects,
                        state_changed: false,
                    };
                };
                return DispatchResult {
                    effects: vec![effect],
                    state_changed: false,
                };
            }
            _ => unreachable!("non-explorer command passed to reduce_explorer_command"),
        }

        DispatchResult {
            effects,
            state_changed,
        }
    }
}
