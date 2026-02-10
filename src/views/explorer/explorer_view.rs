//! 文件浏览器视图（纯渲染 + 命中测试）

use crate::core::text_window;
use crate::kernel::{GitFileStatus, GitFileStatusKind};
use crate::models::{FileTreeRow, NodeId};
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style;
use crate::ui::core::theme::Theme;
use rustc_hash::FxHashMap;
use unicode_width::UnicodeWidthStr;

pub struct ExplorerView {
    area: Option<Rect>,
}

pub struct ExplorerPaintCtx<'a> {
    pub area: Rect,
    pub rows: &'a [FileTreeRow],
    pub selected_id: Option<NodeId>,
    pub scroll_offset: usize,
    pub git_status_by_id: &'a FxHashMap<NodeId, GitFileStatus>,
    pub theme: &'a Theme,
}

impl ExplorerView {
    pub fn new() -> Self {
        Self { area: None }
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.area.is_some_and(|a| a.contains(Pos::new(x, y)))
    }

    pub fn view_height(&self) -> Option<usize> {
        let area = self.area?;
        Some(area.h as usize)
    }

    fn render_row_parts(
        &self,
        row: &FileTreeRow,
        is_selected: bool,
        git_status: Option<GitFileStatus>,
        width: u16,
        theme: &Theme,
    ) -> (String, char, Style, Style) {
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

        let width = width as usize;
        if width == 0 {
            return (String::new(), ' ', row_style, row_style);
        }

        let trailing_width = 1usize;
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

        let kind = git_status.and_then(|s| s.primary_kind());
        let marker = kind.map(|k| k.marker()).unwrap_or(' ');
        let marker_style = match kind {
            Some(GitFileStatusKind::Conflict) => row_style.fg(theme.error_fg),
            Some(GitFileStatusKind::Untracked) => row_style.fg(theme.palette_muted_fg),
            Some(GitFileStatusKind::Added) => row_style.fg(theme.syntax_string_fg),
            Some(GitFileStatusKind::Modified) => row_style.fg(theme.header_fg),
            None => row_style,
        };

        let left_pad = format!("{left}{pad}");
        (left_pad, marker, row_style, marker_style)
    }

    pub fn paint(&mut self, painter: &mut Painter, ctx: ExplorerPaintCtx<'_>) {
        let ExplorerPaintCtx {
            area,
            rows,
            selected_id,
            scroll_offset,
            git_status_by_id,
            theme,
        } = ctx;
        self.area = Some(area);

        if area.is_empty() {
            return;
        }

        let bg_style = Style::default().bg(theme.sidebar_bg);
        painter.fill_rect(area, bg_style);

        if rows.is_empty() {
            let style = Style::default()
                .bg(theme.sidebar_bg)
                .fg(theme.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "Empty folder", style, area);
            return;
        }

        let visible_height = area.h as usize;
        let visible_end = (scroll_offset + visible_height).min(rows.len());
        let marker_x = area.x.saturating_add(area.w.saturating_sub(1));
        for (idx, row) in rows[scroll_offset..visible_end].iter().enumerate() {
            let y = area.y.saturating_add((idx.min(u16::MAX as usize)) as u16);
            if y >= area.bottom() {
                break;
            }

            let is_selected = selected_id == Some(row.id);
            let git_status = git_status_by_id.get(&row.id).copied();
            let (left_pad, marker, row_style, marker_style) =
                self.render_row_parts(row, is_selected, git_status, area.w, theme);

            let row_clip = Rect::new(area.x, y, area.w, 1);
            painter.text_clipped(Pos::new(area.x, y), left_pad, row_style, row_clip);
            painter.text_clipped(
                Pos::new(marker_x, y),
                marker.to_string(),
                marker_style,
                row_clip,
            );
        }
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
