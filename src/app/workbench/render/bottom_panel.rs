use super::super::Workbench;
use crate::kernel::{BottomPanelTab, SearchResultItem, SearchViewport};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

impl Workbench {
    pub(super) fn render_bottom_panel(&mut self, frame: &mut Frame, area: Rect) {
        let tab = self.store.state().ui.bottom_panel.active_tab.clone();
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);
        frame.render_widget(Block::default().style(base_style), area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        let tabs_area = rows[0];
        let content_area = rows[1];

        self.render_bottom_panel_tabs(frame, tabs_area, &tab);

        match tab {
            BottomPanelTab::Problems => self.render_bottom_panel_problems(frame, content_area),
            BottomPanelTab::CodeActions => {
                self.render_bottom_panel_code_actions(frame, content_area)
            }
            BottomPanelTab::Locations => self.render_bottom_panel_locations(frame, content_area),
            BottomPanelTab::Symbols => self.render_bottom_panel_symbols(frame, content_area),
            BottomPanelTab::SearchResults => {
                self.render_bottom_panel_search_results(frame, content_area)
            }
            BottomPanelTab::Logs => self.render_bottom_panel_logs(frame, content_area),
            BottomPanelTab::Terminal => self.render_bottom_panel_terminal(frame, content_area),
        }
    }

