use super::super::Workbench;
use crate::core::text_window;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub(super) fn render_confirm_dialog(workbench: &Workbench, frame: &mut Frame, area: Rect) {
    use ratatui::widgets::Clear;

    let dialog = &workbench.store.state().ui.confirm_dialog;
    if !dialog.visible {
        return;
    }

    let width = 50.min(area.width.saturating_sub(4));
    let height = 5.min(area.height.saturating_sub(2));
    if width < 20 || height < 3 {
        return;
    }

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let base_style = Style::default()
        .bg(workbench.theme.palette_bg)
        .fg(workbench.theme.palette_fg);
    frame.render_widget(Block::default().style(base_style), dialog_area);

    let inner = Rect::new(
        dialog_area.x.saturating_add(1),
        dialog_area.y.saturating_add(1),
        dialog_area.width.saturating_sub(2),
        dialog_area.height.saturating_sub(2),
    );
    if inner.height < 2 || inner.width < 10 {
        return;
    }

    let title_line = Line::from(Span::styled(
        "Confirm",
        Style::default()
            .fg(workbench.theme.header_fg)
            .add_modifier(Modifier::BOLD),
    ));
    let msg_line = Line::from(dialog.message.as_str());
    let hint_line = Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(workbench.theme.accent_fg)),
        Span::raw(" Close  "),
        Span::styled(
            "[Esc]",
            Style::default().fg(workbench.theme.palette_muted_fg),
        ),
        Span::raw(" Cancel"),
    ]);

    let content = Paragraph::new(vec![title_line, msg_line, Line::raw(""), hint_line])
        .style(base_style)
        .wrap(Wrap { trim: true });
    frame.render_widget(content, inner);
}

pub(super) fn render_explorer_context_menu(
    workbench: &mut Workbench,
    frame: &mut Frame,
    area: Rect,
) {
    use ratatui::widgets::Clear;

    let menu = &workbench.store.state().ui.explorer_context_menu;
    if !menu.visible {
        workbench.last_explorer_context_menu_area = None;
        return;
    }

    let items = &menu.items;
    if items.is_empty() || area.width == 0 || area.height == 0 {
        workbench.last_explorer_context_menu_area = None;
        return;
    }

    if area.width < 3 || area.height < 3 {
        workbench.last_explorer_context_menu_area = None;
        return;
    }

    let mut max_label_w = 0usize;
    for item in items {
        max_label_w = max_label_w.max(item.label().width());
    }

    let desired_inner_width = (max_label_w.saturating_add(4)).min(u16::MAX as usize) as u16;
    let desired_inner_height = (items.len().min(u16::MAX as usize)) as u16;
    let width = desired_inner_width.saturating_add(2).min(area.width).max(3);
    let height = desired_inner_height
        .saturating_add(2)
        .min(area.height)
        .max(3);

    let right = area.x.saturating_add(area.width);
    let bottom = area.y.saturating_add(area.height);

    let mut x = menu.anchor.0.max(area.x);
    let mut y = menu.anchor.1.max(area.y);
    if x.saturating_add(width) > right {
        x = right.saturating_sub(width);
    }
    if y.saturating_add(height) > bottom {
        y = bottom.saturating_sub(height);
    }

    let popup_area = Rect::new(x, y, width, height);
    workbench.last_explorer_context_menu_area = Some(popup_area);

    frame.render_widget(Clear, popup_area);

    let base_style = Style::default()
        .bg(workbench.theme.palette_bg)
        .fg(workbench.theme.palette_fg);
    let border_style = Style::default()
        .fg(workbench.theme.focus_border)
        .bg(workbench.theme.palette_bg);
    let selected_style = Style::default()
        .bg(workbench.theme.palette_selected_bg)
        .fg(workbench.theme.palette_selected_fg);

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(base_style),
        popup_area,
    );

    let inner = Rect::new(
        popup_area.x.saturating_add(1),
        popup_area.y.saturating_add(1),
        popup_area.width.saturating_sub(2),
        popup_area.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let selected = menu.selected.min(items.len().saturating_sub(1));
    let mut lines = Vec::new();
    for (idx, item) in items.iter().enumerate().take(inner.height as usize) {
        let is_selected = idx == selected;
        let style = if is_selected {
            selected_style
        } else {
            base_style
        };
        let prefix = if is_selected { "▸ " } else { "  " };
        let mut text = format!("{prefix}{}", item.label());
        let pad_to = inner.width as usize;
        let current_w = text.width();
        if current_w < pad_to {
            text.push_str(&" ".repeat(pad_to - current_w));
        }
        lines.push(Line::from(Span::styled(text, style)));
    }

    frame.render_widget(Paragraph::new(lines).style(base_style), inner);
}

