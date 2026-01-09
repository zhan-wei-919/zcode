use super::util;
use super::Workbench;
use crate::core::event::Key;
use crate::core::view::EventResult;
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::KeybindingContext;
use crate::kernel::{
    Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, PendingAction,
    SearchResultItem, SearchViewport, SidebarTab,
};
use crate::views::{
    compute_editor_pane_layout, hit_test_editor_mouse, hit_test_editor_tab, hit_test_tab_hover,
    TabHitResult,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders};
use std::time::Instant;

impl Workbench {
    pub(super) fn handle_key_event(&mut self, key_event: &KeyEvent) -> EventResult {
        let _scope = perf::scope("input.key");

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

        let context = self.keybinding_context();
        let key: Key = (*key_event).into();

        if let Some(cmd) = self.keybindings.resolve(context, &key).cloned() {
            let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd));
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
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::InsertChar(ch)));
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
            _ => EventResult::Ignored,
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

    pub(super) fn handle_editor_mouse(
        &mut self,
        event: &crossterm::event::MouseEvent,
    ) -> EventResult {
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
            .or_else(|| self.last_editor_inner_areas.get(0).copied());
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

    pub(super) fn handle_explorer_mouse(
        &mut self,
        event: &crossterm::event::MouseEvent,
    ) -> EventResult {
        let _scope = perf::scope("input.mouse.explorer");
        if !self.explorer.contains(event.column, event.row) {
            return EventResult::Ignored;
        }

        let scroll_offset = self.store.state().explorer.scroll_offset;
        let rows_len = self.store.state().explorer.rows.len();

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
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

    pub(super) fn handle_search_mouse(
        &mut self,
        event: &crossterm::event::MouseEvent,
    ) -> EventResult {
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

    pub(super) fn handle_bottom_panel_mouse(
        &mut self,
        event: &crossterm::event::MouseEvent,
    ) -> EventResult {
        let Some(panel_area) = self.last_bottom_panel_area else {
            return EventResult::Ignored;
        };
        if !util::rect_contains(panel_area, event.column, event.row) {
            return EventResult::Ignored;
        }

        let inner = Block::default().borders(Borders::ALL).inner(panel_area);
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
                    let idx = ((rel as u32) * 3 / (tabs_area.width as u32)).min(2) as u8;
                    let tab = match idx {
                        0 => BottomPanelTab::Problems,
                        1 => BottomPanelTab::SearchResults,
                        _ => BottomPanelTab::Logs,
                    };
                    let _ = self.dispatch_kernel(KernelAction::BottomPanelSetActiveTab { tab });
                    return EventResult::Consumed;
                }

                if self.store.state().ui.bottom_panel.active_tab != BottomPanelTab::SearchResults {
                    return EventResult::Ignored;
                }

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

                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                if self.store.state().ui.bottom_panel.active_tab != BottomPanelTab::SearchResults {
                    return EventResult::Ignored;
                }
                let _ = self.dispatch_kernel(KernelAction::SearchScroll {
                    delta: -3,
                    viewport: SearchViewport::BottomPanel,
                });
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                if self.store.state().ui.bottom_panel.active_tab != BottomPanelTab::SearchResults {
                    return EventResult::Ignored;
                }
                let _ = self.dispatch_kernel(KernelAction::SearchScroll {
                    delta: 3,
                    viewport: SearchViewport::BottomPanel,
                });
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }
}
