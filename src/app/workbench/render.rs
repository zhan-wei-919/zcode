use super::palette;
use super::Workbench;
use crate::core::text_window;
use crate::kernel::services::adapters::perf;
use crate::kernel::{
    Action as KernelAction, BottomPanelTab, EditorAction, FocusTarget, SearchResultItem,
    SearchViewport, SidebarTab, SplitDirection,
};
use crate::views::{compute_editor_pane_layout, cursor_position_editor, render_editor_pane};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

pub(super) fn render(workbench: &mut Workbench, frame: &mut Frame, area: Rect) {
    let _scope = perf::scope("render.frame");
    workbench.last_render_area = Some(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(0),
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

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(super::ACTIVITY_BAR_WIDTH),
            Constraint::Min(0),
        ])
        .split(body_area);

    let activity_area = columns[0];
    let content_area = columns[1];

    workbench.last_activity_bar_area =
        (activity_area.width > 0 && activity_area.height > 0).then_some(activity_area);
    if activity_area.width > 0 && activity_area.height > 0 {
        let _scope = perf::scope("render.activity");
        workbench.render_activity_bar(frame, activity_area);
    }

    let (main_area, bottom_panel_area) = if workbench.store.state().ui.bottom_panel.visible {
        let panel_height = super::util::bottom_panel_height(content_area.height);
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(panel_height)])
            .split(content_area);
        let panel_area = (areas[1].height > 0).then_some(areas[1]);
        (areas[0], panel_area)
    } else {
        (content_area, None)
    };

    workbench.last_bottom_panel_area = bottom_panel_area;

    if workbench.store.state().ui.sidebar_visible && main_area.width > 0 {
        let sidebar_width = super::util::sidebar_width(main_area.width);
        let body_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar_width), Constraint::Min(0)])
            .split(main_area);

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
        workbench.render_editor_panes(frame, main_area);
    }

    if let Some(panel_area) = bottom_panel_area {
        let _scope = perf::scope("render.panel");
        workbench.render_bottom_panel(frame, panel_area);
    }

    if !workbench.store.state().ui.command_palette.visible
        && !workbench.store.state().ui.input_dialog.visible
        && !workbench.store.state().ui.confirm_dialog.visible
        && !workbench.store.state().ui.explorer_context_menu.visible
    {
        if workbench.store.state().ui.signature_help.visible {
            workbench.render_signature_help_popup(frame, area);
        }
        if workbench.store.state().ui.completion.visible {
            workbench.render_completion_popup(frame, area);
        } else if workbench.store.state().ui.hover_message.is_some() {
            workbench.render_hover_popup(frame, area);
        }
    }

    if workbench.store.state().ui.explorer_context_menu.visible {
        render_explorer_context_menu(workbench, frame, area);
    } else {
        workbench.last_explorer_context_menu_area = None;
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

struct ThinVSeparator {
    fg: Color,
    bg: Color,
}

impl Widget for ThinVSeparator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Use box-drawing chars so the separator connects across cells (avoids "dashed" look).
        let style = Style::default().fg(self.fg).bg(self.bg);
        let right = area.x.saturating_add(area.width);
        let bottom = area.y.saturating_add(area.height);
        for y in area.y..bottom {
            for x in area.x..right {
                buf[(x, y)].set_char('│').set_style(style);
            }
        }
    }
}

struct ThinHSeparator {
    fg: Color,
    bg: Color,
}

impl Widget for ThinHSeparator {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Use box-drawing chars so the separator connects across cells (avoids "dashed" look).
        let style = Style::default().fg(self.fg).bg(self.bg);
        let right = area.x.saturating_add(area.width);
        let bottom = area.y.saturating_add(area.height);
        for y in area.y..bottom {
            for x in area.x..right {
                buf[(x, y)].set_char('─').set_style(style);
            }
        }
    }
}

