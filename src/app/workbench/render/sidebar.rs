use super::super::Workbench;
use crate::kernel::{SearchViewport, SidebarTab};
use crate::models::{FileTreeRow, NodeId};
use crate::ui::backend::Backend;
use crate::ui::core::geom::Pos;
use crate::ui::core::geom::Rect as UiRect;
use crate::ui::core::id::IdPath;
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod, Style as UiStyle};
use crate::ui::core::tree::{Axis, Node, NodeKind, Sense, UiTree};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl Workbench {
    pub(super) fn paint_activity_bar(&self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let base = UiStyle::default()
            .bg(self.ui_theme.activity_bg)
            .fg(self.ui_theme.activity_fg);
        let active_style = UiStyle::default()
            .bg(self.ui_theme.activity_active_bg)
            .fg(self.ui_theme.activity_active_fg)
            .add_mod(Mod::BOLD);

        let state = self.store.state();
        let active_pane = state.ui.editor_layout.active_pane;
        let pane = state.editor.pane(active_pane);

        let settings_active = self.settings_path.as_ref().is_some_and(|settings_path| {
            pane.and_then(|p| p.active_tab())
                .and_then(|t| t.path.as_ref())
                .is_some_and(|p| p == settings_path)
        });

        painter.fill_rect(area, base);

        let slot_h = super::super::util::activity_slot_height(area.h);
        for (i, item) in super::super::util::activity_items().iter().enumerate() {
            let slot_top = area.y.saturating_add((i as u16).saturating_mul(slot_h));
            if slot_top >= area.bottom() {
                break;
            }

            let active = match item {
                super::super::util::ActivityItem::Explorer => {
                    state.ui.sidebar_visible && state.ui.sidebar_tab == SidebarTab::Explorer
                }
                super::super::util::ActivityItem::Panel => state.ui.bottom_panel.visible,
                super::super::util::ActivityItem::Palette => state.ui.command_palette.visible,
                super::super::util::ActivityItem::Git => {
                    state.git.repo_root.is_some() && state.ui.git_panel_expanded
                }
                super::super::util::ActivityItem::Settings => settings_active,
            };

            let remaining = area.bottom().saturating_sub(slot_top);
            let h = slot_h.min(remaining).max(1);
            let slot = UiRect::new(area.x, slot_top, area.w, h);
            if slot.is_empty() {
                continue;
            }

            if active {
                painter.fill_rect(slot, active_style);
            }

            let icon_y = slot.y.saturating_add(slot.h / 2);
            let icon = item.icon();
            let icon_w = icon.width().unwrap_or(1).min(u16::MAX as usize) as u16;
            let x = slot.x.saturating_add(slot.w.saturating_sub(icon_w) / 2);

            let style = if active { active_style } else { base };
            let row_clip = UiRect::new(slot.x, icon_y, slot.w, 1);
            painter.text_clipped(Pos::new(x, icon_y), icon.to_string(), style, row_clip);
        }
    }

    pub(super) fn render_sidebar(&mut self, backend: &mut dyn Backend, area: UiRect) {
        if area.is_empty() {
            self.layout_cache.sidebar_tabs_area = None;
            self.layout_cache.sidebar_content_area = None;
            return;
        }

        let mut painter = Painter::new();
        let ui_full = area;

        let inner = UiRect::new(area.x, area.y, area.w.saturating_sub(1), area.h);
        let sep = UiRect::new(
            area.x.saturating_add(area.w.saturating_sub(1)),
            area.y,
            1.min(area.w),
            area.h,
        );
        if !sep.is_empty() {
            let splitter_id = IdPath::root("workbench")
                .push_str("sidebar_splitter")
                .finish();
            self.ui_tree.push(Node {
                id: splitter_id,
                rect: sep,
                layer: 0,
                z: 0,
                sense: Sense::HOVER | Sense::DRAG_SOURCE,
                kind: NodeKind::Splitter {
                    axis: Axis::Vertical,
                },
            });

            let hovered =
                self.ui_runtime.hovered() == Some(splitter_id) || self.sidebar_split_dragging;
            let fg = if hovered {
                self.ui_theme.focus_border
            } else {
                self.ui_theme.separator
            };
            let style = UiStyle::default().bg(self.ui_theme.sidebar_bg).fg(fg);
            for dx in 0..sep.w {
                painter.vline(Pos::new(sep.x.saturating_add(dx), sep.y), sep.h, '│', style);
            }
        }

        if inner.is_empty() {
            self.layout_cache.sidebar_tabs_area = None;
            self.layout_cache.sidebar_content_area = None;
            backend.draw(ui_full, painter.cmds());
            return;
        }

        // Clear the sidebar background so old content doesn't leak through on partial redraws.
        painter.fill_rect(inner, UiStyle::default().bg(self.ui_theme.sidebar_bg));

        let tab_height = 1u16;
        if inner.h <= tab_height {
            self.layout_cache.sidebar_tabs_area = Some(inner);
            self.layout_cache.sidebar_content_area = None;
            backend.draw(ui_full, painter.cmds());
            return;
        }

        let (tabs_area, content_area) = inner.split_top(tab_height);

        self.layout_cache.sidebar_tabs_area = Some(tabs_area);
        self.layout_cache.sidebar_content_area = Some(content_area);

        let active_tab = self.store.state().ui.sidebar_tab;
        let tab_active = UiStyle::default()
            .fg(self.ui_theme.header_fg)
            .add_mod(Mod::BOLD);
        let tab_inactive = UiStyle::default().fg(self.ui_theme.palette_muted_fg);

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

        let ui_tabs = tabs_area;
        if !ui_tabs.is_empty() {
            painter.fill_rect(ui_tabs, UiStyle::default().bg(self.ui_theme.sidebar_bg));

            const EXPLORER_LABEL: &str = " EXPLORER ";
            const SEARCH_LABEL: &str = " SEARCH ";

            let y = ui_tabs.y;
            let mut x = ui_tabs.x;
            painter.text_clipped(Pos::new(x, y), EXPLORER_LABEL, explorer_style, ui_tabs);
            x = x.saturating_add(
                UnicodeWidthStr::width(EXPLORER_LABEL).min(u16::MAX as usize) as u16,
            );
            painter.text_clipped(Pos::new(x, y), SEARCH_LABEL, search_style, ui_tabs);
        }

        match active_tab {
            SidebarTab::Explorer => {
                self.layout_cache.git_panel_area = None;
                self.layout_cache.git_branch_areas.clear();

                let (show_git_panel, branches_len) = {
                    let state = self.store.state();
                    (
                        state.git.repo_root.is_some() && state.ui.git_panel_expanded,
                        state.git.branches.len(),
                    )
                };

                let (tree_area, git_area) = if show_git_panel && content_area.h >= 3 {
                    let branches_len = branches_len.clamp(1, 8) as u16;
                    let max_git_height = content_area.h.saturating_sub(1);
                    let git_height = (1 + branches_len).min(max_git_height);
                    let (tree_area, git_area) = content_area.split_bottom(git_height);
                    (tree_area, Some(git_area))
                } else {
                    (content_area, None)
                };

                self.sync_explorer_view_height(tree_area.h);
                let state = self.store.state();
                let explorer_state = &state.explorer;
                let ui_area = tree_area;
                self.explorer.paint(
                    &mut painter,
                    crate::views::ExplorerPaintCtx {
                        area: ui_area,
                        rows: &explorer_state.rows,
                        selected_id: explorer_state.selected(),
                        scroll_offset: explorer_state.scroll_offset,
                        git_status_by_id: &explorer_state.git_status_by_id,
                        theme: &self.ui_theme,
                    },
                );
                push_explorer_nodes(
                    &mut self.ui_tree,
                    tree_area,
                    explorer_state.root_id(),
                    &explorer_state.rows,
                    explorer_state.scroll_offset,
                );

                if let Some(git_area) = git_area {
                    self.paint_git_panel(&mut painter, git_area);
                }
            }
            SidebarTab::Search => {
                let search_box_height = 2u16.min(content_area.h);
                let results_height = content_area.h.saturating_sub(search_box_height);
                self.sync_search_view_height(SearchViewport::Sidebar, results_height);
                let search_state = &self.store.state().search;
                let ui_area = content_area;
                self.search_view
                    .paint(&mut painter, ui_area, search_state, &self.ui_theme);
            }
        }

        backend.draw(ui_full, painter.cmds());
    }
}

