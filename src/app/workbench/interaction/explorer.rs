use super::super::util;
use super::super::Workbench;
use crate::core::event::{InputEvent, MouseButton, MouseEvent, MouseEventKind};
use crate::kernel::services::adapters::perf;
use crate::kernel::Action as KernelAction;
use crate::tui::view::EventResult;
use crate::ui::core::input::{DragPayload, UiEvent};
use crate::ui::core::tree::NodeKind;
use std::path::Path;
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn handle_explorer_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let _scope = perf::scope("input.mouse.explorer");
        let in_tree = self.explorer.contains(event.column, event.row);
        let in_git = self
            .last_git_panel_area
            .is_some_and(|a| util::rect_contains(a, event.column, event.row));

        let is_armed = self.ui_runtime.is_pressed() || self.ui_runtime.capture().is_some();
        let captured_is_explorer_row = self
            .ui_runtime
            .capture()
            .and_then(|id| self.ui_tree.node(id))
            .is_some_and(|n| matches!(n.kind, NodeKind::ExplorerRow { .. }));

        if !in_tree && !in_git && !is_armed {
            return EventResult::Ignored;
        }

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

                // Arm the UI runtime so we can detect click vs drag.
                let _ = self.ui_runtime.on_input(&InputEvent::Mouse(*event), &self.ui_tree);
                EventResult::Consumed
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let out = self.ui_runtime.on_input(&InputEvent::Mouse(*event), &self.ui_tree);

                let captured_is_explorer_row = self
                    .ui_runtime
                    .capture()
                    .and_then(|id| self.ui_tree.node(id))
                    .is_some_and(|n| matches!(n.kind, NodeKind::ExplorerRow { .. }));

                if out.needs_redraw || captured_is_explorer_row {
                    return EventResult::Consumed;
                }

                EventResult::Ignored
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let out = self.ui_runtime.on_input(&InputEvent::Mouse(*event), &self.ui_tree);

                let mut handled = false;
                for ev in out.events {
                    match ev {
                        UiEvent::Click { id, button, .. } if button == MouseButton::Left => {
                            let Some(node) = self.ui_tree.node(id) else {
                                continue;
                            };
                            let NodeKind::ExplorerRow { node_id } = node.kind else {
                                continue;
                            };

                            let Some(row) = self
                                .store
                                .state()
                                .explorer
                                .rows
                                .iter()
                                .position(|r| r.id == node_id)
                            else {
                                continue;
                            };
                            let _ = self.dispatch_kernel(KernelAction::ExplorerClickRow {
                                row,
                                now: Instant::now(),
                            });
                            handled = true;
                        }
                        UiEvent::Drop { payload, target, .. } => {
                            let Some(target_node) = self.ui_tree.node(target) else {
                                continue;
                            };

                            let DragPayload::ExplorerNode { node_id } = payload else {
                                continue;
                            };

                            match target_node.kind {
                                NodeKind::EditorArea { pane } => {
                                    let Some((path, is_dir)) = self
                                        .store
                                        .state()
                                        .explorer
                                        .path_and_kind_for(node_id)
                                    else {
                                        continue;
                                    };
                                    if is_dir {
                                        continue;
                                    }

                                    let _ =
                                        self.dispatch_kernel(KernelAction::EditorSetActivePane {
                                            pane,
                                        });
                                    let _ = self.dispatch_kernel(KernelAction::OpenPath(path));
                                    handled = true;
                                }
                                NodeKind::ExplorerFolderDrop { node_id: to_dir_id } => {
                                    let Some((from_path, from_is_dir)) = self
                                        .store
                                        .state()
                                        .explorer
                                        .path_and_kind_for(node_id)
                                    else {
                                        continue;
                                    };

                                    let Some((to_dir_path, to_is_dir)) = self
                                        .store
                                        .state()
                                        .explorer
                                        .path_and_kind_for(to_dir_id)
                                    else {
                                        continue;
                                    };
                                    if !to_is_dir {
                                        continue;
                                    }

                                    let Some(to) = compute_explorer_move_target(
                                        from_path.as_path(),
                                        from_is_dir,
                                        to_dir_path.as_path(),
                                    ) else {
                                        continue;
                                    };

                                    let _ = self.dispatch_kernel(KernelAction::ExplorerMovePath {
                                        from: from_path,
                                        to,
                                    });
                                    handled = true;
                                }
                                NodeKind::ExplorerRow { node_id: to_row_id } => {
                                    let state = self.store.state();
                                    let explorer = &state.explorer;

                                    let Some((from_path, from_is_dir)) =
                                        explorer.path_and_kind_for(node_id)
                                    else {
                                        continue;
                                    };

                                    let Some((to_path, to_is_dir)) =
                                        explorer.path_and_kind_for(to_row_id)
                                    else {
                                        continue;
                                    };

                                    let to_dir = if to_is_dir {
                                        to_path.as_path()
                                    } else {
                                        to_path
                                            .parent()
                                            .unwrap_or(state.workspace_root.as_path())
                                    };

                                    let Some(to) = compute_explorer_move_target(
                                        from_path.as_path(),
                                        from_is_dir,
                                        to_dir,
                                    ) else {
                                        continue;
                                    };

                                    let _ = self.dispatch_kernel(KernelAction::ExplorerMovePath {
                                        from: from_path,
                                        to,
                                    });
                                    handled = true;
                                }
                                _ => {}
                            }
                        }
                        UiEvent::DragEnd { .. } => handled = true,
                        _ => {}
                    }
                }

                if handled || out.needs_redraw || captured_is_explorer_row {
                    return EventResult::Consumed;
                }

                EventResult::Ignored
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if in_git {
                    return EventResult::Consumed;
                }
                let _ = self.ui_runtime.on_input(&InputEvent::Mouse(*event), &self.ui_tree);
                EventResult::Consumed
            }
            MouseEventKind::Up(MouseButton::Right) => {
                let out = self.ui_runtime.on_input(&InputEvent::Mouse(*event), &self.ui_tree);

                let mut handled = false;
                for ev in out.events {
                    let UiEvent::ContextMenu { id, pos } = ev else {
                        continue;
                    };

                    let tree_row = match self.ui_tree.node(id).map(|n| n.kind) {
                        Some(NodeKind::ExplorerRow { node_id }) => self
                            .store
                            .state()
                            .explorer
                            .rows
                            .iter()
                            .position(|r| r.id == node_id),
                        _ => None,
                    };

                    let tree_row = tree_row.filter(|row| *row < rows_len);
                    let _ = self.dispatch_kernel(KernelAction::ContextMenuOpen {
                        request: crate::kernel::state::ContextMenuRequest::Explorer { tree_row },
                        x: pos.x,
                        y: pos.y,
                    });
                    handled = true;
                }

                handled.then_some(EventResult::Consumed).unwrap_or(EventResult::Ignored)
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

fn compute_explorer_move_target(from: &Path, from_is_dir: bool, to_dir: &Path) -> Option<std::path::PathBuf> {
    if from_is_dir && (to_dir == from || to_dir.starts_with(from)) {
        return None;
    }

    let name = from.file_name()?;
    let to = to_dir.join(name);
    (to != from).then_some(to)
}

#[cfg(test)]
#[path = "../../../../tests/unit/app/workbench/explorer_dnd.rs"]
mod tests;
