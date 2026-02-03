use super::super::Workbench;
use crate::core::Command;
use crate::core::event::{MouseButton, MouseEvent, MouseEventKind};
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, EditorAction, PendingAction};
use crate::tui::view::EventResult;
use crate::views::{
    compute_editor_pane_layout, hit_test_editor_mouse, hit_test_editor_tab, hit_test_tab_hover,
    TabHitResult,
};
use std::time::Instant;

impl Workbench {
    pub(in super::super) fn handle_editor_mouse(&mut self, event: &MouseEvent) -> EventResult {
        let _scope = perf::scope("input.mouse.editor");
        let active_pane = self.store.state().ui.editor_layout.active_pane;

        let pane = if self.store.state().editor.pane(active_pane).is_some() {
            active_pane
        } else {
            0
        };

        let area = self
            .last_editor_inner_areas
            .get(pane)
            .copied()
            .or_else(|| self.last_editor_inner_areas.first().copied());
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
                        TabHitResult::Title(index) => {
                            let _ = self.dispatch_kernel(KernelAction::Editor(
                                EditorAction::SetActiveTab { pane, index },
                            ));
                        }
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
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((x, y)) = hit_test_editor_mouse(&layout, event.column, event.row) {
                    let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseDrag {
                        pane,
                        x,
                        y,
                    }));
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::MouseUp { pane }));
                EventResult::Consumed
            }
            MouseEventKind::Moved => {
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
