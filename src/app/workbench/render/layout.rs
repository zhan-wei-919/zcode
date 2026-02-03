use super::super::palette;
use super::super::Workbench;
use super::dialogs::{
    input_dialog_cursor, render_confirm_dialog, render_explorer_context_menu, render_input_dialog,
};
use super::terminal::cursor_position_terminal;
use crate::kernel::services::adapters::perf;
use crate::kernel::{BottomPanelTab, FocusTarget, SidebarTab};
use crate::views::{compute_editor_pane_layout, cursor_position_editor};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;
use ratatui::Frame;

pub(super) fn render(workbench: &mut Workbench, frame: &mut Frame, area: Rect) {
    let _scope = perf::scope("render.frame");
    workbench.last_render_area = Some(area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(0),
            Constraint::Min(0),
            Constraint::Length(super::super::STATUS_HEIGHT),
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
            Constraint::Length(super::super::ACTIVITY_BAR_WIDTH),
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
        let panel_height = super::super::util::bottom_panel_height(content_area.height);
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
        let sidebar_width = super::super::util::sidebar_width(main_area.width);
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

pub(super) struct ThinVSeparator {
    pub(super) fg: Color,
    pub(super) bg: Color,
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

pub(super) struct ThinHSeparator {
    pub(super) fg: Color,
    pub(super) bg: Color,
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
        FocusTarget::BottomPanel => match workbench.store.state().ui.bottom_panel.active_tab {
            BottomPanelTab::Terminal => cursor_position_terminal(workbench),
            _ => None,
        },
        FocusTarget::CommandPalette => None,
    }
}
