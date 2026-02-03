use super::super::Workbench;
use crate::kernel::{FocusTarget, SidebarTab};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

impl Workbench {
    fn active_label(&self) -> &'static str {
        match self.store.state().ui.focus {
            FocusTarget::Explorer => match self.store.state().ui.sidebar_tab {
                SidebarTab::Explorer => "Explorer",
                SidebarTab::Search => "Search",
            },
            FocusTarget::Editor => "Editor",
            FocusTarget::BottomPanel => "Panel",
            FocusTarget::CommandPalette => "Palette",
        }
    }

    pub(super) fn render_header(&mut self, _frame: &mut Frame, _area: Rect) {}

    pub(super) fn render_status(&self, frame: &mut Frame, area: Rect) {
        let (mode, cursor_info) = if let Some(pane) = self
            .store
            .state()
            .editor
            .pane(self.store.state().ui.editor_layout.active_pane)
        {
            if let Some(tab) = pane.active_tab() {
                let (row, col) = tab.buffer.cursor();
                let dirty = if tab.dirty { " [+]" } else { "" };
                let file_name = tab
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| tab.title.clone());

                (
                    format!("{}{}", file_name, dirty),
                    format!("Ln {}, Col {}", row + 1, col + 1),
                )
            } else {
                ("No file".to_string(), String::new())
            }
        } else {
            ("No file".to_string(), String::new())
        };

        let active = self.active_label();

        let text = format!("{} | {} | {}", mode, cursor_info, active);
        frame.render_widget(Paragraph::new(text), area);
    }
}
