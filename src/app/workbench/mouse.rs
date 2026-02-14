use super::util;
use super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, SidebarTab, SplitDirection};
use crate::tui::view::EventResult;
use crate::ui::core::geom::Pos;
use crate::ui::core::geom::Rect;
use crate::ui::core::id::IdPath;
use crate::ui::core::input::UiEvent;
use crate::ui::core::runtime::UiRuntimeOutput;
use crate::ui::core::tree::Sense;

impl Workbench {
    fn editor_pane_at(&self, x: u16, y: u16) -> Option<usize> {
        self.layout_cache
            .editor_areas
            .iter()
            .enumerate()
            .find_map(|(i, area)| util::rect_contains(*area, x, y).then_some(i))
    }

    fn handle_activity_bar_click(&mut self, event: &MouseEvent) -> bool {
        let Some(area) = self.layout_cache.activity_bar_area else {
            return false;
        };
        let row = event.row.saturating_sub(area.y);
        let slot_h = util::activity_slot_height(area.h);
        let idx = if slot_h > 1 {
            row.saturating_div(slot_h)
        } else {
            row
        };
        let Some(item) = util::activity_item_at_row(idx) else {
            return false;
        };

        match item {
            util::ActivityItem::Explorer => {
                let active = self.store.state().ui.sidebar_visible
                    && self.store.state().ui.sidebar_tab == SidebarTab::Explorer;
                let cmd = if active {
                    Command::ToggleSidebar
                } else {
                    Command::FocusExplorer
                };
                self.dispatch_kernel(KernelAction::RunCommand(cmd))
            }
            util::ActivityItem::Panel => {
                if self.store.state().ui.bottom_panel.visible {
                    return self
                        .dispatch_kernel(KernelAction::RunCommand(Command::ToggleBottomPanel));
                }

                let mut changed = self.dispatch_kernel(KernelAction::BottomPanelSetActiveTab {
                    tab: crate::kernel::BottomPanelTab::Terminal,
                });
                changed |=
                    self.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
                changed
            }
            util::ActivityItem::Palette => {
                self.dispatch_kernel(KernelAction::RunCommand(Command::CommandPalette))
            }
            util::ActivityItem::Git => {
                self.dispatch_kernel(KernelAction::RunCommand(Command::GitTogglePanel))
            }
            util::ActivityItem::Settings => {
                self.dispatch_kernel(KernelAction::RunCommand(Command::OpenSettings))
            }
        }
    }

    fn handle_sidebar_tabs_click(&mut self, event: &MouseEvent) -> bool {
        let Some(area) = self.layout_cache.sidebar_tabs_area else {
            return false;
        };

        let mid = area.x + (area.w / 2);
        let cmd = if event.column < mid {
            Command::FocusExplorer
        } else {
            Command::FocusSearch
        };
        self.dispatch_kernel(KernelAction::RunCommand(cmd))
    }

