use super::super::util;
use super::super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::kernel::services::adapters::perf;
use crate::kernel::Action as KernelAction;
use crate::tui::view::EventResult;
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn handle_explorer_mouse(&mut self, event: &MouseEvent) -> EventResult {
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
}
