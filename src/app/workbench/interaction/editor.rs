use super::super::{util, Workbench};
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::editor::TabId;
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, EditorAction, PendingAction};
use crate::tui::view::EventResult;
use crate::ui::core::input::{DragPayload, UiEvent};
use crate::ui::core::runtime::UiRuntimeOutput;
use crate::ui::core::tree::NodeKind;
use crate::views::{
    compute_editor_pane_layout, hit_test_editor_mouse, hit_test_editor_mouse_drag,
    hit_test_editor_tab, hit_test_tab_hover, tab_insertion_index, TabHitResult,
};
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn handle_editor_mouse(
        &mut self,
        event: &MouseEvent,
        ui_out: &UiRuntimeOutput,
    ) -> EventResult {
        let _scope = perf::scope("input.mouse.editor");
        let active_pane = self.store.state().ui.editor_layout.active_pane;

        let pane = if self.store.state().editor.pane(active_pane).is_some() {
            active_pane
        } else {
            0
        };

        let area = self
            .layout_cache
            .editor_inner_areas
            .get(pane)
            .copied()
            .or_else(|| self.layout_cache.editor_inner_areas.first().copied());
        let Some(area) = area else {
            return EventResult::Ignored;
        };

        let Some(pane_state) = self.store.state().editor.pane(pane) else {
            return EventResult::Ignored;
        };
        let config = &self.store.state().editor.config;
        let layout = compute_editor_pane_layout(area, pane_state, config);

        let hovered_idx = self
            .store
            .state()
            .ui
            .hovered_tab
            .filter(|(hp, _)| *hp == pane)
            .map(|(_, i)| i);

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(result) =
                    hit_test_editor_tab(&layout, pane_state, event.column, event.row, hovered_idx)
                {
                    match result {
                        TabHitResult::CloseButton(index) => {
                            let is_dirty = self
                                .store
                                .state()
                                .editor
                                .pane(pane)
                                .is_some_and(|p| p.is_tab_dirty(index));

                            if is_dirty {
                                let _ = self.dispatch_kernel(KernelAction::ShowConfirmDialog {
                                    message: "Unsaved changes. Close anyway?".to_string(),
                                    on_confirm: PendingAction::CloseTab { pane, index },
                                });
                            } else {
                                let _ = self.dispatch_kernel(KernelAction::Editor(
                                    EditorAction::CloseTabAt { pane, index },
                                ));
                            }
                        }
                        // Title clicks are handled on MouseUp via UiRuntime so we can support
                        // drag-and-drop without switching tabs on MouseDown.
                        TabHitResult::Title(_index) => {}
                    }
                    return EventResult::Consumed;
                }

                if let Some((x, y)) = hit_test_editor_mouse(&layout, event.column, event.row) {
                    let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseDown {
                        pane,
                        x,
                        y,
                        now: Instant::now(),
                    }));
                    return EventResult::Consumed;
                }

                EventResult::Ignored
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                if let Some(index) =
                    hit_test_tab_hover(&layout, pane_state, event.column, event.row, hovered_idx)
                {
                    let is_dirty = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .is_some_and(|p| p.is_tab_dirty(index));

                    if is_dirty {
                        let _ = self.dispatch_kernel(KernelAction::ShowConfirmDialog {
                            message: "Unsaved changes. Close anyway?".to_string(),
                            on_confirm: PendingAction::CloseTab { pane, index },
                        });
                    } else {
                        let _ =
                            self.dispatch_kernel(KernelAction::Editor(EditorAction::CloseTabAt {
                                pane,
                                index,
                            }));
                    }
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            MouseEventKind::Down(MouseButton::Right) => EventResult::Consumed,
            MouseEventKind::Drag(MouseButton::Left) => {
                let captured_is_tab = self
                    .ui_runtime
                    .capture()
                    .and_then(|id| self.ui_tree.node(id))
                    .is_some_and(|n| matches!(n.kind, NodeKind::Tab { .. }));
                if captured_is_tab {
                    // Tab drag: do not forward to the editor text selection logic.
                    return EventResult::Consumed;
                }

                if let Some(hit) = hit_test_editor_mouse_drag(&layout, event.column, event.row) {
                    let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseDrag {
                        pane,
                        x: hit.x,
                        y: hit.y,
                        overflow_y: hit.overflow_y,
                        past_right: hit.past_right,
                    }));
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let mut handled = false;
                for ev in &ui_out.events {
                    match ev {
                        UiEvent::Click { id, .. } => {
                            if let Some(node) = self.ui_tree.node(*id) {
                                if let NodeKind::Tab { pane, tab_id } = node.kind {
                                    let tab_id = TabId::new(tab_id);
                                    let Some(index) =
                                        self.store.state().editor.pane(pane).and_then(|p| {
                                            p.tabs.iter().position(|t| t.id == tab_id)
                                        })
                                    else {
                                        continue;
                                    };
                                    let _ = self.dispatch_kernel(KernelAction::Editor(
                                        EditorAction::SetActiveTab { pane, index },
                                    ));
                                    handled = true;
                                }
                            }
                        }
                        UiEvent::DragEnd { id, .. } => {
                            if self
                                .ui_tree
                                .node(*id)
                                .is_some_and(|n| matches!(n.kind, NodeKind::Tab { .. }))
                            {
                                handled = true;
                            }
                        }
                        UiEvent::Drop {
                            payload,
                            target,
                            pos,
                        } => {
                            let Some(target_node) = self.ui_tree.node(*target) else {
                                continue;
                            };
                            let DragPayload::Tab { from_pane, tab_id } = *payload else {
                                continue;
                            };
                            let tab_id = TabId::new(tab_id);

                            match target_node.kind {
                                NodeKind::TabBar { pane: to_pane } => {
                                    let Some(to_pane_state) =
                                        self.store.state().editor.pane(to_pane)
                                    else {
                                        continue;
                                    };
                                    let Some(to_area) = self
                                        .layout_cache
                                        .editor_inner_areas
                                        .get(to_pane)
                                        .copied()
                                        .or_else(|| {
                                            self.layout_cache.editor_inner_areas.first().copied()
                                        })
                                    else {
                                        continue;
                                    };
                                    let config = &self.store.state().editor.config;
                                    let to_layout =
                                        compute_editor_pane_layout(to_area, to_pane_state, config);

                                    let hovered_to = self
                                        .store
                                        .state()
                                        .ui
                                        .hovered_tab
                                        .filter(|(hp, _)| *hp == to_pane)
                                        .map(|(_, i)| i);

                                    let Some(to_index) = tab_insertion_index(
                                        &to_layout,
                                        to_pane_state,
                                        pos.x,
                                        pos.y,
                                        hovered_to,
                                    ) else {
                                        continue;
                                    };

                                    let _ = self.dispatch_kernel(KernelAction::Editor(
                                        EditorAction::MoveTab {
                                            tab_id,
                                            from_pane,
                                            to_pane,
                                            to_index,
                                        },
                                    ));
                                    let _ =
                                        self.dispatch_kernel(KernelAction::EditorSetActivePane {
                                            pane: to_pane,
                                        });
                                    handled = true;
                                }
                                NodeKind::EditorSplitDrop { drop, .. } => {
                                    let cmd = match drop {
                                        crate::ui::core::tree::SplitDrop::Right => {
                                            Command::SplitEditorVertical
                                        }
                                        crate::ui::core::tree::SplitDrop::Down => {
                                            Command::SplitEditorHorizontal
                                        }
                                    };
                                    let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd));

                                    let to_pane = 1usize;
                                    let to_index = self
                                        .store
                                        .state()
                                        .editor
                                        .pane(to_pane)
                                        .map(|p| p.tabs.len())
                                        .unwrap_or(0);
                                    let _ = self.dispatch_kernel(KernelAction::Editor(
                                        EditorAction::MoveTab {
                                            tab_id,
                                            from_pane,
                                            to_pane,
                                            to_index,
                                        },
                                    ));
                                    let _ =
                                        self.dispatch_kernel(KernelAction::EditorSetActivePane {
                                            pane: to_pane,
                                        });
                                    handled = true;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }

                if handled {
                    return EventResult::Consumed;
                }

                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseUp { pane }));
                EventResult::Consumed
            }
            MouseEventKind::Up(MouseButton::Right) => {
                let mut handled = false;
                for ev in &ui_out.events {
                    let UiEvent::ContextMenu { id, pos } = *ev else {
                        continue;
                    };

                    let Some(node) = self.ui_tree.node(id) else {
                        continue;
                    };
                    match node.kind {
                        NodeKind::Tab { pane, tab_id } => {
                            let tab_id = TabId::new(tab_id);
                            let Some(index) = self
                                .store
                                .state()
                                .editor
                                .pane(pane)
                                .and_then(|p| p.tabs.iter().position(|t| t.id == tab_id))
                            else {
                                continue;
                            };
                            let _ = self.dispatch_kernel(KernelAction::ContextMenuOpen {
                                request: crate::kernel::state::ContextMenuRequest::Tab {
                                    pane,
                                    index,
                                },
                                x: pos.x,
                                y: pos.y,
                            });
                            handled = true;
                        }
                        NodeKind::EditorArea { pane } => {
                            let _ = self.dispatch_kernel(KernelAction::ContextMenuOpen {
                                request: crate::kernel::state::ContextMenuRequest::EditorArea {
                                    pane,
                                },
                                x: pos.x,
                                y: pos.y,
                            });
                            handled = true;
                        }
                        _ => {}
                    }
                }

                if handled {
                    EventResult::Consumed
                } else {
                    EventResult::Ignored
                }
            }
            MouseEventKind::Moved => {
                // Track the last mouse position inside the editor content so idle-hover requests
                // can use the hovered symbol (VSCode/Helix-like) instead of the cursor.
                let idle_target =
                    if let Some((x, y)) = hit_test_editor_mouse(&layout, event.column, event.row) {
                        pane_state.active_tab().and_then(|tab| {
                            let visible_lines = tab.visible_lines_in_viewport(
                                tab.viewport.line_offset,
                                tab.viewport.height.max(1),
                            );
                            let row = visible_lines.get(y as usize).copied()?;

                            let col = crate::kernel::editor::screen_to_col(
                                &tab.viewport,
                                &tab.buffer,
                                config.tab_size,
                                row,
                                x,
                            )?;

                            Some(super::super::IdleHoverTarget {
                                pane,
                                row,
                                col,
                                anchor: (event.column, event.row),
                            })
                        })
                    } else {
                        None
                    };

                self.hover_popup.target = idle_target;

                if let Some(index) =
                    hit_test_tab_hover(&layout, pane_state, event.column, event.row, hovered_idx)
                {
                    let _ = self.dispatch_kernel(KernelAction::SetHoveredTab { pane, index });
                } else {
                    let _ = self.dispatch_kernel(KernelAction::ClearHoveredTab);
                }

                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                if self.store.state().ui.completion.visible
                    && self
                        .completion_doc
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    let step = config.scroll_step().max(1) as isize;
                    let _ = self.scroll_completion_doc_by(-step);
                    return EventResult::Consumed;
                }
                if self.store.state().ui.hover_message.is_some()
                    && self
                        .hover_popup
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    let step = config.scroll_step().max(1) as isize;
                    let _ = self.scroll_hover_popup_by(-step);
                    return EventResult::Consumed;
                }

                let delta_lines = -(config.scroll_step() as isize);
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::Scroll {
                    pane,
                    delta_lines,
                }));
                let cmd = Command::ScrollUp;
                self.maybe_schedule_semantic_tokens_debounce(&cmd);
                self.maybe_schedule_inlay_hints_debounce(&cmd);
                self.maybe_schedule_folding_range_debounce(&cmd);
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                if self.store.state().ui.completion.visible
                    && self
                        .completion_doc
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    let step = config.scroll_step().max(1) as isize;
                    let _ = self.scroll_completion_doc_by(step);
                    return EventResult::Consumed;
                }
                if self.store.state().ui.hover_message.is_some()
                    && self
                        .hover_popup
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    let step = config.scroll_step().max(1) as isize;
                    let _ = self.scroll_hover_popup_by(step);
                    return EventResult::Consumed;
                }

                let delta_lines = config.scroll_step() as isize;
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::Scroll {
                    pane,
                    delta_lines,
                }));
                let cmd = Command::ScrollDown;
                self.maybe_schedule_semantic_tokens_debounce(&cmd);
                self.maybe_schedule_inlay_hints_debounce(&cmd);
                self.maybe_schedule_folding_range_debounce(&cmd);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }
}
