use super::super::Workbench;
use super::layout::ThinVSeparator;
use crate::kernel::{BottomPanelTab, SearchViewport, SidebarTab};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

impl Workbench {
    pub(super) fn render_activity_bar(&self, frame: &mut Frame, area: Rect) {
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

        let slot_h = super::super::util::activity_slot_height(content.height);
        for (i, item) in super::super::util::activity_items().iter().enumerate() {
            let slot_top = content.y.saturating_add((i as u16).saturating_mul(slot_h));
            if slot_top >= content.y.saturating_add(content.height) {
                break;
            }

            let active = match item {
                super::super::util::ActivityItem::Explorer => {
                    state.ui.sidebar_visible && state.ui.sidebar_tab == SidebarTab::Explorer
                }
                super::super::util::ActivityItem::Search => {
                    state.ui.sidebar_visible && state.ui.sidebar_tab == SidebarTab::Search
                }
                super::super::util::ActivityItem::Problems => {
                    state.ui.bottom_panel.visible
                        && state.ui.bottom_panel.active_tab == BottomPanelTab::Problems
                }
                super::super::util::ActivityItem::Results => {
                    state.ui.bottom_panel.visible
                        && state.ui.bottom_panel.active_tab == BottomPanelTab::SearchResults
                }
                super::super::util::ActivityItem::Logs => {
                    state.ui.bottom_panel.visible
                        && state.ui.bottom_panel.active_tab == BottomPanelTab::Logs
                }
                super::super::util::ActivityItem::Find => search_bar.is_some_and(|sb| {
                    sb.visible && sb.mode == crate::kernel::editor::SearchBarMode::Search
                }),
                super::super::util::ActivityItem::Replace => search_bar.is_some_and(|sb| {
                    sb.visible && sb.mode == crate::kernel::editor::SearchBarMode::Replace
                }),
                super::super::util::ActivityItem::Palette => state.ui.command_palette.visible,
                super::super::util::ActivityItem::Git => {
                    state.git.repo_root.is_some() && state.ui.git_panel_expanded
                }
                super::super::util::ActivityItem::Settings => settings_active,
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

    pub(super) fn render_sidebar(&mut self, frame: &mut Frame, area: Rect) {
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
}
