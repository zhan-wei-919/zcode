use crate::core::Command;

use super::{
    Action, AppState, BottomPanelTab, EditorAction, Effect, FocusTarget, SearchResultItem,
    SearchViewport, SidebarTab, SplitDirection,
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

    pub fn dispatch(&mut self, action: Action) -> DispatchResult {
        match action {
            Action::RunCommand(cmd) => self.dispatch_command(cmd),
            Action::Editor(editor_action) => match editor_action {
                EditorAction::OpenFile {
                    pane,
                    path,
                    content,
                } => {
                    let pending = self
                        .state
                        .ui
                        .pending_editor_nav
                        .as_ref()
                        .filter(|p| p.pane == pane && p.path == path)
                        .map(|p| p.byte_offset);

                    let (mut state_changed, mut effects) =
                        self.state.editor.dispatch_action(EditorAction::OpenFile {
                            pane,
                            path,
                            content,
                        });

                    if let Some(byte_offset) = pending {
                        let (changed, cmd_effects) = self
                            .state
                            .editor
                            .dispatch_action(EditorAction::GotoByteOffset { pane, byte_offset });
                        state_changed |= changed;
                        effects.extend(cmd_effects);
                        self.state.ui.pending_editor_nav = None;
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
            },
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
            Action::EditorSetActivePane { pane } => {
                let panes = self.state.ui.editor_layout.panes.max(1);
                let pane = pane.min(panes - 1);
                let prev = self.state.ui.editor_layout.active_pane;
                let prev_focus = self.state.ui.focus;

                self.state.ui.editor_layout.active_pane = pane;
                self.state.ui.focus = FocusTarget::Editor;

                DispatchResult {
                    effects: Vec::new(),
                    state_changed: pane != prev || prev_focus != FocusTarget::Editor,
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
            Action::BottomPanelSetActiveTab { tab } => {
                let prev_visible = self.state.ui.bottom_panel.visible;
                let prev = self.state.ui.bottom_panel.active_tab;
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.bottom_panel.active_tab = tab;
                DispatchResult {
                    effects: Vec::new(),
                    state_changed: !prev_visible || prev != tab,
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
            Action::DirLoaded { path, entries } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_dir_loaded(path, entries),
            },
            Action::DirLoadError { path } => DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.explorer.apply_dir_load_error(path),
            },
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
                            let (changed, effects) = self.state.editor.close_tab_at(pane, index);
                            return DispatchResult {
                                effects,
                                state_changed: changed || true,
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
                state_changed = true;
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
                self.state.ui.bottom_panel.active_tab = match self.state.ui.bottom_panel.active_tab
                {
                    BottomPanelTab::Problems => BottomPanelTab::SearchResults,
                    BottomPanelTab::SearchResults => BottomPanelTab::Logs,
                    BottomPanelTab::Logs => BottomPanelTab::Problems,
                };
                state_changed = true;
            }
            Command::PrevBottomPanelTab => {
                self.state.ui.bottom_panel.visible = true;
                self.state.ui.bottom_panel.active_tab = match self.state.ui.bottom_panel.active_tab
                {
                    BottomPanelTab::Problems => BottomPanelTab::Logs,
                    BottomPanelTab::SearchResults => BottomPanelTab::Problems,
                    BottomPanelTab::Logs => BottomPanelTab::SearchResults,
                };
                state_changed = true;
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
                let matches = crate::kernel::palette::match_indices(&query);

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
                let cmd = crate::kernel::palette::PALETTE_ITEMS[matches[selected]]
                    .command
                    .clone();

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
                }
            }
            Command::SearchResultsMoveDown => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.move_selection(1, viewport);
                }
            }
            Command::SearchResultsScrollUp => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.scroll(-3, viewport);
                }
            }
            Command::SearchResultsScrollDown => {
                if let Some(viewport) = search_viewport_for_focus(&self.state.ui) {
                    state_changed = self.state.search.scroll(3, viewport);
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
                            byte_offset,
                        });

                    return DispatchResult {
                        effects: vec![Effect::LoadFile(path)],
                        state_changed: true,
                    };
                }
            }
            Command::OpenFile => {
                // UI should translate selection -> path and dispatch Action::OpenPath.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::services::ports::EditorConfig;
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
}
