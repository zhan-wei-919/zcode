use super::super::Workbench;
use crate::core::text_window;
use crate::kernel::palette::match_items;
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style as UiStyle;
use unicode_width::UnicodeWidthStr;

const MAX_COMPLETIONS: usize = 8;
const PROMPT: &str = ":";

impl Workbench {
    /// vim 风格 `:` 命令行，占据状态栏那一行；上方浮出命令名补全列表。
    /// 由 F1 / Ctrl+Shift+P 唤起（无模态编辑时 `:` 不能当全局触发键）。
    pub(super) fn paint_command_line(&self, painter: &mut Painter, status_area: UiRect) {
        if status_area.is_empty() {
            return;
        }
        let line = &self.store.state().ui.command_line;

        let base = UiStyle::default()
            .bg(self.theme.core.statusbar_bg)
            .fg(self.theme.core.palette_fg);
        painter.fill_rect(status_area, base);

        let prompt_style = UiStyle::default()
            .bg(self.theme.core.statusbar_bg)
            .fg(self.theme.core.accent_fg);
        painter.text_clipped(
            Pos::new(status_area.x, status_area.y),
            PROMPT,
            prompt_style,
            status_area,
        );

        let prompt_w = PROMPT.width() as u16;
        let avail = status_area.w.saturating_sub(prompt_w) as usize;
        let (start, end) = text_window::window(&line.input, line.cursor, avail);
        let visible = line.input.get(start..end).unwrap_or_default();
        painter.text_clipped(
            Pos::new(status_area.x.saturating_add(prompt_w), status_area.y),
            visible,
            base,
            status_area,
        );

        self.paint_command_line_completions(painter, status_area);
    }

    fn paint_command_line_completions(&self, painter: &mut Painter, status_area: UiRect) {
        let line = &self.store.state().ui.command_line;
        let matches = match_items(&line.input);
        if matches.is_empty() {
            return;
        }

        let total = matches.len();
        let count = total.min(MAX_COMPLETIONS);
        let selected = line.selected.min(total.saturating_sub(1));
        // 保持选中项可见：选中超出窗口时把窗口下移。
        let start = if selected >= count {
            selected - count + 1
        } else {
            0
        };

        let top = status_area.y.saturating_sub(count as u16);
        let base = UiStyle::default()
            .bg(self.theme.core.popup_bg)
            .fg(self.theme.core.palette_fg);
        let selected_style = UiStyle::default()
            .bg(self.theme.core.palette_selected_bg)
            .fg(self.theme.core.palette_selected_fg);

        for (row, item) in matches.iter().skip(start).take(count).enumerate() {
            let y = top.saturating_add(row as u16);
            if y >= status_area.y {
                break;
            }
            let idx = start + row;
            let is_selected = idx == selected;
            let style = if is_selected { selected_style } else { base };
            let row_rect = UiRect::new(status_area.x, y, status_area.w, 1);
            painter.fill_rect(row_rect, style);

            let prefix = if is_selected { "▸ " } else { "  " };
            painter.text_clipped(Pos::new(status_area.x, y), prefix, style, row_rect);

            let prefix_w = prefix.width() as u16;
            let max_w = status_area.w.saturating_sub(prefix_w) as usize;
            let mut label = item.label.to_string();
            if label.width() > max_w {
                let trunc = text_window::truncate_to_width(&label, max_w);
                label.truncate(trunc);
            }
            painter.text_clipped(
                Pos::new(status_area.x.saturating_add(prefix_w), y),
                label,
                style,
                row_rect,
            );
        }
    }

    pub(super) fn command_line_cursor(&self) -> Option<(u16, u16)> {
        if !self.store.state().ui.command_line.active {
            return None;
        }
        let area = self.frame_layout.render_area?;
        if area.h == 0 {
            return None;
        }
        let status_y = area.bottom().saturating_sub(super::super::STATUS_HEIGHT);
        let line = &self.store.state().ui.command_line;

        let prompt_w = PROMPT.width() as u16;
        let avail = area.w.saturating_sub(prompt_w) as usize;
        let (start, _end) = text_window::window(&line.input, line.cursor, avail);
        let before = line.input.get(start..line.cursor).unwrap_or_default();
        let x = area
            .x
            .saturating_add(prompt_w)
            .saturating_add(before.width() as u16)
            .min(area.x + area.w.saturating_sub(1));
        Some((x, status_y))
    }
}
