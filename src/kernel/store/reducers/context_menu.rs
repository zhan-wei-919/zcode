use crate::core::Command;
use crate::kernel::editor::{EditorAction, TabId};
use crate::kernel::state::{
    ContextMenuAction, ContextMenuEntry, ContextMenuRequest, ContextMenuState,
    ExplorerClipboardMode, ExplorerMenuAction, PendingAction, TabMenuAction,
};
use crate::kernel::{Action, Effect, FocusTarget, SidebarTab};
use std::path::{Path, PathBuf};

fn action_entry(label: &'static str, action: ContextMenuAction, enabled: bool) -> ContextMenuEntry {
    if enabled {
        ContextMenuEntry::action(label, action)
    } else {
        ContextMenuEntry::disabled_action(label, action)
    }
}

impl super::Store {
    fn explorer_selected_path_text(&self, relative: bool) -> Option<String> {
        let (path, _) = self.state.explorer.selected_path_and_kind()?;
        if !relative {
            return Some(path.to_string_lossy().to_string());
        }

        Some(
            path.strip_prefix(&self.state.workspace_root)
                .ok()
                .and_then(|rel| {
                    if rel.as_os_str().is_empty() {
                        None
                    } else {
                        Some(rel.to_string_lossy().to_string())
                    }
                })
                .unwrap_or_else(|| path.to_string_lossy().to_string()),
        )
    }

    fn first_selectable_context_menu_index(items: &[ContextMenuEntry]) -> Option<usize> {
        items.iter().position(ContextMenuEntry::is_selectable)
    }

    fn move_context_menu_selection(
        items: &[ContextMenuEntry],
        current: usize,
        delta: isize,
    ) -> Option<usize> {
        if items.is_empty() || delta == 0 {
            return None;
        }
        if !items.iter().any(ContextMenuEntry::is_selectable) {
            return None;
        }

        let len = items.len() as isize;
        let mut next = current.min(items.len().saturating_sub(1)) as isize;
        let step = if delta > 0 { 1 } else { -1 };

        for _ in 0..delta.unsigned_abs() {
            loop {
                next = (next + step).rem_euclid(len);
                if items[next as usize].is_selectable() {
                    break;
                }
            }
        }

        Some(next as usize)
    }

    fn open_context_menu(
        &mut self,
        request: ContextMenuRequest,
        x: u16,
        y: u16,
        items: Vec<ContextMenuEntry>,
    ) -> bool {
        let prev = self.state.ui.context_menu.clone();
        let selected = Self::first_selectable_context_menu_index(&items).unwrap_or(0);
        self.state.ui.context_menu = ContextMenuState {
            visible: true,
            anchor: (x, y),
            selected,
            items,
            request: Some(request),
        };
        self.state.ui.context_menu != prev
    }

    fn is_workspace_entry_path(&self, path: &Path) -> bool {
        let root = self.state.workspace_root.as_path();
        path != root && path.starts_with(root)
    }

    fn explorer_paste_target_path(&self, source: &Path, source_is_dir: bool) -> Option<PathBuf> {
        if !self.is_workspace_entry_path(source) {
            return None;
        }

        let destination_dir = self.state.explorer.selected_create_parent_dir();
        if !destination_dir.starts_with(&self.state.workspace_root) {
            return None;
        }

        let file_name = source.file_name()?;
        let target = destination_dir.join(file_name);
        if target.as_path() == source {
            return None;
        }

        if source_is_dir && target.starts_with(source) {
            return None;
        }

        Some(target)
    }

    pub(super) fn set_explorer_clipboard_from_selection(
        &mut self,
        mode: ExplorerClipboardMode,
    ) -> bool {
        let Some((path, is_dir)) = self.state.explorer.selected_path_and_kind() else {
            return false;
        };
        if !self.is_workspace_entry_path(path.as_path()) {
            return false;
        }

        self.state.explorer.set_clipboard(path, is_dir, mode)
    }

    pub(super) fn explorer_paste_effect(&self) -> Option<Effect> {
        let payload = self.state.explorer.clipboard()?.clone();
        let to = self.explorer_paste_target_path(payload.path.as_path(), payload.is_dir)?;

        Some(match payload.mode {
            ExplorerClipboardMode::Cut => Effect::RenamePath {
                from: payload.path,
                to,
                overwrite: false,
            },
            ExplorerClipboardMode::Copy => Effect::CopyPath {
                from: payload.path,
                to,
                overwrite: false,
            },
        })
    }

