use super::util;
use super::Workbench;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, SidebarTab, SplitDirection};
use crate::tui::view::EventResult;

impl Workbench {
    fn editor_pane_at(&self, x: u16, y: u16) -> Option<usize> {
        self.last_editor_areas
            .iter()
            .enumerate()
            .find_map(|(i, area)| util::rect_contains(*area, x, y).then_some(i))
    }

    fn handle_activity_bar_click(&mut self, event: &MouseEvent) -> bool {
        let Some(area) = self.last_activity_bar_area else {
            return false;
        };
        let row = event.row.saturating_sub(area.y);
        let slot_h = util::activity_slot_height(area.height);
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
            util::ActivityItem::Search => {
                let active = self.store.state().ui.sidebar_visible
                    && self.store.state().ui.sidebar_tab == SidebarTab::Search;
                let cmd = if active {
                    Command::ToggleSidebar
                } else {
                    Command::FocusSearch
                };
                self.dispatch_kernel(KernelAction::RunCommand(cmd))
            }
            util::ActivityItem::Problems
            | util::ActivityItem::Results
            | util::ActivityItem::Logs => {
                let Some(tab) = item.bottom_panel_tab() else {
                    return false;
                };

                let active = self.store.state().ui.bottom_panel.visible
                    && self.store.state().ui.bottom_panel.active_tab == tab;
                if active {
                    return self
                        .dispatch_kernel(KernelAction::RunCommand(Command::ToggleBottomPanel));
                }

                let mut changed =
                    self.dispatch_kernel(KernelAction::BottomPanelSetActiveTab { tab });
                changed |=
                    self.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
                changed
            }
            util::ActivityItem::Find => {
                let cmd = {
                    let state = self.store.state();
                    let pane = state.ui.editor_layout.active_pane;
                    let search_bar = state.editor.pane(pane).map(|p| &p.search_bar);
                    if let Some(sb) = search_bar {
                        if sb.visible {
                            if sb.mode == crate::kernel::editor::SearchBarMode::Replace {
                                Some(Command::EditorSearchBarToggleReplaceMode)
                            } else {
                                Some(Command::EditorSearchBarClose)
                            }
                        } else {
                            Some(Command::Find)
                        }
                    } else {
                        Some(Command::Find)
                    }
                };
                cmd.is_some_and(|cmd| self.dispatch_kernel(KernelAction::RunCommand(cmd)))
            }
            util::ActivityItem::Replace => {
                let cmd = {
                    let state = self.store.state();
                    let pane = state.ui.editor_layout.active_pane;
                    let search_bar = state.editor.pane(pane).map(|p| &p.search_bar);
                    if let Some(sb) = search_bar {
                        if sb.visible {
                            if sb.mode == crate::kernel::editor::SearchBarMode::Search {
                                Some(Command::EditorSearchBarToggleReplaceMode)
                            } else {
                                Some(Command::EditorSearchBarClose)
                            }
                        } else {
                            Some(Command::Replace)
                        }
                    } else {
                        Some(Command::Replace)
                    }
                };
                cmd.is_some_and(|cmd| self.dispatch_kernel(KernelAction::RunCommand(cmd)))
            }
            util::ActivityItem::Palette => {
                self.dispatch_kernel(KernelAction::RunCommand(Command::CommandPalette))
            }
            util::ActivityItem::Settings => {
                self.dispatch_kernel(KernelAction::RunCommand(Command::OpenSettings))
            }
        }
    }

    fn handle_sidebar_tabs_click(&mut self, event: &MouseEvent) -> bool {
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

    pub(super) fn handle_mouse_area(&mut self, event: &MouseEvent) -> bool {
        let _scope = perf::scope("input.mouse.area");
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
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
            MouseEventKind::Down(MouseButton::Right) => {
                if self
                    .last_bottom_panel_area
                    .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return self.dispatch_kernel(KernelAction::RunCommand(Command::FocusBottomPanel));
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
            _ => {}
        }
        false
    }

    pub(super) fn handle_editor_split_mouse(&mut self, event: &MouseEvent) -> Option<EventResult> {
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
