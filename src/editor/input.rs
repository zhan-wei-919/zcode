//zcode/src/editor/input.rs
use super::state::Editor;
use std::io;
use crossterm::event::{self, Event, KeyCode}; 

impl Editor {
    fn input_char(&mut self, c:char) {
        let old_grapheme_len = self.line_grapheme_len(self.cursor.0);
        let pos = self.get_char_pos();
        self.rope.insert_char(pos,c);
        let new_grapheme_len = self.line_grapheme_len(self.cursor.0);
        self.cursor.1 += new_grapheme_len - old_grapheme_len;
        self.invalidate_pos_cache();
    }

    fn input_enter(&mut self) {
        let pos = self.get_char_pos();
        self.rope.insert_char(pos, '\n');
        self.cursor.0 += 1;
        self.cursor.1 = 0;
        self.invalidate_pos_cache();
        self.ensure_cursor_visible();
        self.insert_indent();
    }

    fn insert_indent(&mut self){
        // 这个函数会通过一个复杂的算法知道我们会进行多少缩进，但是我现在还没实现
        // 目前这个函数什么都不做
    }

    fn input_backspace(&mut self){
        if self.cursor.1 > 0 {
            // 删除当前行的字符
            let current_pos = self.get_char_pos();
            self.cursor.1 -= 1;
            self.invalidate_pos_cache();
            let prev_pos = self.get_char_pos();
            self.rope.remove(prev_pos..current_pos);
        } else if self.cursor.0 > 0 {
            // 删除换行符，合并到上一行
            let prev_line_len = self.line_grapheme_len(self.cursor.0 - 1);
            self.cursor.0 -= 1;
            self.cursor.1 = prev_line_len;
            self.invalidate_pos_cache();
            let pos = self.get_char_pos();
            // 删除换行符（一个char）
            self.rope.remove(pos..(pos + 1));
            self.ensure_cursor_visible();
        }
    }

    fn input_tab(&mut self){
        let pos = self.get_char_pos();
        self.rope.insert(pos, "    ");
        self.cursor.1 += 4;
        self.invalidate_pos_cache();
        self.ensure_cursor_visible();
    }

    fn input_left(&mut self){
        if self.cursor.1 > 0 {
            self.cursor.1 -= 1;
            self.invalidate_pos_cache();
        }
    }

    fn input_right(&mut self){
        let line_len = self.line_grapheme_len(self.cursor.0);
        if self.cursor.1 < line_len {
            self.cursor.1 += 1;
            self.invalidate_pos_cache();
        }
    }

    fn input_up(&mut self){
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            let line_len = self.line_grapheme_len(self.cursor.0);
            self.cursor.1 = self.cursor.1.min(line_len);
            self.invalidate_pos_cache();
            self.ensure_cursor_visible();
        }
    }

    fn input_down(&mut self){
        if self.cursor.0 + 1 < self.rope.len_lines() {
            self.cursor.0 += 1;
            let line_len = self.line_grapheme_len(self.cursor.0);
            self.cursor.1 = self.cursor.1.min(line_len);
            self.invalidate_pos_cache();
            self.ensure_cursor_visible();
        }
    }

    // 确保光标在视口内可见
    fn ensure_cursor_visible(&mut self) {
        if self.cursor.0 < self.viewport_offset {
            self.viewport_offset = self.cursor.0;
        } else if self.cursor.0 >= self.viewport_offset + self.viewport_height {
            self.viewport_offset = self.cursor.0.saturating_sub(self.viewport_height.saturating_sub(1));
        }
    }

    pub fn handle_input(&mut self) -> io::Result<bool> {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Esc => return Ok(true), 
                KeyCode::Char(c) => self.input_char(c),
                KeyCode::Enter => self.input_enter(),
                KeyCode::Backspace => self.input_backspace(),
                KeyCode::Tab => self.input_tab(),
                KeyCode::Left => self.input_left(),
                KeyCode::Right => self.input_right(),
                KeyCode::Up => self.input_up(),
                KeyCode::Down => self.input_down(),
                _ => {}
            }
        }
        Ok(false)
    }
}