    fn build_explorer_context_menu_items(&self) -> Vec<ContextMenuEntry> {
        let can_mutate_selected = self
            .state
            .explorer
            .selected_path_and_kind()
            .is_some_and(|(path, _)| self.is_workspace_entry_path(path.as_path()));
        let can_copy_path = self.state.explorer.selected_path_and_kind().is_some();
        let can_paste = self.explorer_paste_effect().is_some();

        vec![
            action_entry(
                "New File",
                ContextMenuAction::Explorer(ExplorerMenuAction::NewFile),
                true,
            ),
            action_entry(
                "New Folder",
                ContextMenuAction::Explorer(ExplorerMenuAction::NewFolder),
                true,
            ),
            ContextMenuEntry::separator(),
            action_entry(
                "Cut",
                ContextMenuAction::Explorer(ExplorerMenuAction::Cut),
                can_mutate_selected,
            ),
            action_entry(
                "Copy",
                ContextMenuAction::Explorer(ExplorerMenuAction::Copy),
                can_mutate_selected,
            ),
            action_entry(
                "Paste",
                ContextMenuAction::Explorer(ExplorerMenuAction::Paste),
                can_paste,
            ),
            ContextMenuEntry::separator(),
            action_entry(
                "Rename",
                ContextMenuAction::Explorer(ExplorerMenuAction::Rename),
                can_mutate_selected,
            ),
            action_entry(
                "Delete",
                ContextMenuAction::Explorer(ExplorerMenuAction::Delete),
                can_mutate_selected,
            ),
            ContextMenuEntry::separator(),
            action_entry(
                "Copy Path",
                ContextMenuAction::Explorer(ExplorerMenuAction::CopyPath),
                can_copy_path,
            ),
            action_entry(
                "Copy Relative Path",
                ContextMenuAction::Explorer(ExplorerMenuAction::CopyRelativePath),
                can_copy_path,
            ),
        ]
    }

    fn build_tab_context_menu_items(
        &self,
        pane: usize,
        index: Option<usize>,
    ) -> Vec<ContextMenuEntry> {
        let tab_count = self
            .state
            .editor
            .pane(pane)
            .map(|pane_state| pane_state.tabs.len())
            .unwrap_or(0);
        let can_target_tab = index.is_some_and(|idx| idx < tab_count);

        vec![
            action_entry(
                "Close",
                ContextMenuAction::Tab(TabMenuAction::Close),
                can_target_tab,
            ),
            action_entry(
                "Close Others",
                ContextMenuAction::Tab(TabMenuAction::CloseOthers),
                can_target_tab && tab_count > 1,
            ),
            action_entry(
                "Close to the Right",
                ContextMenuAction::Tab(TabMenuAction::CloseToRight),
                can_target_tab && index.is_some_and(|idx| idx + 1 < tab_count),
            ),
            action_entry(
                "Close All",
                ContextMenuAction::Tab(TabMenuAction::CloseAll),
                tab_count > 0,
            ),
            ContextMenuEntry::separator(),
            action_entry(
                "Split Right",
                ContextMenuAction::Tab(TabMenuAction::SplitRight),
                can_target_tab,
            ),
            action_entry(
                "Split Down",
                ContextMenuAction::Tab(TabMenuAction::SplitDown),
                can_target_tab,
            ),
        ]
    }

    fn build_editor_area_context_menu_items(&self, pane: usize) -> Vec<ContextMenuEntry> {
        let has_active_tab = self
            .state
            .editor
            .pane(pane)
            .and_then(|pane_state| pane_state.active_tab())
            .is_some();

        vec![
            action_entry(
                "Cut",
                ContextMenuAction::RunCommand(Command::Cut),
                has_active_tab,
            ),
            action_entry(
                "Copy",
                ContextMenuAction::RunCommand(Command::Copy),
                has_active_tab,
            ),
            action_entry(
                "Paste",
                ContextMenuAction::RunCommand(Command::Paste),
                has_active_tab,
            ),
            action_entry(
                "Select All",
                ContextMenuAction::RunCommand(Command::SelectAll),
                has_active_tab,
            ),
            ContextMenuEntry::separator(),
            action_entry(
                "Go to Definition",
                ContextMenuAction::RunCommand(Command::LspDefinition),
                has_active_tab,
            ),
            action_entry(
                "Find References",
                ContextMenuAction::RunCommand(Command::LspReferences),
                has_active_tab,
            ),
            action_entry(
                "Rename Symbol",
                ContextMenuAction::RunCommand(Command::LspRename),
                has_active_tab,
            ),
            ContextMenuEntry::separator(),
            action_entry(
                "Format Document",
                ContextMenuAction::RunCommand(Command::LspFormat),
                has_active_tab,
            ),
            action_entry(
                "Code Action",
                ContextMenuAction::RunCommand(Command::LspCodeAction),
                has_active_tab,
            ),
        ]
    }

