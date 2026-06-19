use super::util;
use super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::Action as KernelAction;
use crate::tui::view::EventResult;
use crate::ui::core::geom::Pos;
use crate::ui::core::id::IdPath;
use crate::ui::core::input::UiEvent;
use crate::ui::core::runtime::UiRuntimeOutput;
use crate::ui::core::tree::Sense;

impl Workbench {
    pub(super) fn editor_pane_at(&self, x: u16, y: u16) -> Option<usize> {
        self.frame_layout
            .editor
            .outer_areas
            .iter()
            .enumerate()
            .find_map(|(i, area)| util::rect_contains(*area, x, y).then_some(i))
    }

    pub(super) fn handle_mouse_area(&mut self, event: &MouseEvent) -> bool {
        let _scope = perf::scope("input.mouse.area");
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Down(MouseButton::Right) => {
                if self
                    .frame_layout
                    .sidebar_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self.dispatch_kernel(KernelAction::RunCommand(Command::FocusExplorer));
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
