use super::Workbench;
use crate::core::event::{InputEvent, MouseEvent, MouseEventKind};
use crate::kernel::{FocusTarget, SidebarTab};
use crate::tui::view::EventResult;
use crate::ui::core::id::IdPath;
use crate::ui::core::input::UiEvent;
use crate::ui::core::tree::NodeKind;

pub(super) fn handle_input(workbench: &mut Workbench, event: &InputEvent) -> EventResult {
    if matches!(
        event,
        InputEvent::Key(_) | InputEvent::Mouse(_) | InputEvent::Paste(_)
    ) {
        workbench.record_user_input(event);
    }

    match event {
        InputEvent::Key(key_event) => workbench.handle_key_event(key_event),
        InputEvent::Paste(text) => workbench.handle_paste(text),
        InputEvent::Mouse(mouse_event) => {
            let mouse_event = workbench.normalize_mouse_event(*mouse_event);
            let mouse_input_event = InputEvent::Mouse(mouse_event);

            if workbench.store.state().ui.context_menu.visible {
                let overlay_id = IdPath::root("workbench")
                    .push_str("context_menu")
                    .push_str("overlay")
                    .finish();
                let out = workbench
                    .ui_runtime
                    .on_input(&mouse_input_event, &workbench.ui_tree);

                let mut state_changed = false;
                for ev in out.events {
                    match ev {
                        UiEvent::HoverChanged { to: Some(id), .. } => {
                            if let Some(node) = workbench.ui_tree.node(id) {
                                if let NodeKind::MenuItem { index, .. } = node.kind {
                                    state_changed |= workbench.dispatch_kernel(
                                        crate::kernel::Action::ContextMenuSetSelected { index },
                                    );
                                }
                            }
                        }
                        UiEvent::Click { id, .. } => {
                            if let Some(node) = workbench.ui_tree.node(id) {
                                match node.kind {
                                    NodeKind::MenuItem { index, .. } => {
                                        state_changed |= workbench.dispatch_kernel(
                                            crate::kernel::Action::ContextMenuSetSelected { index },
                                        );
                                        state_changed |= workbench.dispatch_kernel(
                                            crate::kernel::Action::ContextMenuConfirm,
                                        );
                                    }
                                    _ => {
                                        if id == overlay_id {
                                            state_changed |= workbench.dispatch_kernel(
                                                crate::kernel::Action::ContextMenuClose,
                                            );
                                        }
                                    }
                                }
                            } else if id == overlay_id {
                                state_changed |= workbench
                                    .dispatch_kernel(crate::kernel::Action::ContextMenuClose);
                            }
                        }
                        UiEvent::ContextMenu { .. } => {
                            state_changed |=
                                workbench.dispatch_kernel(crate::kernel::Action::ContextMenuClose);
                        }
                        _ => {}
                    }
                }

                if state_changed || out.needs_redraw {
                    return EventResult::Consumed;
                }

                // Modal overlay: swallow mouse events even when it doesn't redraw.
                return EventResult::Ignored;
            }

            // When theme editor is visible, handle mouse directly without
            // area-based focus switching (which would steal focus to Editor).
            if workbench.store.state().ui.theme_editor.visible {
                return workbench.handle_theme_editor_mouse(&mouse_event);
            }

            if let Some(result) = workbench.handle_sidebar_split_mouse(&mouse_event) {
                return result;
            }

            if let Some(result) = workbench.handle_editor_split_mouse(&mouse_event) {
                return result;
            }

            let state_changed = workbench.handle_mouse_area(&mouse_event);
            let focus = workbench.store.state().ui.focus;

            let result = match focus {
                FocusTarget::Explorer => {
                    if workbench.store.state().ui.sidebar_tab == SidebarTab::Search {
                        workbench.handle_search_mouse(&mouse_event)
                    } else {
                        workbench.handle_explorer_mouse(&mouse_event)
                    }
                }
                FocusTarget::Editor => workbench.handle_editor_mouse(&mouse_event),
                FocusTarget::BottomPanel => workbench.handle_bottom_panel_mouse(&mouse_event),
                FocusTarget::CommandPalette => EventResult::Ignored,
                FocusTarget::ThemeEditor => workbench.handle_theme_editor_mouse(&mouse_event),
            };

            if state_changed && matches!(result, EventResult::Ignored) {
                return EventResult::Consumed;
            }

            result
        }
        InputEvent::Resize(_, _) => EventResult::Consumed,
        _ => EventResult::Ignored,
    }
}

impl Workbench {
    fn normalize_mouse_event(&mut self, event: MouseEvent) -> MouseEvent {
        match event.kind {
            MouseEventKind::Down(_) => {
                self.mouse_has_reported_down = true;
                event
            }
            MouseEventKind::Up(button)
                if self.mouse_up_as_down_compat && !self.mouse_has_reported_down =>
            {
                MouseEvent {
                    kind: MouseEventKind::Down(button),
                    ..event
                }
            }
            _ => event,
        }
    }
}