pub(super) fn cursor_position(workbench: &Workbench) -> Option<(u16, u16)> {
    if workbench.store.state().ui.input_dialog.visible {
        return input_dialog_cursor(workbench);
    }

    if workbench.store.state().ui.explorer_context_menu.visible {
        return None;
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

    fn render_header(&mut self, _frame: &mut Frame, _area: Rect) {}

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
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base = Style::default()
            .bg(self.theme.activity_bg)
            .fg(self.theme.activity_fg);
        let active_style = Style::default()
            .bg(self.theme.activity_active_bg)
            .fg(self.theme.activity_active_fg)
            .add_modifier(Modifier::BOLD);

        let content = area;
        frame.render_widget(Block::default().style(base), content);

        let state = self.store.state();
        let active_pane = state.ui.editor_layout.active_pane;
        let pane = state.editor.pane(active_pane);
        let search_bar = pane.map(|p| &p.search_bar);

        let settings_active = self.settings_path.as_ref().is_some_and(|settings_path| {
            pane.and_then(|p| p.active_tab())
                .and_then(|t| t.path.as_ref())
                .is_some_and(|p| p == settings_path)
        });

        let slot_h = super::util::activity_slot_height(content.height);
        for (i, item) in super::util::activity_items().iter().enumerate() {
            let slot_top = content.y.saturating_add((i as u16).saturating_mul(slot_h));
            if slot_top >= content.y.saturating_add(content.height) {
                break;
            }

            let active = match item {
                super::util::ActivityItem::Explorer => {
                    state.ui.sidebar_visible && state.ui.sidebar_tab == SidebarTab::Explorer
                }
                super::util::ActivityItem::Search => {
                    state.ui.sidebar_visible && state.ui.sidebar_tab == SidebarTab::Search
                }
                super::util::ActivityItem::Problems => {
                    state.ui.bottom_panel.visible
                        && state.ui.bottom_panel.active_tab == BottomPanelTab::Problems
                }
                super::util::ActivityItem::Results => {
                    state.ui.bottom_panel.visible
                        && state.ui.bottom_panel.active_tab == BottomPanelTab::SearchResults
                }
                super::util::ActivityItem::Logs => {
                    state.ui.bottom_panel.visible
                        && state.ui.bottom_panel.active_tab == BottomPanelTab::Logs
                }
                super::util::ActivityItem::Find => search_bar.is_some_and(|sb| {
                    sb.visible && sb.mode == crate::kernel::editor::SearchBarMode::Search
                }),
                super::util::ActivityItem::Replace => search_bar.is_some_and(|sb| {
                    sb.visible && sb.mode == crate::kernel::editor::SearchBarMode::Replace
                }),
                super::util::ActivityItem::Palette => state.ui.command_palette.visible,
                super::util::ActivityItem::Git => {
                    state.git.repo_root.is_some() && state.ui.git_panel_expanded
                }
                super::util::ActivityItem::Settings => settings_active,
            };

            let remaining = content
                .y
                .saturating_add(content.height)
                .saturating_sub(slot_top);
            let h = slot_h.min(remaining).max(1);
            let slot = Rect::new(content.x, slot_top, content.width, h);
            if slot.width == 0 || slot.height == 0 {
                continue;
            }

            if active {
                frame.render_widget(Block::default().style(active_style), slot);
            }

            let icon_y = slot.y.saturating_add(slot.height / 2);
            let style = if active { active_style } else { base };
            let cell = Rect::new(slot.x, icon_y, slot.width, 1);

            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(item.icon().to_string(), style)))
                    .alignment(Alignment::Center),
                cell,
            );
        }
    }

    fn render_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            self.last_sidebar_tabs_area = None;
            self.last_sidebar_content_area = None;
            return;
        }

        let inner = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
        let sep = Rect::new(
            area.x.saturating_add(area.width.saturating_sub(1)),
            area.y,
            1.min(area.width),
            area.height,
        );
        if sep.width > 0 {
            frame.render_widget(
                ThinVSeparator {
                    fg: self.theme.separator,
                    bg: self.theme.palette_bg,
                },
                sep,
            );
        }

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
            .fg(self.theme.header_fg)
            .add_modifier(Modifier::BOLD);
        let tab_inactive = Style::default().fg(self.theme.palette_muted_fg);

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
                self.last_git_panel_area = None;
                self.last_git_branch_areas.clear();

                let (show_git_panel, branches_len) = {
                    let state = self.store.state();
                    (
                        state.git.repo_root.is_some() && state.ui.git_panel_expanded,
                        state.git.branches.len(),
                    )
                };

                let (tree_area, git_area) = if show_git_panel && content_area.height >= 3 {
                    let branches_len = branches_len.max(1).min(8) as u16;
                    let max_git_height = content_area.height.saturating_sub(1);
                    let git_height = (1 + branches_len).min(max_git_height);
                    let tree_height = content_area.height.saturating_sub(git_height);
                    let tree_area = Rect::new(
                        content_area.x,
                        content_area.y,
                        content_area.width,
                        tree_height,
                    );
                    let git_area = Rect::new(
                        content_area.x,
                        content_area.y.saturating_add(tree_height),
                        content_area.width,
                        content_area.height.saturating_sub(tree_height),
                    );
                    (tree_area, Some(git_area))
                } else {
                    (content_area, None)
                };

                self.sync_explorer_view_height(tree_area.height);
                let state = self.store.state();
                let explorer_state = &state.explorer;
                self.explorer.render(
                    frame,
                    tree_area,
                    &explorer_state.rows,
                    explorer_state.selected(),
                    explorer_state.scroll_offset,
                    &explorer_state.git_status_by_id,
                    &self.theme,
                );

                if let Some(git_area) = git_area {
                    self.render_git_panel(frame, git_area);
                }
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

    fn render_git_panel(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let state = self.store.state();
        if state.git.repo_root.is_none() {
            return;
        };
        if !state.ui.git_panel_expanded {
            return;
        }

        self.last_git_panel_area = Some(area);

        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);
        frame.render_widget(Block::default().style(base_style), area);

        let sep_area = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        frame.render_widget(
            ThinHSeparator {
                fg: self.theme.separator,
                bg: self.theme.palette_bg,
            },
            sep_area,
        );

        if area.height <= 1 {
            return;
        }

        let active_style = Style::default()
            .bg(self.theme.palette_selected_bg)
            .fg(self.theme.palette_selected_fg)
            .add_modifier(Modifier::BOLD);
        let inactive_style = base_style;

        let max_items = (area.height - 1) as usize;

        let active_branch = state.git.head.as_ref().and_then(|head| {
            if head.detached {
                None
            } else {
                head.branch.as_deref()
            }
        });

        if state.git.branches.is_empty() {
            if let Some(head) = state.git.head.as_ref() {
                let mut label = head.display();
                let end = text_window::truncate_to_width(&label, area.width as usize);
                label.truncate(end);
                let line_area = Rect::new(area.x, area.y + 1, area.width, 1);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(label, inactive_style))),
                    line_area,
                );
            }
            return;
        }

        let mut branches: Vec<&str> =
            Vec::with_capacity(state.git.branches.len().saturating_add(1));
        if let Some(active) = active_branch {
            branches.push(active);
        }
        for branch in &state.git.branches {
            if Some(branch.as_str()) != active_branch {
                branches.push(branch);
            }
        }

        for (idx, branch) in branches.into_iter().take(max_items).enumerate() {
            let y = area.y + 1 + idx as u16;
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            let mut label = branch.to_string();
            let end = text_window::truncate_to_width(&label, area.width as usize);
            label.truncate(end);

            let is_active = Some(branch) == active_branch;
            let style = if is_active {
                active_style
            } else {
                inactive_style
            };
            let line_area = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label, style))),
                line_area,
            );
            self.last_git_branch_areas
                .push((branch.to_string(), line_area));
        }
    }

    fn render_bottom_panel(&mut self, frame: &mut Frame, area: Rect) {
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

        if area.width == 0 || area.height == 0 {
            return;
        }

        let max_line_width = lines
            .iter()
            .map(|line| UnicodeWidthStr::width(*line))
            .max()
            .unwrap_or(1);

        let desired_width = max_line_width.saturating_add(2);
        let desired_height = lines.len().saturating_add(2);

        let width = desired_width.max(4).min(area.width as usize).max(1) as u16;
        let height = desired_height.max(3).min(area.height as usize).max(1) as u16;

        let right = area.x.saturating_add(area.width);
        let mut x = cx;
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }
        let below = cy.saturating_add(1);
        let bottom = area.y.saturating_add(area.height);
        let mut y = if below.saturating_add(height) <= bottom {
            below
        } else {
            cy.saturating_sub(height)
        };
        if y < area.y {
            y = area.y;
        }

        let popup_area = Rect::new(x, y, width, height);
        frame.render_widget(Clear, popup_area);
        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);
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

        let content = lines.join("\n");
        frame.render_widget(
            Paragraph::new(content)
                .style(base_style)
                .wrap(Wrap { trim: true }),
            inner,
        );
    }

    fn render_signature_help_popup(&self, frame: &mut Frame, area: Rect) {
        let signature_help = &self.store.state().ui.signature_help;
        if !signature_help.visible || signature_help.text.trim().is_empty() {
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

        let mut lines: Vec<&str> = signature_help.text.lines().collect();
        if lines.is_empty() {
            return;
        }
        if lines.len() > 4 {
            lines.truncate(4);
        }

        if area.width == 0 || area.height == 0 {
            return;
        }

        let max_line_width = lines
            .iter()
            .map(|line| UnicodeWidthStr::width(*line))
            .max()
            .unwrap_or(1);

        let desired_width = max_line_width.saturating_add(2);
        let desired_height = lines.len().saturating_add(2);

        let width = desired_width.max(8).min(area.width as usize).max(1) as u16;
        let height = desired_height.max(3).min(area.height as usize).max(1) as u16;

        let right = area.x.saturating_add(area.width);
        let mut x = cx;
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }

        let below = cy.saturating_add(1);
        let bottom = area.y.saturating_add(area.height);
        let prefer_above = self.store.state().ui.completion.visible;
        let mut y = if prefer_above {
            let above = cy.saturating_sub(height);
            if cy >= height && above >= area.y {
                above
            } else if below.saturating_add(height) <= bottom {
                below
            } else {
                area.y
            }
        } else if below.saturating_add(height) <= bottom {
            below
        } else {
            cy.saturating_sub(height)
        };
        if y < area.y {
            y = area.y;
        }

        let popup_area = Rect::new(x, y, width, height);
        frame.render_widget(Clear, popup_area);
        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);
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

        let content = lines.join("\n");
        frame.render_widget(
            Paragraph::new(content)
                .style(base_style)
                .wrap(Wrap { trim: true }),
            inner,
        );
    }

    fn render_completion_popup(&self, frame: &mut Frame, area: Rect) {
        let completion = &self.store.state().ui.completion;
        if !completion.visible || completion.items.is_empty() {
            return;
        }

        let active_pane = self.store.state().ui.editor_layout.active_pane;
        let pane = completion
            .request
            .as_ref()
            .map(|req| req.pane)
            .unwrap_or(active_pane);
        let pane_area = match self
            .last_editor_areas
            .get(pane)
            .or_else(|| self.last_editor_areas.get(active_pane))
        {
            Some(area) => *area,
            None => return,
        };
        let Some(pane_state) = self.store.state().editor.pane(pane) else {
            return;
        };
        let config = &self.store.state().editor.config;
        let layout = compute_editor_pane_layout(pane_area, pane_state, config);
        let Some((cx, cy)) = cursor_position_editor(&layout, pane_state, config) else {
            return;
        };

        let max_items = 8usize;
        let selected = completion
            .selected
            .min(completion.items.len().saturating_sub(1));
        let mut start = 0usize;
        if selected >= max_items {
            start = selected + 1 - max_items;
        }
        let end = (start + max_items).min(completion.items.len());

        let mut lines = Vec::with_capacity(end.saturating_sub(start));
        let mut max_width = 1usize;
        for (i, item) in completion.items.iter().enumerate().take(end).skip(start) {
            let is_selected = i == selected;
            let row_bg = if is_selected {
                self.theme.palette_selected_bg
            } else {
                self.theme.palette_bg
            };
            let marker_style = Style::default()
                .fg(if is_selected {
                    self.theme.focus_border
                } else {
                    self.theme.palette_muted_fg
                })
                .bg(row_bg);
            let label_style = Style::default().fg(self.theme.palette_fg).bg(row_bg);
            let detail_style = Style::default().fg(self.theme.palette_muted_fg).bg(row_bg);
            let marker = if is_selected { ">" } else { " " };

            let text = item.label.as_str();
            let mut detail = "";
            if let Some(d) = item.detail.as_deref() {
                if !d.trim().is_empty() {
                    detail = d;
                }
            }

            let width = if detail.is_empty() {
                UnicodeWidthStr::width(text)
            } else {
                UnicodeWidthStr::width(text).saturating_add(1 + UnicodeWidthStr::width(detail))
            };
            max_width = max_width.max(width.saturating_add(2));

            let mut spans = vec![
                Span::styled(marker, marker_style),
                Span::styled(" ", label_style),
                Span::styled(text, label_style),
            ];
            if !detail.is_empty() {
                spans.push(Span::styled(" ", label_style));
                spans.push(Span::styled(detail, detail_style));
            }
            lines.push(Line::from(spans));
        }

        if area.width == 0 || area.height == 0 {
            return;
        }

        let desired_width = max_width.saturating_add(2);
        let desired_height = lines.len().saturating_add(2);

        let width = desired_width.max(8).min(area.width as usize).max(1) as u16;
        let height = desired_height.max(3).min(area.height as usize).max(1) as u16;

        let right = area.x.saturating_add(area.width);
        let mut x = cx;
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }
        let below = cy.saturating_add(1);
        let bottom = area.y.saturating_add(area.height);
        let mut y = if below.saturating_add(height) <= bottom {
            below
        } else {
            cy.saturating_sub(height)
        };
        if y < area.y {
            y = area.y;
        }

        let popup_area = Rect::new(x, y, width, height);
        frame.render_widget(Clear, popup_area);

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

    fn render_editor_panes(&mut self, frame: &mut Frame, area: Rect) {
        let panes = self.store.state().ui.editor_layout.panes.max(1);
        let hovered = self.store.state().ui.hovered_tab;
        let workspace_empty = self.store.state().explorer.rows.is_empty();
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
                        workspace_empty,
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
                                    workspace_empty,
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

                        // Split separator: avoid box borders (more nvim-like), just paint a 1-cell bar.
                        frame.render_widget(
                            ThinVSeparator {
                                fg: self.theme.separator,
                                bg: self.theme.palette_bg,
                            },
                            sep_area,
                        );

                        let left_inner = left_area;
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
                                    workspace_empty,
                                );
                            }
                        }

                        let right_inner = right_area;
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
                                    workspace_empty,
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
                                    workspace_empty,
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

                        // Split separator: avoid box borders (more nvim-like), just paint a 1-cell bar.
                        frame.render_widget(
                            ThinHSeparator {
                                fg: self.theme.separator,
                                bg: self.theme.palette_bg,
                            },
                            sep_area,
                        );

                        let top_inner = top_area;
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
                                    workspace_empty,
                                );
                            }
                        }

                        let bottom_inner = bottom_area;
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
                                    workspace_empty,
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
                        workspace_empty,
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

    fn sync_locations_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_locations_panel_height == Some(height) {
            return;
        }
        self.last_locations_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::LocationsSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_code_actions_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_code_actions_panel_height == Some(height) {
            return;
        }
        self.last_code_actions_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::CodeActionsSetViewHeight {
            height: height as usize,
        });
    }

    fn sync_symbols_view_height(&mut self, height: u16) {
        if height == 0 {
            return;
        }
        if self.last_symbols_panel_height == Some(height) {
            return;
        }
        self.last_symbols_panel_height = Some(height);
        let _ = self.dispatch_kernel(KernelAction::SymbolsSetViewHeight {
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

fn render_explorer_context_menu(workbench: &mut Workbench, frame: &mut Frame, area: Rect) {
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

fn input_dialog_cursor(workbench: &Workbench) -> Option<(u16, u16)> {
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
