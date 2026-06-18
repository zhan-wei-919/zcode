use super::util;
use super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, SidebarTab};
use crate::tui::view::EventResult;
use crate::ui::core::geom::Pos;
use crate::ui::core::id::IdPath;
use crate::ui::core::input::UiEvent;
use crate::ui::core::runtime::UiRuntimeOutput;
use crate::ui::core::tree::Sense;

impl Workbench {
    fn editor_pane_at(&self, x: u16, y: u16) -> Option<usize> {
        self.frame_layout
            .editor
            .outer_areas
            .iter()
            .enumerate()
            .find_map(|(i, area)| util::rect_contains(*area, x, y).then_some(i))
    }

    fn handle_activity_bar_click(&mut self, event: &MouseEvent) -> bool {
        let Some(area) = self.frame_layout.activity_bar_area else {
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
                self.dispatch_kernel(KernelAction::RunCommand(Command::OpenDiagnostics))
            }
            util::ActivityItem::Palette => {
                self.dispatch_kernel(KernelAction::RunCommand(Command::OpenCommandLine))
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
        let Some(area) = self.frame_layout.sidebar_tabs_area else {
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
                    .frame_layout
                    .activity_bar_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self.handle_activity_bar_click(event);
                } else if self
                    .frame_layout
                    .sidebar_tabs_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self.handle_sidebar_tabs_click(event);
                } else if self
                    .frame_layout
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
                    .frame_layout
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
            || self.frame_layout.sidebar_container_area.is_none()
        {
            self.interaction.sidebar_split_dragging = false;
            return None;
        }

        // Arm dragging on mouse down if we hit the splitter node from the last rendered tree.
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            let pos = Pos::new(event.column, event.row);
            let hit = self.ui_tree.hit_test_with_sense(pos, Sense::DRAG_SOURCE);
            if hit.is_none_or(|n| n.id != splitter_id) {
                return None;
            }

            self.interaction.sidebar_split_dragging = true;
            self.update_sidebar_width(event.column);
            return Some(EventResult::Consumed);
        }

        if !self.interaction.sidebar_split_dragging {
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
            self.interaction.sidebar_split_dragging = false;
            handled = true;
        }

        handled.then_some(EventResult::Consumed)
    }

    fn update_sidebar_width(&mut self, column: u16) {
        let Some(area) = self.frame_layout.sidebar_container_area else {
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
}
