//! 全局搜索视图（纯渲染 + 命中测试）

use crate::app::theme::UiTheme;
use crate::kernel::{SearchResultItem, SearchState};
use crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const SEARCH_BOX_HEIGHT: u16 = 2;
const SEARCH_LABEL: &str = "Search: ";

pub struct SearchView {
    area: Option<Rect>,
    search_area: Option<Rect>,
    results_area: Option<Rect>,
}

impl SearchView {
    pub fn new() -> Self {
        Self {
            area: None,
            search_area: None,
            results_area: None,
        }
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.area
            .map(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
            .unwrap_or(false)
    }

    pub fn results_view_height(&self) -> Option<usize> {
        self.results_area.map(|a| a.height as usize)
    }

    pub fn hit_test_results_row(&self, event: &MouseEvent, scroll_offset: usize) -> Option<usize> {
        let area = self.results_area?;
        if event.column < area.x || event.column >= area.x + area.width {
            return None;
        }
        if event.row < area.y || event.row >= area.y + area.height {
            return None;
        }

        Some((event.row - area.y) as usize + scroll_offset)
    }

    pub fn cursor_position(
        &self,
        query: &str,
        cursor: usize,
        case_sensitive: bool,
        use_regex: bool,
    ) -> Option<(u16, u16)> {
        let area = self.search_area?;
        if area.width == 0 || area.height == 0 {
            return None;
        }

        let (query_start, _query_end, indicators) =
            query_window(query, cursor, area.width, case_sensitive, use_regex);
        let cursor = cursor.min(query.len());
        let prefix_width = UnicodeWidthStr::width(&query[query_start..cursor]) as u16;
        let x = area
            .x
            .saturating_add(UnicodeWidthStr::width(SEARCH_LABEL) as u16)
            .saturating_add(prefix_width)
            .min(
                area.x
                    .saturating_add(area.width.saturating_sub(indicators.width)),
            );
        Some((x, area.y))
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, state: &SearchState, theme: &UiTheme) {
        self.area = Some(area);

        let search_box_height = SEARCH_BOX_HEIGHT.min(area.height);
        let results_height = area.height.saturating_sub(search_box_height);

        let search_area = Rect::new(area.x, area.y, area.width, search_box_height);
        let results_area = Rect::new(
            area.x,
            area.y + search_box_height,
            area.width,
            results_height,
        );

        self.search_area = (search_area.width > 0 && search_area.height > 0).then_some(search_area);
        self.results_area =
            (results_area.width > 0 && results_area.height > 0).then_some(results_area);

        let indicators = Indicators::new(state.case_sensitive, state.use_regex);
        let (query_start, query_end, _) = query_window(
            &state.query,
            state.query_cursor,
            search_area.width,
            state.case_sensitive,
            state.use_regex,
        );
        let visible_query = &state.query[query_start..query_end];

        let status = if state.searching {
            format!(
                "Searching... {} files ({} with matches)",
                state.files_searched, state.files_with_matches
            )
        } else if let Some(err) = &state.last_error {
            format!("Error: {}", err)
        } else if state.total_matches > 0 {
            format!(
                "{} results in {} files",
                state.total_matches, state.file_count
            )
        } else if !state.query.is_empty() {
            "No results".to_string()
        } else {
            "Enter search term".to_string()
        };

        let label_style = Style::default().fg(theme.header_fg);
        let query_style = Style::default().fg(theme.palette_fg);
        let indicator_style = Style::default().fg(theme.palette_muted_fg);
        let muted_style = Style::default().fg(theme.palette_muted_fg);

        let search_line = Line::from(vec![
            Span::styled(SEARCH_LABEL, label_style),
            Span::styled(visible_query, query_style),
            Span::raw(indicators.pad_between_query),
            Span::styled(indicators.case_label, indicator_style),
            Span::styled(indicators.regex_label, indicator_style),
        ]);

        let status_line = Line::from(Span::styled(status, muted_style));

        let search_widget = Paragraph::new(vec![search_line, status_line]).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.separator)),
        );
        frame.render_widget(search_widget, search_area);

        if results_area.width == 0 || results_area.height == 0 {
            return;
        }

        if state.items.is_empty() {
            let msg = Line::from(Span::styled("No results", muted_style));
            frame.render_widget(Paragraph::new(msg), results_area);
            return;
        }

        let height = results_area.height as usize;
        let start = state.sidebar_view.scroll_offset.min(state.items.len());
        let end = (start + height).min(state.items.len());
        let selected = state
            .selected_index
            .min(state.items.len().saturating_sub(1));

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        for (row, item) in state.items.iter().enumerate().take(end).skip(start) {
            let is_selected = row == selected;
            let bg = is_selected.then_some(theme.palette_selected_bg);
            let marker_style = Style::default()
                .fg(if is_selected {
                    theme.focus_border
                } else {
                    theme.palette_muted_fg
                })
                .bg(bg.unwrap_or(Color::Reset));
            let marker = if is_selected { ">" } else { " " };

            match *item {
                SearchResultItem::FileHeader { file_index } => {
                    let Some(file) = state.files.get(file_index) else {
                        continue;
                    };
                    let file_name = file
                        .path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.path.to_string_lossy().to_string());

                    let icon = if file.expanded { "▼ " } else { "▶ " };
                    let match_count = file.matches.len();
                    let file_style = Style::default()
                        .fg(theme.accent_fg)
                        .add_modifier(Modifier::BOLD)
                        .bg(bg.unwrap_or(Color::Reset));
                    let count_style = Style::default()
                        .fg(theme.palette_muted_fg)
                        .bg(bg.unwrap_or(Color::Reset));

                    lines.push(Line::from(vec![
                        Span::styled(marker, marker_style),
                        Span::raw(" "),
                        Span::styled(icon, Style::default().bg(bg.unwrap_or(Color::Reset))),
                        Span::styled(file_name, file_style),
                        Span::styled(format!(" ({})", match_count), count_style),
                    ]));
                }
                SearchResultItem::MatchLine {
                    file_index,
                    match_index,
                } => {
                    let Some(file) = state.files.get(file_index) else {
                        continue;
                    };
                    let Some(match_info) = file.matches.get(match_index) else {
                        continue;
                    };
                    let line_style = Style::default()
                        .fg(theme.palette_muted_fg)
                        .bg(bg.unwrap_or(Color::Reset));
                    let col_style = Style::default()
                        .fg(theme.header_fg)
                        .bg(bg.unwrap_or(Color::Reset));

                    lines.push(Line::from(vec![
                        Span::styled(marker, marker_style),
                        Span::raw("  "),
                        Span::styled(format!("L{}:", match_info.line + 1), line_style),
                        Span::raw(" "),
                        Span::styled(format!("col {}", match_info.col + 1), col_style),
                    ]));
                }
            }
        }

        let list_widget = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::NONE)
                .border_style(Style::default().fg(theme.separator)),
        );
        frame.render_widget(list_widget, results_area);
    }
}

