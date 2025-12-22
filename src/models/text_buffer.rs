//! 文本缓冲区模型
//!
//! 职责：
//! - 文本存储（Rope）
//! - 光标和选区管理
//! - 行列 ↔ 字符偏移映射

use super::selection::Selection;
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub struct TextBuffer {
    rope: Rope,
    cursor: (usize, usize),
    selection: Option<Selection>,
    cached_char_pos: Option<usize>,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            cursor: (0, 0),
            selection: None,
            cached_char_pos: Some(0),
        }
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            cursor: (0, 0),
            selection: None,
            cached_char_pos: Some(0),
        }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor = (row, col);
        self.invalidate_char_pos_cache();
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn selection_mut(&mut self) -> Option<&mut Selection> {
        self.selection.as_mut()
    }

    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    pub fn update_selection_cursor(&mut self, pos: (usize, usize)) {
        if let Some(sel) = &mut self.selection {
            sel.update_cursor(pos, &self.rope);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn has_selection(&self) -> bool {
        self.selection
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    pub fn line(&self, row: usize) -> Option<String> {
        if row < self.rope.len_lines() {
            self.rope.line(row).as_str().map(|s| s.to_string())
        } else {
            None
        }
    }

    pub fn line_slice(&self, row: usize) -> Option<ropey::RopeSlice<'_>> {
        if row < self.rope.len_lines() {
            Some(self.rope.line(row))
        } else {
            None
        }
    }

    pub fn cursor_char_offset(&mut self) -> usize {
        if self.cached_char_pos.is_none() {
            let char_offset = self.rope.line_to_char(self.cursor.0)
                + self.grapheme_to_char_index(self.cursor.0, self.cursor.1);
            self.cached_char_pos = Some(char_offset);
        }
        self.cached_char_pos.unwrap()
    }

    pub fn set_cursor_char_offset_cache(&mut self, char_offset: usize) {
        self.cached_char_pos = Some(char_offset);
    }

    pub fn pos_to_char(&self, pos: (usize, usize)) -> usize {
        self.rope.line_to_char(pos.0) + self.grapheme_to_char_index(pos.0, pos.1)
    }

    pub fn grapheme_to_char_index(&self, row: usize, grapheme_index: usize) -> usize {
        self.rope
            .line(row)
            .as_str()
            .unwrap_or("")
            .graphemes(true)
            .take(grapheme_index)
            .map(|g| g.chars().count())
            .sum()
    }

    pub fn line_grapheme_len(&self, row: usize) -> usize {
        let line = self.rope.line(row).as_str().unwrap_or("");
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        without_newline.graphemes(true).count()
    }

    pub fn insert_char(&mut self, c: char) {
        let pos = self.cursor_char_offset();
        self.rope.insert_char(pos, c);
        self.invalidate_char_pos_cache();
    }

    pub fn insert_str(&mut self, s: &str) {
        let pos = self.cursor_char_offset();
        self.rope.insert(pos, s);
        self.invalidate_char_pos_cache();
    }

    pub fn remove_range(&mut self, start: usize, end: usize) {
        self.rope.remove(start..end);
        self.invalidate_char_pos_cache();
    }

    pub fn delete_selection_with_offsets(&mut self, start_char: usize, end_char: usize) -> bool {
        if let Some(selection) = &self.selection {
            if !selection.is_empty() {
                let (start, _end) = selection.range();

                self.rope.remove(start_char..end_char);
                self.cursor = start;
                self.selection = None;
                self.invalidate_char_pos_cache();

                return true;
            }
        }
        false
    }

    fn invalidate_char_pos_cache(&mut self) {
        self.cached_char_pos = None;
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_buffer_basic() {
        let mut buffer = TextBuffer::from_text("hello\nworld");

        assert_eq!(buffer.len_lines(), 2);
        assert_eq!(buffer.cursor(), (0, 0));

        buffer.set_cursor(1, 2);
        assert_eq!(buffer.cursor(), (1, 2));
    }

    #[test]
    fn test_pos_to_char() {
        let buffer = TextBuffer::from_text("hello\nworld");

        assert_eq!(buffer.pos_to_char((0, 0)), 0);
        assert_eq!(buffer.pos_to_char((1, 0)), 6);
    }

    #[test]
    fn test_insert_char() {
        let mut buffer = TextBuffer::new();
        buffer.insert_char('a');

        assert_eq!(buffer.text(), "a");
    }

    #[test]
    fn test_line_grapheme_len() {
        let buffer = TextBuffer::from_text("hello\nworld\n");

        assert_eq!(buffer.line_grapheme_len(0), 5);
        assert_eq!(buffer.line_grapheme_len(1), 5);
    }

    #[test]
    fn test_has_selection() {
        let mut buffer = TextBuffer::new();
        assert!(!buffer.has_selection());

        buffer.set_selection(Some(Selection::new(
            (0, 0),
            super::super::selection::Granularity::Char,
        )));
        assert!(!buffer.has_selection());
    }
}
