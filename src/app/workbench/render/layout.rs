use super::super::palette;
use super::super::Workbench;
use super::dialogs::{
    input_dialog_cursor, render_confirm_dialog, render_context_menu, render_input_dialog,
};
use super::terminal::cursor_position_terminal;
use crate::kernel::services::adapters::perf;
use crate::kernel::{BottomPanelTab, FocusTarget, SidebarTab};
use crate::ui::backend::Backend;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::input::DragPayload;
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod as UiMod, Style as UiStyle};
use crate::ui::core::theme::Theme;
use crate::ui::core::tree::NodeKind;
use crate::views::theme_editor::paint_theme_editor;
use crate::views::{
    compute_editor_pane_layout, cursor_position_editor, tab_insertion_index, tab_insertion_x,
};

pub(super) fn render(workbench: &mut Workbench, backend: &mut dyn Backend, area: Rect) {
    let _scope = perf::scope("render.frame");
    workbench.last_render_area = Some(area);
    workbench.ui_tree.clear();

    let (body_area, status_area) = area.split_bottom(super::super::STATUS_HEIGHT);

    if !status_area.is_empty() {
        let _scope = perf::scope("render.status");
        let mut painter = Painter::new();
        workbench.paint_status(&mut painter, status_area);
        backend.draw(status_area, painter.cmds());
    }

    let (activity_area, content_area) = body_area.split_left(super::super::ACTIVITY_BAR_WIDTH);

    workbench.last_activity_bar_area = (!activity_area.is_empty()).then_some(activity_area);
    if !activity_area.is_empty() {
        let _scope = perf::scope("render.activity");
        let mut painter = Painter::new();
        workbench.paint_activity_bar(&mut painter, activity_area);
        backend.draw(activity_area, painter.cmds());
    }

    let (main_area, bottom_panel_area) = if workbench.store.state().ui.bottom_panel.visible {
        let panel_height = super::super::util::bottom_panel_height(content_area.h);
        let (main_area, panel_area) = content_area.split_bottom(panel_height);
        let panel_area = (!panel_area.is_empty()).then_some(panel_area);
        (main_area, panel_area)
    } else {
        (content_area, None)
    };

    workbench.last_bottom_panel_area = bottom_panel_area;

    if workbench.store.state().ui.theme_editor.visible {
        let mut painter = Painter::new();
        let areas = paint_theme_editor(
            &mut painter,
            main_area,
            &workbench.store.state().ui.theme_editor,
            &workbench.ui_theme,
            workbench.terminal_color_support,
        );
        workbench.last_theme_editor_token_list_area = areas.token_list;
        workbench.last_theme_editor_hue_bar_area = areas.hue_bar;
        workbench.last_theme_editor_sv_palette_area = areas.sv_palette;
        backend.draw(main_area, painter.cmds());
    } else if workbench.store.state().ui.sidebar_visible && main_area.w > 0 {
        workbench.last_sidebar_container_area = Some(main_area);

        let available = main_area.w;
        let desired = workbench
            .store
            .state()
            .ui
            .sidebar_width
            .unwrap_or_else(|| super::super::util::sidebar_width(available));
        let sidebar_width = super::super::util::clamp_sidebar_width(available, desired);
        let (sidebar_area, editor_area) = main_area.split_left(sidebar_width);

        workbench.last_sidebar_area = (!sidebar_area.is_empty()).then_some(sidebar_area);

        if !sidebar_area.is_empty() {
            let _scope = perf::scope("render.sidebar");
            workbench.render_sidebar(backend, sidebar_area);
        } else {
            workbench.last_sidebar_tabs_area = None;
            workbench.last_sidebar_content_area = None;
        }

        let _scope = perf::scope("render.editors");
        workbench.render_editor_panes(backend, editor_area);
    } else {
        workbench.last_sidebar_area = None;
        workbench.last_sidebar_tabs_area = None;
        workbench.last_sidebar_content_area = None;
        workbench.last_sidebar_container_area = None;
        workbench.sidebar_split_dragging = false;
        let _scope = perf::scope("render.editors");
        workbench.render_editor_panes(backend, main_area);
    }

    if let Some(panel_area) = bottom_panel_area {
        let _scope = perf::scope("render.panel");
        let mut painter = Painter::new();
        workbench.paint_bottom_panel(&mut painter, panel_area);
        backend.draw(panel_area, painter.cmds());
    }

    render_drag_preview(workbench, backend, area);

    if !workbench.store.state().ui.command_palette.visible
        && !workbench.store.state().ui.input_dialog.visible
        && !workbench.store.state().ui.confirm_dialog.visible
        && !workbench.store.state().ui.context_menu.visible
    {
        if workbench.store.state().ui.signature_help.visible {
            let mut painter = Painter::new();
            workbench.paint_signature_help_popup(&mut painter, area);
            backend.draw(area, painter.cmds());
        }
        if workbench.store.state().ui.completion.visible {
            let mut painter = Painter::new();
            workbench.paint_completion_popup(&mut painter, area);
            backend.draw(area, painter.cmds());
        } else if workbench.store.state().ui.hover_message.is_some() {
            let mut painter = Painter::new();
            workbench.paint_hover_popup(&mut painter, area);
            backend.draw(area, painter.cmds());
        }
    }

    if workbench.store.state().ui.context_menu.visible {
        let mut painter = Painter::new();
        render_context_menu(workbench, &mut painter, area);
        backend.draw(area, painter.cmds());
    }

    if workbench.store.state().ui.command_palette.visible {
        let _scope = perf::scope("render.palette");
        let mut painter = Painter::new();
        palette::render(workbench, &mut painter, area);
        backend.draw(area, painter.cmds());
    }

    if workbench.store.state().ui.input_dialog.visible {
        let mut painter = Painter::new();
        render_input_dialog(workbench, &mut painter, area);
        backend.draw(area, painter.cmds());
    }

    if workbench.store.state().ui.confirm_dialog.visible {
        let mut painter = Painter::new();
        render_confirm_dialog(workbench, &mut painter, area);
        backend.draw(area, painter.cmds());
    }

    let cursor = {
        let _scope = perf::scope("render.cursor");
        cursor_position(workbench)
    };
    backend.set_cursor(cursor.map(|(x, y)| Pos::new(x, y)));
}

