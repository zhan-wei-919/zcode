//! 文件浏览器视图（纯渲染 + 命中测试）

use crate::app::theme::UiTheme;
use crate::core::event::MouseEvent;
use crate::core::text_window;
use crate::kernel::GitFileStatusKind;
use crate::models::{FileTreeRow, NodeId};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use rustc_hash::FxHashMap;
use unicode_width::UnicodeWidthStr;

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

    fn render_row(
        &self,
        row: &FileTreeRow,
        is_selected: bool,
        git_status: Option<GitFileStatusKind>,
        width: u16,
        theme: &UiTheme,
    ) -> Line<'static> {
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

        let row_style = if is_selected {
            Style::default()
                .bg(theme.palette_selected_bg)
                .fg(theme.palette_selected_fg)
        } else if row.is_dir {
            Style::default().fg(theme.accent_fg)
        } else {
            Style::default().fg(theme.palette_fg)
        };

        let status_char = git_status.map(|s| s.marker()).unwrap_or(' ');
        let status_style = match git_status {
            Some(GitFileStatusKind::Modified) => row_style.fg(theme.header_fg),
            Some(GitFileStatusKind::Added) => row_style.fg(Color::Green),
            Some(GitFileStatusKind::Untracked) => row_style.fg(theme.palette_muted_fg),
            Some(GitFileStatusKind::Conflict) => row_style.fg(theme.error_fg),
            None => row_style,
        };

        let width = width as usize;
        if width == 0 {
            return Line::default();
        }

        let trailing_width = if width >= 2 { 2 } else { 1 };
        let left_target_width = width.saturating_sub(trailing_width);

        let mut left = format!("{indent}{icon}{name}");
        if left_target_width == 0 {
            left.clear();
        } else {
            let end = text_window::truncate_to_width(&left, left_target_width);
            left.truncate(end);
        }
        let left_width = left.width();
        let pad = " ".repeat(left_target_width.saturating_sub(left_width));

        if trailing_width == 1 {
            return Line::from(vec![
                Span::styled(left, row_style),
                Span::styled(pad, row_style),
                Span::styled(status_char.to_string(), status_style),
            ]);
        }

        Line::from(vec![
            Span::styled(left, row_style),
            Span::styled(pad, row_style),
            Span::styled(" ", row_style),
            Span::styled(status_char.to_string(), status_style),
        ])
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        rows: &[FileTreeRow],
        selected_id: Option<NodeId>,
        scroll_offset: usize,
        git_status_by_id: &FxHashMap<NodeId, GitFileStatusKind>,
        theme: &UiTheme,
    ) {
        self.area = Some(area);

        if rows.is_empty() {
            let style = Style::default()
                .bg(theme.palette_bg)
                .fg(theme.palette_muted_fg);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled("Empty folder", style))),
                area,
            );
            return;
        }

        let visible_height = area.height as usize;
        let visible_end = (scroll_offset + visible_height).min(rows.len());

        let lines: Vec<Line> = rows[scroll_offset..visible_end]
            .iter()
            .map(|row| {
                let is_selected = selected_id == Some(row.id);
                let git_status = git_status_by_id.get(&row.id).copied();
                self.render_row(row, is_selected, git_status, area.width, theme)
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), area);
    }
}

impl Default for ExplorerView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/views/explorer/explorer_view.rs"]
mod tests;
