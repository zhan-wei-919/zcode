use super::util;
use super::Workbench;
use crate::core::event::Key;
use crate::core::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{KeybindingContext, KeybindingService};
use crate::kernel::{
    Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, PendingAction,
    SearchResultItem, SearchViewport, SidebarTab,
};
use crate::tui::view::EventResult;
use crate::views::{
    compute_editor_pane_layout, hit_test_editor_mouse, hit_test_editor_tab, hit_test_tab_hover,
    TabHitResult,
};
use ratatui::layout::Rect;
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

impl Workbench {
    pub(super) fn record_user_input(&mut self) {
        self.last_input_at = Instant::now();
        self.idle_hover_last_request = None;
        self.pending_completion_deadline = None;
        self.pending_inlay_hints_deadline = None;
        self.pending_folding_range_deadline = None;
        if self.store.state().ui.focus == FocusTarget::BottomPanel
            && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal
        {
            self.terminal_cursor_visible = true;
            self.terminal_cursor_last_blink = Instant::now();
        }

        if let Some(service) = self
            .kernel_services
            .get_mut::<crate::kernel::services::adapters::LspService>()
        {
            service.cancel_hover();
        }

        if self.store.state().ui.hover_message.is_some() {
            let _ = self.dispatch_kernel(KernelAction::LspHover {
                text: String::new(),
            });
        }
    }