struct Indicators<'a> {
    case_label: &'a str,
    regex_label: &'a str,
    pad_between_query: &'a str,
    width: u16,
}

impl<'a> Indicators<'a> {
    fn new(case_sensitive: bool, use_regex: bool) -> Self {
        let case_label = if case_sensitive { "[Aa]" } else { "[aa]" };
        let regex_label = if use_regex { "[.*]" } else { "[  ]" };
        let pad_between_query = " ";
        let width = UnicodeWidthStr::width(pad_between_query) as u16
            + UnicodeWidthStr::width(case_label) as u16
            + UnicodeWidthStr::width(regex_label) as u16;

        Self {
            case_label,
            regex_label,
            pad_between_query,
            width,
        }
    }
}

fn query_window<'a>(
    query: &'a str,
    cursor: usize,
    area_width: u16,
    case_sensitive: bool,
    use_regex: bool,
) -> (usize, usize, Indicators<'a>) {
    let cursor = cursor.min(query.len());
    let indicators = Indicators::new(case_sensitive, use_regex);
    let label_width = UnicodeWidthStr::width(SEARCH_LABEL) as u16;

    let available = area_width
        .saturating_sub(label_width)
        .saturating_sub(indicators.width) as usize;

    if available == 0 {
        return (cursor, cursor, indicators);
    }

    let start = compute_window_start(query, cursor, available);
    let end = start + truncate_to_width(&query[start..], available);
    (start, end.min(query.len()), indicators)
}

fn compute_window_start(query: &str, cursor: usize, available: usize) -> usize {
    let cursor = cursor.min(query.len());
    let prefix = &query[..cursor];
    if UnicodeWidthStr::width(prefix) <= available {
        return 0;
    }

    let mut start = cursor;
    let mut used = 0usize;

    let indices: Vec<(usize, char)> = prefix.char_indices().collect();
    for (idx, ch) in indices.into_iter().rev() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > available {
            break;
        }
        used += w;
        start = idx;
    }

    start
}

fn truncate_to_width(s: &str, max_width: usize) -> usize {
    if max_width == 0 || s.is_empty() {
        return 0;
    }

    let mut used = 0usize;
    let mut end = 0usize;
    for (idx, ch) in s.char_indices() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max_width {
            break;
        }
        used += w;
        end = idx + ch.len_utf8();
    }

    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn test_hit_test_results_row() {
        let mut view = SearchView::new();
        view.results_area = Some(Rect::new(0, 2, 10, 3));

        let ev = MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 1,
            row: 3,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };

        assert_eq!(view.hit_test_results_row(&ev, 0), Some(1));
        assert_eq!(view.hit_test_results_row(&ev, 2), Some(3));
    }
}
