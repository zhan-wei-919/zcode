//! 命令执行器：将命令转换为实际的编辑器操作
//! 
//! 这个模块负责：
//! - 解释命令的语义
//! - 调用底层的编辑操作
//! - 保持命令和实现的解耦

use crate::editor::core::Editor;
use super::command::Command;

impl Editor {
    /// 执行命令
    /// 
    /// 返回：命令是否被处理（true = 已处理，false = 未知命令）
    pub fn execute_command(&mut self, command: &Command) -> bool {
        match command {
            // ==================== 光标移动 ====================
            Command::CursorLeft => {
                self.input_left();
                true
            }
            Command::CursorRight => {
                self.input_right();
                true
            }
            Command::CursorUp => {
                self.input_up();
                true
            }
            Command::CursorDown => {
                self.input_down();
                true
            }
            Command::CursorLineStart => {
                let (row, _) = self.model.cursor();
                self.model.set_cursor(row, 0);
                self.ensure_cursor_visible();
                true
            }
            Command::CursorLineEnd => {
                let (row, _) = self.model.cursor();
                let line_len = self.model.line_grapheme_len(row);
                self.model.set_cursor(row, line_len);
                self.ensure_cursor_visible();
                true
            }
            Command::CursorFileStart => {
                self.model.set_cursor(0, 0);
                self.ensure_cursor_visible();
                true
            }
            Command::CursorFileEnd => {
                let last_line = self.model.len_lines().saturating_sub(1);
                let line_len = self.model.line_grapheme_len(last_line);
                self.model.set_cursor(last_line, line_len);
                self.ensure_cursor_visible();
                true
            }
            
            // ==================== 编辑操作 ====================
            Command::InsertChar(c) => {
                self.delete_selection();
                self.input_char(*c);
                true
            }
            Command::InsertNewline => {
                self.delete_selection();
                self.input_enter();
                true
            }
            Command::InsertTab => {
                self.input_tab();
                true
            }
            Command::DeleteBackward => {
                if !self.delete_selection() {
                    self.input_backspace();
                }
                true
            }
            Command::DeleteForward => {
                // TODO: 实现 Delete 键
                true
            }
            
            // ==================== 选择操作 ====================
            Command::ClearSelection => {
                self.model.set_selection(None);
                true
            }
            Command::SelectAll => {
                // TODO: 实现全选
                true
            }
            
            // ==================== 滚动操作 ====================
            Command::ScrollUp => {
                let step = self.scroll_step();
                self.view.scroll_vertical(-(step as isize), self.model.len_lines());
                true
            }
            Command::ScrollDown => {
                let step = self.scroll_step();
                self.view.scroll_vertical(step as isize, self.model.len_lines());
                true
            }
            Command::PageUp => {
                let step = self.view.viewport_height();
                self.view.scroll_vertical(-(step as isize), self.model.len_lines());
                // 同时移动光标
                let (row, col) = self.model.cursor();
                let new_row = row.saturating_sub(step);
                self.model.set_cursor(new_row, col);
                true
            }
            Command::PageDown => {
                let step = self.view.viewport_height();
                self.view.scroll_vertical(step as isize, self.model.len_lines());
                // 同时移动光标
                let (row, col) = self.model.cursor();
                let new_row = (row + step).min(self.model.len_lines() - 1);
                self.model.set_cursor(new_row, col);
                true
            }
            
            // ==================== 未实现的命令 ====================
            _ => {
                // 未知命令
                false
            }
        }
    }
    
    /// 删除选区内容
    /// 返回：是否删除了内容
    pub(crate) fn delete_selection(&mut self) -> bool {
        if let Some(selection) = self.model.selection() {
            if !selection.is_empty() {
                let (start, end) = selection.range();
                let deleted = self.model.delete_selection();
                if deleted {
                    // 删除后，确保不超出新的行数范围
                    let total_lines = self.model.len_lines();
                    self.view.invalidate_layout_range(start.0, (end.0 + 1).min(total_lines));
                }
                return deleted;
            }
        }
        false
    }
    
    /// 确保光标在视口内可见
    pub(crate) fn ensure_cursor_visible(&mut self) {
        let cursor = self.model.cursor();
        let viewport_height = self.view.viewport_height();
        let viewport_offset = self.view.viewport_offset();
        
        // 垂直滚动调整
        if cursor.0 < viewport_offset {
            self.view.scroll_vertical(-(viewport_offset as isize - cursor.0 as isize), self.model.len_lines());
        } else if cursor.0 >= viewport_offset + viewport_height {
            let delta = cursor.0 as isize - (viewport_offset + viewport_height - 1) as isize;
            self.view.scroll_vertical(delta, self.model.len_lines());
        }
    }
    