    pub(super) fn handle_key_event(&mut self, key_event: &KeyEvent) -> EventResult {
        let _scope = perf::scope("input.key");

        if self.store.state().ui.explorer_context_menu.visible {
            match key_event.code {
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuClose);
                    return EventResult::Consumed;
                }
                KeyCode::Up => {
                    let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuMoveSelection {
                        delta: -1,
                    });
                    return EventResult::Consumed;
                }
                KeyCode::Down => {
                    let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuMoveSelection {
                        delta: 1,
                    });
                    return EventResult::Consumed;
                }
                KeyCode::Enter => {
                    let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuConfirm);
                    return EventResult::Consumed;
                }
                _ => {
                    let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuClose);
                }
            }
        }

        if self.store.state().ui.input_dialog.visible {
            match (key_event.code, key_event.modifiers) {
                (KeyCode::Enter, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogAccept);
                    return EventResult::Consumed;
                }
                (KeyCode::Esc, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCancel);
                    return EventResult::Consumed;
                }
                (KeyCode::Backspace, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogBackspace);
                    return EventResult::Consumed;
                }
                (KeyCode::Left, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCursorLeft);
                    return EventResult::Consumed;
                }
                (KeyCode::Right, _) => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogCursorRight);
                    return EventResult::Consumed;
                }
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::InputDialogAppend(ch));
                    return EventResult::Consumed;
                }
                _ => return EventResult::Consumed,
            }
        }

        if self.store.state().ui.confirm_dialog.visible {
            match key_event.code {
                KeyCode::Enter => {
                    let _ = self.dispatch_kernel(KernelAction::ConfirmDialogAccept);
                    return EventResult::Consumed;
                }
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::ConfirmDialogCancel);
                    return EventResult::Consumed;
                }
                _ => return EventResult::Consumed,
            }
        }

        if self.store.state().ui.completion.visible {
            match key_event.code {
                KeyCode::Esc => {
                    let _ = self.dispatch_kernel(KernelAction::CompletionClose);
                    return EventResult::Consumed;
                }
                KeyCode::Tab => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: 1 });
                    return EventResult::Consumed;
                }
                KeyCode::BackTab => {
                    let _ =
                        self.dispatch_kernel(KernelAction::CompletionMoveSelection { delta: -1 });
                    return EventResult::Consumed;
                }
                KeyCode::Enter => {
                    let pane = self.store.state().ui.editor_layout.active_pane;
                    let before_version = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                        .map(|tab| tab.edit_version);
                    let _ = self.dispatch_kernel(KernelAction::CompletionConfirm);
                    let after_version = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|pane| pane.active_tab())
                        .map(|tab| tab.edit_version);
                    if before_version != after_version {
                        let refresh = Command::Paste;
                        self.maybe_schedule_semantic_tokens_debounce(&refresh);
                        self.maybe_schedule_inlay_hints_debounce(&refresh);
                        self.maybe_schedule_folding_range_debounce(&refresh);
                    }
                    return EventResult::Consumed;
                }
                _ => {}
            }
        }

        let context = self.keybinding_context();
        let key: Key = (*key_event).into();

        let cmd = self
            .kernel_services
            .get::<KeybindingService>()
            .and_then(|service| service.resolve(context, &key).cloned());

        let terminal_active = self.store.state().ui.focus == FocusTarget::BottomPanel
            && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal;

        if terminal_active
            && !matches!(
                cmd.as_ref(),
                Some(Command::ToggleBottomPanel | Command::FocusBottomPanel)
            )
        {
            if let Some(bytes) = terminal_bytes_for_key_event(key_event) {
                if let Some(id) = self
                    .store
                    .state()
                    .terminal
                    .active_session()
                    .map(|s| s.id)
                {
                    let _ = self.dispatch_kernel(KernelAction::TerminalWrite { id, bytes });
                }
                return EventResult::Consumed;
            }
        }

        if let Some(cmd) = cmd {
            if cmd == Command::Copy
                && self.store.state().ui.focus == FocusTarget::BottomPanel
                && self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Logs
            {
                self.copy_logs_to_clipboard();
                return EventResult::Consumed;
            }

            let cmd_for_schedule = cmd.clone();
            let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd));
            self.maybe_schedule_completion_debounce(&cmd_for_schedule);
            self.maybe_schedule_semantic_tokens_debounce(&cmd_for_schedule);
            self.maybe_schedule_inlay_hints_debounce(&cmd_for_schedule);
            self.maybe_schedule_folding_range_debounce(&cmd_for_schedule);
            if self.store.state().ui.should_quit {
                return EventResult::Quit;
            }
            return EventResult::Consumed;
        }

        match context {
            KeybindingContext::EditorSearchBar => {
                let pane = self.store.state().ui.editor_layout.active_pane;
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                        let _ = self.dispatch_kernel(KernelAction::Editor(
                            EditorAction::SearchBarAppend { pane, ch },
                        ));
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                }
            }
            KeybindingContext::Editor => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let cmd = Command::InsertChar(ch);
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd.clone()));
                    self.maybe_schedule_completion_debounce(&cmd);
                    self.maybe_schedule_semantic_tokens_debounce(&cmd);
                    self.maybe_schedule_inlay_hints_debounce(&cmd);
                    self.maybe_schedule_folding_range_debounce(&cmd);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            KeybindingContext::SidebarSearch => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::SearchAppend(ch));
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            KeybindingContext::CommandPalette => match (key_event.code, key_event.modifiers) {
                (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                    let _ = self.dispatch_kernel(KernelAction::PaletteAppend(ch));
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            },
            _ => EventResult::Ignored,
        }
    }

    pub(super) fn handle_paste(&mut self, text: &str) -> EventResult {
        let _scope = perf::scope("input.paste");
        let context = self.keybinding_context();
        match context {
            KeybindingContext::Editor => {
                let pane = self.store.state().ui.editor_layout.active_pane;
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::InsertText {
                    pane,
                    text: text.to_string(),
                }));
                let refresh = Command::Paste;
                self.maybe_schedule_semantic_tokens_debounce(&refresh);
                self.maybe_schedule_inlay_hints_debounce(&refresh);
                self.maybe_schedule_folding_range_debounce(&refresh);
                EventResult::Consumed
            }
            KeybindingContext::EditorSearchBar => {
                let pane = self.store.state().ui.editor_layout.active_pane;
                for ch in text.chars() {
                    let _ =
                        self.dispatch_kernel(KernelAction::Editor(EditorAction::SearchBarAppend {
                            pane,
                            ch,
                        }));
                }
                EventResult::Consumed
            }
            KeybindingContext::SidebarSearch => {
                for ch in text.chars() {
                    let _ = self.dispatch_kernel(KernelAction::SearchAppend(ch));
                }
                EventResult::Consumed
            }
            KeybindingContext::CommandPalette => {
                for ch in text.chars() {
                    let _ = self.dispatch_kernel(KernelAction::PaletteAppend(ch));
                }
                EventResult::Consumed
            }
            KeybindingContext::BottomPanel => {
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    if let Some(id) = self
                        .store
                        .state()
                        .terminal
                        .active_session()
                        .map(|s| s.id)
                    {
                        let _ = self.dispatch_kernel(KernelAction::TerminalWrite {
                            id,
                            bytes: text.as_bytes().to_vec(),
                        });
                        return EventResult::Consumed;
                    }
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn copy_logs_to_clipboard(&mut self) {
        let Some(clipboard) = self
            .kernel_services
            .get_mut::<crate::kernel::services::adapters::ClipboardService>()
        else {
            return;
        };

        if self.logs.is_empty() {
            return;
        }

        let mut text = String::new();
        for (idx, line) in self.logs.iter().enumerate() {
            if idx > 0 {
                text.push('\n');
            }
            text.push_str(line);
        }

        if let Err(err) = clipboard.set_text(&text) {
            self.logs.push_back(format!("[clipboard] {err}"));
            while self.logs.len() > super::LOG_BUFFER_CAP {
                self.logs.pop_front();
            }
        }
    }

    fn keybinding_context(&self) -> KeybindingContext {
        let ui = &self.store.state().ui;

        if ui.command_palette.visible && ui.focus == FocusTarget::CommandPalette {
            return KeybindingContext::CommandPalette;
        }

        match ui.focus {
            FocusTarget::Explorer => match ui.sidebar_tab {
                SidebarTab::Explorer => KeybindingContext::SidebarExplorer,
                SidebarTab::Search => KeybindingContext::SidebarSearch,
            },
            FocusTarget::BottomPanel => KeybindingContext::BottomPanel,
            FocusTarget::CommandPalette => KeybindingContext::CommandPalette,
            FocusTarget::Editor => {
                let pane = ui.editor_layout.active_pane;
                let visible = self
                    .store
                    .state()
                    .editor
                    .pane(pane)
                    .is_some_and(|p| p.search_bar.visible);
                if visible {
                    KeybindingContext::EditorSearchBar
                } else {
                    KeybindingContext::Editor
                }
            }
        }
    }

    fn maybe_schedule_completion_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let should_schedule = match cmd {
            Command::InsertChar(ch) => completion_debounce_triggered_by_inserted_char(*ch),
            Command::DeleteBackward | Command::DeleteForward => true,
            _ => false,
        };

        if !should_schedule {
            return;
        }

        if !completion_debounce_context_allowed(tab) {
            return;
        }

        self.pending_completion_deadline = Some(Instant::now() + super::COMPLETION_DEBOUNCE_DELAY);
    }

    fn maybe_schedule_semantic_tokens_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let edit_should_schedule = matches!(
            cmd,
            Command::InsertChar(_)
                | Command::InsertNewline
                | Command::InsertTab
                | Command::DeleteBackward
                | Command::DeleteForward
                | Command::DeleteLine
                | Command::DeleteToLineEnd
                | Command::DeleteSelection
                | Command::Undo
                | Command::Redo
                | Command::Paste
                | Command::Cut
        );

        let move_should_schedule = matches!(
            cmd,
            Command::CursorUp
                | Command::CursorDown
                | Command::ScrollUp
                | Command::ScrollDown
                | Command::PageUp
                | Command::PageDown
        ) && self
            .store
            .state()
            .lsp
            .server_capabilities
            .as_ref()
            .is_some_and(|c| c.semantic_tokens_range)
            && tab.buffer.len_lines().max(1) >= 2000;

        if edit_should_schedule || move_should_schedule {
            self.pending_semantic_tokens_deadline =
                Some(Instant::now() + super::SEMANTIC_TOKENS_DEBOUNCE_DELAY);
        }
    }

    fn maybe_schedule_inlay_hints_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let should_schedule = matches!(
            cmd,
            Command::InsertChar(_)
                | Command::InsertNewline
                | Command::InsertTab
                | Command::DeleteBackward
                | Command::DeleteForward
                | Command::DeleteLine
                | Command::DeleteToLineEnd
                | Command::DeleteSelection
                | Command::Undo
                | Command::Redo
                | Command::Paste
                | Command::Cut
                | Command::CursorUp
                | Command::CursorDown
                | Command::ScrollUp
                | Command::ScrollDown
                | Command::PageUp
                | Command::PageDown
        );

        if should_schedule {
            self.pending_inlay_hints_deadline =
                Some(Instant::now() + super::INLAY_HINTS_DEBOUNCE_DELAY);
        }
    }

    fn maybe_schedule_folding_range_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let should_schedule = matches!(
            cmd,
            Command::InsertChar(_)
                | Command::InsertNewline
                | Command::InsertTab
                | Command::DeleteBackward
                | Command::DeleteForward
                | Command::DeleteLine
                | Command::DeleteToLineEnd
                | Command::DeleteSelection
                | Command::Undo
                | Command::Redo
                | Command::Paste
                | Command::Cut
        );

        if should_schedule {
            self.pending_folding_range_deadline =
                Some(Instant::now() + super::FOLDING_RANGE_DEBOUNCE_DELAY);
        }
    }

    pub(super) fn handle_editor_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let _scope = perf::scope("input.mouse.editor");
        let active_pane = self.store.state().ui.editor_layout.active_pane;

        let pane = if self.store.state().editor.pane(active_pane).is_some() {
            active_pane
        } else {
            0
        };

        let area = self
            .last_editor_inner_areas
            .get(pane)
            .copied()
            .or_else(|| self.last_editor_inner_areas.first().copied());
        let Some(area) = area else {
            return EventResult::Ignored;
        };

        let Some(pane_state) = self.store.state().editor.pane(pane) else {
            return EventResult::Ignored;
        };
        let config = &self.store.state().editor.config;
        let layout = compute_editor_pane_layout(area, pane_state, config);

        let hovered_idx = self
            .store
            .state()
            .ui
            .hovered_tab
            .filter(|(hp, _)| *hp == pane)
            .map(|(_, i)| i);

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(result) =
                    hit_test_editor_tab(&layout, pane_state, event.column, event.row, hovered_idx)
                {
                    match result {
                        TabHitResult::Title(index) => {
                            let _ = self.dispatch_kernel(KernelAction::Editor(
                                EditorAction::SetActiveTab { pane, index },
                            ));
                        }
                        TabHitResult::CloseButton(index) => {
                            let is_dirty = self
                                .store
                                .state()
                                .editor
                                .pane(pane)
                                .is_some_and(|p| p.is_tab_dirty(index));

                            if is_dirty {
                                let _ = self.dispatch_kernel(KernelAction::ShowConfirmDialog {
                                    message: "Unsaved changes. Close anyway?".to_string(),
                                    on_confirm: PendingAction::CloseTab { pane, index },
                                });
                            } else {
                                let _ = self.dispatch_kernel(KernelAction::Editor(
                                    EditorAction::CloseTabAt { pane, index },
                                ));
                            }
                        }
                    }
                    return EventResult::Consumed;
                }

                if let Some((x, y)) = hit_test_editor_mouse(&layout, event.column, event.row) {
                    let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseDown {
                        pane,
                        x,
                        y,
                        now: Instant::now(),
                    }));
                    return EventResult::Consumed;
                }

                EventResult::Ignored
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                if let Some(index) =
                    hit_test_tab_hover(&layout, pane_state, event.column, event.row, hovered_idx)
                {
                    let is_dirty = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .is_some_and(|p| p.is_tab_dirty(index));

                    if is_dirty {
                        let _ = self.dispatch_kernel(KernelAction::ShowConfirmDialog {
                            message: "Unsaved changes. Close anyway?".to_string(),
                            on_confirm: PendingAction::CloseTab { pane, index },
                        });
                    } else {
                        let _ =
                            self.dispatch_kernel(KernelAction::Editor(EditorAction::CloseTabAt {
                                pane,
                                index,
                            }));
                    }
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((x, y)) = hit_test_editor_mouse(&layout, event.column, event.row) {
                    let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseDrag {
                        pane,
                        x,
                        y,
                    }));
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseUp { pane }));
                EventResult::Consumed
            }
            MouseEventKind::Moved => {
                if let Some(index) =
                    hit_test_tab_hover(&layout, pane_state, event.column, event.row, hovered_idx)
                {
                    let _ = self.dispatch_kernel(KernelAction::SetHoveredTab { pane, index });
                } else {
                    let _ = self.dispatch_kernel(KernelAction::ClearHoveredTab);
                }
                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                let delta_lines = -(config.scroll_step() as isize);
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::Scroll {
                    pane,
                    delta_lines,
                }));
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let delta_lines = config.scroll_step() as isize;
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::Scroll {
                    pane,
                    delta_lines,
                }));
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    pub(super) fn handle_explorer_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let _scope = perf::scope("input.mouse.explorer");
        let in_tree = self.explorer.contains(event.column, event.row);
        let in_git = self
            .last_git_panel_area
            .is_some_and(|a| util::rect_contains(a, event.column, event.row));

        if !in_tree && !in_git {
            return EventResult::Ignored;
        }

        let scroll_offset = self.store.state().explorer.scroll_offset;
        let rows_len = self.store.state().explorer.rows.len();

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if in_git {
                    let Some((branch, _)) = self
                        .last_git_branch_areas
                        .iter()
                        .find(|(_, rect)| util::rect_contains(*rect, event.column, event.row))
                    else {
                        return EventResult::Consumed;
                    };

                    let state = self.store.state();
                    let is_active = state.git.head.as_ref().is_some_and(|head| {
                        !head.detached && head.branch.as_deref() == Some(branch.as_str())
                    });
                    if !is_active {
                        let _ = self.dispatch_kernel(KernelAction::GitCheckoutBranch {
                            branch: branch.clone(),
                        });
                    }
                    return EventResult::Consumed;
                }

                if let Some(row) = self.explorer.hit_test_row(event, scroll_offset) {
                    if row < rows_len {
                        let _ = self.dispatch_kernel(KernelAction::ExplorerClickRow {
                            row,
                            now: Instant::now(),
                        });
                    }
                }
                EventResult::Consumed
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if in_git {
                    return EventResult::Consumed;
                }
                let tree_row = self
                    .explorer
                    .hit_test_row(event, scroll_offset)
                    .filter(|row| *row < rows_len);
                let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuOpen {
                    tree_row,
                    x: event.column,
                    y: event.row,
                });
                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                let _ = self.dispatch_kernel(KernelAction::ExplorerScroll { delta: -3 });
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let _ = self.dispatch_kernel(KernelAction::ExplorerScroll { delta: 3 });
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    pub(super) fn handle_explorer_context_menu_mouse(
        &mut self,
        event: &MouseEvent,
    ) -> Option<EventResult> {
        if !self.store.state().ui.explorer_context_menu.visible {
            return None;
        }

        let Some(area) = self.last_explorer_context_menu_area else {
            let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuClose);
            return None;
        };

        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        if inner.width == 0 || inner.height == 0 {
            let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuClose);
            return None;
        }

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Down(MouseButton::Right) => {
                if util::rect_contains(inner, event.column, event.row) {
                    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
                        let idx = event.row.saturating_sub(inner.y) as usize;
                        let _ =
                            self.dispatch_kernel(KernelAction::ExplorerContextMenuSetSelected {
                                index: idx,
                            });
                        let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuConfirm);
                    }
                    return Some(EventResult::Consumed);
                }

                if util::rect_contains(area, event.column, event.row) {
                    return Some(EventResult::Consumed);
                }

                let _ = self.dispatch_kernel(KernelAction::ExplorerContextMenuClose);
                None
            }
            _ => None,
        }
    }

    pub(super) fn handle_search_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let _scope = perf::scope("input.mouse.search");
        if !self.search_view.contains(event.column, event.row) {
            return EventResult::Ignored;
        }

        let viewport = SearchViewport::Sidebar;
        let scroll_offset = self.store.state().search.sidebar_view.scroll_offset;
        let items_len = self.store.state().search.items.len();

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(row) = self.search_view.hit_test_results_row(event, scroll_offset) {
                    if row < items_len {
                        let item = self.store.state().search.items.get(row).copied();
                        let _ =
                            self.dispatch_kernel(KernelAction::SearchClickRow { row, viewport });
                        match item {
                            Some(SearchResultItem::FileHeader { .. }) => {
                                let _ = self.dispatch_kernel(KernelAction::RunCommand(
                                    Command::SearchResultsToggleExpand,
                                ));
                            }
                            Some(SearchResultItem::MatchLine { .. }) => {
                                let _ = self.dispatch_kernel(KernelAction::RunCommand(
                                    Command::SearchResultsOpenSelected,
                                ));
                            }
                            None => {}
                        }
                        return EventResult::Consumed;
                    }
                }
                EventResult::Ignored
            }
            MouseEventKind::ScrollUp => {
                let _ = self.dispatch_kernel(KernelAction::SearchScroll {
                    delta: -3,
                    viewport,
                });
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let _ = self.dispatch_kernel(KernelAction::SearchScroll { delta: 3, viewport });
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    pub(super) fn handle_bottom_panel_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let Some(panel_area) = self.last_bottom_panel_area else {
            return EventResult::Ignored;
        };
        if !util::rect_contains(panel_area, event.column, event.row) {
            return EventResult::Ignored;
        }

        let inner = panel_area;
        if inner.width == 0 || inner.height == 0 {
            return EventResult::Ignored;
        }

        let tabs_area = Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height));
        let content_area = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if util::rect_contains(tabs_area, event.column, event.row) {
                    if tabs_area.width == 0 {
                        return EventResult::Consumed;
                    }
                    let rel = event.column.saturating_sub(tabs_area.x);
                    let mut offset = 0u16;
                    for (tab, label) in self.bottom_panel_tabs() {
                        let width = UnicodeWidthStr::width(label.as_str()) as u16;
                        if rel < offset.saturating_add(width) {
                            let _ =
                                self.dispatch_kernel(KernelAction::BottomPanelSetActiveTab { tab });
                            return EventResult::Consumed;
                        }
                        offset = offset.saturating_add(width);
                    }
                    return EventResult::Consumed;
                }

                let active_tab = self.store.state().ui.bottom_panel.active_tab.clone();
                if active_tab == BottomPanelTab::SearchResults {
                    if content_area.width == 0 || content_area.height == 0 {
                        return EventResult::Ignored;
                    }

                    let list_area = Rect::new(
                        content_area.x,
                        content_area.y.saturating_add(1),
                        content_area.width,
                        content_area.height.saturating_sub(1),
                    );

                    if !util::rect_contains(list_area, event.column, event.row) {
                        return EventResult::Ignored;
                    }

                    let viewport = SearchViewport::BottomPanel;
                    let scroll_offset = self.store.state().search.panel_view.scroll_offset;
                    let items_len = self.store.state().search.items.len();
                    let row = (event.row.saturating_sub(list_area.y) as usize) + scroll_offset;
                    if row >= items_len {
                        return EventResult::Ignored;
                    }

                    let item = self.store.state().search.items.get(row).copied();
                    let _ = self.dispatch_kernel(KernelAction::SearchClickRow { row, viewport });
                    match item {
                        Some(SearchResultItem::FileHeader { .. }) => {
                            let _ = self.dispatch_kernel(KernelAction::RunCommand(
                                Command::SearchResultsToggleExpand,
                            ));
                        }
                        Some(SearchResultItem::MatchLine { .. }) => {
                            let _ = self.dispatch_kernel(KernelAction::RunCommand(
                                Command::SearchResultsOpenSelected,
                            ));
                        }
                        None => {}
                    }

                    return EventResult::Consumed;
                }
                if active_tab == BottomPanelTab::Problems {
                    if content_area.width == 0 || content_area.height == 0 {
                        return EventResult::Ignored;
                    }

                    let scroll_offset = self.store.state().problems.scroll_offset();
                    let items_len = self.store.state().problems.items().len();
                    let row = (event.row.saturating_sub(content_area.y) as usize) + scroll_offset;
                    if row >= items_len {
                        return EventResult::Ignored;
                    }

                    let now = Instant::now();
                    let double_click_ms = self.store.state().editor.config.double_click_ms;
                    let is_double = self
                        .last_problems_click
                        .map(|(last_time, last_row)| {
                            last_row == row
                                && now.duration_since(last_time).as_millis() as u64
                                    <= double_click_ms
                        })
                        .unwrap_or(false);

                    if is_double {
                        self.last_problems_click = None;
                    } else {
                        self.last_problems_click = Some((now, row));
                    }

                    let _ = self.dispatch_kernel(KernelAction::ProblemsClickRow { row });
                    if is_double {
                        let _ = self.dispatch_kernel(KernelAction::RunCommand(
                            Command::SearchResultsOpenSelected,
                        ));
                    }

                    return EventResult::Consumed;
                }
                if active_tab == BottomPanelTab::Locations {
                    if content_area.width == 0 || content_area.height == 0 {
                        return EventResult::Ignored;
                    }

                    let scroll_offset = self.store.state().locations.scroll_offset();
                    let items_len = self.store.state().locations.items().len();
                    let row = (event.row.saturating_sub(content_area.y) as usize) + scroll_offset;
                    if row >= items_len {
                        return EventResult::Ignored;
                    }

                    let now = Instant::now();
                    let double_click_ms = self.store.state().editor.config.double_click_ms;
                    let is_double = self
                        .last_locations_click
                        .map(|(last_time, last_row)| {
                            last_row == row
                                && now.duration_since(last_time).as_millis() as u64
                                    <= double_click_ms
                        })
                        .unwrap_or(false);

                    if is_double {
                        self.last_locations_click = None;
                    } else {
                        self.last_locations_click = Some((now, row));
                    }

                    let _ = self.dispatch_kernel(KernelAction::LocationsClickRow { row });
                    if is_double {
                        let _ = self.dispatch_kernel(KernelAction::RunCommand(
                            Command::SearchResultsOpenSelected,
                        ));
                    }

                    return EventResult::Consumed;
                }
                if active_tab == BottomPanelTab::Symbols {
                    if content_area.width == 0 || content_area.height == 0 {
                        return EventResult::Ignored;
                    }

                    let scroll_offset = self.store.state().symbols.scroll_offset();
                    let items_len = self.store.state().symbols.items().len();
                    let row = (event.row.saturating_sub(content_area.y) as usize) + scroll_offset;
                    if row >= items_len {
                        return EventResult::Ignored;
                    }

                    let now = Instant::now();
                    let double_click_ms = self.store.state().editor.config.double_click_ms;
                    let is_double = self
                        .last_symbols_click
                        .map(|(last_time, last_row)| {
                            last_row == row
                                && now.duration_since(last_time).as_millis() as u64
                                    <= double_click_ms
                        })
                        .unwrap_or(false);

                    if is_double {
                        self.last_symbols_click = None;
                    } else {
                        self.last_symbols_click = Some((now, row));
                    }

                    let _ = self.dispatch_kernel(KernelAction::SymbolsClickRow { row });
                    if is_double {
                        let _ = self.dispatch_kernel(KernelAction::RunCommand(
                            Command::SearchResultsOpenSelected,
                        ));
                    }

                    return EventResult::Consumed;
                }
                if active_tab == BottomPanelTab::CodeActions {
                    if content_area.width == 0 || content_area.height == 0 {
                        return EventResult::Ignored;
                    }

                    let scroll_offset = self.store.state().code_actions.scroll_offset();
                    let items_len = self.store.state().code_actions.items().len();
                    let row = (event.row.saturating_sub(content_area.y) as usize) + scroll_offset;
                    if row >= items_len {
                        return EventResult::Ignored;
                    }

                    let now = Instant::now();
                    let double_click_ms = self.store.state().editor.config.double_click_ms;
                    let is_double = self
                        .last_code_actions_click
                        .map(|(last_time, last_row)| {
                            last_row == row
                                && now.duration_since(last_time).as_millis() as u64
                                    <= double_click_ms
                        })
                        .unwrap_or(false);

                    if is_double {
                        self.last_code_actions_click = None;
                    } else {
                        self.last_code_actions_click = Some((now, row));
                    }

                    let _ = self.dispatch_kernel(KernelAction::CodeActionsClickRow { row });
                    if is_double {
                        let _ = self.dispatch_kernel(KernelAction::RunCommand(
                            Command::SearchResultsOpenSelected,
                        ));
                    }

                    return EventResult::Consumed;
                }

                EventResult::Ignored
            }
            MouseEventKind::ScrollUp => {
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    if let Some(id) = self
                        .store
                        .state()
                        .terminal
                        .active_session()
                        .map(|s| s.id)
                    {
                        let _ = self.dispatch_kernel(KernelAction::TerminalScroll { id, delta: -3 });
                    }
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::SearchResults {
                    let _ = self.dispatch_kernel(KernelAction::SearchScroll {
                        delta: -3,
                        viewport: SearchViewport::BottomPanel,
                    });
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Problems {
                    let _ = self
                        .dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsScrollUp));
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Locations {
                    let _ = self
                        .dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsScrollUp));
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Symbols {
                    let _ = self
                        .dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsScrollUp));
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::CodeActions {
                    let _ = self
                        .dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsScrollUp));
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            MouseEventKind::ScrollDown => {
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Terminal {
                    if let Some(id) = self
                        .store
                        .state()
                        .terminal
                        .active_session()
                        .map(|s| s.id)
                    {
                        let _ = self.dispatch_kernel(KernelAction::TerminalScroll { id, delta: 3 });
                    }
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::SearchResults {
                    let _ = self.dispatch_kernel(KernelAction::SearchScroll {
                        delta: 3,
                        viewport: SearchViewport::BottomPanel,
                    });
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Problems {
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(
                        Command::SearchResultsScrollDown,
                    ));
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Locations {
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(
                        Command::SearchResultsScrollDown,
                    ));
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::Symbols {
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(
                        Command::SearchResultsScrollDown,
                    ));
                    return EventResult::Consumed;
                }
                if self.store.state().ui.bottom_panel.active_tab == BottomPanelTab::CodeActions {
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(
                        Command::SearchResultsScrollDown,
                    ));
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}

fn completion_debounce_triggered_by_inserted_char(inserted: char) -> bool {
    inserted.is_alphanumeric() || inserted == '_'
}

fn completion_debounce_context_allowed(tab: &crate::kernel::editor::EditorTabState) -> bool {
    if tab.is_in_string_or_comment_at_cursor() {
        return false;
    }

    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let end_char = cursor_char_offset.min(rope.len_chars());

    let mut start_char = end_char;
    while start_char > 0 {
        let ch = rope.char(start_char - 1);
        if ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch) {
            start_char = start_char.saturating_sub(1);
        } else {
            break;
        }
    }

    if start_char != end_char {
        let first = rope.char(start_char);
        if first == '_' || unicode_xid::UnicodeXID::is_xid_start(first) {
            return true;
        }
    }

    if start_char > 0 && rope.char(start_char - 1) == '.' {
        return true;
    }
    if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
        return true;
    }

    false
}

