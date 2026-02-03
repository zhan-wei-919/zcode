use super::super::util;
use super::super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::kernel::Action as KernelAction;
use crate::tui::view::EventResult;
use ratatui::layout::Rect;

impl Workbench {
    pub(in super::super) fn handle_explorer_context_menu_mouse(
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
}