fn input_dialog_area(area: Rect) -> Rect {
    super::super::util::centered_rect(60, 7, area)
}

pub(super) fn render_input_dialog(workbench: &Workbench, frame: &mut Frame, area: Rect) {
    use ratatui::widgets::Clear;

    let dialog = &workbench.store.state().ui.input_dialog;
    if !dialog.visible {
        return;
    }

    let popup_area = input_dialog_area(area);
    if popup_area.width < 20 || popup_area.height < 5 {
        return;
    }

    frame.render_widget(Clear, popup_area);

    let base_style = Style::default()
        .bg(workbench.theme.palette_bg)
        .fg(workbench.theme.palette_fg);
    let muted_style = Style::default().fg(workbench.theme.palette_muted_fg);

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

    let title = if dialog.title.is_empty() {
        "Input"
    } else {
        dialog.title.as_str()
    };
    let title_style = Style::default()
        .fg(workbench.theme.header_fg)
        .add_modifier(Modifier::BOLD);

    let prefix = "> ";
    let prefix_w = prefix.width() as u16;
    let cursor = dialog.cursor.min(dialog.value.len());
    let (v_start, v_end) = text_window::window(
        dialog.value.as_str(),
        cursor,
        inner.width.saturating_sub(prefix_w) as usize,
    );
    let visible_value = dialog.value.get(v_start..v_end).unwrap_or_default();

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(title, title_style)));
    lines.push(Line::from(vec![
        Span::styled("> ", base_style),
        Span::styled(visible_value, base_style),
    ]));

    if let Some(err) = dialog.error.as_deref() {
        lines.push(Line::from(Span::styled(
            err,
            Style::default().fg(workbench.theme.error_fg),
        )));
    } else {
        lines.push(Line::from(Span::raw("")));
    }

    lines.push(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(workbench.theme.accent_fg)),
        Span::raw(" Create  "),
        Span::styled("[Esc]", muted_style),
        Span::raw(" Cancel"),
    ]));

    frame.render_widget(Paragraph::new(lines).style(base_style), inner);
}

pub(super) fn input_dialog_cursor(workbench: &Workbench) -> Option<(u16, u16)> {
    let area = workbench.last_render_area?;
    let dialog = &workbench.store.state().ui.input_dialog;
    if !dialog.visible {
        return None;
    }

    let popup_area = input_dialog_area(area);
    if popup_area.width < 4 || popup_area.height < 3 {
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

    let cursor = dialog.cursor.min(dialog.value.len());
    let prefix_w = "> ".width() as u16;
    let (start, _end) = text_window::window(
        dialog.value.as_str(),
        cursor,
        inner.width.saturating_sub(prefix_w) as usize,
    );
    let before = dialog.value.get(start..cursor).unwrap_or_default();
    let before_w = before.width() as u16;

    let x = inner
        .x
        .saturating_add(prefix_w)
        .saturating_add(before_w)
        .min(inner.x + inner.width.saturating_sub(1));
    // Title line is at inner.y, input line is at inner.y + 1.
    let y = inner.y.saturating_add(1);

    Some((x, y))
}
