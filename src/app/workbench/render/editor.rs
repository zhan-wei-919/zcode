use super::super::Workbench;
use crate::kernel::SplitDirection;
use crate::kernel::editor::EditorPaneState;
use crate::ui::backend::Backend;
use crate::ui::core::geom::Pos;
use crate::ui::core::geom::Rect as UiRect;
use crate::ui::core::id::IdPath;
use crate::ui::core::layout::Insets;
use crate::ui::core::painter::{BorderKind, Painter};
use crate::ui::core::style::Style as UiStyle;
use crate::ui::core::tree::{Axis, Node, NodeKind, Sense, SplitDrop};
use crate::views::{
    compute_editor_pane_layout, cursor_position_editor, paint_editor_pane, EditorPaneLayout,
};
use unicode_width::UnicodeWidthStr;

impl Workbench {
    fn register_editor_splitter_node(&mut self, sep_area: UiRect, direction: SplitDirection) {
        if sep_area.is_empty() {
            return;
        }
        let axis = match direction {
            SplitDirection::Vertical => Axis::Vertical,
            SplitDirection::Horizontal => Axis::Horizontal,
        };
        let id = IdPath::root("workbench").push_str("editor_splitter").finish();
        self.ui_tree.push(Node {
            id,
            rect: sep_area,
            layer: 0,
            z: 0,
            sense: Sense::HOVER | Sense::DRAG_SOURCE,
            kind: NodeKind::Splitter { axis },
        });
    }

    pub(super) fn paint_hover_popup(&self, painter: &mut Painter, area: UiRect) {
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

        if area.is_empty() {
            return;
        }

        let max_line_width = lines
            .iter()
            .map(|line| UnicodeWidthStr::width(*line))
            .max()
            .unwrap_or(1);

        let desired_width = max_line_width.saturating_add(2);
        let desired_height = lines.len().saturating_add(2);

        let width = desired_width.max(4).min(area.w as usize).max(1) as u16;
        let height = desired_height.max(3).min(area.h as usize).max(1) as u16;

        let right = area.right();
        let mut x = cx;
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }
        let below = cy.saturating_add(1);
        let bottom = area.bottom();
        let mut y = if below.saturating_add(height) <= bottom {
            below
        } else {
            cy.saturating_sub(height)
        };
        if y < area.y {
            y = area.y;
        }

        let popup_area = UiRect::new(x, y, width, height);
        let base_style = UiStyle::default()
            .bg(self.ui_theme.palette_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(popup_area, base_style);

        let inner = popup_area.inset(Insets::all(1));
        if inner.is_empty() {
            return;
        }

        let wrapped = wrap_lines(&lines, inner.w, inner.h as usize);
        let text_style = UiStyle::default().fg(self.ui_theme.palette_fg);
        for (idx, line) in wrapped.into_iter().enumerate() {
            let y = inner.y.saturating_add(idx.min(u16::MAX as usize) as u16);
            if y >= inner.bottom() {
                break;
            }
            let row_clip = UiRect::new(inner.x, y, inner.w, 1);
            painter.text_clipped(Pos::new(inner.x, y), line, text_style, row_clip);
        }
    }

    pub(super) fn paint_signature_help_popup(&self, painter: &mut Painter, area: UiRect) {
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

        if area.is_empty() {
            return;
        }

        let max_line_width = lines
            .iter()
            .map(|line| UnicodeWidthStr::width(*line))
            .max()
            .unwrap_or(1);

        let desired_width = max_line_width.saturating_add(2);
        let desired_height = lines.len().saturating_add(2);

        let width = desired_width.max(8).min(area.w as usize).max(1) as u16;
        let height = desired_height.max(3).min(area.h as usize).max(1) as u16;

        let right = area.right();
        let mut x = cx;
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }

        let below = cy.saturating_add(1);
        let bottom = area.bottom();
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

