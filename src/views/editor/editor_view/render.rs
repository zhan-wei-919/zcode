use super::EditorView;
use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::models::slice_to_cow;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

impl View for EditorView {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        match event {
            InputEvent::Key(_) => EventResult::Ignored,
            InputEvent::Mouse(mouse_event) => self.handle_mouse(mouse_event),
            InputEvent::Paste(text) => {
                self.handle_paste(text);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let total_lines = self.buffer.len_lines();
        let max_line_width = total_lines.to_string().len();
        let gutter_width = (max_line_width + 2) as u16;

        let content_width = area.width.saturating_sub(gutter_width);
        let content_area = Rect::new(area.x + gutter_width, area.y, content_width, area.height);

        self.viewport.set_area(content_area);
        self.viewport
            .update(&self.buffer, area.height as usize, content_width as usize);

        let (visible_start, visible_end) = self.viewport.visible_range(total_lines);

        let gutter_lines: Vec<Line> = (visible_start..visible_end)
            .map(|i| {
                Line::from(Span::styled(
                    format!("{:>width$} ", i + 1, width = max_line_width),
                    Style::default().fg(self.theme.palette_muted_fg),
                ))
            })
            .collect();

        let gutter_area = Rect::new(area.x, area.y, gutter_width, area.height);
        let gutter_widget = Paragraph::new(gutter_lines);
        frame.render_widget(gutter_widget, gutter_area);

        let content_lines: Vec<Line> = (visible_start..visible_end)
            .map(|i| {
                if let Some(slice) = self.buffer.line_slice(i) {
                    let line_str = slice_to_cow(slice);
                    self.render_line(&line_str, i)
                } else {
                    Line::default()
                }
            })
            .collect();

        let content_widget = Paragraph::new(content_lines).block(Block::default());
        frame.render_widget(content_widget, content_area);
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        let area = self.viewport.area()?;
        let (row, _) = self.buffer.cursor();
        let offset = self.viewport.viewport_offset();

        if row < offset || row >= offset + self.viewport.viewport_height() {
            return None;
        }

        let x = area.x + self.viewport.get_cursor_display_x(&self.buffer);
        let y = area.y + (row - offset) as u16;

        Some((x, y))
    }
}

impl EditorView {
    fn render_line(&self, line_str: &str, row: usize) -> Line<'static> {
        let expanded = self.viewport.expand_tabs_cow(line_str);
        let graphemes: Vec<&str> = expanded.graphemes(true).collect();

        let selection = self.buffer.selection();
        let selection_range = selection.map(|s| s.range());

        if selection_range.is_none() {
            return self.render_line_plain(&graphemes);
        }

        let ((start_row, start_col), (end_row, end_col)) = selection_range.unwrap();

        if row < start_row || row > end_row {
            return self.render_line_plain(&graphemes);
        }

        let (sel_start, sel_end) = if row == start_row && row == end_row {
            (start_col, end_col)
        } else if row == start_row {
            (start_col, graphemes.len())
        } else if row == end_row {
            (0, end_col)
        } else {
            (0, graphemes.len())
        };

        self.render_line_with_selection(&graphemes, sel_start, sel_end)
    }

    fn render_line_plain(&self, graphemes: &[&str]) -> Line<'static> {
        let horiz = self.viewport.horiz_offset() as usize;
        let mut skip = 0;
        let mut acc = 0usize;

        for g in graphemes.iter() {
            if acc >= horiz {
                break;
            }
            acc += g.width();
            skip += 1;
        }

        let visible: String = graphemes.iter().skip(skip).copied().collect();
        Line::from(visible)
    }

    fn render_line_with_selection(
        &self,
        graphemes: &[&str],
        sel_start: usize,
        sel_end: usize,
    ) -> Line<'static> {
        let horiz = self.viewport.horiz_offset() as usize;
        let mut skip = 0;
        let mut acc = 0usize;

        for g in graphemes.iter() {
            if acc >= horiz {
                break;
            }
            acc += g.width();
            skip += 1;
        }

        let mut spans = Vec::new();
        let mut current = String::new();
        let mut in_sel = false;

        for (idx, g) in graphemes.iter().enumerate().skip(skip) {
            let should_highlight = idx >= sel_start && idx < sel_end;

            if should_highlight != in_sel {
                if !current.is_empty() {
                    if in_sel {
                        spans.push(Span::styled(
                            current.clone(),
                            Style::default()
                                .bg(self.theme.palette_selected_bg)
                                .fg(self.theme.palette_selected_fg),
                        ));
                    } else {
                        spans.push(Span::raw(current.clone()));
                    }
                    current.clear();
                }
                in_sel = should_highlight;
            }
            current.push_str(g);
        }

        if !current.is_empty() {
            if in_sel {
                spans.push(Span::styled(
                    current,
                    Style::default()
                        .bg(self.theme.palette_selected_bg)
                        .fg(self.theme.palette_selected_fg),
                ));
            } else {
                spans.push(Span::raw(current));
            }
        }

        Line::from(spans)
    }
}