    fn close_tabs_with_unsaved_guard(
        &mut self,
        pane: usize,
        tab_ids: Vec<u64>,
    ) -> super::DispatchResult {
        if tab_ids.is_empty() {
            return super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        }

        let has_dirty = self.state.editor.pane(pane).is_some_and(|pane_state| {
            pane_state
                .tabs
                .iter()
                .any(|tab| tab_ids.contains(&tab.id.raw()) && tab.dirty)
        });

        if has_dirty {
            self.state.ui.confirm_dialog.visible = true;
            self.state.ui.confirm_dialog.message = if tab_ids.len() == 1 {
                "Unsaved changes. Close anyway?".to_string()
            } else {
                format!("Unsaved changes. Close {} tabs anyway?", tab_ids.len())
            };
            self.state.ui.confirm_dialog.on_confirm =
                Some(PendingAction::CloseTabsBatch { pane, tab_ids });
            return super::DispatchResult {
                effects: Vec::new(),
                state_changed: true,
            };
        }

        let mut result = self.dispatch(Action::Editor(EditorAction::CloseTabsById {
            pane,
            tab_ids,
        }));
        result.state_changed = true;
        result
    }

    fn split_tab_to_other_pane(
        &mut self,
        pane: usize,
        tab_id: u64,
        split_command: Command,
    ) -> super::DispatchResult {
        let mut result = self.dispatch(Action::RunCommand(split_command));
        if self.state.ui.editor_layout.panes < 2 {
            result.state_changed = true;
            return result;
        }

        let to_pane = if pane == 0 { 1 } else { 0 };
        let to_index = self
            .state
            .editor
            .pane(to_pane)
            .map(|pane_state| pane_state.tabs.len())
            .unwrap_or(0);

        let (moved, mut move_effects) = self.state.editor.dispatch_action(EditorAction::MoveTab {
            tab_id: TabId::new(tab_id),
            from_pane: pane,
            to_pane,
            to_index,
        });
        result.effects.append(&mut move_effects);

        if moved {
            self.state.ui.editor_layout.active_pane = to_pane;
            self.state.ui.focus = FocusTarget::Editor;
            self.push_git_refresh_for_pane(pane, &mut result.effects);
            self.push_git_refresh_for_pane(to_pane, &mut result.effects);
            result.state_changed = true;
        }

        result.state_changed = true;
        result
    }

    fn dispatch_tab_menu_action(
        &mut self,
        action: TabMenuAction,
        request: Option<ContextMenuRequest>,
    ) -> super::DispatchResult {
        let Some((pane, request_index)) = request.and_then(|req| match req {
            ContextMenuRequest::Tab { pane, index } => Some((pane, Some(index))),
            ContextMenuRequest::TabBar { pane } => Some((pane, None)),
            _ => None,
        }) else {
            return super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            };
        };