        let popup_area = UiRect::new(x, y, width, height);
        let base_style = UiStyle::default()
            .bg(self.ui_theme.palette_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(popup_area, base_style);

        let inner = popup_area.inset(Insets::all(1));
        if inner.is_empty() {
            return;
        }

        let wrapped = wrap_lines(&lines, inner.w, inner.h as usize);
        let text_style = UiStyle::default().fg(self.ui_theme.palette_fg);
        for (idx, line) in wrapped.into_iter().enumerate() {
            let y = inner.y.saturating_add(idx.min(u16::MAX as usize) as u16);
            if y >= inner.bottom() {
                break;
            }
            let row_clip = UiRect::new(inner.x, y, inner.w, 1);
            painter.text_clipped(Pos::new(inner.x, y), line, text_style, row_clip);
        }
    }

    pub(super) fn paint_completion_popup(&self, painter: &mut Painter, area: UiRect) {
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

        let mut rows = Vec::with_capacity(end.saturating_sub(start));
        let mut max_inner_width = 1usize;
        for (i, item) in completion.items.iter().enumerate().take(end).skip(start) {
            let is_selected = i == selected;
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
            // marker + space + text + optional (space + detail)
            let inner_w = 2usize.saturating_add(width);
            max_inner_width = max_inner_width.max(inner_w);
            rows.push((is_selected, marker, text.to_string(), detail.to_string()));
        }

        if area.is_empty() {
            return;
        }

        let desired_width = max_inner_width.saturating_add(2);
        let desired_height = rows.len().saturating_add(2);

        let width = desired_width.max(8).min(area.w as usize).max(1) as u16;
        let height = desired_height.max(3).min(area.h as usize).max(1) as u16;

        let right = area.right();
        let mut x = cx;
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }
        let below = cy.saturating_add(1);
        let bottom = area.bottom();
        let mut y = if below.saturating_add(height) <= bottom {
            below
        } else {
            cy.saturating_sub(height)
        };
        if y < area.y {
            y = area.y;
        }

