use super::util;
use super::Workbench;
use crate::core::text_window;
use crate::kernel::palette::match_items;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub(super) fn render(workbench: &Workbench, frame: &mut Frame, area: Rect) {
    let popup_area = util::centered_rect(90, 10, area);
    if popup_area.width == 0 || popup_area.height == 0 {
        return;
    }

    frame.render_widget(Clear, popup_area);

    let base_style = Style::default()
        .bg(workbench.theme.palette_bg)
        .fg(workbench.theme.palette_fg);
    let muted_style = Style::default().fg(workbench.theme.palette_muted_fg);
    let selected_style = Style::default()
        .bg(workbench.theme.palette_selected_bg)
        .fg(workbench.theme.palette_selected_fg);
    let title_style = Style::default()
        .fg(workbench.theme.header_fg)
        .add_modifier(Modifier::BOLD);

    frame.render_widget(Block::default().style(base_style), popup_area);

    let inner = Rect::new(
        popup_area.x.saturating_add(1),
        popup_area.y.saturating_add(1),
        popup_area.width.saturating_sub(2),
        popup_area.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let query = &workbench.store.state().ui.command_palette.query;
    let matches = match_items(query);
    let selected = workbench
        .store
        .state()
        .ui
        .command_palette
        .selected
        .min(matches.len().saturating_sub(1));

    let prefix = "> ";
    let prefix_w = prefix.width() as u16;
    let (q_start, q_end) = text_window::window(
        query,
        query.len(),
        inner.width.saturating_sub(prefix_w) as usize,
    );
    let visible_query = query.get(q_start..q_end).unwrap_or_default();

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("Command Palette", title_style)));
    lines.push(Line::from(vec![
        Span::styled(prefix, base_style),
        Span::styled(visible_query, base_style),
    ]));
    lines.push(Line::from(Span::raw("")));

    let max_items = inner.height.saturating_sub(3) as usize;
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

    frame.render_widget(Paragraph::new(lines).style(base_style), inner);
}

pub(super) fn cursor(workbench: &Workbench) -> Option<(u16, u16)> {
    let area = workbench.last_render_area?;
    let popup_area = util::centered_rect(90, 10, area);

    let query = &workbench.store.state().ui.command_palette.query;
    if popup_area.width < 3 || popup_area.height < 3 {
        return None;
    }

    let inner = Rect::new(
        popup_area.x.saturating_add(1),
        popup_area.y.saturating_add(1),
        popup_area.width.saturating_sub(2),
        popup_area.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height < 2 {
        return None;
    }

    let prefix = "> ";
    let prefix_w = prefix.width() as u16;
    let cursor = query.len();
    let (start, _end) =
        text_window::window(query, cursor, inner.width.saturating_sub(prefix_w) as usize);
    let before = query.get(start..cursor).unwrap_or_default();

    let x = inner
        .x
        .saturating_add(prefix_w)
        .saturating_add(before.width() as u16)
        .min(inner.x + inner.width.saturating_sub(1));
    let y = inner.y.saturating_add(1);

    Some((x, y))
}
