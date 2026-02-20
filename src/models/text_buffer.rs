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

    pub fn get_selection_text(&self) -> Option<String> {
        let selection = self.selection.as_ref()?;
        if selection.is_empty() {
            return None;
        }

        let (start_pos, end_pos) = selection.range();
        let start_char = self.pos_to_char(start_pos);
        let end_char = self.pos_to_char(end_pos);

        Some(self.rope.slice(start_char..end_char).to_string())
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
            Some(slice_to_cow(self.rope.line(row)).into_owned())
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
        if let Some(char_offset) = self.cached_char_pos {
            return char_offset;
        }

        let char_offset = self.rope.line_to_char(self.cursor.0)
            + self.grapheme_to_char_index(self.cursor.0, self.cursor.1);
        self.cached_char_pos = Some(char_offset);
        char_offset
    }

    pub fn set_cursor_char_offset_cache(&mut self, char_offset: usize) {
        self.cached_char_pos = Some(char_offset);
    }

    pub fn pos_to_char(&self, pos: (usize, usize)) -> usize {
        self.rope.line_to_char(pos.0) + self.grapheme_to_char_index(pos.0, pos.1)
    }

    fn char_range_for_grapheme_range_same_row(
        &self,
        row: usize,
        start_grapheme: usize,
        end_grapheme: usize,
    ) -> (usize, usize) {
        let line_char_start = self.rope.line_to_char(row);
        let slice = self.rope.line(row);
        if slice.len_bytes() == slice.len_chars() {
            let len = ascii_line_len_no_newline(slice);
            let start = start_grapheme.min(len);
            let end = end_grapheme.min(len);
            return (
                line_char_start.saturating_add(start),
                line_char_start.saturating_add(end),
            );
        }

        let line = slice_to_cow(slice);
        let mut grapheme_idx = 0usize;
        let mut chars_taken = 0usize;
        let mut start_chars: Option<usize> = None;
        let mut end_chars: Option<usize> = None;

        for g in line.graphemes(true) {
            if start_chars.is_none() && grapheme_idx == start_grapheme {
                start_chars = Some(chars_taken);
            }
            if end_chars.is_none() && grapheme_idx == end_grapheme {
                end_chars = Some(chars_taken);
                break;
            }

            chars_taken = chars_taken.saturating_add(g.chars().count());
            grapheme_idx = grapheme_idx.saturating_add(1);
        }

        let start_chars = start_chars.unwrap_or(chars_taken);
        let end_chars = end_chars.unwrap_or(chars_taken);
        (
            line_char_start.saturating_add(start_chars),
            line_char_start.saturating_add(end_chars),
        )
    }

    pub fn grapheme_to_char_index(&self, row: usize, grapheme_index: usize) -> usize {
        let slice = self.rope.line(row);
        if slice.len_bytes() == slice.len_chars() {
            let len = ascii_line_len_no_newline(slice);
            return grapheme_index.min(len);
        }
        let line = slice_to_cow(slice);
        line.graphemes(true)
            .take(grapheme_index)
            .map(|g| g.chars().count())
            .sum()
    }

    pub(crate) fn cursor_pos_from_char_offset(&self, char_offset: usize) -> (usize, usize) {
        let char_offset = char_offset.min(self.rope.len_chars());
        let row = self.rope.char_to_line(char_offset);
        let line_char_start = self.rope.line_to_char(row);
        let col_chars = char_offset.saturating_sub(line_char_start);

        let slice = self.rope.line(row);
        if slice.len_bytes() == slice.len_chars() {
            let len = ascii_line_len_no_newline(slice);
            return (row, col_chars.min(len));
        }

        let line = slice_to_cow(slice);
        let mut taken_chars = 0usize;
        let mut col_graphemes = 0usize;
        for g in line.graphemes(true) {
            let g_chars = g.chars().count();
            if taken_chars + g_chars > col_chars {
                break;
            }
            taken_chars += g_chars;
            col_graphemes += 1;
        }

        (row, col_graphemes)
    }

    pub fn line_grapheme_len(&self, row: usize) -> usize {
        let slice = self.rope.line(row);
        if slice.len_bytes() == slice.len_chars() {
            return ascii_line_len_no_newline(slice);
        }
        let line = slice_to_cow(slice);
        let without_newline = line.strip_suffix('\n').unwrap_or(&line);
        let without_newline = without_newline
            .strip_suffix('\r')
            .unwrap_or(without_newline);
        without_newline.graphemes(true).count()
    }

    // ==================== 原子操作方法（返回 EditOp）====================

    /// 插入字符，返回 EditOp
    pub fn insert_char_op(&mut self, c: char, parent: OpId) -> EditOp {
        let cursor_before = self.cursor;
        let char_offset = self.cursor_char_offset();

        self.rope.insert_char(char_offset, c);

        let cursor_after_char_offset = char_offset.saturating_add(1);
        let cursor_after = self.cursor_pos_from_char_offset(cursor_after_char_offset);
        self.cursor = cursor_after;
        self.cached_char_pos = Some(cursor_after_char_offset);

        EditOp::insert(
            parent,
            char_offset,
            c.to_string(),
            cursor_before,
            cursor_after,
        )
    }

    /// 插入字符串，返回 EditOp
    pub fn insert_str_op(&mut self, s: &str, parent: OpId) -> EditOp {
        let cursor_before = self.cursor;
        let char_offset = self.cursor_char_offset();

        self.rope.insert(char_offset, s);

        let cursor_after_char_offset = char_offset.saturating_add(s.chars().count());
        let cursor_after = self.cursor_pos_from_char_offset(cursor_after_char_offset);
        self.cursor = cursor_after;
        self.cached_char_pos = Some(cursor_after_char_offset);

        EditOp::insert(
            parent,
            char_offset,
            s.to_string(),
            cursor_before,
            cursor_after,
        )
    }

    pub fn insert_str_op_with_cursor_after_char_offset(
        &mut self,
        s: &str,
        cursor_after: (usize, usize),
        cursor_after_char_offset: usize,
        parent: OpId,
    ) -> EditOp {
        let cursor_before = self.cursor;
        let char_offset = self.cursor_char_offset();

        self.rope.insert(char_offset, s);
        self.cursor = cursor_after;
        self.cached_char_pos = Some(cursor_after_char_offset);

        EditOp::insert(
            parent,
            char_offset,
            s.to_string(),
            cursor_before,
            cursor_after,
        )
    }

    /// 向后删除（Backspace），返回 EditOp
    pub fn delete_backward_op(&mut self, parent: OpId) -> Option<EditOp> {
        let (row, col) = self.cursor;
        let cursor_before = self.cursor;

        if col > 0 {
            let (start, end) = self.char_range_for_grapheme_range_same_row(row, col - 1, col);
            let deleted: String = self.rope.slice(start..end).to_string();

            self.rope.remove(start..end);
            let cursor_after = (row, col - 1);
            self.cursor = cursor_after;
            self.cached_char_pos = Some(start);

            Some(EditOp::delete(
                parent,
                start,
                end,
                deleted,
                cursor_before,
                cursor_after,
            ))
        } else if row > 0 {
            let prev_len = self.line_grapheme_len(row - 1);
            let start = self.pos_to_char((row, 0)) - 1;
            let end = start + 1;
            let deleted = "\n".to_string();

            self.rope.remove(start..end);
            let cursor_after = (row - 1, prev_len);
            self.cursor = cursor_after;
            self.cached_char_pos = Some(start);

            Some(EditOp::delete(
                parent,
                start,
                end,
                deleted,
                cursor_before,
                cursor_after,
            ))
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
            let (start, end) = self.char_range_for_grapheme_range_same_row(row, col, col + 1);
            let deleted: String = self.rope.slice(start..end).to_string();

            self.rope.remove(start..end);
            // 光标位置不变
            self.cached_char_pos = Some(start);

            Some(EditOp::delete(
                parent,
                start,
                end,
                deleted,
                cursor_before,
                cursor_before,
            ))
        } else if row + 1 < self.len_lines() {
            let start = self.pos_to_char((row, col));
            let end = start + 1;
            let deleted = "\n".to_string();

            self.rope.remove(start..end);
            // 光标位置不变
            self.cached_char_pos = Some(start);

            Some(EditOp::delete(
                parent,
                start,
                end,
                deleted,
                cursor_before,
                cursor_before,
            ))
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
        self.cached_char_pos = Some(start_char);

        Some(EditOp::delete(
            parent,
            start_char,
            end_char,
            deleted,
            cursor_before,
            cursor_after,
        ))
    }

    // ==================== Undo/Redo 支持 ====================

    /// 替换整个 Rope（用于 Undo/Redo）
    pub fn set_rope(&mut self, rope: Rope) {
        self.rope = rope;
        self.selection = None;
        self.invalidate_char_pos_cache();
    }

    /// 替换指定范围的文本（用于搜索替换）
    pub fn replace_range(&mut self, start_char: usize, end_char: usize, text: &str) {
        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, text);
        self.invalidate_char_pos_cache();
    }

    pub fn replace_range_op(
        &mut self,
        start_char: usize,
        end_char: usize,
        inserted: &str,
        cursor_after: (usize, usize),
        cursor_after_char_offset: usize,
        parent: OpId,
    ) -> EditOp {
        let cursor_before = self.cursor;
        let deleted: String = self.rope.slice(start_char..end_char).to_string();

        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, inserted);

        self.cursor = cursor_after;
        self.selection = None;
        self.cached_char_pos = Some(cursor_after_char_offset);

        EditOp::replace(
            parent,
            start_char,
            end_char,
            deleted,
            inserted.to_string(),
            cursor_before,
            cursor_after,
        )
    }

    pub fn replace_range_op_auto_cursor(
        &mut self,
        start_char: usize,
        end_char: usize,
        inserted: &str,
        parent: OpId,
    ) -> EditOp {
        let cursor_before = self.cursor;
        let deleted: String = self.rope.slice(start_char..end_char).to_string();

        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, inserted);

        let cursor_after_char_offset = start_char.saturating_add(inserted.chars().count());
        let row = self.rope.char_to_line(cursor_after_char_offset);
        let line_char_start = self.rope.line_to_char(row);
        let col_chars = cursor_after_char_offset.saturating_sub(line_char_start);

        let slice = self.rope.line(row);
        let line = slice_to_cow(slice);

        let mut taken_chars = 0usize;
        let mut col_graphemes = 0usize;
        for g in unicode_segmentation::UnicodeSegmentation::graphemes(line.as_ref(), true) {
            let g_chars = g.chars().count();
            if taken_chars + g_chars > col_chars {
                break;
            }
            taken_chars += g_chars;
            col_graphemes += 1;
        }

        let cursor_after = (row, col_graphemes);
        self.cursor = cursor_after;
        self.selection = None;
        self.cached_char_pos = Some(cursor_after_char_offset);

        EditOp::replace(
            parent,
            start_char,
            end_char,
            deleted,
            inserted.to_string(),
            cursor_before,
            cursor_after,
        )
    }

    pub fn replace_range_op_adjust_cursor(
        &mut self,
        start_char: usize,
        end_char: usize,
        inserted: &str,
        parent: OpId,
    ) -> EditOp {
        let cursor_before = self.cursor;
        let cursor_before_char_offset = self.cursor_char_offset();
        let deleted: String = self.rope.slice(start_char..end_char).to_string();

        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, inserted);

        let inserted_chars = inserted.chars().count();
        let deleted_chars = end_char.saturating_sub(start_char);

        let mut cursor_after_char_offset = if cursor_before_char_offset < start_char {
            cursor_before_char_offset
        } else if cursor_before_char_offset >= end_char {
            cursor_before_char_offset
                .saturating_add(inserted_chars)
                .saturating_sub(deleted_chars)
        } else {
            start_char.saturating_add(inserted_chars)
        };
        cursor_after_char_offset = cursor_after_char_offset.min(self.rope.len_chars());

        let row = self.rope.char_to_line(cursor_after_char_offset);
        let line_char_start = self.rope.line_to_char(row);
        let col_chars = cursor_after_char_offset.saturating_sub(line_char_start);

        let slice = self.rope.line(row);
        let line = slice_to_cow(slice);

        let mut taken_chars = 0usize;
        let mut col_graphemes = 0usize;
        for g in unicode_segmentation::UnicodeSegmentation::graphemes(line.as_ref(), true) {
            let g_chars = g.chars().count();
            if taken_chars + g_chars > col_chars {
                break;
            }
            taken_chars += g_chars;
            col_graphemes += 1;
        }

        let cursor_after = (row, col_graphemes);
        self.cursor = cursor_after;
        self.selection = None;
        self.cached_char_pos = Some(cursor_after_char_offset);

        EditOp::replace(
            parent,
            start_char,
            end_char,
            deleted,
            inserted.to_string(),
            cursor_before,
            cursor_after,
        )
    }

    fn invalidate_char_pos_cache(&mut self) {
        self.cached_char_pos = None;
    }
}

fn ascii_line_len_no_newline(slice: RopeSlice<'_>) -> usize {
    let mut len = slice.len_chars();
    if len == 0 {
        return 0;
    }

    if slice.char(len.saturating_sub(1)) == '\n' {
        len = len.saturating_sub(1);
        if len > 0 && slice.char(len.saturating_sub(1)) == '\r' {
            len = len.saturating_sub(1);
        }
    }

    len
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/models/text_buffer.rs"]
mod tests;