    /// 计算动态滚动步长（视口高度的 1/6）
    pub(crate) fn scroll_step(&self) -> usize {
        (self.view.viewport_height() / 6).max(1)
    }
    
    // 以下是原有的输入方法，现在作为私有方法被命令执行器调用
    
    fn input_char(&mut self, c: char) {
        let cursor = self.model.cursor();
        let old_grapheme_len = self.model.line_grapheme_len(cursor.0);
        
        self.model.insert_char(c);
        
        let new_grapheme_len = self.model.line_grapheme_len(cursor.0);
        self.model.set_cursor(cursor.0, cursor.1 + (new_grapheme_len - old_grapheme_len));
        
        // 细粒度失效：只标记当前行
        self.view.invalidate_layout_range(cursor.0, cursor.0 + 1);
    }

    fn input_enter(&mut self) {
        let cursor = self.model.cursor();
        
        self.model.insert_char('\n');
        self.model.set_cursor(cursor.0 + 1, 0);
        
        // Enter 只影响当前行 + 新插入的行（2行）
        // 使用 min 确保不超出边界
        let total_lines = self.model.len_lines();
        self.view.invalidate_layout_range(cursor.0, (cursor.0 + 2).min(total_lines));
        self.ensure_cursor_visible();
    }

    fn input_backspace(&mut self) {
        let cursor = self.model.cursor();
        
        if cursor.1 > 0 {
            // 行内删除
            // 获取要删除的字符在文档中的绝对偏移
            let start_char = self.model.pos_to_char((cursor.0, cursor.1 - 1));
            let end_char = self.model.pos_to_char((cursor.0, cursor.1));
            
            self.model.remove_range(start_char, end_char);
            self.model.set_cursor(cursor.0, cursor.1 - 1);
            self.view.invalidate_layout_range(cursor.0, cursor.0 + 1);
        } else if cursor.0 > 0 {
            // 跨行删除（合并到上一行）
            let prev_line_len = self.model.line_grapheme_len(cursor.0 - 1);
            
            // 获取当前行开头在文档中的绝对字符偏移
            let line_start_char = self.model.pos_to_char((cursor.0, 0));
            // 换行符在行开头之前
            self.model.remove_range(line_start_char - 1, line_start_char);
            
            self.model.set_cursor(cursor.0 - 1, prev_line_len);
            
            // 删除换行符后，行数减少了，需要基于新的行数来失效布局
            let total_lines = self.model.len_lines();
            self.view.invalidate_layout_range(cursor.0 - 1, (cursor.0 + 1).min(total_lines));
        }
    }

    fn input_tab(&mut self) {
        self.delete_selection();
        self.input_char('\t');
    }

    fn input_left(&mut self) {
        let cursor = self.model.cursor();
        if cursor.1 > 0 {
            self.model.set_cursor(cursor.0, cursor.1 - 1);
        } else if cursor.0 > 0 {
            let prev_line_len = self.model.line_grapheme_len(cursor.0 - 1);
            self.model.set_cursor(cursor.0 - 1, prev_line_len);
        }
        self.ensure_cursor_visible();
    }

    fn input_right(&mut self) {
        let cursor = self.model.cursor();
        let line_len = self.model.line_grapheme_len(cursor.0);
        
        if cursor.1 < line_len {
            self.model.set_cursor(cursor.0, cursor.1 + 1);
        } else if cursor.0 + 1 < self.model.len_lines() {
            self.model.set_cursor(cursor.0 + 1, 0);
        }
        self.ensure_cursor_visible();
    }

    fn input_up(&mut self) {
        let cursor = self.model.cursor();
        if cursor.0 > 0 {
            let new_row = cursor.0 - 1;
            let line_len = self.model.line_grapheme_len(new_row);
            self.model.set_cursor(new_row, cursor.1.min(line_len));
            self.ensure_cursor_visible();
        }
    }

    fn input_down(&mut self) {
        let cursor = self.model.cursor();
        if cursor.0 + 1 < self.model.len_lines() {
            let new_row = cursor.0 + 1;
            let line_len = self.model.line_grapheme_len(new_row);
            self.model.set_cursor(new_row, cursor.1.min(line_len));
            self.ensure_cursor_visible();
        }
    }
}

