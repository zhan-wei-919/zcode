use super::Workbench;
use crate::core::event::InputEvent;
use crate::core::view::EventResult;
use crate::kernel::{FocusTarget, SidebarTab};

pub(super) fn handle_input(workbench: &mut Workbench, event: &InputEvent) -> EventResult {
    match event {
        InputEvent::Key(key_event) => workbench.handle_key_event(key_event),
        InputEvent::Paste(text) => workbench.handle_paste(text),
        InputEvent::Mouse(mouse_event) => {
            if let Some(result) = workbench.handle_editor_split_mouse(mouse_event) {
                return result;
            }

            let state_changed = workbench.handle_mouse_area(mouse_event);
            let focus = workbench.store.state().ui.focus;

            let result = match focus {
                FocusTarget::Explorer => {
                    if workbench.store.state().ui.sidebar_tab == SidebarTab::Search {
                        workbench.handle_search_mouse(mouse_event)
                    } else {
                        workbench.handle_explorer_mouse(mouse_event)
                    }
                }
                FocusTarget::Editor => workbench.handle_editor_mouse(mouse_event),
                FocusTarget::BottomPanel => workbench.handle_bottom_panel_mouse(mouse_event),
                FocusTarget::CommandPalette => EventResult::Ignored,
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
