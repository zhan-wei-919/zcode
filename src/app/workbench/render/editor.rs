use super::super::Workbench;
use super::layout::{ThinHSeparator, ThinVSeparator};
use crate::kernel::SplitDirection;
use crate::views::{compute_editor_pane_layout, cursor_position_editor, render_editor_pane};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

impl Workbench {
    pub(super) fn render_hover_popup(&self, frame: &mut Frame, area: Rect) {
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

    pub(super) fn render_signature_help_popup(&self, frame: &mut Frame, area: Rect) {
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

    pub(super) fn render_completion_popup(&self, frame: &mut Frame, area: Rect) {
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

    pub(super) fn render_editor_panes(&mut self, frame: &mut Frame, area: Rect) {
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
}
