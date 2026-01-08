use crate::app::theme::UiTheme;
use crate::kernel::editor::{
    EditorPaneState, HighlightKind, HighlightSpan, SearchBarField, SearchBarMode, SearchBarState,
};
use crate::kernel::services::ports::EditorConfig;
use crate::models::slice_to_cow;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::layout::EditorPaneLayout;

pub fn render_editor_pane(
    frame: &mut Frame,
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
    theme: &UiTheme,
    hovered_tab: Option<usize>,
) {
    if layout.area.width == 0 || layout.area.height == 0 {
        return;
    }

    render_tabs(frame, layout.tab_area, pane, theme, hovered_tab);

    if let Some(search_area) = layout.search_area {
        render_search_bar(frame, search_area, &pane.search_bar, theme);
    }

    render_editor(frame, layout, pane, config, theme);
}

pub fn cursor_position_editor(
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
) -> Option<(u16, u16)> {
    if pane.search_bar.visible {
        let area = layout.search_area?;
        return cursor_position_search_bar(area, &pane.search_bar);
    }

    let tab = pane.active_tab()?;
    let (row, _col) = tab.buffer.cursor();
    let offset = tab.viewport.line_offset;

    if layout.content_area.width == 0 || layout.content_area.height == 0 {
        return None;
    }

    if row < offset || row >= offset + (layout.editor_area.height as usize).max(1) {
        return None;
    }

    let cursor_x_abs = cursor_display_x_abs(&tab.buffer, config.tab_size);
    let cursor_x_rel = cursor_x_abs.saturating_sub(tab.viewport.horiz_offset) as u16;

    let x = layout.content_area.x.saturating_add(cursor_x_rel);
    let y = layout.content_area.y.saturating_add((row - offset) as u16);

    Some((x, y))
}

fn render_tabs(
    frame: &mut Frame,
    area: Rect,
    pane: &EditorPaneState,
    theme: &UiTheme,
    hovered_tab: Option<usize>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let titles: Vec<Line> = pane
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let active = i == pane.active;
            let is_hovered = hovered_tab == Some(i);
            let fg = if active {
                theme.sidebar_tab_active_fg
            } else {
                theme.sidebar_tab_inactive_fg
            };
            let mut spans = Vec::with_capacity(5);
            spans.push(Span::raw(" "));
            if tab.dirty {
                spans.push(Span::styled("● ", Style::default().fg(fg)));
            }
            spans.push(Span::styled(
                tab.title.as_str(),
                Style::default().fg(fg).add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ));
            spans.push(Span::raw(" "));
            if is_hovered {
                spans.push(Span::styled("×", Style::default().fg(theme.accent_fg)));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect();

    let tabs_widget = Tabs::new(titles)
        .select(pane.active)
        .highlight_style(Style::default().bg(theme.sidebar_tab_active_bg))
        .padding("", "");

    frame.render_widget(tabs_widget, area);
}

fn render_search_bar(frame: &mut Frame, area: Rect, state: &SearchBarState, theme: &UiTheme) {
    if !state.visible || area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(Clear, area);

    let match_info = if state.searching {
        "Searching...".to_string()
    } else if let Some(err) = state.last_error.as_deref() {
        format!("Error: {}", err)
    } else if state.matches.is_empty() {
        if state.search_text.is_empty() {
            String::new()
        } else {
            "No results".to_string()
        }
    } else {
        let current = state.current_match_index.map(|i| i + 1).unwrap_or(0);
        format!("{}/{}", current, state.matches.len())
    };

    let case_indicator = if state.case_sensitive { "[Aa]" } else { "[aa]" };
    let regex_indicator = if state.use_regex { "[.*]" } else { "[  ]" };

    match state.mode {
        SearchBarMode::Search => {
            let search_style = if state.focused_field == SearchBarField::Search {
                Style::default().fg(theme.palette_fg)
            } else {
                Style::default().fg(theme.palette_muted_fg)
            };

            let line = Line::from(vec![
                Span::styled("Find: ", Style::default().fg(theme.header_fg)),
                Span::styled(state.search_text.as_str(), search_style),
                Span::raw(" "),
                Span::styled(case_indicator, Style::default().fg(theme.palette_muted_fg)),
                Span::styled(regex_indicator, Style::default().fg(theme.palette_muted_fg)),
                Span::raw(" "),
                Span::styled(match_info, Style::default().fg(theme.header_fg)),
            ]);

            let widget = Paragraph::new(line).block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.separator)),
            );
            frame.render_widget(widget, area);
        }
        SearchBarMode::Replace => {
            let top = Rect::new(area.x, area.y, area.width, 1.min(area.height));
            let bottom = Rect::new(
                area.x,
                area.y.saturating_add(1),
                area.width,
                area.height.saturating_sub(1),
            );

            let search_style = if state.focused_field == SearchBarField::Search {
                Style::default().fg(theme.palette_fg)
            } else {
                Style::default().fg(theme.palette_muted_fg)
            };

            let search_line = Line::from(vec![
                Span::styled("Find: ", Style::default().fg(theme.header_fg)),
                Span::styled(state.search_text.as_str(), search_style),
                Span::raw(" "),
                Span::styled(case_indicator, Style::default().fg(theme.palette_muted_fg)),
                Span::styled(regex_indicator, Style::default().fg(theme.palette_muted_fg)),
                Span::raw(" "),
                Span::styled(match_info, Style::default().fg(theme.header_fg)),
            ]);
            frame.render_widget(Paragraph::new(search_line), top);

            let replace_style = if state.focused_field == SearchBarField::Replace {
                Style::default().fg(theme.palette_fg)
            } else {
                Style::default().fg(theme.palette_muted_fg)
            };

            let replace_line = Line::from(vec![
                Span::styled("Replace: ", Style::default().fg(theme.header_fg)),
                Span::styled(state.replace_text.as_str(), replace_style),
            ]);

            let widget = Paragraph::new(replace_line).block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.separator)),
            );
            frame.render_widget(widget, bottom);
        }
    }
}

