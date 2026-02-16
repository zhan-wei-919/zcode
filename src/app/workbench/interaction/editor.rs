use super::super::{util, Workbench};
use crate::core::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::core::Command;
use crate::kernel::editor::{EditorTabState, TabId};
use crate::kernel::services::adapters::perf;
use crate::kernel::{Action as KernelAction, EditorAction, PendingAction};
use crate::models::Granularity;
use crate::tui::view::EventResult;
use crate::ui::core::geom::Pos;
use crate::ui::core::input::{DragPayload, UiEvent};
use crate::ui::core::runtime::UiRuntimeOutput;
use crate::ui::core::tree::NodeKind;
use crate::views::editor::coord;
use crate::views::editor::markdown::MarkdownDocument;
use crate::views::{
    compute_editor_pane_layout, hit_test_editor_mouse, hit_test_editor_mouse_drag,
    hit_test_editor_tab, hit_test_editor_vertical_scrollbar, hit_test_search_bar,
    hit_test_tab_hover, tab_insertion_index, vertical_scrollbar_metrics,
    EditorVerticalScrollbarHitResult, SearchBarHitResult, TabHitResult,
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

        let pane = if let Some(drag) = self.editor_scrollbar_drag {
            drag.pane
        } else if self.store.state().editor.pane(active_pane).is_some() {
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

        let active_md_tab_id = self.ensure_markdown_view_for_active_tab(pane);
        let Some(pane_state) = self.store.state().editor.pane(pane) else {
            return EventResult::Ignored;
        };
        let config = &self.store.state().editor.config;
        let tab_size = config.tab_size;
        let click_slop = config.click_slop;
        let triple_click_ms = config.triple_click_ms;
        let scroll_step = config.scroll_step();
        let layout = compute_editor_pane_layout(area, pane_state, config);
        let scrollbar_metrics = pane_state.active_tab().and_then(|tab| {
            vertical_scrollbar_metrics(
                &layout,
                tab.buffer.len_lines().max(1),
                layout.editor_area.h as usize,
                tab.viewport.line_offset,
            )
        });

        let hovered_idx = self
            .store
            .state()
            .ui
            .hovered_tab
            .filter(|(hp, _)| *hp == pane)
            .map(|(_, i)| i);

        if matches!(
            event.kind,
            MouseEventKind::Moved
                | MouseEventKind::Down(_)
                | MouseEventKind::Up(_)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight
        ) {
            let pointer = Pos::new(event.column, event.row);
            self.editor_scrollbar_hover = layout
                .v_scrollbar_area
                .filter(|area| area.contains(pointer))
                .map(|_| pane);
        }

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.editor_scrollbar_drag = None;

                if let Some(result) =
                    hit_test_search_bar(&layout, &pane_state.search_bar, event.column, event.row)
                {
                    let cmd = match result {
                        SearchBarHitResult::PrevMatch => Command::FindPrev,
                        SearchBarHitResult::NextMatch => Command::FindNext,
                        SearchBarHitResult::Close => Command::EditorSearchBarClose,
                    };
                    let _ = self.dispatch_kernel(KernelAction::RunCommand(cmd));
                    return EventResult::Consumed;
                }

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

                if let Some(metrics) = scrollbar_metrics {
                    if let Some(hit) = hit_test_editor_vertical_scrollbar(
                        &layout,
                        &metrics,
                        event.column,
                        event.row,
                    ) {
                        match hit {
                            EditorVerticalScrollbarHitResult::Thumb { row } => {
                                let track_top = metrics.track_area.y;
                                let thumb_top = metrics.thumb_area.y.saturating_sub(track_top);
                                let grab_offset = row.saturating_sub(thumb_top);
                                self.editor_scrollbar_drag =
                                    Some(super::super::EditorScrollbarDragState {
                                        pane,
                                        grab_offset,
                                    });
                            }
                            EditorVerticalScrollbarHitResult::Track { row } => {
                                let grab_offset = metrics.thumb_area.h.saturating_sub(1) / 2;
                                let pointer_row = metrics.track_area.y.saturating_add(row);
                                let target_offset =
                                    metrics.line_offset_for_pointer_row(pointer_row, grab_offset);
                                let _ = self.scroll_editor_to_line_offset(pane, target_offset);
                                self.editor_scrollbar_drag =
                                    Some(super::super::EditorScrollbarDragState {
                                        pane,
                                        grab_offset,
                                    });
                            }
                        }

                        return EventResult::Consumed;
                    }
                }

                if let Some((x, y)) = hit_test_editor_mouse(&layout, event.column, event.row) {
                    if let Some(tab) = pane_state.active_tab() {
                        let visible = tab.visible_lines_in_viewport(
                            tab.viewport.line_offset,
                            tab.viewport.height.max(1),
                        );
                        if let Some(&row) = visible.get(y as usize) {
                            let col = {
                                let md = active_md_tab_id
                                    .and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                coord::resolve_source_col(tab, md, row, x, tab_size)
                            };
                            if let Some(col) = col {
                                // Ensure per-pane tracker exists
                                while self.editor_mouse.len() <= pane {
                                    self.editor_mouse.push(
                                        super::super::mouse_tracker::EditorMouseTracker::new(),
                                    );
                                }
                                let granularity = self.editor_mouse[pane].click(
                                    event.column,
                                    event.row,
                                    Instant::now(),
                                    click_slop,
                                    triple_click_ms,
                                );

                                if granularity == Granularity::Char {
                                    let toggle = {
                                        let md = active_md_tab_id
                                            .and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                        md.and_then(|doc| {
                                            markdown_task_toggle_edit(tab, doc, row, col)
                                        })
                                    };
                                    if let Some((start_char, text)) = toggle {
                                        let _ = self.dispatch_kernel(KernelAction::Editor(
                                            EditorAction::ReplaceRangeChars {
                                                pane,
                                                start_char,
                                                end_char: start_char.saturating_add(1),
                                                text: text.to_string(),
                                            },
                                        ));
                                        self.editor_mouse[pane].stop_drag();
                                        return EventResult::Consumed;
                                    }
                                }

                                let _ = self.dispatch_kernel(KernelAction::Editor(
                                    EditorAction::PlaceCursor {
                                        pane,
                                        row,
                                        col,
                                        granularity,
                                    },
                                ));
                            }
                        }
                    }
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
                if let Some(drag) = self.editor_scrollbar_drag {
                    if drag.pane == pane {
                        if let Some(metrics) = scrollbar_metrics {
                            let target_offset =
                                metrics.line_offset_for_pointer_row(event.row, drag.grab_offset);
                            let _ = self.scroll_editor_to_line_offset(pane, target_offset);
                        }
                        return EventResult::Consumed;
                    }
                }

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
                    // Ensure per-pane tracker exists
                    while self.editor_mouse.len() <= pane {
                        self.editor_mouse
                            .push(super::super::mouse_tracker::EditorMouseTracker::new());
                    }
                    if !self.editor_mouse[pane].dragging() {
                        return EventResult::Consumed;
                    }

                    // Handle overflow scrolling in workbench layer
                    if hit.overflow_y != 0 {
                        if let Some(tab) = self
                            .store
                            .state()
                            .editor
                            .pane(pane)
                            .and_then(|p| p.active_tab())
                        {
                            let max_offset = tab
                                .buffer
                                .len_lines()
                                .max(1)
                                .saturating_sub(tab.viewport.height.max(1));
                            let current = tab.viewport.line_offset;
                            let target = if hit.overflow_y < 0 {
                                current.saturating_sub((-hit.overflow_y) as usize)
                            } else {
                                (current + hit.overflow_y as usize).min(max_offset)
                            };
                            if target != current {
                                let delta = target as isize - current as isize;
                                let _ = self.scroll_editor_vertically(pane, delta);
                            }
                        }
                    }

                    // Convert screen coordinates to source coordinates
                    if let Some(tab) = self
                        .store
                        .state()
                        .editor
                        .pane(pane)
                        .and_then(|p| p.active_tab())
                    {
                        let visible = tab.visible_lines_in_viewport(
                            tab.viewport.line_offset,
                            tab.viewport.height.max(1),
                        );
                        if let Some(&row) = visible.get(hit.y as usize) {
                            let col = if hit.past_right {
                                tab.buffer.line_grapheme_len(row)
                            } else {
                                let md = active_md_tab_id
                                    .and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                match coord::resolve_source_col(tab, md, row, hit.x, tab_size) {
                                    Some(c) => c,
                                    None => return EventResult::Consumed,
                                }
                            };
                            let _ = self.dispatch_kernel(KernelAction::Editor(
                                EditorAction::ExtendSelection { pane, row, col },
                            ));
                        }
                    }
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.editor_scrollbar_drag.take().is_some() {
                    return EventResult::Consumed;
                }

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

                // Ensure per-pane tracker exists
                while self.editor_mouse.len() <= pane {
                    self.editor_mouse
                        .push(super::super::mouse_tracker::EditorMouseTracker::new());
                }
                self.editor_mouse[pane].stop_drag();
                let _ =
                    self.dispatch_kernel(KernelAction::Editor(EditorAction::EndSelectionGesture {
                        pane,
                    }));
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
                        NodeKind::TabBar { pane } => {
                            let _ = self.dispatch_kernel(KernelAction::ContextMenuOpen {
                                request: crate::kernel::state::ContextMenuRequest::TabBar { pane },
                                x: pos.x,
                                y: pos.y,
                            });
                            handled = true;
                        }
                        NodeKind::EditorArea { pane } => {
                            if let Some((x, y)) = hit_test_editor_mouse(&layout, pos.x, pos.y) {
                                let md_tab_id = self.ensure_markdown_view_for_active_tab(pane);
                                if let Some(tab) = self
                                    .store
                                    .state()
                                    .editor
                                    .pane(pane)
                                    .and_then(|p| p.active_tab())
                                {
                                    let visible = tab.visible_lines_in_viewport(
                                        tab.viewport.line_offset,
                                        tab.viewport.height.max(1),
                                    );
                                    if let Some(&row) = visible.get(y as usize) {
                                        let col = {
                                            let md = md_tab_id.and_then(|tab_id| {
                                                self.markdown_doc_for_tab(tab_id)
                                            });
                                            coord::resolve_source_col(tab, md, row, x, tab_size)
                                        };
                                        if let Some(col) = col {
                                            // If right-click is inside existing non-empty selection, keep it
                                            let inside_selection =
                                                tab.buffer.selection().is_some_and(|sel| {
                                                    !sel.is_empty() && sel.contains((row, col))
                                                });
                                            if !inside_selection {
                                                let _ = self.dispatch_kernel(KernelAction::Editor(
                                                    EditorAction::PlaceCursor {
                                                        pane,
                                                        row,
                                                        col,
                                                        granularity: Granularity::Word,
                                                    },
                                                ));
                                            }
                                        }
                                    }
                                }
                            }

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

                            let col =
                                coord::screen_to_col(&tab.viewport, &tab.buffer, tab_size, row, x)?;

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
                    let step = scroll_step.max(1) as isize;
                    let _ = self.scroll_completion_doc_by(-step);
                    return EventResult::Consumed;
                }
                if self.store.state().ui.hover_message.is_some()
                    && self
                        .hover_popup
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    let step = scroll_step.max(1) as isize;
                    let _ = self.scroll_hover_popup_by(-step);
                    return EventResult::Consumed;
                }

                if event.modifiers.contains(KeyModifiers::SHIFT) {
                    let _ = self.scroll_editor_horizontally(
                        pane,
                        -Self::mouse_horizontal_step(scroll_step, tab_size),
                    );
                    return EventResult::Consumed;
                }

                let _ = self.scroll_editor_vertically(pane, -(scroll_step as isize));
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                if self.store.state().ui.completion.visible
                    && self
                        .completion_doc
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    let step = scroll_step.max(1) as isize;
                    let _ = self.scroll_completion_doc_by(step);
                    return EventResult::Consumed;
                }
                if self.store.state().ui.hover_message.is_some()
                    && self
                        .hover_popup
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    let step = scroll_step.max(1) as isize;
                    let _ = self.scroll_hover_popup_by(step);
                    return EventResult::Consumed;
                }

                if event.modifiers.contains(KeyModifiers::SHIFT) {
                    let _ = self.scroll_editor_horizontally(
                        pane,
                        Self::mouse_horizontal_step(scroll_step, tab_size),
                    );
                    return EventResult::Consumed;
                }

                let _ = self.scroll_editor_vertically(pane, scroll_step as isize);
                EventResult::Consumed
            }
            MouseEventKind::ScrollLeft => {
                if self.store.state().ui.completion.visible
                    && self
                        .completion_doc
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return EventResult::Consumed;
                }
                if self.store.state().ui.hover_message.is_some()
                    && self
                        .hover_popup
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return EventResult::Consumed;
                }

                let _ = self.scroll_editor_horizontally(
                    pane,
                    -Self::mouse_horizontal_step(scroll_step, tab_size),
                );
                EventResult::Consumed
            }
            MouseEventKind::ScrollRight => {
                if self.store.state().ui.completion.visible
                    && self
                        .completion_doc
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return EventResult::Consumed;
                }
                if self.store.state().ui.hover_message.is_some()
                    && self
                        .hover_popup
                        .last_area
                        .is_some_and(|a| util::rect_contains(a, event.column, event.row))
                {
                    return EventResult::Consumed;
                }

                let _ = self.scroll_editor_horizontally(
                    pane,
                    Self::mouse_horizontal_step(scroll_step, tab_size),
                );
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn mouse_horizontal_step(scroll_step: usize, tab_size: u8) -> isize {
        let scroll_step = scroll_step.max(1);
        let tab_size = (tab_size as usize).max(1);
        scroll_step.saturating_mul(tab_size) as isize
    }

    fn scroll_editor_to_line_offset(&mut self, pane: usize, target_offset: usize) -> bool {
        let current_offset = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|p| p.active_tab())
            .map(|tab| tab.viewport.line_offset);
        let Some(current_offset) = current_offset else {
            return false;
        };
        if current_offset == target_offset {
            return false;
        }

        let delta_lines = target_offset as isize - current_offset as isize;
        self.scroll_editor_vertically(pane, delta_lines)
    }

    fn scroll_editor_vertically(&mut self, pane: usize, delta_lines: isize) -> bool {
        if delta_lines == 0 {
            return false;
        }

        let changed = self.dispatch_kernel(KernelAction::Editor(EditorAction::Scroll {
            pane,
            delta_lines,
        }));

        let cmd = if delta_lines > 0 {
            Command::ScrollDown
        } else {
            Command::ScrollUp
        };
        self.maybe_schedule_semantic_tokens_debounce(&cmd);
        self.maybe_schedule_inlay_hints_debounce(&cmd);
        self.maybe_schedule_folding_range_debounce(&cmd);

        changed
    }

    fn scroll_editor_horizontally(&mut self, pane: usize, delta_columns: isize) -> bool {
        if delta_columns == 0 {
            return false;
        }
        self.dispatch_kernel(KernelAction::Editor(EditorAction::ScrollHorizontal {
            pane,
            delta_columns,
        }))
    }
}

fn markdown_task_toggle_edit(
    tab: &EditorTabState,
    md: &MarkdownDocument,
    row: usize,
    col: usize,
) -> Option<(usize, &'static str)> {
    let marker = md.task_marker(row, tab.buffer.rope())?;
    if marker.source_end <= marker.source_start || marker.source_start + 1 >= marker.source_end {
        return None;
    }

    let rope = tab.buffer.rope();
    let line_start_byte = rope.line_to_byte(row);
    let line_start_char = rope.line_to_char(row);
    let marker_start_char = rope.byte_to_char(line_start_byte + marker.source_start);
    let marker_end_char = rope.byte_to_char(line_start_byte + marker.source_end);
    let toggle_char_start = rope.byte_to_char(line_start_byte + marker.source_start + 1);

    let marker_start_col = marker_start_char.saturating_sub(line_start_char);
    let marker_end_col = marker_end_char.saturating_sub(line_start_char);
    if col < marker_start_col || col >= marker_end_col {
        return None;
    }

    let replacement = if marker.checked { " " } else { "x" };
    Some((toggle_char_start, replacement))
}
