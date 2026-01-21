use super::palette;
use super::Workbench;
use crate::kernel::services::adapters::perf;
use crate::kernel::{
    Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, SearchResultItem,
    SearchViewport, SidebarTab, SplitDirection,
};
use crate::views::{compute_editor_pane_layout, cursor_position_editor, render_editor_pane};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub(super) fn render(workbench: &mut Workbench, frame: &mut Frame, area: Rect) {
    let _scope = perf::scope("render.frame");
    workbench.last_render_area = Some(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(super::HEADER_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(super::STATUS_HEIGHT),
        ])
        .split(area);

    let header_area = chunks[0];
    let body_area = chunks[1];
    let status_area = chunks[2];

    {
        let _scope = perf::scope("render.header");
        workbench.render_header(frame, header_area);
    }
    {
        let _scope = perf::scope("render.status");
        workbench.render_status(frame, status_area);
    }

    let (main_area, bottom_panel_area) = if workbench.store.state().ui.bottom_panel.visible {
        let panel_height = super::util::bottom_panel_height(body_area.height);
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(panel_height)])
            .split(body_area);
        let panel_area = (areas[1].height > 0).then_some(areas[1]);
        (areas[0], panel_area)
    } else {
        (body_area, None)
    };

    workbench.last_bottom_panel_area = bottom_panel_area;

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(super::ACTIVITY_BAR_WIDTH),
            Constraint::Min(0),
        ])
        .split(main_area);

    let activity_area = columns[0];
    let content_area = columns[1];

    workbench.last_activity_bar_area =
        (activity_area.width > 0 && activity_area.height > 0).then_some(activity_area);
    if activity_area.width > 0 && activity_area.height > 0 {
        let _scope = perf::scope("render.activity");
        workbench.render_activity_bar(frame, activity_area);
    }

    if workbench.store.state().ui.sidebar_visible && content_area.width > 0 {
        let sidebar_width = super::util::sidebar_width(content_area.width);
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar_width), Constraint::Min(0)])
            .split(content_area);

        workbench.last_sidebar_area =
            (body_chunks[0].width > 0 && body_chunks[0].height > 0).then_some(body_chunks[0]);

        if body_chunks[0].width > 0 && body_chunks[0].height > 0 {
            let _scope = perf::scope("render.sidebar");
            workbench.render_sidebar(frame, body_chunks[0]);
        } else {
            workbench.last_sidebar_tabs_area = None;
            workbench.last_sidebar_content_area = None;
        }

        let _scope = perf::scope("render.editors");
        workbench.render_editor_panes(frame, body_chunks[1]);
    } else {
        workbench.last_sidebar_area = None;
        workbench.last_sidebar_tabs_area = None;
        workbench.last_sidebar_content_area = None;
        let _scope = perf::scope("render.editors");
        workbench.render_editor_panes(frame, content_area);
    }

    if let Some(panel_area) = bottom_panel_area {
        let _scope = perf::scope("render.panel");
        workbench.render_bottom_panel(frame, panel_area);
    }

    if workbench.store.state().ui.hover_message.is_some()
        && !workbench.store.state().ui.command_palette.visible
        && !workbench.store.state().ui.input_dialog.visible
        && !workbench.store.state().ui.confirm_dialog.visible
    {
        workbench.render_hover_popup(frame, area);
    }

    if workbench.store.state().ui.command_palette.visible {
        let _scope = perf::scope("render.palette");
        palette::render(workbench, frame, area);
    }

    if workbench.store.state().ui.input_dialog.visible {
        render_input_dialog(workbench, frame, area);
    }

    if workbench.store.state().ui.confirm_dialog.visible {
        render_confirm_dialog(workbench, frame, area);
    }

    let cursor = {
        let _scope = perf::scope("render.cursor");
        cursor_position(workbench)
    };
    if let Some((x, y)) = cursor {
        frame.set_cursor_position((x, y));
    }
}

