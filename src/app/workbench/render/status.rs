use super::super::Workbench;
use crate::kernel::editor::DiskState;
use crate::kernel::{FocusTarget, SidebarTab};
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style as UiStyle;

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
            FocusTarget::ThemeEditor => "Theme",
        }
    }

    pub(super) fn paint_status(&self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let (mode, cursor_info) = if let Some(pane) = self
            .store
            .state()
            .editor
            .pane(self.store.state().ui.editor_layout.active_pane)
        {
            if let Some(tab) = pane.active_tab() {
                let (row, col) = tab.buffer.cursor();
                let dirty = if tab.dirty { " [+]" } else { "" };
                let disk_indicator = match &tab.disk_state {
                    DiskState::ConflictExternalModified => " [CONFLICT]",
                    DiskState::MissingOnDisk => " [DELETED]",
                    DiskState::ReloadedFromDisk { .. } => " [RELOADED]",
                    DiskState::InSync => "",
                };
                let file_name = tab
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| tab.title.clone());

                (
                    format!("{}{}{}", file_name, dirty, disk_indicator),
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
        let style = UiStyle::default()
            .bg(self.ui_theme.statusbar_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(area, style);
        painter.text_clipped(Pos::new(area.x, area.y), text, style, area);
    }
}
