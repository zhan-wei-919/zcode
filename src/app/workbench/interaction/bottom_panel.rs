use super::super::util;
use super::super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::{Action as KernelAction, BottomPanelTab, SearchResultItem, SearchViewport};
use crate::tui::view::EventResult;
use crate::ui::core::geom::Rect;
use std::time::Instant;
use unicode_width::UnicodeWidthStr;

impl Workbench {
    pub(in super::super) fn handle_bottom_panel_mouse(
        &mut self,
        event: &MouseEvent,
    ) -> EventResult {
        let Some(panel_area) = self.last_bottom_panel_area else {
            return EventResult::Ignored;
        };
        if !util::rect_contains(panel_area, event.column, event.row) {
            return EventResult::Ignored;
        }

        let inner = panel_area;
        if inner.is_empty() {
            return EventResult::Ignored;
        }

        let tabs_area = Rect::new(inner.x, inner.y, inner.w, 1.min(inner.h));
        let content_area = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.w,
            inner.h.saturating_sub(1),
        );

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if util::rect_contains(tabs_area, event.column, event.row) {
                    if tabs_area.w == 0 {
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
                    if content_area.is_empty() {
                        return EventResult::Ignored;
                    }

                    let list_area = Rect::new(
                        content_area.x,
                        content_area.y.saturating_add(1),
                        content_area.w,
                        content_area.h.saturating_sub(1),
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
                    if content_area.is_empty() {
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
                    if content_area.is_empty() {
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
                    if content_area.is_empty() {
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
                    if content_area.is_empty() {
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
                    if let Some(id) = self.store.state().terminal.active_session().map(|s| s.id) {
                        let _ =
                            self.dispatch_kernel(KernelAction::TerminalScroll { id, delta: -3 });
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
                    if let Some(id) = self.store.state().terminal.active_session().map(|s| s.id) {
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
