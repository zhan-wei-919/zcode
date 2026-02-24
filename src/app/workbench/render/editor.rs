use super::super::{CompletionDocKey, Workbench};
use crate::kernel::editor::EditorPaneState;
use crate::kernel::SplitDirection;
use crate::ui::backend::Backend;
use crate::ui::core::geom::Pos;
use crate::ui::core::geom::Rect as UiRect;
use crate::ui::core::id::IdPath;
use crate::ui::core::layout::Insets;
use crate::ui::core::painter::Painter;
use crate::ui::core::style::Style as UiStyle;
use crate::ui::core::tree::{Axis, Node, NodeKind, Sense, SplitDrop};
use crate::views::doc;
use crate::views::editor::markdown::MarkdownDocument;
use crate::views::{
    compute_editor_pane_layout, compute_tab_row_layout, cursor_position_editor, paint_editor_pane,
    EditorPaneLayout, EditorPaneRenderOptions, TransientRowHighlight,
};
use unicode_width::UnicodeWidthStr;

pub(super) const MAX_DOC_RENDER_LINES: usize = doc::MAX_RENDER_LINES;

impl Workbench {
    fn register_editor_splitter_node(&mut self, sep_area: UiRect, direction: SplitDirection) {
        if sep_area.is_empty() {
            return;
        }
        let axis = match direction {
            SplitDirection::Vertical => Axis::Vertical,
            SplitDirection::Horizontal => Axis::Horizontal,
        };
        let id = IdPath::root("workbench")
            .push_str("editor_splitter")
            .finish();
        self.ui_tree.push(Node {
            id,
            rect: sep_area,
            layer: 0,
            z: 0,
            sense: Sense::HOVER | Sense::DRAG_SOURCE,
            kind: NodeKind::Splitter { axis },
        });
    }

    pub(super) fn paint_hover_popup(&mut self, painter: &mut Painter, area: UiRect) {
        self.hover_popup.last_area = None;
        self.hover_popup.total_lines = 0;

        let has_text = self
            .store
            .state()
            .ui
            .hover_message
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty());
        if !has_text {
            self.hover_popup.scroll = 0;
            self.hover_popup.render_cache.clear();
            return;
        }

        let active_pane = self.store.state().ui.editor_layout.active_pane;
        let pane_area = match self.layout_cache.editor_areas.get(active_pane) {
            Some(area) => *area,
            None => return,
        };
        let Some(pane_state) = self.store.state().editor.pane(active_pane) else {
            return;
        };
        let tab_size = self.store.state().editor.config.tab_size;
        let layout = {
            let config = &self.store.state().editor.config;
            compute_editor_pane_layout(pane_area, pane_state, config)
        };
        let (cx, cy) = if let Some((x, y)) = self.hover_popup.last_anchor {
            (x, y)
        } else {
            let config = &self.store.state().editor.config;
            let Some((cx, cy)) = cursor_position_editor(&layout, pane_state, config) else {
                return;
            };
            (cx, cy)
        };

        if area.is_empty() {
            return;
        }

        if area.w < 3 || area.h < 3 {
            return;
        }

        let (cache_key, inner_w, rendered, cache_hit) = {
            let text = self
                .store
                .state()
                .ui
                .hover_message
                .as_deref()
                .unwrap_or_default();

            let natural_w = doc::natural_width_with_tab_size(text, tab_size);
            let max_inner_w = area.w.saturating_sub(2).max(1);
            let inner_w = (natural_w.min(u16::MAX as usize) as u16)
                .clamp(1, 120)
                .min(max_inner_w);
            let (key, rendered, hit) =
                self.hover_popup
                    .render_cache
                    .get_or_render(text, inner_w, MAX_DOC_RENDER_LINES);
            (key, inner_w, rendered, hit)
        };

        let total_lines = rendered.len();
        if !cache_hit {
            self.reset_hover_popup_scroll();
        }
        debug_assert_eq!(cache_key.width, inner_w);
        self.hover_popup.total_lines = total_lines;

        let desired_width = inner_w.saturating_add(2).max(3);

        // Allow a generous popup height; content beyond the viewport is scrollable.
        let max_height = (area.h * 2 / 3).max(5).min(area.h);
        if max_height < 3 {
            return;
        }
        let desired_height = total_lines.saturating_add(2).min(u16::MAX as usize).max(3) as u16;
        let height = desired_height.min(max_height);
        let width = desired_width.min(area.w).max(3);