        let popup_area = UiRect::new(x, y, width, height);
        let base_style = UiStyle::default()
            .bg(self.ui_theme.palette_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(popup_area, base_style);

        let border_style = UiStyle::default()
            .fg(self.ui_theme.focus_border)
            .bg(self.ui_theme.palette_bg);
        painter.border(popup_area, border_style, BorderKind::Plain);

        let inner = popup_area.inset(Insets::all(1));
        if inner.is_empty() {
            return;
        }

        let selected_bg = UiStyle::default().bg(self.ui_theme.palette_selected_bg);
        let marker_selected = UiStyle::default().fg(self.ui_theme.focus_border);
        let marker_normal = UiStyle::default().fg(self.ui_theme.palette_muted_fg);
        let label_style = UiStyle::default().fg(self.ui_theme.palette_fg);
        let detail_style = UiStyle::default().fg(self.ui_theme.palette_muted_fg);

        for (idx, (is_selected, marker, label, detail)) in rows.into_iter().enumerate() {
            let y = inner.y.saturating_add(idx.min(u16::MAX as usize) as u16);
            if y >= inner.bottom() {
                break;
            }
            let row_area = UiRect::new(inner.x, y, inner.w, 1);
            if is_selected {
                painter.fill_rect(row_area, selected_bg);
            }

            let mut x = inner.x;
            let marker_style = if is_selected { marker_selected } else { marker_normal };
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_area);
            x = x.saturating_add(1);
            painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_area);
            x = x.saturating_add(1);
            let label_w = label.width().min(u16::MAX as usize) as u16;
            painter.text_clipped(Pos::new(x, y), label, label_style, row_area);
            x = x.saturating_add(label_w);
            if !detail.trim().is_empty() {
                painter.text_clipped(Pos::new(x, y), " ", UiStyle::default(), row_area);
                x = x.saturating_add(1);
                painter.text_clipped(Pos::new(x, y), detail, detail_style, row_area);
            }
        }
    }

    fn draw_editor_pane(
        &self,
        backend: &mut dyn Backend,
        layout: &EditorPaneLayout,
        pane_state: &EditorPaneState,
        hovered_tab: Option<usize>,
        workspace_empty: bool,
    ) {
        let mut painter = Painter::new();
        let config = &self.store.state().editor.config;
        paint_editor_pane(
            &mut painter,
            layout,
            pane_state,
            config,
            &self.ui_theme,
            hovered_tab,
            workspace_empty,
        );

        backend.draw(layout.area, painter.cmds());
    }

    pub(super) fn render_editor_panes(&mut self, backend: &mut dyn Backend, area: UiRect) {
        let panes = self.store.state().ui.editor_layout.panes.max(1);
        let hovered = self.store.state().ui.hovered_tab;
        let workspace_empty = self.store.state().explorer.rows.is_empty();
        self.last_editor_areas.clear();
        self.last_editor_areas.reserve(panes.min(2));
        self.last_editor_inner_areas.clear();
        self.last_editor_inner_areas.reserve(panes.min(2));
        self.last_editor_content_sizes.resize_with(panes, || (0, 0));
        self.last_editor_container_area = (!area.is_empty()).then_some(area);
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
                    push_editor_area_node(&mut self.ui_tree, 0, &layout);
                    push_editor_tab_nodes(
                        &mut self.ui_tree,
                        0,
                        &layout,
                        pane_state,
                        hovered_for_pane(0),
                    );
                    push_editor_split_drop_zones(&mut self.ui_tree, 0, &layout);
                    self.draw_editor_pane(backend, &layout, pane_state, hovered_for_pane(0), workspace_empty);
                }
            }
            2 => {
                let direction = self.store.state().ui.editor_layout.split_direction;
                match direction {
                    SplitDirection::Vertical => {
                        let available = area.w;
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
                                push_editor_area_node(&mut self.ui_tree, active, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    active,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(active),
                                );
                                self.draw_editor_pane(
                                    backend,
                                    &layout,
                                    pane_state,
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

                        let left_area = UiRect::new(area.x, area.y, left_width, area.h);
                        let sep_area = UiRect::new(area.x + left_width, area.y, 1, area.h);
                        let right_area =
                            UiRect::new(area.x + left_width + 1, area.y, right_width, area.h);
                        self.register_editor_splitter_node(sep_area, SplitDirection::Vertical);

                        self.last_editor_areas.push(left_area);
                        self.last_editor_areas.push(right_area);

                        // Split separator: avoid box borders (more nvim-like), just paint a 1-cell bar.
                        if !sep_area.is_empty() {
                            let mut painter = Painter::new();
                            let style = UiStyle::default()
                                .fg(self.ui_theme.separator)
                                .bg(self.ui_theme.palette_bg);
                            for dx in 0..sep_area.w {
                                painter.vline(
                                    Pos::new(sep_area.x.saturating_add(dx), sep_area.y),
                                    sep_area.h,
                                    '│',
                                    style,
                                );
                            }
                            backend.draw(sep_area, painter.cmds());
                        }

                        let left_inner = left_area;
                        self.last_editor_inner_areas.push(left_inner);
                        if !left_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(0) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(left_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(0, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(0) {
                                push_editor_area_node(&mut self.ui_tree, 0, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    0,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(0),
                                );
                                self.draw_editor_pane(
                                    backend,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(0),
                                    workspace_empty,
                                );
                            }
                        }

                        let right_inner = right_area;
                        self.last_editor_inner_areas.push(right_inner);
                        if !right_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(1) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(right_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(1, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(1) {
                                push_editor_area_node(&mut self.ui_tree, 1, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    1,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(1),
                                );
                                self.draw_editor_pane(
                                    backend,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(1),
                                    workspace_empty,
                                );
                            }
                        }
                    }
                    SplitDirection::Horizontal => {
                        let available = area.h;
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
                                push_editor_area_node(&mut self.ui_tree, active, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    active,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(active),
                                );
                                self.draw_editor_pane(
                                    backend,
                                    &layout,
                                    pane_state,
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

                        let top_area = UiRect::new(area.x, area.y, area.w, top_height);
                        let sep_area = UiRect::new(area.x, area.y + top_height, area.w, 1);
                        let bottom_area =
                            UiRect::new(area.x, area.y + top_height + 1, area.w, bottom_height);
                        self.register_editor_splitter_node(sep_area, SplitDirection::Horizontal);

                        self.last_editor_areas.push(top_area);
                        self.last_editor_areas.push(bottom_area);

                        // Split separator: avoid box borders (more nvim-like), just paint a 1-cell bar.
                        if !sep_area.is_empty() {
                            let mut painter = Painter::new();
                            let style = UiStyle::default()
                                .fg(self.ui_theme.separator)
                                .bg(self.ui_theme.palette_bg);
                            for dy in 0..sep_area.h {
                                painter.hline(
                                    Pos::new(sep_area.x, sep_area.y.saturating_add(dy)),
                                    sep_area.w,
                                    '─',
                                    style,
                                );
                            }
                            backend.draw(sep_area, painter.cmds());
                        }

                        let top_inner = top_area;
                        self.last_editor_inner_areas.push(top_inner);
                        if !top_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(0) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(top_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(0, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(0) {
                                push_editor_area_node(&mut self.ui_tree, 0, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    0,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(0),
                                );
                                self.draw_editor_pane(
                                    backend,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(0),
                                    workspace_empty,
                                );
                            }
                        }

                        let bottom_inner = bottom_area;
                        self.last_editor_inner_areas.push(bottom_inner);
                        if !bottom_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(1) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(bottom_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(1, &layout);
                            if let Some(pane_state) = self.store.state().editor.pane(1) {
                                push_editor_area_node(&mut self.ui_tree, 1, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    1,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(1),
                                );
                                self.draw_editor_pane(
                                    backend,
                                    &layout,
                                    pane_state,
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
                    push_editor_area_node(&mut self.ui_tree, active, &layout);
                    push_editor_tab_nodes(
                        &mut self.ui_tree,
                        active,
                        &layout,
                        pane_state,
                        hovered_for_pane(active),
                    );
                    self.draw_editor_pane(
                        backend,
                        &layout,
                        pane_state,
                        hovered_for_pane(active),
                        workspace_empty,
                    );
                }
            }
        }
    }
}

fn wrap_lines(lines: &[&str], width: u16, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    for line in lines {
        let mut s = line.trim();
        while !s.is_empty() && out.len() < max_lines {
            let end = crate::core::text_window::truncate_to_width(s, width as usize);
            if end == 0 {
                break;
            }
            out.push(s[..end].to_string());
            s = s[end..].trim_start();
        }
        if out.len() >= max_lines {
            break;
        }
    }
    out
}

fn push_editor_area_node(
    ui_tree: &mut crate::ui::core::tree::UiTree,
    pane: usize,
    layout: &EditorPaneLayout,
) {
    let area = layout.area;
    if area.is_empty() {
        return;
    }

    let id = IdPath::root("workbench")
        .push_str("editor_area")
        .push_u64(pane as u64)
        .finish();
    ui_tree.push(Node {
        id,
        rect: area,
        layer: 0,
        z: 0,
        sense: Sense::DROP_TARGET | Sense::CONTEXT_MENU,
        kind: NodeKind::EditorArea { pane },
    });
}

fn push_editor_split_drop_zones(
    ui_tree: &mut crate::ui::core::tree::UiTree,
    pane: usize,
    layout: &EditorPaneLayout,
) {
    let area = layout.editor_area;
    if area.is_empty() {
        return;
    }

    // Drag-to-split zones (VSCode-like). This is intentionally coarse and only used as a drop
    // target while dragging a tab.
    let right_w = area.w.saturating_div(3).max(10).min(area.w);
    let down_h = area.h.saturating_div(3).max(4).min(area.h);
    if right_w == 0 || down_h == 0 {
        return;
    }

    let down = UiRect::new(area.x, area.bottom().saturating_sub(down_h), area.w, down_h);
    if !down.is_empty() {
        let id = IdPath::root("workbench")
            .push_str("editor_split_drop")
            .push_u64(pane as u64)
            .push_str("down")
            .finish();
        ui_tree.push(Node {
            id,
            rect: down,
            layer: 1,
            z: 0,
            sense: Sense::DROP_TARGET,
            kind: NodeKind::EditorSplitDrop {
                pane,
                drop: SplitDrop::Down,
            },
        });
    }

    let right = UiRect::new(area.right().saturating_sub(right_w), area.y, right_w, area.h);
    if !right.is_empty() {
        let id = IdPath::root("workbench")
            .push_str("editor_split_drop")
            .push_u64(pane as u64)
            .push_str("right")
            .finish();
        // Push after `down` so it wins in the overlapping bottom-right region.
        ui_tree.push(Node {
            id,
            rect: right,
            layer: 1,
            z: 0,
            sense: Sense::DROP_TARGET,
            kind: NodeKind::EditorSplitDrop {
                pane,
                drop: SplitDrop::Right,
            },
        });
    }
}

fn push_editor_tab_nodes(
    ui_tree: &mut crate::ui::core::tree::UiTree,
    pane: usize,
    layout: &EditorPaneLayout,
    pane_state: &EditorPaneState,
    hovered_tab: Option<usize>,
) {
    let area = layout.tab_area;
    if area.is_empty() {
        return;
    }

    // Tab bar drop target (needed for cross-pane tab moves).
    let tabbar_id = IdPath::root("workbench")
        .push_str("tabbar")
        .push_u64(pane as u64)
        .finish();
    ui_tree.push(Node {
        id: tabbar_id,
        rect: area,
        layer: 0,
        z: 0,
        sense: Sense::DROP_TARGET,
        kind: NodeKind::TabBar { pane },
    });

    const PADDING_LEFT: u16 = 1;
    const PADDING_RIGHT: u16 = 1;
    const CLOSE_BUTTON_WIDTH: u16 = 2;
    const DIVIDER: u16 = 1;

    let right = area.right();
    let mut x = area.x;

    for (i, tab) in pane_state.tabs.iter().enumerate() {
        if x >= right {
            break;
        }

        let start = x;
        x = x.saturating_add(PADDING_LEFT).min(right);

        let mut title_width = UnicodeWidthStr::width(tab.title.as_str());
        if tab.dirty {
            title_width = title_width.saturating_add(2);
        }
        x = x
            .saturating_add(title_width.min(u16::MAX as usize) as u16)
            .min(right);

        x = x.saturating_add(PADDING_RIGHT).min(right);

        let close_start = x;
        if hovered_tab == Some(i) {
            x = x.saturating_add(CLOSE_BUTTON_WIDTH).min(right);
        }
        let end = x;

        // Avoid making the close button draggable: if it is visible, exclude it from the tab's
        // interactive rect.
        let tab_end = if hovered_tab == Some(i) { close_start } else { end };
        if tab_end > start {
            let node_id = IdPath::root("workbench")
                .push_str("tab")
                .push_u64(pane as u64)
                .push_u64(tab.id.raw())
                .finish();
            ui_tree.push(Node {
                id: node_id,
                rect: UiRect::new(start, area.y, tab_end - start, area.h),
                layer: 0,
                z: 0,
                sense: Sense::CLICK | Sense::DRAG_SOURCE | Sense::CONTEXT_MENU,
                kind: NodeKind::Tab {
                    pane,
                    tab_id: tab.id,
                },
            });
        }

        if i + 1 == pane_state.tabs.len() {
            break;
        }

        x = x.saturating_add(DIVIDER).min(right);
    }
}