fn render_drag_preview(workbench: &Workbench, backend: &mut dyn Backend, area: Rect) {
    let Some(payload) = workbench.ui_runtime.drag_payload() else {
        return;
    };

    let mut painter = Painter::new();

    if let Some(pos) = workbench.ui_runtime.last_pos() {
        let label = match payload {
            DragPayload::Tab { from_pane, tab_id } => workbench
                .store
                .state()
                .editor
                .pane(*from_pane)
                .and_then(|pane| pane.tabs.iter().find(|t| t.id == *tab_id))
                .or_else(|| {
                    workbench
                        .store
                        .state()
                        .editor
                        .panes
                        .iter()
                        .flat_map(|pane| pane.tabs.iter())
                        .find(|t| t.id == *tab_id)
                })
                .map(|tab| tab.title.clone()),
            DragPayload::ExplorerNode { node_id } => workbench
                .store
                .state()
                .explorer
                .path_and_kind_for(*node_id)
                .map(|(path, is_dir)| {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string_lossy().to_string());
                    let suffix = if is_dir { "/" } else { "" };
                    format!("{name}{suffix}")
                }),
        };

        if let Some(label) = label {
            paint_drag_chip(&mut painter, area, pos, label.as_str(), &workbench.ui_theme);
        }
    }

    let Some(over) = workbench.ui_runtime.drag_over() else {
        if painter.cmds().is_empty() {
            return;
        }
        backend.draw(area, painter.cmds());
        return;
    };
    let Some(target) = workbench.ui_tree.node(over) else {
        if painter.cmds().is_empty() {
            return;
        }
        backend.draw(area, painter.cmds());
        return;
    };

    match (payload, target.kind) {
        (DragPayload::Tab { .. }, NodeKind::TabBar { pane }) => {
            if let Some(pos) = workbench.ui_runtime.last_pos() {
                if let Some(pane_state) = workbench.store.state().editor.pane(pane) {
                    if let Some(to_area) = workbench
                        .last_editor_inner_areas
                        .get(pane)
                        .copied()
                        .or_else(|| workbench.last_editor_inner_areas.first().copied())
                    {
                        let config = &workbench.store.state().editor.config;
                        let layout = compute_editor_pane_layout(to_area, pane_state, config);
                        let hovered_to = workbench
                            .store
                            .state()
                            .ui
                            .hovered_tab
                            .filter(|(hp, _)| *hp == pane)
                            .map(|(_, i)| i);
                        if let Some(insertion_index) =
                            tab_insertion_index(&layout, pane_state, pos.x, pos.y, hovered_to)
                        {
                            if let Some(x) =
                                tab_insertion_x(&layout, pane_state, hovered_to, insertion_index)
                            {
                                let marker_style = UiStyle::default()
                                    .bg(workbench.ui_theme.focus_border)
                                    .fg(workbench.ui_theme.palette_fg);
                                painter.style_rect(
                                    Rect::new(x, layout.tab_area.y, 1, 1),
                                    marker_style,
                                );
                            }
                        }
                    }
                }
            }
        }
        (DragPayload::ExplorerNode { .. }, NodeKind::ExplorerFolderDrop { .. }) => {
            let highlight = UiStyle::default()
                .bg(workbench.ui_theme.palette_selected_bg)
                .fg(workbench.ui_theme.palette_selected_fg);
            painter.style_rect(target.rect, highlight);
        }
        (DragPayload::ExplorerNode { .. }, NodeKind::ExplorerRow { .. }) => {
            let highlight = UiStyle::default()
                .bg(workbench.ui_theme.palette_selected_bg)
                .fg(workbench.ui_theme.palette_selected_fg);
            painter.style_rect(target.rect, highlight);
        }
        (DragPayload::ExplorerNode { .. }, NodeKind::EditorArea { pane }) => {
            if let Some(pane_state) = workbench.store.state().editor.pane(pane) {
                if let Some(to_area) = workbench
                    .last_editor_inner_areas
                    .get(pane)
                    .copied()
                    .or_else(|| workbench.last_editor_inner_areas.first().copied())
                {
                    let config = &workbench.store.state().editor.config;
                    let layout = compute_editor_pane_layout(to_area, pane_state, config);

                    // Keep the preview subtle: tint only the tab row so we don't override editor content.
                    let rect = layout.tab_area;
                    if !rect.is_empty() {
                        let highlight = UiStyle::default()
                            .bg(workbench.ui_theme.palette_selected_bg)
                            .fg(workbench.ui_theme.palette_selected_fg);
                        painter.style_rect(rect, highlight);
                    }
                }
            }
        }
        (DragPayload::Tab { .. }, NodeKind::EditorSplitDrop { drop, .. }) => {
            let highlight = UiStyle::default()
                .bg(workbench.ui_theme.palette_selected_bg)
                .fg(workbench.ui_theme.palette_selected_fg);
            painter.style_rect(target.rect, highlight);

            let label = match drop {
                crate::ui::core::tree::SplitDrop::Right => "Split Right",
                crate::ui::core::tree::SplitDrop::Down => "Split Down",
            };
            let label_w =
                unicode_width::UnicodeWidthStr::width(label).min(u16::MAX as usize) as u16;
            let x = target
                .rect
                .x
                .saturating_add(target.rect.w.saturating_sub(label_w) / 2);
            let y = target.rect.y.saturating_add(target.rect.h / 2);
            let row = Rect::new(target.rect.x, y, target.rect.w, 1.min(target.rect.h));
            if !row.is_empty() {
                painter.text_clipped(Pos::new(x, y), label, highlight, row);
            }
        }
        _ => {}
    }

    if painter.cmds().is_empty() {
        return;
    }
    backend.draw(area, painter.cmds());
}