    fn render_bottom_panel_tabs(&self, frame: &mut Frame, area: Rect, active: &BottomPanelTab) {
        let tab_active = Style::default()
            .fg(self.theme.header_fg)
            .add_modifier(Modifier::BOLD);
        let tab_inactive = Style::default().fg(self.theme.palette_muted_fg);

        let mut spans = Vec::new();
        for (tab, label) in self.bottom_panel_tabs() {
            let style = if &tab == active {
                tab_active
            } else {
                tab_inactive
            };
            spans.push(Span::styled(label, style));
        }

        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_bottom_panel_problems(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let height = area.height as usize;
        self.sync_problems_view_height(area.height);

        let problems_state = &self.store.state().problems;
        let problems = problems_state.items();
        if problems.is_empty() {
            let msg = Line::from(Span::styled(
                "No problems",
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg), area);
            return;
        }

        let start = problems_state.scroll_offset().min(problems.len());
        let end = (start + height).min(problems.len());
        let selected = problems_state
            .selected_index()
            .min(problems.len().saturating_sub(1));

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        for (i, item) in problems.iter().enumerate().take(end).skip(start) {
            let file_name = item
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| item.path.to_string_lossy().to_string());
            let line = item.range.start_line.saturating_add(1);
            let col = item.range.start_col.saturating_add(1);
            let is_selected = i == selected;
            let marker_style = if is_selected {
                Style::default().fg(self.theme.focus_border)
            } else {
                Style::default().fg(self.theme.palette_muted_fg)
            };
            let marker = if is_selected { ">" } else { " " };
            let severity_style = match item.severity {
                crate::kernel::problems::ProblemSeverity::Error => {
                    Style::default().fg(self.theme.error_fg)
                }
                crate::kernel::problems::ProblemSeverity::Warning => {
                    Style::default().fg(self.theme.warning_fg)
                }
                crate::kernel::problems::ProblemSeverity::Information => {
                    Style::default().fg(self.theme.palette_muted_fg)
                }
                crate::kernel::problems::ProblemSeverity::Hint => {
                    Style::default().fg(self.theme.palette_muted_fg)
                }
            };
            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(
                    format!("{}:{}:{} ", file_name, line, col),
                    Style::default().fg(self.theme.accent_fg),
                ),
                Span::styled(format!("[{}] ", item.severity.label()), severity_style),
                Span::raw(item.message.as_str()),
            ]));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_bottom_panel_locations(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let height = area.height as usize;
        self.sync_locations_view_height(area.height);

        let locations_state = &self.store.state().locations;
        let locations = locations_state.items();
        if locations.is_empty() {
            let msg = Line::from(Span::styled(
                "No locations",
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg), area);
            return;
        }

        let start = locations_state.scroll_offset().min(locations.len());
        let end = (start + height).min(locations.len());
        let selected = locations_state
            .selected_index()
            .min(locations.len().saturating_sub(1));

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        for (i, item) in locations.iter().enumerate().take(end).skip(start) {
            let file_name = item
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| item.path.to_string_lossy().to_string());
            let line = item.line.saturating_add(1);
            let col = item.column.saturating_add(1);
            let is_selected = i == selected;
            let marker_style = if is_selected {
                Style::default().fg(self.theme.focus_border)
            } else {
                Style::default().fg(self.theme.palette_muted_fg)
            };
            let marker = if is_selected { ">" } else { " " };
            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(
                    format!("{}:{}:{} ", file_name, line, col),
                    Style::default().fg(self.theme.accent_fg),
                ),
            ]));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_bottom_panel_code_actions(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let height = area.height as usize;
        self.sync_code_actions_view_height(area.height);

        let actions_state = &self.store.state().code_actions;
        let actions = actions_state.items();
        if actions.is_empty() {
            let msg = Line::from(Span::styled(
                "No actions",
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg), area);
            return;
        }

        let start = actions_state.scroll_offset().min(actions.len());
        let end = (start + height).min(actions.len());
        let selected = actions_state
            .selected_index()
            .min(actions.len().saturating_sub(1));

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        for (i, action) in actions.iter().enumerate().take(end).skip(start) {
            let is_selected = i == selected;
            let marker_style = if is_selected {
                Style::default().fg(self.theme.focus_border)
            } else {
                Style::default().fg(self.theme.palette_muted_fg)
            };
            let marker = if is_selected { ">" } else { " " };

            let title_style = if action.is_preferred {
                Style::default()
                    .fg(self.theme.accent_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.palette_fg)
            };

            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(action.title.as_str(), title_style),
            ]));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_bottom_panel_symbols(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let height = area.height as usize;
        self.sync_symbols_view_height(area.height);

        let symbols_state = &self.store.state().symbols;
        let symbols = symbols_state.items();
        if symbols.is_empty() {
            let msg = Line::from(Span::styled(
                "No symbols",
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg), area);
            return;
        }

        let start = symbols_state.scroll_offset().min(symbols.len());
        let end = (start + height).min(symbols.len());
        let selected = symbols_state
            .selected_index()
            .min(symbols.len().saturating_sub(1));

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        for (i, item) in symbols.iter().enumerate().take(end).skip(start) {
            let file_name = item
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| item.path.to_string_lossy().to_string());
            let line = item.line.saturating_add(1);
            let col = item.column.saturating_add(1);
            let is_selected = i == selected;
            let marker_style = if is_selected {
                Style::default().fg(self.theme.focus_border)
            } else {
                Style::default().fg(self.theme.palette_muted_fg)
            };
            let marker = if is_selected { ">" } else { " " };

            let kind = symbol_kind_label(item.kind);
            let indent = "  ".repeat(item.level.min(32));

            let mut spans = Vec::with_capacity(8);
            spans.push(Span::styled(marker, marker_style));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("{}:{}:{} ", file_name, line, col),
                Style::default().fg(self.theme.accent_fg),
            ));
            spans.push(Span::styled(
                format!("[{}] ", kind),
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            spans.push(Span::raw(indent));
            spans.push(Span::styled(
                item.name.as_str(),
                Style::default().fg(self.theme.palette_fg),
            ));
            if let Some(detail) = item.detail.as_deref().filter(|s| !s.is_empty()) {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    detail,
                    Style::default().fg(self.theme.palette_muted_fg),
                ));
            }

            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_bottom_panel_logs(&self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        if area.width < 3 || area.height < 3 {
            return;
        }

        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);
        let border_style = Style::default()
            .fg(self.theme.focus_border)
            .bg(self.theme.palette_bg);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .style(base_style),
            area,
        );

        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if self.logs.is_empty() {
            let msg = Line::from(Span::styled(
                "No logs yet",
                Style::default()
                    .bg(self.theme.palette_bg)
                    .fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg), inner);
            return;
        }

        let height = inner.height as usize;
        let visible = height.min(self.logs.len());
        let start = self.logs.len().saturating_sub(visible);

        let mut lines = Vec::with_capacity(visible);
        for line in self.logs.iter().skip(start) {
            lines.push(Line::from(line.as_str()));
        }

        frame.render_widget(Paragraph::new(lines).style(base_style), inner);
    }

    fn render_bottom_panel_search_results(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        let summary_area = rows[0];
        let list_area = rows[1];

        self.sync_search_view_height(SearchViewport::BottomPanel, list_area.height);
        let snapshot = self
            .store
            .state()
            .search
            .snapshot(SearchViewport::BottomPanel);

        let summary = if snapshot.searching {
            format!(
                "Searching... {} files ({} with matches)",
                snapshot.files_searched, snapshot.files_with_matches
            )
        } else if let Some(err) = snapshot.last_error {
            format!("Error: {}", err)
        } else if snapshot.total_matches > 0 {
            format!(
                "{} results in {} files",
                snapshot.total_matches, snapshot.file_count
            )
        } else if !snapshot.search_text.is_empty() {
            "No results".to_string()
        } else {
            "Enter search term in Search sidebar".to_string()
        };

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                summary,
                Style::default().fg(self.theme.palette_muted_fg),
            ))),
            summary_area,
        );

        if list_area.width == 0 || list_area.height == 0 {
            return;
        }

        if snapshot.items.is_empty() {
            return;
        }

        let search_state = &self.store.state().search;

        let height = list_area.height as usize;
        let start = snapshot.scroll_offset.min(snapshot.items.len());
        let end = (start + height).min(snapshot.items.len());
        let selected = snapshot
            .selected_index
            .min(snapshot.items.len().saturating_sub(1));

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        for (i, item) in snapshot.items.iter().enumerate().take(end).skip(start) {
            let is_selected = i == selected;
            let marker_style = if is_selected {
                Style::default().fg(self.theme.focus_border)
            } else {
                Style::default().fg(self.theme.palette_muted_fg)
            };
            let marker = if is_selected { ">" } else { " " };

            match *item {
                SearchResultItem::FileHeader { file_index } => {
                    let Some(file) = search_state.files.get(file_index) else {
                        continue;
                    };
                    let file_name = file
                        .path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| file.path.to_string_lossy().to_string());
                    let icon = if file.expanded { "▼" } else { "▶" };
                    let match_count = file.matches.len();
                    lines.push(Line::from(vec![
                        Span::styled(marker, marker_style),
                        Span::raw(" "),
                        Span::styled(format!("{} ", icon), Style::default()),
                        Span::styled(file_name, Style::default().fg(self.theme.accent_fg)),
                        Span::styled(
                            format!(" ({})", match_count),
                            Style::default().fg(self.theme.palette_muted_fg),
                        ),
                    ]));
                }
                SearchResultItem::MatchLine {
                    file_index,
                    match_index,
                } => {
                    let Some(file) = search_state.files.get(file_index) else {
                        continue;
                    };
                    let Some(match_info) = file.matches.get(match_index) else {
                        continue;
                    };
                    lines.push(Line::from(vec![
                        Span::styled(marker, marker_style),
                        Span::raw("  "),
                        Span::styled(
                            format!("L{}:", match_info.line + 1),
                            Style::default().fg(self.theme.palette_muted_fg),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            format!("col {}", match_info.col + 1),
                            Style::default().fg(self.theme.header_fg),
                        ),
                    ]));
                }
            }
        }

        frame.render_widget(Paragraph::new(lines), list_area);
    }
}

fn symbol_kind_label(kind: u32) -> &'static str {
    match kind {
        1 => "file",
        2 => "mod",
        3 => "ns",
        4 => "pkg",
        5 => "class",
        6 => "method",
        7 => "prop",
        8 => "field",
        9 => "ctor",
        10 => "enum",
        11 => "iface",
        12 => "fn",
        13 => "var",
        14 => "const",
        15 => "str",
        16 => "num",
        17 => "bool",
        18 => "array",
        19 => "obj",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "op",
        26 => "type",
        _ => "?",
    }
}
