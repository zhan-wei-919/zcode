//! 全局搜索视图（纯渲染 + 命中测试）

use crate::core::event::MouseEvent;
use crate::core::text_window;
use crate::kernel::{SearchResultItem, SearchState};
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod, Style};
use crate::ui::core::theme::Theme;
use unicode_width::UnicodeWidthStr;

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
            .is_some_and(|a| a.contains(Pos::new(x, y)))
    }

    pub fn results_view_height(&self) -> Option<usize> {
        self.results_area.map(|a| a.h as usize)
    }

    pub fn hit_test_results_row(&self, event: &MouseEvent, scroll_offset: usize) -> Option<usize> {
        let area = self.results_area?;
        if !area.contains(Pos::new(event.column, event.row)) {
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
        if area.is_empty() {
            return None;
        }

        let (query_start, _query_end, indicators) =
            query_window(query, cursor, area.w, case_sensitive, use_regex);
        let cursor = cursor.min(query.len());
        let prefix_width = UnicodeWidthStr::width(&query[query_start..cursor]) as u16;
        let x = area
            .x
            .saturating_add(UnicodeWidthStr::width(SEARCH_LABEL) as u16)
            .saturating_add(prefix_width)
            .min(
                area.x
                    .saturating_add(area.w.saturating_sub(indicators.width)),
            );
        Some((x, area.y))
    }

    pub fn paint(&mut self, painter: &mut Painter, area: Rect, state: &SearchState, theme: &Theme) {
        self.area = Some(area);
        if area.is_empty() {
            self.search_area = None;
            self.results_area = None;
            return;
        }

        let search_box_height = SEARCH_BOX_HEIGHT.min(area.h);
        let results_height = area.h.saturating_sub(search_box_height);

        let search_area = Rect::new(area.x, area.y, area.w, search_box_height);
        let results_area = Rect::new(
            area.x,
            area.y + search_box_height,
            area.w,
            results_height,
        );

        self.search_area = (!search_area.is_empty()).then_some(search_area);
        self.results_area =
            (!results_area.is_empty()).then_some(results_area);

        let bg = Style::default().bg(theme.palette_bg);
        painter.fill_rect(area, bg);

        let indicators = Indicators::new(state.case_sensitive, state.use_regex);
        let (query_start, query_end, _) = query_window(
            &state.query,
            state.query_cursor,
            search_area.w,
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

        if !search_area.is_empty() {
            let label_style = Style::default().fg(theme.header_fg);
            let query_style = Style::default().fg(theme.palette_fg);
            let indicator_style = Style::default().fg(theme.palette_muted_fg);
            let muted_style = Style::default().fg(theme.palette_muted_fg);

            let row0 = Rect::new(search_area.x, search_area.y, search_area.w, 1);
            let mut x = search_area.x;
            painter.text_clipped(Pos::new(x, search_area.y), SEARCH_LABEL, label_style, row0);
            x = x.saturating_add(UnicodeWidthStr::width(SEARCH_LABEL).min(u16::MAX as usize) as u16);
            painter.text_clipped(Pos::new(x, search_area.y), visible_query, query_style, row0);
            x = x.saturating_add(UnicodeWidthStr::width(visible_query).min(u16::MAX as usize) as u16);
            painter.text_clipped(
                Pos::new(x, search_area.y),
                indicators.pad_between_query,
                indicator_style,
                row0,
            );
            x = x.saturating_add(UnicodeWidthStr::width(indicators.pad_between_query).min(u16::MAX as usize) as u16);
            painter.text_clipped(
                Pos::new(x, search_area.y),
                indicators.case_label,
                indicator_style,
                row0,
            );
            x = x.saturating_add(UnicodeWidthStr::width(indicators.case_label).min(u16::MAX as usize) as u16);
            painter.text_clipped(
                Pos::new(x, search_area.y),
                indicators.regex_label,
                indicator_style,
                row0,
            );

            if search_area.h >= 2 {
                let row1_y = search_area.y.saturating_add(1);
                let row1 = Rect::new(search_area.x, row1_y, search_area.w, 1);
                painter.text_clipped(Pos::new(search_area.x, row1_y), status, muted_style, row1);
            }
        }

        if results_area.is_empty() {
            return;
        }

        let muted_style = Style::default().fg(theme.palette_muted_fg);

        if state.items.is_empty() {
            let row = Rect::new(results_area.x, results_area.y, results_area.w, 1);
            painter.text_clipped(Pos::new(results_area.x, results_area.y), "No results", muted_style, row);
            return;
        }

        let height = results_area.h as usize;
        let start = state.sidebar_view.scroll_offset.min(state.items.len());
        let end = (start + height).min(state.items.len());
        let selected = state
            .selected_index
            .min(state.items.len().saturating_sub(1));

        let mut out_row = 0usize;
        for (row, item) in state.items.iter().enumerate().take(end).skip(start) {
            if out_row >= height {
                break;
            }
            let y = results_area.y.saturating_add(out_row.min(u16::MAX as usize) as u16);
            if y >= results_area.bottom() {
                break;
            }

            let is_selected = row == selected;
            let bg = if is_selected { theme.palette_selected_bg } else { theme.palette_bg };
            let row_bg = Style::default().bg(bg);
            let clip = Rect::new(results_area.x, y, results_area.w, 1);
            painter.fill_rect(clip, row_bg);

            let marker_style = Style::default().fg(if is_selected {
                theme.focus_border
            } else {
                theme.palette_muted_fg
            });
            let marker = if is_selected { ">" } else { " " };

            let mut x = results_area.x;
            painter.text_clipped(Pos::new(x, y), marker, marker_style, clip);
            x = x.saturating_add(1);

            match *item {
                SearchResultItem::FileHeader { file_index } => {
                    let Some(file) = state.files.get(file_index) else {
                        out_row = out_row.saturating_add(1);
                        continue;
                    };
                    let file_name = file
                        .path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.path.to_string_lossy().to_string());

                    let icon = if file.expanded { "▼ " } else { "▶ " };
                    let match_count = file.matches.len();
                    let icon_style = Style::default();
                    let file_style =
                        Style::default().fg(theme.accent_fg).add_mod(Mod::BOLD);
                    let count_style = Style::default().fg(theme.palette_muted_fg);

                    painter.text_clipped(Pos::new(x, y), " ", Style::default(), clip);
                    x = x.saturating_add(1);
                    painter.text_clipped(Pos::new(x, y), icon, icon_style, clip);
                    x = x.saturating_add(UnicodeWidthStr::width(icon).min(u16::MAX as usize) as u16);
                    let file_w = UnicodeWidthStr::width(file_name.as_str()).min(u16::MAX as usize) as u16;
                    painter.text_clipped(Pos::new(x, y), file_name, file_style, clip);
                    x = x.saturating_add(file_w);
                    painter.text_clipped(
                        Pos::new(x, y),
                        format!(" ({})", match_count),
                        count_style,
                        clip,
                    );
                }
                SearchResultItem::MatchLine {
                    file_index,
                    match_index,
                } => {
                    let Some(file) = state.files.get(file_index) else {
                        out_row = out_row.saturating_add(1);
                        continue;
                    };
                    let Some(match_info) = file.matches.get(match_index) else {
                        out_row = out_row.saturating_add(1);
                        continue;
                    };
                    let line_style = Style::default().fg(theme.palette_muted_fg);
                    let col_style = Style::default().fg(theme.header_fg);

                    painter.text_clipped(Pos::new(x, y), "  ", Style::default(), clip);
                    x = x.saturating_add(2);
                    let l = format!("L{}:", match_info.line + 1);
                    let l_w = UnicodeWidthStr::width(l.as_str()).min(u16::MAX as usize) as u16;
                    painter.text_clipped(Pos::new(x, y), l, line_style, clip);
                    x = x.saturating_add(l_w);
                    painter.text_clipped(Pos::new(x, y), " ", Style::default(), clip);
                    x = x.saturating_add(1);
                    let c = format!("col {}", match_info.col + 1);
                    painter.text_clipped(Pos::new(x, y), c, col_style, clip);
                }
            }

            out_row = out_row.saturating_add(1);
        }
    }
}

impl Default for SearchView {
    fn default() -> Self {
        Self::new()
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

    let start = text_window::compute_window_start(query, cursor, available);
    let end = start + text_window::truncate_to_width(&query[start..], available);
    (start, end.min(query.len()), indicators)
}

#[cfg(test)]
#[path = "../../../tests/unit/views/search/search_view.rs"]
mod tests;
