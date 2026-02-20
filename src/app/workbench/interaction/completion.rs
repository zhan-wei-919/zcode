use super::super::Workbench;
use crate::core::Command;
use crate::kernel::lsp_registry;
use crate::kernel::store::strategy_for_tab;
use crate::kernel::{Action as KernelAction, FocusTarget};

impl Workbench {
    pub(super) fn maybe_trigger_completion(&mut self, cmd: &Command) {
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
        let should_schedule = match cmd {
            Command::InsertChar(ch) => strategy.debounce_triggered_by_char(*ch),
            Command::DeleteBackward | Command::DeleteForward => true,
            _ => false,
        };

        if !should_schedule {
            return;
        }

        if !strategy.context_allows_completion(tab) {
            return;
        }

        let _ = self.dispatch_kernel(KernelAction::RunCommand(Command::LspCompletion));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::editor::TabId;
    use crate::kernel::services::ports::EditorConfig;
    use std::path::PathBuf;

    #[test]
    fn completion_schedule_gate_checks_context() {
        let config = EditorConfig::default();
        let mut tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from("test.rs"),
            "// comment",
            &config,
        );
        tab.buffer.set_cursor(0, 2);

        let strategy = strategy_for_tab(&tab);
        assert!(!strategy.context_allows_completion(&tab));
    }
}
