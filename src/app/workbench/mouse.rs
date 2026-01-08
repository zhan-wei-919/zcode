use super::util;
use super::Workbench;
use crate::core::view::EventResult;
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, SidebarTab, SplitDirection};
use crossterm::event::{MouseButton, MouseEventKind};

impl Workbench {
    fn editor_pane_at(&self, x: u16, y: u16) -> Option<usize> {
        self.last_editor_areas
            .iter()
            .enumerate()
            .find_map(|(i, area)| util::rect_contains(*area, x, y).then_some(i))
    }

    fn handle_activity_bar_click(&mut self, event: &crossterm::event::MouseEvent) -> bool {
        let Some(area) = self.last_activity_bar_area else {
            return false;
        };
        let row = event.row.saturating_sub(area.y);
        let cmd = match row {
            0 => Some(Command::FocusExplorer),
            1 => Some(Command::FocusSearch),
            _ => None,
        };
        if let Some(cmd) = cmd {
            return self.dispatch_kernel(KernelAction::RunCommand(cmd));
        }
        false
    }

    fn handle_sidebar_tabs_click(&mut self, event: &crossterm::event::MouseEvent) -> bool {
        let Some(area) = self.last_sidebar_tabs_area else {
            return false;
        };

        let mid = area.x + (area.width / 2);
        let cmd = if event.column < mid {
            Command::FocusExplorer
        } else {
            Command::FocusSearch
        };
        self.dispatch_kernel(KernelAction::RunCommand(cmd))
    }

    pub(super) fn handle_mouse_area(&mut self, event: &crossterm::event::MouseEvent) -> bool {
        let _scope = perf::scope("input.mouse.area");
        if let MouseEventKind::Down(MouseButton::Left) = event.kind {
            if self
                .last_bottom_panel_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                return self.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
            } else if self
                .last_activity_bar_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                return self.handle_activity_bar_click(event);
            } else if self
                .last_sidebar_tabs_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                return self.handle_sidebar_tabs_click(event);
            } else if self
                .last_sidebar_area
                .is_some_and(|a| util::rect_contains(a, event.column, event.row))
            {
                let cmd = match self.store.state().ui.sidebar_tab {
                    SidebarTab::Explorer => Command::FocusExplorer,
                    SidebarTab::Search => Command::FocusSearch,
                };
                return self.dispatch_kernel(KernelAction::RunCommand(cmd));
            } else if let Some(pane) = self.editor_pane_at(event.column, event.row) {
                return self.dispatch_kernel(KernelAction::EditorSetActivePane { pane });
            }
        }
        false
    }

    pub(super) fn handle_editor_split_mouse(
        &mut self,
        event: &crossterm::event::MouseEvent,
    ) -> Option<EventResult> {
        let _scope = perf::scope("input.mouse.split");
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let splitter = self.last_editor_splitter_area?;
                if !util::rect_contains(splitter, event.column, event.row) {
                    return None;
                }

                self.editor_split_dragging = true;
                let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::FocusEditor));
                self.update_editor_split_ratio(event.column, event.row);
                Some(EventResult::Consumed)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if !self.editor_split_dragging {
                    return None;
                }
                self.update_editor_split_ratio(event.column, event.row);
                Some(EventResult::Consumed)
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if !self.editor_split_dragging {
                    return None;
                }
                self.editor_split_dragging = false;
                Some(EventResult::Consumed)
            }
            _ => None,
        }
    }

    fn update_editor_split_ratio(&mut self, column: u16, row: u16) {
        let Some(area) = self.last_editor_container_area else {
            return;
        };

        let direction = self.store.state().ui.editor_layout.split_direction;
        let ratio = match direction {
            SplitDirection::Vertical => {
                if area.width < 3 {
                    return;
                }
                let total = area.width.saturating_sub(1);
                if total < 2 {
                    return;
                }

                let mut left = column.saturating_sub(area.x);
                left = left.clamp(1, total.saturating_sub(1));
                ((left as u32) * 1000 / (total as u32)) as u16
            }
            SplitDirection::Horizontal => {
                if area.height < 3 {
                    return;
                }
                let total = area.height.saturating_sub(1);
                if total < 2 {
                    return;
                }

                let mut top = row.saturating_sub(area.y);
                top = top.clamp(1, total.saturating_sub(1));
                ((top as u32) * 1000 / (total as u32)) as u16
            }
        };
        let _ = self.dispatch_kernel(KernelAction::EditorSetSplitRatio { ratio });
    }
}
