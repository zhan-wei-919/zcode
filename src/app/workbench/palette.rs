use super::util;
use super::Workbench;
use crate::kernel::palette::match_items;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub(super) fn render(workbench: &Workbench, frame: &mut Frame, area: Rect) {
    let popup_area = util::centered_rect(90, 10, area);

    frame.render_widget(Clear, popup_area);

    let base_style = Style::default()
        .bg(workbench.theme.palette_bg)
        .fg(workbench.theme.palette_fg);
    let muted_style = Style::default().fg(workbench.theme.palette_muted_fg);
    let selected_style = Style::default()
        .bg(workbench.theme.palette_selected_bg)
        .fg(workbench.theme.palette_selected_fg);

    let query = &workbench.store.state().ui.command_palette.query;
    let matches = match_items(query, workbench.store.state().plugins.palette_items());
    let selected = workbench
        .store
        .state()
        .ui
        .command_palette
        .selected
        .min(matches.len().saturating_sub(1));

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("> ", base_style),
        Span::styled(query.as_str(), base_style),
    ]));
    lines.push(Line::from(Span::raw("")));

    let max_items = popup_area.height.saturating_sub(3) as usize;
    if matches.is_empty() {
        lines.push(Line::from(Span::styled("No matches", muted_style)));
    } else {
        for (pos, item) in matches.iter().take(max_items).enumerate() {
            let is_selected = pos == selected;
            let style = if is_selected {
                selected_style
            } else {
                base_style
            };
            let prefix = if is_selected { "▸ " } else { "  " };
            lines.push(Line::from(vec![
                Span::raw(prefix),
                Span::styled(item.label, style),
            ]));
        }
    }

    let widget = Paragraph::new(lines).style(base_style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(workbench.theme.palette_border))
            .title("Command Palette"),
    );

    frame.render_widget(widget, popup_area);
}

pub(super) fn cursor(workbench: &Workbench) -> Option<(u16, u16)> {
    let area = workbench.last_render_area?;
    let popup_area = util::centered_rect(90, 10, area);

    let query = &workbench.store.state().ui.command_palette.query;
    let query_w = query.width() as u16;
    let x = popup_area
        .x
        .saturating_add(1)
        .saturating_add(2)
        .saturating_add(query_w)
        .min(popup_area.x + popup_area.width.saturating_sub(2));
    let y = popup_area.y.saturating_add(1);

    Some((x, y))
}
