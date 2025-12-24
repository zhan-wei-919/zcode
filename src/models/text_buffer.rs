//! 文本缓冲区模型
//!
//! 职责：
//! - 文本存储（Rope）
//! - 光标和选区管理
//! - 行列 ↔ 字符偏移映射

use super::edit_op::{EditOp, OpId};
use super::selection::Selection;
use ropey::{Rope, RopeSlice};
use std::borrow::Cow;
use std::io::{self, Write};
use unicode_segmentation::UnicodeSegmentation;

/// 从 RopeSlice 获取字符串，优先零拷贝
pub fn slice_to_cow(slice: RopeSlice<'_>) -> Cow<'_, str> {
    match slice.as_str() {
        Some(s) => Cow::Borrowed(s),
        None => Cow::Owned(slice.to_string()),
    }
}

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

    /// 流式写入到 Writer，避免大文件 OOM
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        for chunk in self.rope.chunks() {
            writer.write_all(chunk.as_bytes())?;
        }
        Ok(())
    }

    #[cfg(test)]
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
        let slice = self.rope.line(row);
        let line = slice_to_cow(slice);
        line.graphemes(true)
            .take(grapheme_index)
            .map(|g| g.chars().count())
            .sum()
    }

    pub fn line_grapheme_len(&self, row: usize) -> usize {
        let slice = self.rope.line(row);
        let line = slice_to_cow(slice);
        let without_newline = line.strip_suffix('\n').unwrap_or(&line);
        without_newline.graphemes(true).count()
    }

    // ==================== 原子操作方法（返回 EditOp）====================

    /// 插入字符，返回 EditOp
    pub fn insert_char_op(&mut self, c: char, parent: OpId) -> EditOp {
        let cursor_before = self.cursor;
        let char_offset = self.cursor_char_offset();

        self.rope.insert_char(char_offset, c);

        let cursor_after = if c == '\n' {
            (cursor_before.0 + 1, 0)
        } else {
            (cursor_before.0, cursor_before.1 + 1)
        };
        self.cursor = cursor_after;
        self.cached_char_pos = Some(char_offset + 1);

        EditOp::insert(parent, char_offset, c.to_string(), cursor_before, cursor_after)
    }

    /// 插入字符串，返回 EditOp
    pub fn insert_str_op(&mut self, s: &str, parent: OpId) -> EditOp {
        let cursor_before = self.cursor;
        let char_offset = self.cursor_char_offset();

        self.rope.insert(char_offset, s);

        // 计算新光标位置
        let newlines = s.chars().filter(|&c| c == '\n').count();
        let cursor_after = if newlines > 0 {
            let last_newline = s.rfind('\n').unwrap();
            let after_last_newline = &s[last_newline + 1..];
            (cursor_before.0 + newlines, after_last_newline.graphemes(true).count())
        } else {
            (cursor_before.0, cursor_before.1 + s.graphemes(true).count())
        };
        self.cursor = cursor_after;
        self.cached_char_pos = Some(char_offset + s.chars().count());

        EditOp::insert(parent, char_offset, s.to_string(), cursor_before, cursor_after)
    }

    /// 向后删除（Backspace），返回 EditOp
    pub fn delete_backward_op(&mut self, parent: OpId) -> Option<EditOp> {
        let (row, col) = self.cursor;
        let cursor_before = self.cursor;

        if col > 0 {
            let start = self.pos_to_char((row, col - 1));
            let end = self.pos_to_char((row, col));
            let deleted: String = self.rope.slice(start..end).to_string();

            self.rope.remove(start..end);
            let cursor_after = (row, col - 1);
            self.cursor = cursor_after;
            self.invalidate_char_pos_cache();

            Some(EditOp::delete(parent, start, end, deleted, cursor_before, cursor_after))
        } else if row > 0 {
            let prev_len = self.line_grapheme_len(row - 1);
            let start = self.pos_to_char((row, 0)) - 1;
            let end = start + 1;
            let deleted = "\n".to_string();

            self.rope.remove(start..end);
            let cursor_after = (row - 1, prev_len);
            self.cursor = cursor_after;
            self.invalidate_char_pos_cache();

            Some(EditOp::delete(parent, start, end, deleted, cursor_before, cursor_after))
        } else {
            None
        }
    }

    /// 向前删除（Delete），返回 EditOp
    pub fn delete_forward_op(&mut self, parent: OpId) -> Option<EditOp> {
        let (row, col) = self.cursor;
        let cursor_before = self.cursor;
        let line_len = self.line_grapheme_len(row);

        if col < line_len {
            let start = self.pos_to_char((row, col));
            let end = self.pos_to_char((row, col + 1));
            let deleted: String = self.rope.slice(start..end).to_string();

            self.rope.remove(start..end);
            // 光标位置不变
            self.invalidate_char_pos_cache();

            Some(EditOp::delete(parent, start, end, deleted, cursor_before, cursor_before))
        } else if row + 1 < self.len_lines() {
            let start = self.pos_to_char((row, col));
            let end = start + 1;
            let deleted = "\n".to_string();

            self.rope.remove(start..end);
            // 光标位置不变
            self.invalidate_char_pos_cache();

            Some(EditOp::delete(parent, start, end, deleted, cursor_before, cursor_before))
        } else {
            None
        }
    }

    /// 删除选区，返回 EditOp
    pub fn delete_selection_op(&mut self, parent: OpId) -> Option<EditOp> {
        let selection = self.selection.as_ref()?;
        if selection.is_empty() {
            return None;
        }

        let (start_pos, end_pos) = selection.range();
        let start_char = self.pos_to_char(start_pos);
        let end_char = self.pos_to_char(end_pos);
        let cursor_before = self.cursor;

        let deleted: String = self.rope.slice(start_char..end_char).to_string();
        self.rope.remove(start_char..end_char);

        let cursor_after = start_pos;
        self.cursor = cursor_after;
        self.selection = None;
        self.invalidate_char_pos_cache();

        Some(EditOp::delete(parent, start_char, end_char, deleted, cursor_before, cursor_after))
    }

    // ==================== Undo/Redo 支持 ====================

    /// 替换整个 Rope（用于 Undo/Redo）
    pub fn set_rope(&mut self, rope: Rope) {
        self.rope = rope;
        self.invalidate_char_pos_cache();
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
    fn test_insert_char_op() {
        let mut buffer = TextBuffer::new();
        let op = buffer.insert_char_op('a', OpId::root());

        assert_eq!(buffer.text(), "a");
        assert_eq!(buffer.cursor(), (0, 1));
        assert_eq!(op.cursor_before(), (0, 0));
        assert_eq!(op.cursor_after(), (0, 1));
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
