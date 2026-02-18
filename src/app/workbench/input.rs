use super::Workbench;
use crate::core::event::{InputEvent, MouseEventKind};
use crate::kernel::Action as KernelAction;
use crate::tui::view::EventResult;
use crate::ui::core::id::IdPath;
use crate::ui::core::input::UiEvent;
use crate::ui::core::runtime::UiRuntimeOutput;
use crate::ui::core::tree::NodeKind;

use super::mouse_route::{mouse_target_from_focus, plan_mouse_dispatch, FocusPlan, MouseTarget};
use super::util;

fn apply_focus_plan(
    workbench: &mut Workbench,
    event: &crate::core::event::MouseEvent,
    focus_plan: FocusPlan,
) -> bool {
    match focus_plan {
        FocusPlan::BottomPanel
        | FocusPlan::ActivityBar
        | FocusPlan::SidebarTabs
        | FocusPlan::SidebarArea => workbench.handle_mouse_area(event),
        FocusPlan::EditorPane { pane } => {
            workbench.dispatch_kernel(KernelAction::EditorSetActivePane { pane })
        }
    }
}

fn dispatch_by_target(
    workbench: &mut Workbench,
    target: MouseTarget,
    mouse_event: &crate::core::event::MouseEvent,
    ui_out: &UiRuntimeOutput,
) -> EventResult {
    match target {
        MouseTarget::SidebarSplitter => workbench
            .handle_sidebar_split_mouse(mouse_event, ui_out)
            .unwrap_or(EventResult::Ignored),
        MouseTarget::BottomPanelSplitter => workbench
            .handle_bottom_panel_split_mouse(mouse_event, ui_out)
            .unwrap_or(EventResult::Ignored),
        MouseTarget::EditorSplitter => workbench
            .handle_editor_split_mouse(mouse_event, ui_out)
            .unwrap_or(EventResult::Ignored),
        MouseTarget::Explorer => workbench.handle_explorer_mouse(mouse_event, ui_out),
        MouseTarget::Search => workbench.handle_search_mouse(mouse_event),
        MouseTarget::Editor => workbench.handle_editor_mouse(mouse_event, ui_out),
        MouseTarget::BottomPanel => workbench.handle_bottom_panel_mouse(mouse_event),
        MouseTarget::ContextMenu
        | MouseTarget::ThemeEditor
        | MouseTarget::CommandPalette
        | MouseTarget::ByFocus => EventResult::Ignored,
    }
}

pub(super) fn handle_input(workbench: &mut Workbench, event: &InputEvent) -> EventResult {
    // Intercept mouse events within the hover popup before any other processing.
    // This prevents record_user_input from clearing the hover state.
    if let InputEvent::Mouse(me) = event {
        if workbench.store.state().ui.hover_message.is_some() {
            if let Some(area) = workbench.hover_popup.last_area {
                if util::rect_contains(area, me.column, me.row) {
                    match me.kind {
                        MouseEventKind::ScrollUp => {
                            let step =
                                workbench.store.state().editor.config.scroll_step().max(1) as isize;
                            let _ = workbench.scroll_hover_popup_by(-step);
                            return EventResult::Consumed;
                        }
                        MouseEventKind::ScrollDown => {
                            let step =
                                workbench.store.state().editor.config.scroll_step().max(1) as isize;
                            let _ = workbench.scroll_hover_popup_by(step);
                            return EventResult::Consumed;
                        }
                        MouseEventKind::Moved => {
                            return EventResult::Consumed;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

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
            let plan = plan_mouse_dispatch(workbench, mouse_event);

            if !workbench.store.state().ui.sidebar_visible
                && workbench.ui_runtime.capture().is_some()
            {
                workbench.ui_runtime.reset_pointer_state();
            }

            let ui_out = workbench.ui_runtime.on_input(event, &workbench.ui_tree);

            if plan.target == MouseTarget::ContextMenu {
                let overlay_id = IdPath::root("workbench")
                    .push_str("context_menu")
                    .push_str("overlay")
                    .finish();

                let mut state_changed = false;
                for ev in &ui_out.events {
                    match ev {
                        UiEvent::HoverChanged { to: Some(id), .. } => {
                            if let Some(node) = workbench.ui_tree.node(*id) {
                                if let NodeKind::MenuItem { index, .. } = node.kind {
                                    state_changed |= workbench.dispatch_kernel(
                                        crate::kernel::Action::ContextMenuSetSelected { index },
                                    );
                                }
                            }
                        }
                        UiEvent::Click { id, .. } => {
                            if let Some(node) = workbench.ui_tree.node(*id) {
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
                                        if *id == overlay_id {
                                            state_changed |= workbench.dispatch_kernel(
                                                crate::kernel::Action::ContextMenuClose,
                                            );
                                        }
                                    }
                                }
                            } else if *id == overlay_id {
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

                if state_changed || ui_out.needs_redraw {
                    return EventResult::Consumed;
                }

                // Modal overlay: swallow mouse events even when it doesn't redraw.
                return EventResult::Ignored;
            }

            if plan.target == MouseTarget::ThemeEditor {
                return workbench.handle_theme_editor_mouse(mouse_event);
            }

            if plan.target == MouseTarget::CommandPalette {
                return EventResult::Ignored;
            }

            let mut state_changed = false;
            if let Some(focus_plan) = plan.focus_plan {
                state_changed = apply_focus_plan(workbench, mouse_event, focus_plan);
            }

            let dispatch_target = if plan.target == MouseTarget::ByFocus {
                mouse_target_from_focus(
                    workbench.store.state().ui.focus,
                    workbench.store.state().ui.sidebar_tab,
                )
            } else {
                plan.target
            };

            let result = dispatch_by_target(workbench, dispatch_target, mouse_event, &ui_out);

            if (state_changed || ui_out.needs_redraw) && matches!(result, EventResult::Ignored) {
                return EventResult::Consumed;
            }

            result
        }
        InputEvent::Resize(_, _) => EventResult::Consumed,
        _ => EventResult::Ignored,
    }
}