        let right = area.right();
        let mut x = cx;
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }
        if x < area.x {
            x = area.x;
        }
        let below = cy.saturating_add(1);
        let bottom = area.bottom();
        let avail_below = bottom.saturating_sub(below);
        let avail_above = cy.saturating_sub(area.y);
        let can_below = avail_below >= 3;
        let can_above = avail_above >= 3;
        let place_below = match (can_below, can_above) {
            (true, true) => avail_below >= avail_above,
            (true, false) => true,
            (false, true) => false,
            (false, false) => return,
        };
        let avail_h = if place_below {
            avail_below
        } else {
            avail_above
        };
        let height = height.min(avail_h);

        let mut y = if place_below {
            below
        } else {
            cy.saturating_sub(height)
        };
        if y < area.y {
            y = area.y;
        }
        if y.saturating_add(height) > area.bottom() {
            y = area.bottom().saturating_sub(height);
        }

        let popup_area = UiRect::new(x, y, width, height);
        self.hover_popup.last_area = Some(popup_area);

        let base_style = UiStyle::default()
            .bg(self.ui_theme.popup_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(popup_area, base_style);

        let inner = popup_area.inset(Insets::all(1));
        if inner.is_empty() {
            return;
        }

        let view_h = inner.h as usize;
        self.hover_popup.scroll =
            doc::clamp_scroll_offset(self.hover_popup.scroll, total_lines, view_h);
        doc::paint_doc_lines(
            painter,
            inner,
            rendered.as_slice(),
            &self.ui_theme,
            base_style,
            self.hover_popup.scroll,
            tab_size,
        );
    }

    pub(super) fn paint_signature_help_popup(&self, painter: &mut Painter, area: UiRect) {
        let signature_help = &self.store.state().ui.signature_help;
        if !signature_help.visible || signature_help.text.trim().is_empty() {
            return;
        }

        let active_pane = self.store.state().ui.editor_layout.active_pane;
        let pane_area = match self.layout_cache.editor_areas.get(active_pane) {
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
            .bg(self.ui_theme.popup_bg)
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

    pub(super) fn paint_completion_popup(&mut self, painter: &mut Painter, area: UiRect) {
        self.completion_doc.last_area = None;
        self.completion_doc.total_lines = 0;

        let completion = &self.store.state().ui.completion;
        if !completion.visible || completion.visible_len() == 0 {
            self.completion_doc.scroll = 0;
            self.completion_doc.key = None;
            self.completion_doc.render_cache.clear();
            return;
        }

        let active_pane = self.store.state().ui.editor_layout.active_pane;
        let pane = completion
            .request
            .as_ref()
            .map(|req| req.pane)
            .unwrap_or(active_pane);
        let pane_area = match self
            .layout_cache
            .editor_areas
            .get(pane)
            .or_else(|| self.layout_cache.editor_areas.get(active_pane))
        {
            Some(area) => *area,
            None => return,
        };
        let Some(pane_state) = self.store.state().editor.pane(pane) else {
            return;
        };
        let tab_size = self.store.state().editor.config.tab_size;
        let layout = {
            let config = &self.store.state().editor.config;
            compute_editor_pane_layout(pane_area, pane_state, config)
        };
        let config = &self.store.state().editor.config;
        let Some((cx, cy)) = cursor_position_editor(&layout, pane_state, config) else {
            return;
        };

        let max_items = 8usize;
        let selected = completion
            .selected
            .min(completion.visible_len().saturating_sub(1));
        let mut start = 0usize;
        if selected >= max_items {
            start = selected + 1 - max_items;
        }
        let end = (start + max_items).min(completion.visible_len());

        let mut rows = Vec::with_capacity(end.saturating_sub(start));
        let mut max_inner_width = 1usize;
        for i in start..end {
            let Some(item) = completion.visible_item(i) else {
                continue;
            };
            let is_selected = i == selected;
            let marker = if is_selected { ">" } else { " " };

            let text = item.label.as_str();
            let mut detail = String::new();
            if let Some(d) = item.detail.as_deref() {
                if !d.trim().is_empty() {
                    detail = d.to_string();
                }
            }
            // When detail is empty and insert_text differs from label, show
            // a simplified insert_text so the user can distinguish items with
            // the same label (e.g. Java's two "class" completions).
            if detail.is_empty() && item.insert_text != text {
                let preview = strip_snippet_markers(&item.insert_text);
                if preview != text {
                    detail = preview;
                }
            }

            let width = if detail.is_empty() {
                UnicodeWidthStr::width(text)
            } else {
                UnicodeWidthStr::width(text)
                    .saturating_add(1 + UnicodeWidthStr::width(detail.as_str()))
            };
            // marker + space + text + optional (space + detail)
            let inner_w = 2usize.saturating_add(width);
            max_inner_width = max_inner_width.max(inner_w);
            rows.push((is_selected, marker, text.to_string(), detail));
        }

        if area.is_empty() {
            return;
        }

        let desired_width = max_inner_width;
        let desired_height = rows.len();

        let width = desired_width.max(6).min(area.w as usize).max(1) as u16;
        let height = desired_height.max(1).min(area.h as usize).max(1) as u16;

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
            .bg(self.ui_theme.popup_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(popup_area, base_style);

        let inner = popup_area;
        if inner.is_empty() {
            return;
        }

        let selected_bg = UiStyle::default().bg(self.ui_theme.palette_selected_bg);
        let marker_selected = UiStyle::default()
            .fg(self.ui_theme.focus_border)
            .bg(self.ui_theme.palette_selected_bg);
        let marker_normal = UiStyle::default()
            .fg(self.ui_theme.palette_muted_fg)
            .bg(self.ui_theme.popup_bg);
        let label_style_normal = UiStyle::default()
            .fg(self.ui_theme.palette_fg)
            .bg(self.ui_theme.popup_bg);
        let label_style_selected = UiStyle::default()
            .fg(self.ui_theme.palette_fg)
            .bg(self.ui_theme.palette_selected_bg);
        let detail_style_normal = UiStyle::default()
            .fg(self.ui_theme.palette_muted_fg)
            .bg(self.ui_theme.popup_bg);
        let detail_style_selected = UiStyle::default()
            .fg(self.ui_theme.palette_muted_fg)
            .bg(self.ui_theme.palette_selected_bg);

        for (idx, (is_selected, marker, label, detail)) in rows.into_iter().enumerate() {
            let y = inner.y.saturating_add(idx.min(u16::MAX as usize) as u16);
            if y >= inner.bottom() {
                break;
            }
            let row_area = UiRect::new(inner.x, y, inner.w, 1);
            painter.fill_rect(row_area, base_style);
            if is_selected {
                painter.fill_rect(row_area, selected_bg);
            }

            let (label_style, detail_style) = if is_selected {
                (label_style_selected, detail_style_selected)
            } else {
                (label_style_normal, detail_style_normal)
            };

            let mut x = inner.x;
            let marker_style = if is_selected {
                marker_selected
            } else {
                marker_normal
            };
            painter.text_clipped(Pos::new(x, y), marker, marker_style, row_area);
            x = x.saturating_add(1);
            painter.text_clipped(
                Pos::new(x, y),
                " ",
                if is_selected { selected_bg } else { base_style },
                row_area,
            );
            x = x.saturating_add(1);
            let label_w = label.width().min(u16::MAX as usize) as u16;
            painter.text_clipped(Pos::new(x, y), label, label_style, row_area);
            x = x.saturating_add(label_w);
            if !detail.trim().is_empty() {
                painter.text_clipped(
                    Pos::new(x, y),
                    " ",
                    if is_selected { selected_bg } else { base_style },
                    row_area,
                );
                x = x.saturating_add(1);
                painter.text_clipped(Pos::new(x, y), detail, detail_style, row_area);
            }
        }

        let doc_key = completion.request.as_ref().map(|req| CompletionDocKey {
            pane: req.pane,
            path: req.path.clone(),
            version: req.version,
            selected,
        });
        if self.completion_doc.key.as_ref() != doc_key.as_ref() {
            self.completion_doc.scroll = 0;
            self.completion_doc.render_cache.clear();
        }
        self.completion_doc.key = doc_key;

        // Documentation panel (Helix-like): show docs for the currently selected item.
        let doc_text = completion.visible_item(selected).and_then(|item| {
            item.documentation
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| item.detail.as_deref().filter(|s| !s.trim().is_empty()))
        });

        if let Some(doc_text) = doc_text {
            let natural_w = doc::natural_width_with_tab_size(doc_text, tab_size);

            let max_area = completion_doc_area(area, popup_area, cy, 30);
            let Some(mut doc_area_max) = max_area else {
                return;
            };
            if doc_area_max.w < 3 || doc_area_max.h < 3 {
                return;
            }

            // Prefer a narrow doc panel: shrink to natural width (capped) rather than using
            // all available space. Long docs scroll instead of taking over the screen.
            let avail_inner_w = doc_area_max.w.max(1);
            let inner_w = (natural_w.min(u16::MAX as usize) as u16)
                .clamp(1, 120)
                .min(avail_inner_w);

            let (_cache_key, rendered, _cache_hit) = self
                .completion_doc
                .render_cache
                .get_or_render(doc_text, inner_w, MAX_DOC_RENDER_LINES);
            let total_lines = rendered.len();
            self.completion_doc.total_lines = total_lines;

            let desired_h = total_lines.min(u16::MAX as usize).max(1) as u16;
            let desired_w = inner_w.max(1);

            doc_area_max.w = desired_w.min(doc_area_max.w);
            doc_area_max.h = desired_h.min(doc_area_max.h);

            // Align above/below doc popups with the completion list rather than full screen.
            let place_side = doc_area_max.x == popup_area.right();
            if !place_side {
                let mut x = popup_area.x;
                if x.saturating_add(doc_area_max.w) > area.right() {
                    x = area.right().saturating_sub(doc_area_max.w);
                }
                if x < area.x {
                    x = area.x;
                }
                doc_area_max.x = x;
            }

            // Draw separator between completion list and side doc panel.
            if place_side && doc_area_max.w > 1 {
                let sep_style = UiStyle::default()
                    .fg(self.ui_theme.separator)
                    .bg(self.ui_theme.popup_bg);
                for row in 0..popup_area.h {
                    let sy = popup_area.y.saturating_add(row);
                    if sy >= area.bottom() {
                        break;
                    }
                    let clip = UiRect::new(popup_area.right(), sy, 1, 1);
                    painter.text_clipped(Pos::new(popup_area.right(), sy), "│", sep_style, clip);
                }
                doc_area_max.x = doc_area_max.x.saturating_add(1);
                doc_area_max.w = doc_area_max.w.saturating_sub(1);
            }

            painter.fill_rect(doc_area_max, base_style);

            self.completion_doc.last_area = Some(doc_area_max);

            let inner = doc_area_max;
            if inner.is_empty() {
                return;
            }

            let view_h = inner.h as usize;
            self.completion_doc.scroll =
                doc::clamp_scroll_offset(self.completion_doc.scroll, total_lines, view_h);
            doc::paint_doc_lines(
                painter,
                inner,
                rendered.as_slice(),
                &self.ui_theme,
                base_style,
                self.completion_doc.scroll,
                tab_size,
            );
        } else {
            self.completion_doc.render_cache.clear();
        }
    }

    fn draw_editor_pane(
        &self,
        backend: &mut dyn Backend,
        pane: usize,
        layout: &EditorPaneLayout,
        pane_state: &EditorPaneState,
        markdown: Option<&MarkdownDocument>,
        mut options: EditorPaneRenderOptions,
    ) {
        let mut painter = Painter::new();
        let config = &self.store.state().editor.config;
        options.show_vertical_scrollbar = self.show_editor_vertical_scrollbar(pane, layout);
        paint_editor_pane(
            &mut painter,
            layout,
            pane_state,
            config,
            &self.ui_theme,
            options,
            markdown,
        );

        backend.draw(layout.area, painter.cmds());
    }

    fn show_editor_vertical_scrollbar(&self, pane: usize, layout: &EditorPaneLayout) -> bool {
        if layout.v_scrollbar_area.is_none() {
            return false;
        }

        if self
            .editor_scrollbar_drag
            .is_some_and(|drag| drag.pane == pane)
        {
            return true;
        }

        self.store.state().ui.focus == crate::kernel::FocusTarget::Editor
            && self.editor_scrollbar_hover == Some(pane)
    }

    fn definition_jump_row_highlight_for_pane(&self, pane: usize) -> Option<TransientRowHighlight> {
        let highlight = self.definition_jump_highlight?;
        if highlight.pane != pane {
            return None;
        }

        let tab_id = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane_state| pane_state.active_tab())
            .map(|tab| tab.id)?;
        if tab_id != highlight.tab_id {
            return None;
        }

        let elapsed = highlight.started_at.elapsed();
        if elapsed >= super::super::DEFINITION_JUMP_HIGHLIGHT_DURATION {
            return None;
        }

        Some(TransientRowHighlight { row: highlight.row })
    }

    pub(super) fn render_editor_panes(&mut self, backend: &mut dyn Backend, area: UiRect) {
        let panes = self.store.state().ui.editor_layout.panes.max(1);
        let hovered = self.store.state().ui.hovered_tab;
        let workspace_empty = self.store.state().explorer.rows.is_empty();
        self.layout_cache.editor_areas.clear();
        self.layout_cache.editor_areas.reserve(panes.min(2));
        self.layout_cache.editor_inner_areas.clear();
        self.layout_cache.editor_inner_areas.reserve(panes.min(2));
        self.viewport_cache
            .editor_content_sizes
            .resize_with(panes, || (0, 0));
        self.viewport_cache
            .applied_editor_content_sizes
            .resize_with(panes, || (0, 0));
        self.layout_cache.editor_container_area = (!area.is_empty()).then_some(area);
        let hovered_for_pane = |p: usize| hovered.filter(|(hp, _)| *hp == p).map(|(_, i)| i);
        let transient_row_highlights = (0..panes)
            .map(|p| self.definition_jump_row_highlight_for_pane(p))
            .collect::<Vec<_>>();
        let pane_options = |p: usize| EditorPaneRenderOptions {
            hovered_tab: hovered_for_pane(p),
            workspace_empty,
            show_vertical_scrollbar: false,
            transient_row_highlight: transient_row_highlights.get(p).copied().flatten(),
        };

        match panes {
            1 => {
                self.editor_split_dragging = false;
                self.layout_cache.editor_areas.push(area);
                self.layout_cache.editor_inner_areas.push(area);

                let layout = {
                    let Some(pane_state) = self.store.state().editor.pane(0) else {
                        return;
                    };
                    let config = &self.store.state().editor.config;
                    compute_editor_pane_layout(area, pane_state, config)
                };
                self.sync_editor_viewport_size(0, &layout);
                let md_tab_id = self.ensure_markdown_view_for_active_tab(0);
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
                    let markdown = md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                    self.draw_editor_pane(
                        backend,
                        0,
                        &layout,
                        pane_state,
                        markdown,
                        pane_options(0),
                    );
                }
            }
            2 => {
                let direction = self.store.state().ui.editor_layout.split_direction;
                match direction {
                    SplitDirection::Vertical => {
                        let available = area.w;
                        if available < 3 {
                            self.editor_split_dragging = false;
                            self.layout_cache.editor_areas.push(area);
                            self.layout_cache.editor_inner_areas.push(area);

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
                            let md_tab_id = self.ensure_markdown_view_for_active_tab(active);
                            if let Some(pane_state) = self.store.state().editor.pane(active) {
                                push_editor_area_node(&mut self.ui_tree, active, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    active,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(active),
                                );
                                let markdown =
                                    md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                self.draw_editor_pane(
                                    backend,
                                    active,
                                    &layout,
                                    pane_state,
                                    markdown,
                                    pane_options(active),
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

                        self.layout_cache.editor_areas.push(left_area);
                        self.layout_cache.editor_areas.push(right_area);

                        // Split separator: avoid box borders (more nvim-like), just paint a 1-cell bar.
                        if !sep_area.is_empty() {
                            let mut painter = Painter::new();
                            let style = UiStyle::default()
                                .fg(self.ui_theme.separator)
                                .bg(self.ui_theme.editor_bg);
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
                        self.layout_cache.editor_inner_areas.push(left_inner);
                        if !left_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(0) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(left_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(0, &layout);
                            let md_tab_id = self.ensure_markdown_view_for_active_tab(0);
                            if let Some(pane_state) = self.store.state().editor.pane(0) {
                                push_editor_area_node(&mut self.ui_tree, 0, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    0,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(0),
                                );
                                let markdown =
                                    md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                self.draw_editor_pane(
                                    backend,
                                    0,
                                    &layout,
                                    pane_state,
                                    markdown,
                                    pane_options(0),
                                );
                            }
                        }

                        let right_inner = right_area;
                        self.layout_cache.editor_inner_areas.push(right_inner);
                        if !right_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(1) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(right_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(1, &layout);
                            let md_tab_id = self.ensure_markdown_view_for_active_tab(1);
                            if let Some(pane_state) = self.store.state().editor.pane(1) {
                                push_editor_area_node(&mut self.ui_tree, 1, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    1,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(1),
                                );
                                let markdown =
                                    md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                self.draw_editor_pane(
                                    backend,
                                    1,
                                    &layout,
                                    pane_state,
                                    markdown,
                                    pane_options(1),
                                );
                            }
                        }
                    }
                    SplitDirection::Horizontal => {
                        let available = area.h;
                        if available < 3 {
                            self.editor_split_dragging = false;
                            self.layout_cache.editor_areas.push(area);
                            self.layout_cache.editor_inner_areas.push(area);

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
                            let md_tab_id = self.ensure_markdown_view_for_active_tab(active);
                            if let Some(pane_state) = self.store.state().editor.pane(active) {
                                push_editor_area_node(&mut self.ui_tree, active, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    active,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(active),
                                );
                                let markdown =
                                    md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                self.draw_editor_pane(
                                    backend,
                                    active,
                                    &layout,
                                    pane_state,
                                    markdown,
                                    pane_options(active),
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

                        self.layout_cache.editor_areas.push(top_area);
                        self.layout_cache.editor_areas.push(bottom_area);

                        // Split separator: avoid box borders (more nvim-like), just paint a 1-cell bar.
                        if !sep_area.is_empty() {
                            let mut painter = Painter::new();
                            let style = UiStyle::default()
                                .fg(self.ui_theme.separator)
                                .bg(self.ui_theme.editor_bg);
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
                        self.layout_cache.editor_inner_areas.push(top_inner);
                        if !top_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(0) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(top_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(0, &layout);
                            let md_tab_id = self.ensure_markdown_view_for_active_tab(0);
                            if let Some(pane_state) = self.store.state().editor.pane(0) {
                                push_editor_area_node(&mut self.ui_tree, 0, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    0,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(0),
                                );
                                let markdown =
                                    md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                self.draw_editor_pane(
                                    backend,
                                    0,
                                    &layout,
                                    pane_state,
                                    markdown,
                                    pane_options(0),
                                );
                            }
                        }

                        let bottom_inner = bottom_area;
                        self.layout_cache.editor_inner_areas.push(bottom_inner);
                        if !bottom_inner.is_empty() {
                            let layout = {
                                let Some(pane_state) = self.store.state().editor.pane(1) else {
                                    return;
                                };
                                let config = &self.store.state().editor.config;
                                compute_editor_pane_layout(bottom_inner, pane_state, config)
                            };
                            self.sync_editor_viewport_size(1, &layout);
                            let md_tab_id = self.ensure_markdown_view_for_active_tab(1);
                            if let Some(pane_state) = self.store.state().editor.pane(1) {
                                push_editor_area_node(&mut self.ui_tree, 1, &layout);
                                push_editor_tab_nodes(
                                    &mut self.ui_tree,
                                    1,
                                    &layout,
                                    pane_state,
                                    hovered_for_pane(1),
                                );
                                let markdown =
                                    md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                                self.draw_editor_pane(
                                    backend,
                                    1,
                                    &layout,
                                    pane_state,
                                    markdown,
                                    pane_options(1),
                                );
                            }
                        }
                    }
                }
            }
            _ => {
                self.editor_split_dragging = false;
                self.layout_cache.editor_areas.push(area);
                self.layout_cache.editor_inner_areas.push(area);

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
                let md_tab_id = self.ensure_markdown_view_for_active_tab(active);
                if let Some(pane_state) = self.store.state().editor.pane(active) {
                    push_editor_area_node(&mut self.ui_tree, active, &layout);
                    push_editor_tab_nodes(
                        &mut self.ui_tree,
                        active,
                        &layout,
                        pane_state,
                        hovered_for_pane(active),
                    );
                    let markdown = md_tab_id.and_then(|tab_id| self.markdown_doc_for_tab(tab_id));
                    self.draw_editor_pane(
                        backend,
                        active,
                        &layout,
                        pane_state,
                        markdown,
                        pane_options(active),
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

fn completion_doc_area(
    screen: UiRect,
    popup: UiRect,
    cursor_y: u16,
    side_threshold: u16,
) -> Option<UiRect> {
    if screen.is_empty() || popup.is_empty() {
        return None;
    }

    let right_avail = screen.right().saturating_sub(popup.right());
    if right_avail > side_threshold {
        let x = popup.right();
        let y = popup.y;
        let w = right_avail;
        // Keep the doc panel bounded to the completion popup height (Helix-like). Long docs are
        // scrollable; we don't want the popup to take over the whole screen.
        let h = popup.h;
        let area = UiRect::new(x, y, w, h);
        return (!area.is_empty()).then_some(area);
    }

    let top = screen.y;
    let bottom = screen.bottom();
    let popup_top = popup.y;
    let popup_bottom = popup.bottom();

    // Documentation should not cover the cursor or the completion popup.
    let avail_above = cursor_y
        .min(popup_top)
        .saturating_sub(top)
        .saturating_sub(1);
    let avail_below = bottom
        .saturating_sub(cursor_y.max(popup_bottom))
        .saturating_sub(1);

    let place_below = avail_below >= avail_above;
    let avail_h = if place_below {
        avail_below
    } else {
        avail_above
    };

    if avail_h <= 1 {
        return None;
    }

    let h = avail_h.min(15);
    let y = if place_below {
        bottom.saturating_sub(avail_below)
    } else {
        // Anchor the doc panel to the completion popup/cursor instead of sticking to the top of
        // the screen when there is extra space.
        top.saturating_add(avail_above).saturating_sub(h)
    };
    let area = UiRect::new(screen.x, y, screen.w, h);
    (!area.is_empty()).then_some(area)
}

/// Strip snippet placeholders like `${1:text}`, `$0`, etc. to produce a
/// human-readable single-line preview of the insert text.
fn strip_snippet_markers(snippet: &str) -> String {
    let mut out = String::with_capacity(snippet.len());
    let bytes = snippet.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                // ${N:text} or ${N} — extract the text part if present.
                if let Some(close) = snippet[i..].find('}') {
                    let inner = &snippet[i + 2..i + close];
                    if let Some(colon) = inner.find(':') {
                        out.push_str(&inner[colon + 1..]);
                    }
                    i += close + 1;
                    continue;
                }
            } else if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                // $N — skip.
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                continue;
            }
        }
        // Collapse newlines and tabs into a single space.
        if bytes[i] == b'\n' || bytes[i] == b'\r' || bytes[i] == b'\t' {
            if !out.ends_with(' ') {
                out.push(' ');
            }
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    // Collapse runs of multiple spaces.
    let mut result = String::with_capacity(out.len());
    let mut prev_space = false;
    for ch in out.chars() {
        if ch == ' ' {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(ch);
            prev_space = false;
        }
    }
    result.trim().to_string()
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

    let right = UiRect::new(
        area.right().saturating_sub(right_w),
        area.y,
        right_w,
        area.h,
    );
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
        sense: Sense::DROP_TARGET | Sense::CONTEXT_MENU,
        kind: NodeKind::TabBar { pane },
    });

    let row_layout = compute_tab_row_layout(area, pane_state, hovered_tab);
    for slot in row_layout.slots {
        let Some(tab) = pane_state.tabs.get(slot.index) else {
            continue;
        };
        if slot.hit_end <= slot.start {
            continue;
        }

        let node_id = IdPath::root("workbench")
            .push_str("tab")
            .push_u64(pane as u64)
            .push_u64(tab.id.raw())
            .finish();
        ui_tree.push(Node {
            id: node_id,
            rect: UiRect::new(slot.start, area.y, slot.hit_end - slot.start, area.h),
            layer: 0,
            z: 0,
            sense: Sense::CLICK | Sense::DRAG_SOURCE | Sense::CONTEXT_MENU,
            kind: NodeKind::Tab {
                pane,
                tab_id: tab.id.raw(),
            },
        });
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/app/workbench/render/editor.rs"]
mod tests;