pub(super) fn cursor_position(workbench: &Workbench) -> Option<(u16, u16)> {
    if workbench.store.state().ui.input_dialog.visible {
        return input_dialog_cursor(workbench);
    }

    if workbench.store.state().ui.command_palette.visible
        && workbench.store.state().ui.focus == FocusTarget::CommandPalette
    {
        return palette::cursor(workbench);
    }

    match workbench.store.state().ui.focus {
        FocusTarget::Explorer => {
            if workbench.store.state().ui.sidebar_tab == SidebarTab::Search {
                let search_state = &workbench.store.state().search;
                workbench.search_view.cursor_position(
                    &search_state.query,
                    search_state.query_cursor,
                    search_state.case_sensitive,
                    search_state.use_regex,
                )
            } else {
                None
            }
        }
        FocusTarget::Editor => {
            let pane = workbench.store.state().ui.editor_layout.active_pane;
            let area = *workbench.last_editor_inner_areas.get(pane)?;
            let pane_state = workbench.store.state().editor.pane(pane)?;
            let config = &workbench.store.state().editor.config;
            let layout = compute_editor_pane_layout(area, pane_state, config);
            cursor_position_editor(&layout, pane_state, config)
        }
        FocusTarget::BottomPanel | FocusTarget::CommandPalette => None,
    }
}

impl Workbench {
    fn active_label(&self) -> &'static str {
        match self.store.state().ui.focus {
            FocusTarget::Explorer => match self.store.state().ui.sidebar_tab {
                SidebarTab::Explorer => "Explorer",
                SidebarTab::Search => "Search",
            },
            FocusTarget::Editor => "Editor",
            FocusTarget::BottomPanel => "Panel",
            FocusTarget::CommandPalette => "Palette",
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = "zcode - TUI Editor";
        let header = Paragraph::new(Span::styled(
            title,
            Style::default().fg(self.theme.header_fg),
        ))
        .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(header, area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let (mode, cursor_info) = if let Some(pane) = self
            .store
            .state()
            .editor
            .pane(self.store.state().ui.editor_layout.active_pane)
        {
            if let Some(tab) = pane.active_tab() {
                let (row, col) = tab.buffer.cursor();
                let dirty = if tab.dirty { " [+]" } else { "" };
                let file_name = tab
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| tab.title.clone());

                (
                    format!("{}{}", file_name, dirty),
                    format!("Ln {}, Col {}", row + 1, col + 1),
                )
            } else {
                ("No file".to_string(), String::new())
            }
        } else {
            ("No file".to_string(), String::new())
        };

        let active = self.active_label();

        let text = format!("{} | {} | {}", mode, cursor_info, active);
        frame.render_widget(Paragraph::new(text), area);
    }

    fn render_activity_bar(&self, frame: &mut Frame, area: Rect) {
        let active = self.store.state().ui.sidebar_tab;

        let base = Style::default()
            .bg(self.theme.activity_bg)
            .fg(self.theme.activity_fg);
        let active_style = Style::default()
            .bg(self.theme.activity_active_bg)
            .fg(self.theme.activity_active_fg);

        let explorer_style = if active == SidebarTab::Explorer {
            active_style
        } else {
            base
        };

        let search_style = if active == SidebarTab::Search {
            active_style
        } else {
            base
        };

        let lines = vec![
            Line::from(Span::styled(" E ", explorer_style)),
            Line::from(Span::styled(" S ", search_style)),
        ];

        let widget = Paragraph::new(lines)
            .style(base)
            .block(Block::default().borders(Borders::RIGHT));
        frame.render_widget(widget, area);
    }

