use super::super::Workbench;
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Color as UiColor, Mod as UiMod, Style as UiStyle};

#[cfg(feature = "terminal")]
fn map_vt_color(color: vt100::Color) -> Option<UiColor> {
    match color {
        vt100::Color::Default => None,
        vt100::Color::Idx(index) => Some(UiColor::Indexed(index)),
        vt100::Color::Rgb(r, g, b) => Some(UiColor::Rgb(r, g, b)),
    }
}

#[cfg(feature = "terminal")]
fn style_for_terminal_cell(cell: &vt100::Cell) -> UiStyle {
    let mut style = UiStyle::default();
    if let Some(fg) = map_vt_color(cell.fgcolor()) {
        style = style.fg(fg);
    }
    if let Some(bg) = map_vt_color(cell.bgcolor()) {
        style = style.bg(bg);
    }
    if cell.bold() {
        style = style.add_mod(UiMod::BOLD);
    }
    if cell.dim() {
        style = style.add_mod(UiMod::DIM);
    }
    if cell.italic() {
        style = style.add_mod(UiMod::ITALIC);
    }
    if cell.underline() {
        style = style.add_mod(UiMod::UNDERLINE);
    }
    if cell.inverse() {
        style = style.add_mod(UiMod::REVERSE);
    }
    style
}

pub(super) fn cursor_position_terminal(workbench: &Workbench) -> Option<(u16, u16)> {
    if !workbench.terminal_cursor_visible {
        return None;
    }

    let panel = workbench.layout_cache.bottom_panel_area?;
    if panel.w == 0 || panel.h <= 1 {
        return None;
    }

    let content = UiRect::new(
        panel.x,
        panel.y.saturating_add(1),
        panel.w,
        panel.h.saturating_sub(1),
    );
    if content.is_empty() {
        return None;
    }

    let session = workbench.store.state().terminal.active_session()?;
    if session.scroll_offset > 0 {
        return None;
    }

    #[cfg(feature = "terminal")]
    {
        if session.parser.screen().hide_cursor() {
            return None;
        }
        let (row, col) = session.parser.screen().cursor_position();
        let max_x = content.x.saturating_add(content.w.saturating_sub(1));
        let max_y = content.y.saturating_add(content.h.saturating_sub(1));
        let x = content.x.saturating_add(col).min(max_x);
        let y = content.y.saturating_add(row).min(max_y);
        Some((x, y))
    }

    #[cfg(not(feature = "terminal"))]
    {
        None
    }
}

impl Workbench {
    pub(in super::super) fn terminal_selection_text(&self) -> Option<String> {
        #[cfg(not(feature = "terminal"))]
        {
            return None;
        }

        #[cfg(feature = "terminal")]
        {
            let selection = self.terminal_selection?;
            if selection.is_empty() {
                return None;
            }

            let (start, end) = selection.normalized();
            let session = self.store.state().terminal.active_session()?;
            let screen = session.parser.screen();

            let mut lines = Vec::new();
            for row in start.row..=end.row {
                let col_start = if row == start.row { start.col } else { 0 };
                let col_end = if row == end.row {
                    end.col
                } else {
                    session.cols.saturating_sub(1)
                };

                let mut line = String::new();
                for col in col_start..=col_end {
                    let Some(cell) = screen.cell(row, col) else {
                        continue;
                    };
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let contents = cell.contents();
                    if contents.is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(contents);
                    }
                }
                let trimmed = line.trim_end().to_string();
                lines.push(trimmed);
            }

            let text = lines.join("\n");
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
    }

    fn paint_terminal_selection_overlay(&self, painter: &mut Painter, area: UiRect) {
        let Some(selection) = self.terminal_selection else {
            return;
        };
        if selection.is_empty() {
            return;
        }

        let (start, end) = selection.normalized();
        let style = UiStyle::default()
            .bg(self.ui_theme.palette_selected_bg)
            .fg(self.ui_theme.palette_selected_fg);

        for row in start.row..=end.row {
            let y = area.y.saturating_add(row);
            if y >= area.bottom() {
                break;
            }

            let col_start = if row == start.row { start.col } else { 0 };
            let col_end = if row == end.row {
                end.col
            } else {
                area.w.saturating_sub(1)
            };

            if col_start > col_end {
                continue;
            }

            let x = area.x.saturating_add(col_start);
            let width = col_end
                .saturating_sub(col_start)
                .saturating_add(1)
                .min(area.right().saturating_sub(x));
            if width == 0 {
                continue;
            }
            painter.style_rect(UiRect::new(x, y, width, 1), style);
        }
    }

    pub(super) fn paint_bottom_panel_terminal(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let Some(session_id) = self.store.state().terminal.active_session().map(|s| s.id) else {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(
                Pos::new(area.x, area.y),
                "Terminal starting...",
                style,
                area,
            );
            return;
        };
        self.sync_terminal_view_size(session_id, area.w, area.h);

        #[cfg(feature = "terminal")]
        {
            let Some(session) = self.store.state().terminal.active_session() else {
                return;
            };
            let screen = session.parser.screen();
            for row in 0..area.h {
                let y = area.y.saturating_add(row);
                if y >= area.bottom() {
                    break;
                }
                let row_clip = UiRect::new(area.x, y, area.w, 1);
                for col in 0..area.w {
                    let x = area.x.saturating_add(col);
                    let Some(cell) = screen.cell(row, col) else {
                        continue;
                    };
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    let symbol = if cell.contents().is_empty() {
                        " "
                    } else {
                        cell.contents()
                    };
                    painter.text_clipped(
                        Pos::new(x, y),
                        symbol,
                        style_for_terminal_cell(cell),
                        row_clip,
                    );
                }
            }

            self.paint_terminal_selection_overlay(painter, area);
        }

        #[cfg(not(feature = "terminal"))]
        {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "Terminal disabled", style, area);
        }
    }
}