fn render_editor(
    frame: &mut Frame,
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
    theme: &UiTheme,
) {
    if layout.editor_area.width == 0 || layout.editor_area.height == 0 {
        return;
    }

    let Some(tab) = pane.active_tab() else {
        return;
    };

    let total_lines = tab.buffer.len_lines().max(1);
    let max_line_width = total_lines.to_string().len();

    if config.show_line_numbers && layout.gutter_area.width > 0 {
        let start = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
        let end = (start + layout.editor_area.height as usize).min(total_lines);

        let gutter_lines: Vec<Line> = (start..end)
            .map(|i| {
                Line::from(Span::styled(
                    format!("{:>width$} ", i + 1, width = max_line_width),
                    Style::default().fg(theme.palette_muted_fg),
                ))
            })
            .collect();

        frame.render_widget(Paragraph::new(gutter_lines), layout.gutter_area);
    }

    if layout.content_area.width == 0 || layout.content_area.height == 0 {
        return;
    }

    let height = layout.editor_area.height as usize;
    let start = tab.viewport.line_offset.min(total_lines.saturating_sub(1));
    let end = (start + height).min(total_lines);

    let syntax = tab.highlight_lines(start, end);

    let mut lines = Vec::with_capacity(end.saturating_sub(start));
    for (idx, row) in (start..end).enumerate() {
        let line_spans = syntax
            .as_ref()
            .and_then(|spans| spans.get(idx))
            .map(|spans| spans.as_slice());

        if let Some(slice) = tab.buffer.line_slice(row) {
            let line_str = slice_to_cow(slice);
            lines.push(render_line(
                &line_str,
                row,
                tab.viewport.horiz_offset,
                tab.buffer.selection(),
                line_spans,
                config.tab_size,
                theme,
            ));
        } else {
            lines.push(Line::default());
        }
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default()),
        layout.content_area,
    );
}

