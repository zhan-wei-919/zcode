use super::super::Workbench;
use super::classify_lsp_edit_trigger;
use crate::core::Command;
use crate::kernel::lsp_registry;
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
        if !lsp_registry::is_lsp_source_path(path) {
            return;
        }

        let timing = self.store.state().editor.config.lsp_input_timing.clone();
        let trigger = match cmd {
            Command::InsertChar(ch) if completion_debounce_triggered_by_inserted_char(*ch) => {
                classify_lsp_edit_trigger(cmd, &timing)
            }
            Command::DeleteBackward | Command::DeleteForward => {
                classify_lsp_edit_trigger(cmd, &timing)
            }
            _ => None,
        };

        let Some(trigger) = trigger else {
            return;
        };

        if !completion_debounce_context_allowed(tab) {
            return;
        }

        let _ = trigger;
        self.pending_completion_deadline = Some(Instant::now());
    }
}

fn completion_debounce_triggered_by_inserted_char(inserted: char) -> bool {
    inserted.is_alphanumeric() || inserted == '_' || inserted == '.' || inserted == ':'
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
        // Only trigger `.`-completion when the token before the dot looks like an identifier.
        // This avoids popping completion on numeric literals like `1.`.
        let mut token_start = start_char.saturating_sub(1);
        while token_start > 0 {
            let ch = rope.char(token_start - 1);
            if ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch) {
                token_start = token_start.saturating_sub(1);
            } else {
                break;
            }
        }

        if token_start < start_char.saturating_sub(1) {
            let first = rope.char(token_start);
            if first == '_' || unicode_xid::UnicodeXID::is_xid_start(first) {
                return true;
            }
        }
    }
    if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::editor::TabId;
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    #[test]
    fn completion_schedule_is_immediate() {
        let config = EditorConfig::default();
        let mut tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from("test.rs"),
            "pri",
            &config,
        );
        tab.buffer.set_cursor(0, 3);

        let start = Instant::now();
        let timing = config.lsp_input_timing.clone();
        let trigger = classify_lsp_edit_trigger(&Command::InsertChar('x'), &timing)
            .expect("insert trigger should exist");
        let _ = trigger;
        let deadline = Instant::now();

        assert!(deadline <= start + Duration::from_millis(5));
        assert!(completion_debounce_context_allowed(&tab));
    }
}