fn paint_drag_chip(painter: &mut Painter, screen: Rect, mouse: Pos, label: &str, theme: &Theme) {
    if screen.is_empty() {
        return;
    }

    let label = label.trim();
    if label.is_empty() {
        return;
    }

    // Minimal floating chip: a single-line badge with a thin accent bar + subtle shadow.
    // This looks more "GUI-like" than a boxed tooltip.
    let label_w = unicode_width::UnicodeWidthStr::width(label).min(u16::MAX as usize) as u16;

    let accent_w = 1u16;
    let pad_left = 1u16;
    let pad_right = 1u16;
    let desired_w = accent_w
        .saturating_add(pad_left)
        .saturating_add(label_w)
        .saturating_add(pad_right);
    let w = desired_w.min(screen.w);
    let h = 1u16;

    if w < 4 || screen.h < h {
        return;
    }

    let mut x = mouse.x.saturating_add(1);
    let mut y = mouse.y.saturating_add(1);
    if x.saturating_add(w) > screen.right() {
        x = screen.right().saturating_sub(w);
    }
    if y.saturating_add(h) > screen.bottom() {
        y = screen.bottom().saturating_sub(h);
    }
    x = x.max(screen.x);
    y = y.max(screen.y);

    let rect = Rect::new(x, y, w, h);

    let chip_bg = theme.palette_selected_bg;
    let chip_fg = theme.palette_selected_fg;

    let fill = UiStyle::default().bg(chip_bg).fg(chip_fg);
    painter.fill_rect(rect, fill);

    let accent_style = UiStyle::default().bg(chip_bg).fg(theme.focus_border);
    painter.vline(Pos::new(rect.x, rect.y), rect.h, '\u{258F}', accent_style);

    let text_style = UiStyle::default()
        .bg(chip_bg)
        .fg(chip_fg)
        .add_mod(UiMod::BOLD);
    let text_x = rect.x.saturating_add(accent_w.saturating_add(pad_left));
    painter.text_clipped(Pos::new(text_x, rect.y), label, text_style, rect);
}

pub(super) fn cursor_position(workbench: &Workbench) -> Option<(u16, u16)> {
    if workbench.store.state().ui.input_dialog.visible {
        return input_dialog_cursor(workbench);
    }

    if workbench.store.state().ui.context_menu.visible {
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
        FocusTarget::ThemeEditor => None,
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/app/workbench/render/layout.rs"]
mod tests;
