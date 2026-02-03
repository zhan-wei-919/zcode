use super::super::Workbench;
use crate::core::Command;
use crate::kernel::FocusTarget;
use std::time::Instant;

impl Workbench {
    pub(super) fn maybe_schedule_completion_debounce(&mut self, cmd: &Command) {
        if self.store.state().ui.focus != FocusTarget::Editor {
            return;
        }

        if self
            .kernel_services
            .get::<crate::kernel::services::adapters::LspService>()
            .is_none()
        {
            return;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(tab) = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane| pane.active_tab())
        else {
            return;
        };

        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            return;
        }

        let should_schedule = match cmd {
            Command::InsertChar(ch) => completion_debounce_triggered_by_inserted_char(*ch),
            Command::DeleteBackward | Command::DeleteForward => true,
            _ => false,
        };

        if !should_schedule {
            return;
        }

        if !completion_debounce_context_allowed(tab) {
            return;
        }

        self.pending_completion_deadline =
            Some(Instant::now() + super::super::COMPLETION_DEBOUNCE_DELAY);
    }
}

fn completion_debounce_triggered_by_inserted_char(inserted: char) -> bool {
    inserted.is_alphanumeric() || inserted == '_'
}

fn completion_debounce_context_allowed(tab: &crate::kernel::editor::EditorTabState) -> bool {
    if tab.is_in_string_or_comment_at_cursor() {
        return false;
    }

    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let end_char = cursor_char_offset.min(rope.len_chars());

    let mut start_char = end_char;
    while start_char > 0 {
        let ch = rope.char(start_char - 1);
        if ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch) {
            start_char = start_char.saturating_sub(1);
        } else {
            break;
        }
    }

    if start_char != end_char {
        let first = rope.char(start_char);
        if first == '_' || unicode_xid::UnicodeXID::is_xid_start(first) {
            return true;
        }
    }

    if start_char > 0 && rope.char(start_char - 1) == '.' {
        return true;
    }
    if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
        return true;
    }

    false
}
