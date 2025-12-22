//! 编辑器视图
//!
//! 实现 View trait，负责：
//! - 渲染文本内容
//! - 处理键盘输入
//! - 处理鼠标交互
//! - 管理选区

use super::viewport::Viewport;
use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::core::Command;
use crate::models::{Granularity, Selection, TextBuffer};
use crate::services::EditorConfig;
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct EditorView {
    buffer: TextBuffer,
    viewport: Viewport,
    config: EditorConfig,
    file_path: Option<PathBuf>,
    dirty: bool,
    mouse_state: MouseState,
}

struct MouseState {
    last_click: Option<(u16, u16, Instant)>,
    click_count: u8,
    dragging: bool,
}

impl MouseState {
    fn new() -> Self {
        Self {
            last_click: None,
            click_count: 0,
            dragging: false,
        }
    }

    fn on_click(&mut self, x: u16, y: u16, config: &EditorConfig) -> Granularity {
        let now = Instant::now();

        if let Some((lx, ly, lt)) = self.last_click {
            let dx = (x as i32 - lx as i32).abs();
            let dy = (y as i32 - ly as i32).abs();
            let dt = now.duration_since(lt).as_millis() as u64;

            if dx <= config.click_slop as i32
                && dy <= config.click_slop as i32
                && dt < config.triple_click_ms
            {
                self.click_count = (self.click_count % 3) + 1;
            } else {
                self.click_count = 1;
            }
        } else {
            self.click_count = 1;
        }

        self.last_click = Some((x, y, now));
        self.dragging = true;

        match self.click_count {
            1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        }
    }

    fn on_release(&mut self) {
        self.dragging = false;
    }
}

impl EditorView {
    pub fn new() -> Self {
        Self {
            buffer: TextBuffer::new(),
            viewport: Viewport::new(4),
            config: EditorConfig::default(),
            file_path: None,
            dirty: false,
            mouse_state: MouseState::new(),
        }
    }

