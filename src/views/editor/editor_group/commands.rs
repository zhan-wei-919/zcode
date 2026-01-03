use super::EditorGroup;
use crate::core::Command;

impl EditorGroup {
    pub fn apply_command(&mut self, command: Command) -> bool {
        match command {
            Command::NextTab => {
                let prev = self.active_index;
                self.next_tab();
                self.active_index != prev
            }
            Command::PrevTab => {
                let prev = self.active_index;
                self.prev_tab();
                self.active_index != prev
            }
            Command::CloseTab => self.close_active_tab(),
            Command::Find => {
                if self.search_bar.is_visible() {
                    self.hide_search();
                } else {
                    self.show_search();
                }
                true
            }
            Command::Replace => {
                self.show_replace();
                true
            }
            Command::FindNext => {
                if self.search_bar.is_visible() {
                    self.find_next();
                    true
                } else {
                    false
                }
            }
            Command::FindPrev => {
                if self.search_bar.is_visible() {
                    self.find_prev();
                    true
                } else {
                    false
                }
            }
            cmd => self
                .active_editor_mut()
                .is_some_and(|editor| editor.apply_command(cmd)),
        }
    }

    pub fn tick(&mut self) -> bool {
        for tab in &mut self.tabs {
            tab.editor.tick();
        }
        self.search_bar.poll()
    }

    pub fn set_runtime(&mut self, runtime: tokio::runtime::Handle) {
        self.search_bar.set_runtime(runtime);
    }
}
