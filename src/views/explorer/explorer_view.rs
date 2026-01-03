//! 文件浏览器视图（纯渲染 + 命中测试）

use crate::models::{FileTreeRow, NodeId};
use crate::app::theme::UiTheme;
use crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ExplorerView {
    area: Option<Rect>,
}

impl ExplorerView {
    pub fn new() -> Self {
        Self { area: None }
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.area
            .map(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
            .unwrap_or(false)
    }

    pub fn view_height(&self) -> Option<usize> {
        let area = self.area?;
        Some(area.height as usize)
    }

    pub fn hit_test_row(&self, event: &MouseEvent, scroll_offset: usize) -> Option<usize> {
        let area = self.area?;
        if event.column < area.x || event.column >= area.x + area.width {
            return None;
        }
        if event.row < area.y || event.row >= area.y + area.height {
            return None;
        }

        Some((event.row - area.y) as usize + scroll_offset)
    }

    fn render_row(&self, row: &FileTreeRow, is_selected: bool, theme: &UiTheme) -> Line<'static> {
        let indent = "  ".repeat(row.depth as usize);
        let icon = if row.is_dir {
            if row.is_expanded {
                "▼ "
            } else {
                "▶ "
            }
        } else {
            "  "
        };

        let name = row.name.to_string_lossy().to_string();
        let text = format!("{}{}{}", indent, icon, name);

        let style = if is_selected {
            Style::default()
                .bg(theme.palette_selected_bg)
                .fg(theme.palette_selected_fg)
        } else if row.is_dir {
            Style::default().fg(theme.accent_fg)
        } else {
            Style::default().fg(theme.palette_fg)
        };

        Line::from(Span::styled(text, style))
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        rows: &[FileTreeRow],
        selected_id: Option<NodeId>,
        scroll_offset: usize,
        theme: &UiTheme,
    ) {
        self.area = Some(area);

        let visible_height = area.height as usize;
        let visible_end = (scroll_offset + visible_height).min(rows.len());

        let lines: Vec<Line> = rows[scroll_offset..visible_end]
            .iter()
            .map(|row| {
                let is_selected = selected_id == Some(row.id);
                self.render_row(row, is_selected, theme)
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explorer_view_new() {
        let view = ExplorerView::new();
        assert!(view.area.is_none());
    }
}