fn terminal_bytes_for_key_event(event: &KeyEvent) -> Option<Vec<u8>> {
    match (event.code, event.modifiers) {
        (KeyCode::Char(ch), KeyModifiers::CONTROL) => {
            let ch = ch.to_ascii_lowercase();
            Some(vec![(ch as u8) & 0x1f])
        }
        (KeyCode::Char(ch), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            Some(s.as_bytes().to_vec())
        }
        (KeyCode::Enter, _) => Some(vec![b'\r']),
        (KeyCode::Backspace, _) => Some(vec![0x7f]),
        (KeyCode::Tab, _) => Some(vec![b'\t']),
        (KeyCode::BackTab, _) => Some(b"\x1b[Z".to_vec()),
        (KeyCode::Esc, _) => Some(vec![0x1b]),
        (KeyCode::Up, _) => Some(b"\x1b[A".to_vec()),
        (KeyCode::Down, _) => Some(b"\x1b[B".to_vec()),
        (KeyCode::Right, _) => Some(b"\x1b[C".to_vec()),
        (KeyCode::Left, _) => Some(b"\x1b[D".to_vec()),
        (KeyCode::Home, _) => Some(b"\x1b[H".to_vec()),
        (KeyCode::End, _) => Some(b"\x1b[F".to_vec()),
        (KeyCode::Delete, _) => Some(b"\x1b[3~".to_vec()),
        (KeyCode::PageUp, _) => Some(b"\x1b[5~".to_vec()),
        (KeyCode::PageDown, _) => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}