    fn render_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let is_focused = self.store.state().ui.focus == FocusTarget::Explorer;
        let border_style = if is_focused {
            Style::default().fg(self.theme.focus_border)
        } else {
            Style::default().fg(self.theme.inactive_border)
        };

        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(border_style);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            self.last_sidebar_tabs_area = None;
            self.last_sidebar_content_area = None;
            return;
        }

        let tab_height = 1u16;
        if inner.height <= tab_height {
            self.last_sidebar_tabs_area = Some(inner);
            self.last_sidebar_content_area = None;
            return;
        }

        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(tab_height), Constraint::Min(0)])
            .split(inner);

        let tabs_area = areas[0];
        let content_area = areas[1];

        self.last_sidebar_tabs_area = Some(tabs_area);
        self.last_sidebar_content_area = Some(content_area);

        let active_tab = self.store.state().ui.sidebar_tab;
        let tab_active = Style::default()
            .fg(self.theme.sidebar_tab_active_fg)
            .bg(self.theme.sidebar_tab_active_bg);
        let tab_inactive = Style::default().fg(self.theme.sidebar_tab_inactive_fg);

        let explorer_style = if active_tab == SidebarTab::Explorer {
            tab_active
        } else {
            tab_inactive
        };
        let search_style = if active_tab == SidebarTab::Search {
            tab_active
        } else {
            tab_inactive
        };

        let tab_line = Line::from(vec![
            Span::styled(" EXPLORER ", explorer_style),
            Span::styled(" SEARCH ", search_style),
        ]);

        frame.render_widget(Paragraph::new(tab_line), tabs_area);

        match active_tab {
            SidebarTab::Explorer => {
                self.sync_explorer_view_height(content_area.height);
                let explorer_state = &self.store.state().explorer;
                self.explorer.render(
                    frame,
                    content_area,
                    &explorer_state.rows,
                    explorer_state.selected(),
                    explorer_state.scroll_offset,
                    &self.theme,
                );
            }
            SidebarTab::Search => {
                let search_box_height = 2u16.min(content_area.height);
                let results_height = content_area.height.saturating_sub(search_box_height);
                self.sync_search_view_height(SearchViewport::Sidebar, results_height);
                let search_state = &self.store.state().search;
                self.search_view
                    .render(frame, content_area, search_state, &self.theme);
            }
        }
    }

    fn render_bottom_panel(&mut self, frame: &mut Frame, area: Rect) {
        let tab = self.store.state().ui.bottom_panel.active_tab.clone();
        let is_focused = self.store.state().ui.focus == FocusTarget::BottomPanel;
        let border_style = if is_focused {
            Style::default().fg(self.theme.focus_border)
        } else {
            Style::default().fg(self.theme.inactive_border)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);
        let tabs_area = rows[0];
        let content_area = rows[1];

        self.render_bottom_panel_tabs(frame, tabs_area, &tab);

        match tab {
            BottomPanelTab::Problems => self.render_bottom_panel_problems(frame, content_area),
            BottomPanelTab::SearchResults => {
                self.render_bottom_panel_search_results(frame, content_area)
            }
            BottomPanelTab::Logs => self.render_bottom_panel_logs(frame, content_area),
        }
    }

    fn render_bottom_panel_tabs(&self, frame: &mut Frame, area: Rect, active: &BottomPanelTab) {
        let tab_active = Style::default()
            .fg(self.theme.sidebar_tab_active_fg)
            .bg(self.theme.sidebar_tab_active_bg);
        let tab_inactive = Style::default().fg(self.theme.sidebar_tab_inactive_fg);

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
                    Style::default().fg(Color::Red)
                }
                crate::kernel::problems::ProblemSeverity::Warning => {
                    Style::default().fg(Color::Yellow)
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
                Span::styled(
                    format!("[{}] ", item.severity.label()),
                    severity_style,
                ),
                Span::raw(item.message.as_str()),
            ]));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_bottom_panel_logs(&self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        if self.logs.is_empty() {
            let msg = Line::from(Span::styled(
                "No logs yet",
                Style::default().fg(self.theme.palette_muted_fg),
            ));
            frame.render_widget(Paragraph::new(msg), area);
            return;
        }

        let height = area.height as usize;
        let visible = height.min(self.logs.len());
        let start = self.logs.len().saturating_sub(visible);

        let mut lines = Vec::with_capacity(visible);
        for line in self.logs.iter().skip(start) {
            lines.push(Line::from(line.as_str()));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_hover_popup(&self, frame: &mut Frame, area: Rect) {
        let Some(text) = self.store.state().ui.hover_message.as_ref() else {
            return;
        };
        if text.trim().is_empty() {
            return;
        }

        let active_pane = self.store.state().ui.editor_layout.active_pane;
        let pane_area = match self.last_editor_areas.get(active_pane) {
            Some(area) => *area,
            None => return,
        };
        let Some(pane_state) = self.store.state().editor.pane(active_pane) else {
            return;
        };
        let config = &self.store.state().editor.config;
        let layout = compute_editor_pane_layout(pane_area, pane_state, config);
        let Some((cx, cy)) = cursor_position_editor(&layout, pane_state, config) else {
            return;
        };

        let mut lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return;
        }
        if lines.len() > 6 {
            lines.truncate(6);
        }

        let max_line_width = lines
            .iter()
            .map(|line| UnicodeWidthStr::width(*line))
            .max()
            .unwrap_or(1);
        let width = (max_line_width as u16 + 2)
            .min(area.width.max(1))
            .max(4);
        let height = (lines.len() as u16 + 2).min(area.height.max(1)).max(3);

        let mut x = cx;
        if x + width > area.x + area.width {
            x = area.x + area.width - width;
        }
        let below = cy.saturating_add(1);
        let mut y = if below + height <= area.y + area.height {
            below
        } else {
            cy.saturating_sub(height)
        };
        if y < area.y {
            y = area.y;
        }

        let popup_area = Rect::new(x, y, width, height);
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.focus_border));
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let content = lines.join("\n");
        frame.render_widget(Paragraph::new(content).wrap(Wrap { trim: true }), inner);
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

    fn render_editor_panes(&mut self, frame: &mut Frame, area: Rect) {
        let panes = self.store.state().ui.editor_layout.panes.max(1);
        let hovered = self.store.state().ui.hovered_tab;
        self.last_editor_areas.clear();
        self.last_editor_areas.reserve(panes.min(2));
        self.last_editor_inner_areas.clear();
        self.last_editor_inner_areas.reserve(panes.min(2));
        self.last_editor_content_sizes.resize_with(panes, || (0, 0));
        self.last_editor_container_area = (area.width > 0 && area.height > 0).then_some(area);
        self.last_editor_splitter_area = None;
        let hovered_for_pane = |p: usize| hovered.filter(|(hp, _)| *hp == p).map(|(_, i)| i);

        match panes {
            1 => {
                self.editor_split_dragging = false;
                self.last_editor_areas.push(area);
                self.last_editor_inner_areas.push(area);

                let layout = {
                    let Some(pane_state) = self.store.state().editor.pane(0) else {
                        return;
                    };
                    let config = &self.store.state().editor.config;
                    compute_editor_pane_layout(area, pane_state, config)
                };
                self.sync_editor_viewport_size(0, &layout);
                if let Some(pane_state) = self.store.state().editor.pane(0) {
                    let config = &self.store.state().editor.config;
                    render_editor_pane(
                        frame,
                        &layout,
                        pane_state,
                        config,
                        &self.theme,
                        hovered_for_pane(0),
                    );
                }
            }
            2 => {
                let direction = self.store.state().ui.editor_layout.split_direction;
                match direction {
                    SplitDirection::Vertical => {
                        let available = area.width;
                        if available < 3 {
                            self.editor_split_dragging = false;
                            self.last_editor_areas.push(area);
                            self.last_editor_inner_areas.push(area);

                            let active = self.store.state().ui.editor_layout.active_pane.min(1);
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(active)
                                else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(area, pane_state, config)
                            };
                            self.sync_editor_viewport_size(active, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(active) {
                                let config = &self.store.state().editor.config;
                                render_editor_pane(
                                    frame,
                                    &layout,
                                    pane_state,
                                    config,
                                    &self.theme,
                                    hovered_for_pane(active),
                                );
                            }
                            return;
                        }

                        let total = available.saturating_sub(1);
                        let ratio = self.store.state().ui.editor_layout.split_ratio;
                        let mut left_width = ((total as u32) * (ratio as u32) / 1000) as u16;
                        left_width = left_width.clamp(1, total.saturating_sub(1));
                        let right_width = total.saturating_sub(left_width);

                        let left_area = Rect::new(area.x, area.y, left_width, area.height);
                        let sep_area = Rect::new(area.x + left_width, area.y, 1, area.height);
                        let right_area =
                            Rect::new(area.x + left_width + 1, area.y, right_width, area.height);
                        self.last_editor_splitter_area =
                            (sep_area.width > 0 && sep_area.height > 0).then_some(sep_area);

                        self.last_editor_areas.push(left_area);
                        self.last_editor_areas.push(right_area);

                        let active = self.store.state().ui.editor_layout.active_pane;
                        let focus = self.store.state().ui.focus;

                        let inactive_border = Style::default().fg(self.theme.inactive_border);
                        let active_border = Style::default().fg(self.theme.focus_border);
                        let sep_style = if focus == FocusTarget::Editor {
                            Style::default().fg(self.theme.focus_border)
                        } else {
                            Style::default().fg(self.theme.separator)
                        };

                        frame.render_widget(
                            Block::default()
                                .borders(Borders::LEFT)
                                .border_style(sep_style),
                            sep_area,
                        );

                        let left_border = if active == 0 {
                            active_border
                        } else {
                            inactive_border
                        };
                        let right_border = if active == 1 {
                            active_border
                        } else {
                            inactive_border
                        };

                        let left_block = Block::default()
                            .borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM)
                            .border_style(left_border);
                        let left_inner = left_block.inner(left_area);
                        frame.render_widget(left_block, left_area);
                        self.last_editor_inner_areas.push(left_inner);
                        if left_inner.width > 0 && left_inner.height > 0 {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(0) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(left_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(0, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(0) {
                                let config = &self.store.state().editor.config;
                                render_editor_pane(
                                    frame,
                                    &layout,
                                    pane_state,
                                    config,
                                    &self.theme,
                                    hovered_for_pane(0),
                                );
                            }
                        }

                        let right_block = Block::default()
                            .borders(Borders::RIGHT | Borders::TOP | Borders::BOTTOM)
                            .border_style(right_border);
                        let right_inner = right_block.inner(right_area);
                        frame.render_widget(right_block, right_area);
                        self.last_editor_inner_areas.push(right_inner);
                        if right_inner.width > 0 && right_inner.height > 0 {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(1) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(right_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(1, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(1) {
                                let config = &self.store.state().editor.config;
                                render_editor_pane(
                                    frame,
                                    &layout,
                                    pane_state,
                                    config,
                                    &self.theme,
                                    hovered_for_pane(1),
                                );
                            }
                        }
                    }
                    SplitDirection::Horizontal => {
                        let available = area.height;
                        if available < 3 {
                            self.editor_split_dragging = false;
                            self.last_editor_areas.push(area);
                            self.last_editor_inner_areas.push(area);

                            let active = self.store.state().ui.editor_layout.active_pane.min(1);
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(active)
                                else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(area, pane_state, config)
                            };
                            self.sync_editor_viewport_size(active, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(active) {
                                let config = &self.store.state().editor.config;
                                render_editor_pane(
                                    frame,
                                    &layout,
                                    pane_state,
                                    config,
                                    &self.theme,
                                    hovered_for_pane(active),
                                );
                            }
                            return;
                        }

                        let total = available.saturating_sub(1);
                        let ratio = self.store.state().ui.editor_layout.split_ratio;
                        let mut top_height = ((total as u32) * (ratio as u32) / 1000) as u16;
                        top_height = top_height.clamp(1, total.saturating_sub(1));
                        let bottom_height = total.saturating_sub(top_height);

                        let top_area = Rect::new(area.x, area.y, area.width, top_height);
                        let sep_area = Rect::new(area.x, area.y + top_height, area.width, 1);
                        let bottom_area =
                            Rect::new(area.x, area.y + top_height + 1, area.width, bottom_height);
                        self.last_editor_splitter_area =
                            (sep_area.width > 0 && sep_area.height > 0).then_some(sep_area);

                        self.last_editor_areas.push(top_area);
                        self.last_editor_areas.push(bottom_area);

                        let active = self.store.state().ui.editor_layout.active_pane;
                        let focus = self.store.state().ui.focus;

                        let inactive_border = Style::default().fg(self.theme.inactive_border);
                        let active_border = Style::default().fg(self.theme.focus_border);
                        let sep_style = if focus == FocusTarget::Editor {
                            Style::default().fg(self.theme.focus_border)
                        } else {
                            Style::default().fg(self.theme.separator)
                        };

                        frame.render_widget(
                            Block::default()
                                .borders(Borders::TOP)
                                .border_style(sep_style),
                            sep_area,
                        );

                        let top_border = if active == 0 {
                            active_border
                        } else {
                            inactive_border
                        };
                        let bottom_border = if active == 1 {
                            active_border
                        } else {
                            inactive_border
                        };

                        let top_block = Block::default()
                            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                            .border_style(top_border);
                        let top_inner = top_block.inner(top_area);
                        frame.render_widget(top_block, top_area);
                        self.last_editor_inner_areas.push(top_inner);
                        if top_inner.width > 0 && top_inner.height > 0 {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(0) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(top_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(0, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(0) {
                                let config = &self.store.state().editor.config;
                                render_editor_pane(
                                    frame,
                                    &layout,
                                    pane_state,
                                    config,
                                    &self.theme,
                                    hovered_for_pane(0),
                                );
                            }
                        }

                        let bottom_block = Block::default()
                            .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
                            .border_style(bottom_border);
                        let bottom_inner = bottom_block.inner(bottom_area);
                        frame.render_widget(bottom_block, bottom_area);
                        self.last_editor_inner_areas.push(bottom_inner);
                        if bottom_inner.width > 0 && bottom_inner.height > 0 {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(1) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(bottom_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(1, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(1) {
                                let config = &self.store.state().editor.config;
                                render_editor_pane(
                                    frame,
                                    &layout,
                                    pane_state,
                                    config,
                                    &self.theme,
                                    hovered_for_pane(1),
                                );
                            }
                        }
                    }
                }
            }
            _ => {
                self.editor_split_dragging = false;
                self.last_editor_areas.push(area);
                self.last_editor_inner_areas.push(area);

                let active = self
                    .store
                    .state()
                    .ui
                    .editor_layout
                    .active_pane
                    .min(panes - 1);
                let layout = {
                    let Some(pane_state) = self.store.state().editor.pane(active) else {
                        return;
                    };
                    let config = &self.store.state().editor.config;
                    compute_editor_pane_layout(area, pane_state, config)
                };
                self.sync_editor_viewport_size(active, &layout);
                if let Some(pane_state) = self.store.state().editor.pane(active) {
                    let config = &self.store.state().editor.config;
                    render_editor_pane(
                        frame,
                        &layout,
                        pane_state,
                        config,
                        &self.theme,
                        hovered_for_pane(active),
                    );
                }
            }
        }
    }

    fn sync_editor_viewport_size(&mut self, pane: usize, layout: &crate::views::EditorPaneLayout) {
        if pane >= self.last_editor_content_sizes.len() {
            return;
        }

        let width = layout.content_area.width;
        let height = layout.editor_area.height;
        if width == 0 || height == 0 {
            return;
        }

        let prev = self.last_editor_content_sizes[pane];
        let next = (width, height);
        if prev == next {
            return;
        }
        self.last_editor_content_sizes[pane] = next;

        let _ = self.dispatch_kernel(KernelAction::Editor(EditorAction::SetViewportSize {
            pane,
            width: width as usize,
            height: height as usize,
        }));
    }

    fn sync_explorer_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }

        if self.last_explorer_view_height == Some(height) {
            return;
        }
        self.last_explorer_view_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::ExplorerSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_search_view_height(&mut self, viewport: SearchViewport, height: u16) {
        if height == 0 {
            return;
        }

        let slot = match viewport {
            SearchViewport::Sidebar => &mut self.last_search_sidebar_results_height,
            SearchViewport::BottomPanel => &mut self.last_search_panel_results_height,
        };

        if *slot == Some(height) {
            return;
        }
        *slot = Some(height);

        let _ = self.dispatch_kernel(KernelAction::SearchSetViewHeight {
            viewport,
            height: height as usize,
        });
    }

    fn sync_problems_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_problems_panel_height == Some(height) {
            return;
        }
        self.last_problems_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::ProblemsSetViewHeight {
            height: height as usize,
        });
    }
}

fn render_confirm_dialog(workbench: &Workbench, frame: &mut Frame, area: Rect) {
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

    let block = Block::default()
        .title(" Confirm ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(workbench.theme.palette_border));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    if inner.height < 2 || inner.width < 10 {
        return;
    }

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

    let content = Paragraph::new(vec![msg_line, Line::raw(""), hint_line]);
    frame.render_widget(content, inner);
}

fn input_dialog_area(area: Rect) -> Rect {
    super::util::centered_rect(60, 7, area)
}

fn render_input_dialog(workbench: &Workbench, frame: &mut Frame, area: Rect) {
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

    let title = if dialog.title.is_empty() {
        " Input ".to_string()
    } else {
        format!(" {} ", dialog.title)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(workbench.theme.palette_border));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let base_style = Style::default()
        .bg(workbench.theme.palette_bg)
        .fg(workbench.theme.palette_fg);
    let muted_style = Style::default().fg(workbench.theme.palette_muted_fg);

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("> ", base_style),
        Span::styled(dialog.value.as_str(), base_style),
    ]));

    if let Some(err) = dialog.error.as_deref() {
        lines.push(Line::from(Span::styled(
            err,
            Style::default().fg(Color::Red),
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

fn input_dialog_cursor(workbench: &Workbench) -> Option<(u16, u16)> {
    let area = workbench.last_render_area?;
    let dialog = &workbench.store.state().ui.input_dialog;
    if !dialog.visible {
        return None;
    }

    let popup_area = input_dialog_area(area);
    let inner_x = popup_area.x.saturating_add(1);
    let inner_y = popup_area.y.saturating_add(1);
    if popup_area.width < 4 || popup_area.height < 3 {
        return None;
    }

    let cursor = dialog.cursor.min(dialog.value.len());
    let prefix_w = 2u16;
    let before = dialog.value.get(..cursor).unwrap_or_default();
    let before_w = before.width() as u16;

    let x = inner_x
        .saturating_add(prefix_w)
        .saturating_add(before_w)
        .min(popup_area.x + popup_area.width.saturating_sub(2));
    let y = inner_y;

    Some((x, y))
}