    pub fn with_config(config: EditorConfig) -> Self {
        Self {
            buffer: TextBuffer::new(),
            viewport: Viewport::new(config.tab_size),
            config,
            file_path: None,
            dirty: false,
            mouse_state: MouseState::new(),
        }
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            buffer: TextBuffer::from_text(text),
            viewport: Viewport::new(4),
            config: EditorConfig::default(),
            file_path: None,
            dirty: false,
            mouse_state: MouseState::new(),
        }
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffer
    }

    pub fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    pub fn set_file_path(&mut self, path: PathBuf) {
        self.file_path = Some(path);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    pub fn set_content(&mut self, text: &str) {
        self.buffer = TextBuffer::from_text(text);
        self.dirty = false;
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.buffer.cursor()
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.viewport
            .area()
            .map(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
            .unwrap_or(false)
    }

    fn handle_key(&mut self, event: &crossterm::event::KeyEvent) -> EventResult {
        // 键盘操作时重新启用光标跟随
        self.viewport.enable_follow_cursor();

        match (event.code, event.modifiers) {
            (KeyCode::Left, KeyModifiers::NONE) => self.execute(Command::CursorLeft),
            (KeyCode::Right, KeyModifiers::NONE) => self.execute(Command::CursorRight),
            (KeyCode::Up, KeyModifiers::NONE) => self.execute(Command::CursorUp),
            (KeyCode::Down, KeyModifiers::NONE) => self.execute(Command::CursorDown),
            (KeyCode::Home, KeyModifiers::NONE) => self.execute(Command::CursorLineStart),
            (KeyCode::End, KeyModifiers::NONE) => self.execute(Command::CursorLineEnd),
            (KeyCode::Home, KeyModifiers::CONTROL) => self.execute(Command::CursorFileStart),
            (KeyCode::End, KeyModifiers::CONTROL) => self.execute(Command::CursorFileEnd),
            (KeyCode::PageUp, KeyModifiers::NONE) => self.execute(Command::PageUp),
            (KeyCode::PageDown, KeyModifiers::NONE) => self.execute(Command::PageDown),
            (KeyCode::Enter, KeyModifiers::NONE) => self.execute(Command::InsertNewline),
            (KeyCode::Tab, KeyModifiers::NONE) => self.execute(Command::InsertTab),
            (KeyCode::Backspace, KeyModifiers::NONE) => self.execute(Command::DeleteBackward),
            (KeyCode::Delete, KeyModifiers::NONE) => self.execute(Command::DeleteForward),
            (KeyCode::Esc, KeyModifiers::NONE) => self.execute(Command::ClearSelection),
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => return EventResult::Quit,
            (KeyCode::Char(c), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                self.execute(Command::InsertChar(c))
            }
            _ => return EventResult::Ignored,
        }

        EventResult::Consumed
    }

    fn handle_mouse(&mut self, event: &crossterm::event::MouseEvent) -> EventResult {
        let area = match self.viewport.area() {
            Some(a) => a,
            None => return EventResult::Ignored,
        };

        let x = event.column.saturating_sub(area.x);
        let y = event.row.saturating_sub(area.y);

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // 鼠标点击时重新启用光标跟随
                self.viewport.enable_follow_cursor();

                let granularity = self.mouse_state.on_click(x, y, &self.config);

                if let Some(pos) = self.viewport.screen_to_pos(x, y, &self.buffer) {
                    self.buffer.set_cursor(pos.0, pos.1);

                    let mut selection = Selection::new(pos, granularity);
                    if granularity != Granularity::Char {
                        selection.update_cursor(pos, self.buffer.rope());
                    }
                    self.buffer.set_selection(Some(selection));
                }
                EventResult::Consumed
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.mouse_state.dragging {
                    if let Some(pos) = self.viewport.screen_to_pos(x, y, &self.buffer) {
                        self.buffer.update_selection_cursor(pos);
                        self.buffer.set_cursor(pos.0, pos.1);
                    }
                }
                EventResult::Consumed
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_state.on_release();
                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                let step = self.config.scroll_step();
                self.viewport
                    .scroll_vertical(-(step as isize), self.buffer.len_lines());
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let step = self.config.scroll_step();
                self.viewport
                    .scroll_vertical(step as isize, self.buffer.len_lines());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn execute(&mut self, command: Command) {
        match command {
            Command::CursorLeft => self.cursor_left(),
            Command::CursorRight => self.cursor_right(),
            Command::CursorUp => self.cursor_up(),
            Command::CursorDown => self.cursor_down(),
            Command::CursorLineStart => {
                let (row, _) = self.buffer.cursor();
                self.buffer.set_cursor(row, 0);
            }
            Command::CursorLineEnd => {
                let (row, _) = self.buffer.cursor();
                let len = self.buffer.line_grapheme_len(row);
                self.buffer.set_cursor(row, len);
            }
            Command::CursorFileStart => {
                self.buffer.set_cursor(0, 0);
            }
            Command::CursorFileEnd => {
                let last = self.buffer.len_lines().saturating_sub(1);
                let len = self.buffer.line_grapheme_len(last);
                self.buffer.set_cursor(last, len);
            }
            Command::PageUp => {
                let height = self.viewport.viewport_height();
                self.viewport
                    .scroll_vertical(-(height as isize), self.buffer.len_lines());
                let (row, col) = self.buffer.cursor();
                let new_row = row.saturating_sub(height);
                self.buffer.set_cursor(new_row, col);
            }
            Command::PageDown => {
                let height = self.viewport.viewport_height();
                self.viewport
                    .scroll_vertical(height as isize, self.buffer.len_lines());
                let (row, col) = self.buffer.cursor();
                let new_row = (row + height).min(self.buffer.len_lines().saturating_sub(1));
                self.buffer.set_cursor(new_row, col);
            }
            Command::InsertChar(c) => {
                self.delete_selection();
                self.insert_char(c);
                self.dirty = true;
            }
            Command::InsertNewline => {
                self.delete_selection();
                self.insert_newline();
                self.dirty = true;
            }
            Command::InsertTab => {
                self.delete_selection();
                self.insert_char('\t');
                self.dirty = true;
            }
            Command::DeleteBackward => {
                if !self.delete_selection() {
                    self.delete_backward();
                }
                self.dirty = true;
            }
            Command::DeleteForward => {
                if !self.delete_selection() {
                    self.delete_forward();
                }
                self.dirty = true;
            }
            Command::ClearSelection => {
                self.buffer.clear_selection();
            }
            _ => {}
        }
    }

    fn cursor_left(&mut self) {
        let (row, col) = self.buffer.cursor();
        if col > 0 {
            self.buffer.set_cursor(row, col - 1);
        } else if row > 0 {
            let prev_len = self.buffer.line_grapheme_len(row - 1);
            self.buffer.set_cursor(row - 1, prev_len);
        }
    }

    fn cursor_right(&mut self) {
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);
        if col < line_len {
            self.buffer.set_cursor(row, col + 1);
        } else if row + 1 < self.buffer.len_lines() {
            self.buffer.set_cursor(row + 1, 0);
        }
    }

    fn cursor_up(&mut self) {
        let (row, col) = self.buffer.cursor();
        if row > 0 {
            let new_len = self.buffer.line_grapheme_len(row - 1);
            self.buffer.set_cursor(row - 1, col.min(new_len));
        }
    }

    fn cursor_down(&mut self) {
        let (row, col) = self.buffer.cursor();
        if row + 1 < self.buffer.len_lines() {
            let new_len = self.buffer.line_grapheme_len(row + 1);
            self.buffer.set_cursor(row + 1, col.min(new_len));
        }
    }

    fn insert_char(&mut self, c: char) {
        let (row, col) = self.buffer.cursor();
        self.buffer.insert_char(c);

        if c == '\n' {
            self.buffer.set_cursor(row + 1, 0);
        } else {
            self.buffer.set_cursor(row, col + 1);
        }
    }

    fn insert_newline(&mut self) {
        let (row, _) = self.buffer.cursor();
        self.buffer.insert_char('\n');
        self.buffer.set_cursor(row + 1, 0);
    }

    fn delete_backward(&mut self) {
        let (row, col) = self.buffer.cursor();
        if col > 0 {
            let start = self.buffer.pos_to_char((row, col - 1));
            let end = self.buffer.pos_to_char((row, col));
            self.buffer.remove_range(start, end);
            self.buffer.set_cursor(row, col - 1);
        } else if row > 0 {
            let prev_len = self.buffer.line_grapheme_len(row - 1);
            let start = self.buffer.pos_to_char((row, 0));
            self.buffer.remove_range(start - 1, start);
            self.buffer.set_cursor(row - 1, prev_len);
        }
    }

    fn delete_forward(&mut self) {
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);

        if col < line_len {
            let start = self.buffer.pos_to_char((row, col));
            let end = self.buffer.pos_to_char((row, col + 1));
            self.buffer.remove_range(start, end);
        } else if row + 1 < self.buffer.len_lines() {
            let start = self.buffer.pos_to_char((row, col));
            self.buffer.remove_range(start, start + 1);
        }
    }

    fn delete_selection(&mut self) -> bool {
        if let Some(selection) = self.buffer.selection() {
            if !selection.is_empty() {
                let (start, end) = selection.range();
                let start_char = self.buffer.pos_to_char(start);
                let end_char = self.buffer.pos_to_char(end);
                return self.buffer.delete_selection_with_offsets(start_char, end_char);
            }
        }
        false
    }

    fn render_line(&self, line_str: &str, row: usize) -> Line<'static> {
        let expanded = self.viewport.expand_tabs(line_str);
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
                            Style::default().bg(Color::Blue).fg(Color::White),
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
                    Style::default().bg(Color::Blue).fg(Color::White),
                ));
            } else {
                spans.push(Span::raw(current));
            }
        }

        Line::from(spans)
    }
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for EditorView {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        match event {
            InputEvent::Key(key_event) => self.handle_key(key_event),
            InputEvent::Mouse(mouse_event) => self.handle_mouse(mouse_event),
            _ => EventResult::Ignored,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let total_lines = self.buffer.len_lines();
        let max_line_width = total_lines.to_string().len();
        let gutter_width = (max_line_width + 2) as u16;

        let content_width = area.width.saturating_sub(gutter_width);
        let content_area = Rect::new(
            area.x + gutter_width,
            area.y,
            content_width,
            area.height,
        );

        self.viewport.set_area(content_area);
        self.viewport
            .update(&self.buffer, area.height as usize, content_width as usize);

        let (visible_start, visible_end) = self.viewport.visible_range(total_lines);

        let gutter_lines: Vec<Line> = (visible_start..visible_end)
            .map(|i| {
                Line::from(Span::styled(
                    format!("{:>width$} ", i + 1, width = max_line_width),
                    Style::default().fg(Color::DarkGray),
                ))
            })
            .collect();

        let gutter_area = Rect::new(area.x, area.y, gutter_width, area.height);
        let gutter_widget = Paragraph::new(gutter_lines);
        frame.render_widget(gutter_widget, gutter_area);

        let content_lines: Vec<Line> = (visible_start..visible_end)
            .map(|i| {
                let line_str = self.buffer.line(i).unwrap_or_default();
                self.render_line(&line_str, i)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_view_new() {
        let view = EditorView::new();
        assert_eq!(view.cursor(), (0, 0));
        assert!(!view.is_dirty());
    }

    #[test]
    fn test_editor_view_from_text() {
        let view = EditorView::from_text("hello\nworld");
        assert_eq!(view.buffer().len_lines(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut view = EditorView::from_text("hello\nworld");

        view.execute(Command::CursorRight);
        assert_eq!(view.cursor(), (0, 1));

        view.execute(Command::CursorDown);
        assert_eq!(view.cursor(), (1, 1));

        view.execute(Command::CursorLineEnd);
        assert_eq!(view.cursor(), (1, 5));
    }

    #[test]
    fn test_insert_char() {
        let mut view = EditorView::new();
        view.execute(Command::InsertChar('a'));
        assert_eq!(view.buffer().text(), "a");
        assert!(view.is_dirty());
    }
}