fn push_explorer_nodes(
    ui_tree: &mut UiTree,
    area: UiRect,
    root_id: NodeId,
    rows: &[FileTreeRow],
    scroll_offset: usize,
) {
    if area.is_empty() {
        return;
    }

    // Allow right-click on empty space in the explorer tree.
    let id = IdPath::root("workbench").push_str("explorer_area").finish();
    ui_tree.push(Node {
        id,
        rect: area,
        layer: 0,
        z: 0,
        sense: Sense::CONTEXT_MENU,
        kind: NodeKind::Unknown,
    });

    // Allow dropping into the workspace root by dropping onto empty space in the explorer tree.
    // Row-level drop targets (folder/file rows) win due to higher z-order.
    if !rows.is_empty() {
        let visible_height = area.h as usize;
        let visible_end = (scroll_offset + visible_height).min(rows.len());
        let used_h = visible_end
            .saturating_sub(scroll_offset)
            .min(u16::MAX as usize) as u16;
        let y = area.y.saturating_add(used_h);
        let h = area.bottom().saturating_sub(y);
        if h > 0 {
            let id = IdPath::root("workbench")
                .push_str("explorer_root_drop")
                .finish();
            ui_tree.push(Node {
                id,
                rect: UiRect::new(area.x, y, area.w, h),
                layer: 0,
                z: 0,
                sense: Sense::DROP_TARGET,
                kind: NodeKind::ExplorerFolderDrop {
                    node_id: root_id.to_raw(),
                },
            });
        }
    } else {
        let id = IdPath::root("workbench")
            .push_str("explorer_root_drop")
            .finish();
        ui_tree.push(Node {
            id,
            rect: area,
            layer: 0,
            z: 0,
            sense: Sense::DROP_TARGET,
            kind: NodeKind::ExplorerFolderDrop {
                node_id: root_id.to_raw(),
            },
        });
        return;
    }

    let visible_height = area.h as usize;
    let visible_end = (scroll_offset + visible_height).min(rows.len());
    for (i, row) in rows
        .iter()
        .enumerate()
        .take(visible_end)
        .skip(scroll_offset)
    {
        let y = area
            .y
            .saturating_add((i - scroll_offset).min(u16::MAX as usize) as u16);
        if y >= area.bottom() {
            break;
        }

        let row_id = row.id.to_raw();
        let rect = UiRect::new(area.x, y, area.w, 1);

        let id = IdPath::root("workbench")
            .push_str("explorer_row")
            .push_u64(row_id)
            .finish();
        ui_tree.push(Node {
            id,
            rect,
            layer: 0,
            z: 0,
            sense: Sense::CLICK | Sense::DRAG_SOURCE | Sense::CONTEXT_MENU | Sense::DROP_TARGET,
            kind: NodeKind::ExplorerRow {
                node_id: row.id.to_raw(),
            },
        });

        if row.is_dir {
            let id = IdPath::root("workbench")
                .push_str("explorer_folder_drop")
                .push_u64(row_id)
                .finish();
            ui_tree.push(Node {
                id,
                rect,
                layer: 0,
                z: 0,
                sense: Sense::DROP_TARGET,
                kind: NodeKind::ExplorerFolderDrop {
                    node_id: row.id.to_raw(),
                },
            });
        }
    }
}
