use super::super::Workbench;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub(super) fn cursor_position_terminal(workbench: &Workbench) -> Option<(u16, u16)> {
    if !workbench.terminal_cursor_visible {
        return None;
    }

    let panel = workbench.last_bottom_panel_area?;
    if panel.width == 0 || panel.height <= 1 {
        return None;
    }

    let content = Rect::new(
        panel.x,
        panel.y.saturating_add(1),
        panel.width,
        panel.height.saturating_sub(1),
    );
    if content.width == 0 || content.height == 0 {
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
        let max_x = content.x.saturating_add(content.width.saturating_sub(1));
        let max_y = content.y.saturating_add(content.height.saturating_sub(1));
        let x = content.x.saturating_add(col).min(max_x);
        let y = content.y.saturating_add(row).min(max_y);
        return Some((x, y));
    }

    #[cfg(not(feature = "terminal"))]
    {
        None
    }
}

impl Workbench {
    pub(super) fn render_bottom_panel_terminal(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);

        let Some(session_id) = self.store.state().terminal.active_session().map(|s| s.id) else {
            let msg = Line::from(Span::styled(
                "Terminal starting...",
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg).style(base_style), area);
            return;
        };
        self.sync_terminal_view_size(session_id, area.width, area.height);

        #[cfg(feature = "terminal")]
        {
            let Some(session) = self.store.state().terminal.active_session() else {
                return;
            };
            let rows = session.visible_rows(area.width, area.height);
            let lines = rows.into_iter().map(Line::from).collect::<Vec<_>>();
            frame.render_widget(Paragraph::new(lines).style(base_style), area);
        }

        #[cfg(not(feature = "terminal"))]
        {
            let msg = Line::from(Span::styled(
                "Terminal disabled",
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg).style(base_style), area);
        }
    }
}
