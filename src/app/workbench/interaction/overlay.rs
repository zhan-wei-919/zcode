use super::super::util;
use super::super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::{Action as KernelAction, OverlayKind, SearchResultItem, SearchViewport};
use crate::tui::view::EventResult;

impl Workbench {
    /// 居中浮层鼠标：点击框外即关闭；点击结果行选中并打开；滚轮滚动列表。
    /// 浮层是 telescope 风格，主交互走键盘，这里只做最直接的点选。
    pub(in super::super) fn handle_overlay_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let Some(kind) = self.store.state().ui.overlay.active else {
            return EventResult::Ignored;
        };
        let Some(popup) = self.frame_layout.overlay_area else {
            return EventResult::Ignored;
        };

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !util::rect_contains(popup, event.column, event.row) {
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::CloseOverlay));
                    return EventResult::Consumed;
                }

                // 内层：去掉边框；标题占一行；search 额外有 query + summary 两行。
                let inner_top = popup.y.saturating_add(1);
                let list_top = match kind {
                    OverlayKind::Search => inner_top.saturating_add(3),
                    _ => inner_top.saturating_add(1),
                };
                if event.row < list_top {
                    return EventResult::Consumed;
                }
                let visible_row = (event.row - list_top) as usize;

                match kind {
                    OverlayKind::Search => self.click_search_row(visible_row),
                    OverlayKind::Problems => self.click_flat_row(
                        visible_row,
                        self.store.state().search.panel_view.scroll_offset,
                        kind,
                    ),
                    _ => self.click_flat_row(visible_row, self.flat_scroll_offset(kind), kind),
                }
                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                let _ =
                    self.dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsScrollUp));
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let _ = self
                    .dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsScrollDown));
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn flat_scroll_offset(&self, kind: OverlayKind) -> usize {
        match kind {
            OverlayKind::Problems => self.store.state().problems.scroll_offset(),
            OverlayKind::CodeActions => self.store.state().code_actions.scroll_offset(),
            OverlayKind::Locations => self.store.state().locations.scroll_offset(),
            OverlayKind::Symbols => self.store.state().symbols.scroll_offset(),
            OverlayKind::Search => 0,
        }
    }

    fn click_flat_row(&mut self, visible_row: usize, scroll_offset: usize, kind: OverlayKind) {
        let row = visible_row + scroll_offset;
        let items_len = match kind {
            OverlayKind::Problems => self.store.state().problems.items().len(),
            OverlayKind::CodeActions => self.store.state().code_actions.items().len(),
            OverlayKind::Locations => self.store.state().locations.items().len(),
            OverlayKind::Symbols => self.store.state().symbols.items().len(),
            OverlayKind::Search => 0,
        };
        if row >= items_len {
            return;
        }

        let click = match kind {
            OverlayKind::Problems => KernelAction::ProblemsClickRow { row },
            OverlayKind::CodeActions => KernelAction::CodeActionsClickRow { row },
            OverlayKind::Locations => KernelAction::LocationsClickRow { row },
            OverlayKind::Symbols => KernelAction::SymbolsClickRow { row },
            OverlayKind::Search => return,
        };
        let _ = self.dispatch_kernel(click);
        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsOpenSelected));
    }

    fn click_search_row(&mut self, visible_row: usize) {
        let scroll_offset = self.store.state().search.panel_view.scroll_offset;
        let row = visible_row + scroll_offset;
        let items_len = self.store.state().search.items.len();
        if row >= items_len {
            return;
        }

        let item = self.store.state().search.items.get(row).copied();
        let _ = self.dispatch_kernel(KernelAction::SearchClickRow {
            row,
            viewport: SearchViewport::BottomPanel,
        });
        match item {
            Some(SearchResultItem::FileHeader { .. }) => {
                let _ = self
                    .dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsToggleExpand));
            }
            Some(SearchResultItem::MatchLine { .. }) => {
                let _ = self
                    .dispatch_kernel(KernelAction::RunCommand(Command::SearchResultsOpenSelected));
            }
            None => {}
        }
    }
}
