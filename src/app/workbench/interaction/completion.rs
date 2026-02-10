use super::super::Workbench;
use super::classify_lsp_edit_trigger;
use crate::core::Command;
use crate::kernel::lsp_registry;
use crate::kernel::store::strategy_for_tab;
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

        let strategy = strategy_for_tab(tab);
        let timing = self.store.state().editor.config.lsp_input_timing.clone();
        let trigger = match cmd {
            Command::InsertChar(ch) if strategy.debounce_triggered_by_char(*ch) => {
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

        if !strategy.context_allows_completion(tab) {
            return;
        }

        let _ = trigger;
        self.lsp_debounce.completion = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use crate::app::workbench::interaction::classify_lsp_edit_trigger;
    use crate::core::Command;
    use crate::kernel::editor::TabId;
    use crate::kernel::services::ports::EditorConfig;
    use crate::kernel::store::strategy_for_tab;
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
        let strategy = strategy_for_tab(&tab);
        assert!(strategy.context_allows_completion(&tab));
    }
}
