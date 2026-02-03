use super::super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, SearchResultItem, SearchViewport};
use crate::tui::view::EventResult;

impl Workbench {
    pub(in super::super) fn handle_search_mouse(&mut self, event: &MouseEvent) -> EventResult {
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
}
