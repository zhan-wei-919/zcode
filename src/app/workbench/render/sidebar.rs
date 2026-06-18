use super::super::Workbench;
use crate::models::{FileTreeRow, NodeId};
use crate::ui::backend::Backend;
use crate::ui::core::geom::Pos;
use crate::ui::core::geom::Rect as UiRect;
use crate::ui::core::id::IdPath;
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style as UiStyle;
use crate::ui::core::tree::{Axis, Node, NodeKind, Sense, UiTree};

impl Workbench {
    /// 常驻文件列表：sidebar 只画 explorer，没有 activity bar / 标签栏。
    pub(super) fn render_sidebar(&mut self, backend: &mut dyn Backend, area: UiRect) {
        if area.is_empty() {
            self.frame_layout.sidebar_content_area = None;
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

            let hovered = self.ui_runtime.hovered() == Some(splitter_id)
                || self.interaction.sidebar_split_dragging;
            let fg = if hovered {
                self.theme.core.focus_border
            } else {
                self.theme.core.separator
            };
            let style = UiStyle::default().bg(self.theme.core.sidebar_bg).fg(fg);
            for dx in 0..sep.w {
                painter.vline(Pos::new(sep.x.saturating_add(dx), sep.y), sep.h, '│', style);
            }
        }

        if inner.is_empty() {
            self.frame_layout.sidebar_content_area = None;
            backend.draw(ui_full, painter.cmds());
            return;
        }

        // Clear the sidebar background so old content doesn't leak through on partial redraws.
        painter.fill_rect(inner, UiStyle::default().bg(self.theme.core.sidebar_bg));

        let tree_area = inner;
        self.frame_layout.sidebar_content_area = Some(tree_area);

        self.sync_explorer_view_height(tree_area.h);
        let active_open_file_id = {
            let state = self.store.state();
            let active_pane = state.ui.editor_layout.active_pane;
            state
                .editor
                .pane(active_pane)
                .and_then(|pane| pane.active_tab())
                .and_then(|tab| tab.path.as_deref())
                .and_then(|path| state.explorer.node_id_for_path(path))
        };
        let state = self.store.state();
        let explorer_state = &state.explorer;
        self.explorer.paint(
            &mut painter,
            crate::views::ExplorerPaintCtx {
                area: tree_area,
                rows: &explorer_state.rows,
                selected_id: explorer_state.selected(),
                active_open_file_id,
                scroll_offset: explorer_state.scroll_offset,
                theme: &self.theme.core,
            },
        );
        push_explorer_nodes(
            &mut self.ui_tree,
            tree_area,
            explorer_state.root_id(),
            &explorer_state.rows,
            explorer_state.scroll_offset,
        );

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
