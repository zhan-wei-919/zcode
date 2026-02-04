use super::super::Workbench;
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style as UiStyle;

pub(super) fn cursor_position_terminal(workbench: &Workbench) -> Option<(u16, u16)> {
    if !workbench.terminal_cursor_visible {
        return None;
    }

    let panel = workbench.last_bottom_panel_area?;
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
            let rows = session.visible_rows(area.w, area.h);
            for (idx, row) in rows.into_iter().enumerate() {
                let y = area.y.saturating_add(idx.min(u16::MAX as usize) as u16);
                if y >= area.bottom() {
                    break;
                }
                let row_clip = UiRect::new(area.x, y, area.w, 1);
                painter.text_clipped(Pos::new(area.x, y), row, UiStyle::default(), row_clip);
            }
        }

        #[cfg(not(feature = "terminal"))]
        {
            let style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
            painter.text_clipped(Pos::new(area.x, area.y), "Terminal disabled", style, area);
        }
    }
}