fn render_line(
    line: &str,
    row: usize,
    horiz_offset: u32,
    selection: Option<&crate::models::Selection>,
    highlight_spans: Option<&[HighlightSpan]>,
    tab_size: u8,
    theme: &UiTheme,
) -> Line<'static> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    if line.is_empty() {
        return Line::default();
    }

    let selection_range = selection.and_then(|s| {
        let ((start_row, start_col), (end_row, end_col)) = s.range();
        if row < start_row || row > end_row {
            return None;
        }

        let (sel_start, sel_end) = if row == start_row && row == end_row {
            (start_col, end_col)
        } else if row == start_row {
            (start_col, usize::MAX)
        } else if row == end_row {
            (0, end_col)
        } else {
            (0, usize::MAX)
        };

        Some((sel_start, sel_end))
    });

    let tab_size = tab_size.max(1) as u32;
    let selection_style = Style::default()
        .bg(theme.palette_selected_bg)
        .fg(theme.palette_selected_fg);

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut current_style: Option<Style> = None;
    let mut display_col: u32 = 0;
    let mut byte_offset: usize = 0;
    let mut highlight_idx: usize = 0;

    for (g_idx, g) in line.graphemes(true).enumerate() {
        let g_start = byte_offset;
        let g_end = g_start + g.len();
        byte_offset = g_end;

        let width = if g == "\t" {
            let rem = display_col % tab_size;
            if rem == 0 {
                tab_size
            } else {
                tab_size - rem
            }
        } else {
            g.width() as u32
        };

        if display_col < horiz_offset {
            display_col = display_col.saturating_add(width);
            continue;
        }

        let in_selection = selection_range
            .is_some_and(|(sel_start, sel_end)| g_idx >= sel_start && g_idx < sel_end);

        let style = if in_selection {
            Some(selection_style)
        } else {
            style_for_highlight(highlight_spans, &mut highlight_idx, g_start, theme)
        };

        if style != current_style && !current.is_empty() {
            push_span(&mut spans, &mut current, current_style);
        }
        current_style = style;

        if g == "\t" {
            current.extend(std::iter::repeat(' ').take(width as usize));
        } else {
            current.push_str(g);
        }

        display_col = display_col.saturating_add(width);
    }

    push_span(&mut spans, &mut current, current_style);
    Line::from(spans)
}

fn style_for_highlight(
    highlight_spans: Option<&[HighlightSpan]>,
    highlight_idx: &mut usize,
    byte_offset: usize,
    theme: &UiTheme,
) -> Option<Style> {
    let spans = highlight_spans?;

    while *highlight_idx < spans.len() && spans[*highlight_idx].end <= byte_offset {
        *highlight_idx += 1;
    }

    let span = spans.get(*highlight_idx)?;
    if byte_offset < span.start || byte_offset >= span.end {
        return None;
    }

    let style = match span.kind {
        HighlightKind::Comment => Style::default().fg(theme.palette_muted_fg),
        HighlightKind::String => Style::default().fg(Color::Green),
        HighlightKind::Keyword => Style::default()
            .fg(theme.accent_fg)
            .add_modifier(Modifier::BOLD),
        HighlightKind::Type => Style::default().fg(theme.header_fg),
        HighlightKind::Number => Style::default().fg(Color::Magenta),
        HighlightKind::Attribute => Style::default().fg(Color::Blue),
        HighlightKind::Lifetime => Style::default().fg(Color::Magenta),
    };

    Some(style)
}

fn push_span(spans: &mut Vec<Span<'static>>, current: &mut String, style: Option<Style>) {
    if current.is_empty() {
        return;
    }

    let text = std::mem::take(current);
    match style {
        Some(style) => spans.push(Span::styled(text, style)),
        None => spans.push(Span::raw(text)),
    }
}

fn cursor_position_search_bar(area: Rect, state: &SearchBarState) -> Option<(u16, u16)> {
    let (prefix, text) = match state.focused_field {
        SearchBarField::Search => ("Find: ", state.search_text.as_str()),
        SearchBarField::Replace => ("Replace: ", state.replace_text.as_str()),
    };

    let cursor = state.cursor_pos.min(text.len());
    let before = &text[..cursor];
    let prefix_w = prefix.width() as u16;
    let before_w = before.width() as u16;

    let y = match state.focused_field {
        SearchBarField::Search => area.y,
        SearchBarField::Replace => area.y.saturating_add(1),
    };
    let x = area.x.saturating_add(prefix_w).saturating_add(before_w);
    Some((x, y))
}

fn cursor_display_x_abs(buffer: &crate::models::TextBuffer, tab_size: u8) -> u32 {
    let (row, col) = buffer.cursor();
    let Some(slice) = buffer.line_slice(row) else {
        return 0;
    };
    let line = slice_to_cow(slice);
    let graphemes = line.graphemes(true);

    let mut display_col = 0u32;
    for (i, g) in graphemes.enumerate() {
        if i >= col {
            break;
        }
        if g == "\t" {
            let tab = tab_size as u32;
            let rem = display_col % tab;
            display_col += if rem == 0 { tab } else { tab - rem };
        } else if g == "\n" {
            break;
        } else {
            display_col += g.width() as u32;
        }
    }

    display_col
}