    pub(super) fn handle_mouse_area(&mut self, event: &MouseEvent) -> bool {
        let _scope = perf::scope("input.mouse.area");
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self
                    .layout_cache
                    .bottom_panel_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self
                        .dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
                } else if self
                    .layout_cache
                    .activity_bar_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self.handle_activity_bar_click(event);
                } else if self
                    .layout_cache
                    .sidebar_tabs_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self.handle_sidebar_tabs_click(event);
                } else if self
                    .layout_cache
                    .sidebar_area
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
            MouseEventKind::Down(MouseButton::Right) => {
                if self
                    .layout_cache
                    .bottom_panel_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self
                        .dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
                } else if self
                    .layout_cache
                    .sidebar_area
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
            _ => {}
        }
        false
    }

    pub(super) fn handle_editor_split_mouse(
        &mut self,
        event: &MouseEvent,
        ui_out: &UiRuntimeOutput,
    ) -> Option<EventResult> {
        let _scope = perf::scope("input.mouse.split");
        let splitter_id = IdPath::root("workbench")
            .push_str("editor_splitter")
            .finish();

        // Arm dragging on mouse down if we hit the splitter node from the last rendered tree.
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            let pos = Pos::new(event.column, event.row);
            let hit = self.ui_tree.hit_test_with_sense(pos, Sense::DRAG_SOURCE);
            if hit.is_none_or(|n| n.id != splitter_id) {
                return None;
            }

            self.editor_split_dragging = true;
            let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::FocusEditor));
            self.update_editor_split_ratio(event.column, event.row);
            return Some(EventResult::Consumed);
        }

        // While dragging, keep feeding events to the UI runtime (capture/threshold handling),
        // but only apply updates for the splitter drag session.
        if !self.editor_split_dragging {
            return None;
        }

        let mut handled = false;
        for ev in &ui_out.events {
            match ev {
                UiEvent::DragStart { id, pos } if *id == splitter_id => {
                    handled = true;
                    self.update_editor_split_ratio(pos.x, pos.y);
                }
                UiEvent::DragMove { id, pos, .. } if *id == splitter_id => {
                    handled = true;
                    self.update_editor_split_ratio(pos.x, pos.y);
                }
                UiEvent::DragEnd { id, .. } if *id == splitter_id => {
                    handled = true;
                }
                _ => {}
            }
        }

        if matches!(event.kind, MouseEventKind::Up(MouseButton::Left)) {
            self.editor_split_dragging = false;
            handled = true;
        }

        handled.then_some(EventResult::Consumed)
    }

    pub(super) fn handle_sidebar_split_mouse(
        &mut self,
        event: &MouseEvent,
        ui_out: &UiRuntimeOutput,
    ) -> Option<EventResult> {
        let _scope = perf::scope("input.mouse.sidebar_split");
        let splitter_id = IdPath::root("workbench")
            .push_str("sidebar_splitter")
            .finish();

        if !self.store.state().ui.sidebar_visible
            || self.layout_cache.sidebar_container_area.is_none()
        {
            self.sidebar_split_dragging = false;
            return None;
        }

        // Arm dragging on mouse down if we hit the splitter node from the last rendered tree.
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            let pos = Pos::new(event.column, event.row);
            let hit = self.ui_tree.hit_test_with_sense(pos, Sense::DRAG_SOURCE);
            if hit.is_none_or(|n| n.id != splitter_id) {
                return None;
            }

            self.sidebar_split_dragging = true;
            self.update_sidebar_width(event.column);
            return Some(EventResult::Consumed);
        }

        if !self.sidebar_split_dragging {
            return None;
        }

        let mut handled = false;
        for ev in &ui_out.events {
            match ev {
                UiEvent::DragStart { id, pos } if *id == splitter_id => {
                    handled = true;
                    self.update_sidebar_width(pos.x);
                }
                UiEvent::DragMove { id, pos, .. } if *id == splitter_id => {
                    handled = true;
                    self.update_sidebar_width(pos.x);
                }
                UiEvent::DragEnd { id, .. } if *id == splitter_id => {
                    handled = true;
                }
                _ => {}
            }
        }

        if matches!(event.kind, MouseEventKind::Up(MouseButton::Left)) {
            self.sidebar_split_dragging = false;
            handled = true;
        }

        handled.then_some(EventResult::Consumed)
    }

    pub(super) fn handle_bottom_panel_split_mouse(
        &mut self,
        event: &MouseEvent,
        ui_out: &UiRuntimeOutput,
    ) -> Option<EventResult> {
        let _scope = perf::scope("input.mouse.bottom_panel_split");
        let splitter_id = IdPath::root("workbench")
            .push_str("bottom_panel_splitter")
            .finish();

        if !self.store.state().ui.bottom_panel.visible
            || self.layout_cache.bottom_panel_splitter_area.is_none()
            || self.layout_cache.render_area.is_none()
        {
            self.bottom_panel_split_dragging = false;
            return None;
        }

        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            let pos = Pos::new(event.column, event.row);
            let hit = self.ui_tree.hit_test_with_sense(pos, Sense::DRAG_SOURCE);
            if hit.is_none_or(|n| n.id != splitter_id) {
                return None;
            }

            self.bottom_panel_split_dragging = true;
            self.update_bottom_panel_height_ratio(event.row);
            return Some(EventResult::Consumed);
        }

        if !self.bottom_panel_split_dragging {
            return None;
        }

        let mut handled = false;
        for ev in &ui_out.events {
            match ev {
                UiEvent::DragStart { id, pos } if *id == splitter_id => {
                    handled = true;
                    self.update_bottom_panel_height_ratio(pos.y);
                }
                UiEvent::DragMove { id, pos, .. } if *id == splitter_id => {
                    handled = true;
                    self.update_bottom_panel_height_ratio(pos.y);
                }
                UiEvent::DragEnd { id, .. } if *id == splitter_id => {
                    handled = true;
                }
                _ => {}
            }
        }

        if matches!(event.kind, MouseEventKind::Up(MouseButton::Left)) {
            self.bottom_panel_split_dragging = false;
            handled = true;
        }

        handled.then_some(EventResult::Consumed)
    }

    fn update_editor_split_ratio(&mut self, column: u16, row: u16) {
        let Some(area) = self.layout_cache.editor_container_area else {
            return;
        };

        let direction = self.store.state().ui.editor_layout.split_direction;
        let Some(ratio) = compute_split_ratio(direction, area, column, row) else {
            return;
        };
        let _ = self.dispatch_kernel(KernelAction::EditorSetSplitRatio { ratio });
    }

    fn update_sidebar_width(&mut self, column: u16) {
        let Some(area) = self.layout_cache.sidebar_container_area else {
            return;
        };
        if area.w == 0 {
            return;
        }

        // The draggable separator is the last column of the sidebar area.
        let desired = column.saturating_sub(area.x).saturating_add(1);
        let width = util::clamp_sidebar_width(area.w, desired);
        let _ = self.dispatch_kernel(KernelAction::SidebarSetWidth { width });
    }

    fn update_bottom_panel_height_ratio(&mut self, row: u16) {
        let Some(area) = self.layout_cache.render_area else {
            return;
        };
        let body_h = area.h.saturating_sub(super::STATUS_HEIGHT);
        if body_h < 5 {
            return;
        }

        let content_h = body_h;
        let splitter_h = 1u16;
        let total_without_splitter = content_h.saturating_sub(splitter_h);
        if total_without_splitter < 2 {
            return;
        }

        let bottom_h = area
            .bottom()
            .saturating_sub(super::STATUS_HEIGHT)
            .saturating_sub(row)
            .saturating_sub(splitter_h);
        let bottom_h = bottom_h.clamp(1, total_without_splitter.saturating_sub(1));
        let ratio = ((bottom_h as u32) * 1000 / (total_without_splitter as u32)) as u16;
        let _ = self.dispatch_kernel(KernelAction::BottomPanelSetHeightRatio { ratio });
    }
}

fn compute_split_ratio(
    direction: SplitDirection,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<u16> {
    match direction {
        SplitDirection::Vertical => {
            if area.w < 3 {
                return None;
            }
            let total = area.w.saturating_sub(1);
            if total < 2 {
                return None;
            }

            let mut left = column.saturating_sub(area.x);
            left = left.clamp(1, total.saturating_sub(1));
            Some(((left as u32) * 1000 / (total as u32)) as u16)
        }
        SplitDirection::Horizontal => {
            if area.h < 3 {
                return None;
            }
            let total = area.h.saturating_sub(1);
            if total < 2 {
                return None;
            }

            let mut top = row.saturating_sub(area.y);
            top = top.clamp(1, total.saturating_sub(1));
            Some(((top as u32) * 1000 / (total as u32)) as u16)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app/workbench/mouse_split_ratio.rs"]
mod tests;