        let tab_ids = self
            .state
            .editor
            .pane(pane)
            .map(|pane_state| {
                pane_state
                    .tabs
                    .iter()
                    .map(|tab| tab.id.raw())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let tab_count = tab_ids.len();
        let target_index = request_index.filter(|idx| *idx < tab_count);

        match action {
            TabMenuAction::Close => {
                let Some(index) = target_index else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };
                self.close_tabs_with_unsaved_guard(pane, vec![tab_ids[index]])
            }
            TabMenuAction::CloseOthers => {
                let Some(index) = target_index else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let close_ids = tab_ids
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, tab_id)| (idx != index).then_some(*tab_id))
                    .collect::<Vec<_>>();
                self.close_tabs_with_unsaved_guard(pane, close_ids)
            }
            TabMenuAction::CloseToRight => {
                let Some(index) = target_index else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                let close_ids = tab_ids
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, tab_id)| (idx > index).then_some(*tab_id))
                    .collect::<Vec<_>>();
                self.close_tabs_with_unsaved_guard(pane, close_ids)
            }
            TabMenuAction::CloseAll => self.close_tabs_with_unsaved_guard(pane, tab_ids),
            TabMenuAction::SplitRight => {
                let Some(index) = target_index else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };
                self.split_tab_to_other_pane(pane, tab_ids[index], Command::SplitEditorVertical)
            }
            TabMenuAction::SplitDown => {
                let Some(index) = target_index else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };
                self.split_tab_to_other_pane(pane, tab_ids[index], Command::SplitEditorHorizontal)
            }
        }
    }

    fn dispatch_explorer_menu_action(
        &mut self,
        action: ExplorerMenuAction,
    ) -> super::DispatchResult {
        match action {
            ExplorerMenuAction::NewFile => {
                let mut result = self.dispatch(Action::RunCommand(Command::ExplorerNewFile));
                result.state_changed = true;
                result
            }
            ExplorerMenuAction::NewFolder => {
                let mut result = self.dispatch(Action::RunCommand(Command::ExplorerNewFolder));
                result.state_changed = true;
                result
            }
            ExplorerMenuAction::Rename => {
                let mut result = self.dispatch(Action::RunCommand(Command::ExplorerRename));
                result.state_changed = true;
                result
            }
            ExplorerMenuAction::Delete => {
                let mut result = self.dispatch(Action::RunCommand(Command::ExplorerDelete));
                result.state_changed = true;
                result
            }
            ExplorerMenuAction::CopyPath => {
                let Some(text) = self.explorer_selected_path_text(false) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                super::DispatchResult {
                    effects: vec![Effect::SetClipboardText(text)],
                    state_changed: true,
                }
            }
            ExplorerMenuAction::CopyRelativePath => {
                let Some(text) = self.explorer_selected_path_text(true) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                super::DispatchResult {
                    effects: vec![Effect::SetClipboardText(text)],
                    state_changed: true,
                }
            }
            ExplorerMenuAction::Cut => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self
                    .set_explorer_clipboard_from_selection(ExplorerClipboardMode::Cut),
            },
            ExplorerMenuAction::Copy => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self
                    .set_explorer_clipboard_from_selection(ExplorerClipboardMode::Copy),
            },
            ExplorerMenuAction::Paste => {
                let Some(effect) = self.explorer_paste_effect() else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

                super::DispatchResult {
                    effects: vec![effect],
                    state_changed: false,
                }
            }
        }
    }

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

                let request_for_menu = request.clone();
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

                        let items = self.build_explorer_context_menu_items();
                        state_changed |= self.open_context_menu(request_for_menu, x, y, items);

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
                        if self.state.ui.editor_layout.active_pane != pane {
                            self.state.ui.editor_layout.active_pane = pane;
                            state_changed = true;
                        }

                        let items = self.build_tab_context_menu_items(pane, Some(index));
                        state_changed |= self.open_context_menu(request_for_menu, x, y, items);

                        super::DispatchResult {
                            effects: Vec::new(),
                            state_changed,
                        }
                    }
                    ContextMenuRequest::TabBar { pane } => {
                        if self.state.ui.focus != FocusTarget::Editor {
                            self.state.ui.focus = FocusTarget::Editor;
                            state_changed = true;
                        }
                        if self.state.ui.editor_layout.active_pane != pane {
                            self.state.ui.editor_layout.active_pane = pane;
                            state_changed = true;
                        }

                        let items = self.build_tab_context_menu_items(pane, None);
                        state_changed |= self.open_context_menu(request_for_menu, x, y, items);

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

                        let items = self.build_editor_area_context_menu_items(pane);
                        state_changed |= self.open_context_menu(request_for_menu, x, y, items);

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

                let items = &self.state.ui.context_menu.items;
                let current = self.state.ui.context_menu.selected;
                let Some(next) = Self::move_context_menu_selection(items, current, delta) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                };

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

                if index >= self.state.ui.context_menu.items.len()
                    || !self.state.ui.context_menu.items[index].is_selectable()
                {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let changed = index != self.state.ui.context_menu.selected;
                self.state.ui.context_menu.selected = index;
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
                let item = self.state.ui.context_menu.items.get(selected).cloned();
                let request = self.state.ui.context_menu.request.clone();
                self.state.ui.context_menu = ContextMenuState::default();

                let Some(action) = item.and_then(|entry| entry.enabled_action().cloned()) else {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: true,
                    };
                };

                let mut result = match action {
                    ContextMenuAction::RunCommand(command) => {
                        self.dispatch(Action::RunCommand(command))
                    }
                    ContextMenuAction::Tab(tab_action) => {
                        self.dispatch_tab_menu_action(tab_action, request)
                    }
                    ContextMenuAction::Explorer(explorer_action) => {
                        self.dispatch_explorer_menu_action(explorer_action)
                    }
                };

                result.state_changed = true;
                result
            }
            _ => super::DispatchResult {
                effects: Vec::new(),
                state_changed: false,
            },
        }
    }
}
