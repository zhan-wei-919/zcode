use super::EditorView;
use crate::core::Command;
use crate::models::{slice_to_cow, Granularity, Selection};
use unicode_segmentation::UnicodeSegmentation;

fn is_word_boundary_char(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '（' | '）' | '【' | '】' | '「' | '」' | '，' | '。' | '：' | '；'
        )
}

impl EditorView {
    pub fn apply_command(&mut self, command: Command) -> bool {
        self.viewport.enable_follow_cursor();

        match command {
            Command::Undo => {
                self.undo();
                true
            }
            Command::Redo => {
                self.redo();
                true
            }
            Command::Copy => {
                self.copy();
                true
            }
            Command::Cut => {
                self.cut();
                true
            }
            Command::Paste => {
                self.paste();
                true
            }
            Command::ExtendSelectionLeft => {
                self.extend_selection_left();
                true
            }
            Command::ExtendSelectionRight => {
                self.extend_selection_right();
                true
            }
            Command::ExtendSelectionUp => {
                self.extend_selection_up();
                true
            }
            Command::ExtendSelectionDown => {
                self.extend_selection_down();
                true
            }
            Command::ExtendSelectionLineStart => {
                self.extend_selection_to_line_start();
                true
            }
            Command::ExtendSelectionLineEnd => {
                self.extend_selection_to_line_end();
                true
            }
            Command::ExtendSelectionWordLeft => {
                self.extend_selection_word_left();
                true
            }
            Command::ExtendSelectionWordRight => {
                self.extend_selection_word_right();
                true
            }
            cmd if cmd.is_cursor_command()
                || cmd.is_selection_command()
                || cmd.is_edit_command() =>
            {
                self.execute(cmd);
                true
            }
            _ => false,
        }
    }

    pub(super) fn execute(&mut self, command: Command) {
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
            }
            Command::InsertNewline => {
                self.delete_selection();
                self.insert_char('\n');
            }
            Command::InsertTab => {
                self.delete_selection();
                self.insert_char('\t');
            }
            Command::DeleteBackward => {
                if !self.delete_selection() {
                    self.delete_backward();
                }
            }
            Command::DeleteForward => {
                if !self.delete_selection() {
                    self.delete_forward();
                }
            }
            Command::ClearSelection => {
                self.buffer.clear_selection();
            }
            Command::CursorWordLeft => self.cursor_word_left(),
            Command::CursorWordRight => self.cursor_word_right(),
            Command::SelectAll => self.select_all(),
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

    fn cursor_word_left(&mut self) {
        let (row, col) = self.buffer.cursor();

        if col == 0 {
            if row > 0 {
                let prev_len = self.buffer.line_grapheme_len(row - 1);
                self.buffer.set_cursor(row - 1, prev_len);
            }
            return;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();

        let mut pos = col.min(graphemes.len());

        while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }

        while pos > 0
            && !graphemes[pos - 1]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos -= 1;
        }

        self.buffer.set_cursor(row, pos);
    }

    fn cursor_word_right(&mut self) {
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            if row + 1 < self.buffer.len_lines() {
                self.buffer.set_cursor(row + 1, 0);
            }
            return;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let len = graphemes.len();

        let mut pos = col;

        while pos < len
            && !graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        while pos < len
            && graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        self.buffer.set_cursor(row, pos.min(line_len));
    }

    fn select_all(&mut self) {
        let last_line = self.buffer.len_lines().saturating_sub(1);
        let last_col = self.buffer.line_grapheme_len(last_line);

        let mut selection = Selection::new((0, 0), Granularity::Char);
        selection.update_cursor((last_line, last_col), self.buffer.rope());
        self.buffer.set_selection(Some(selection));
        self.buffer.set_cursor(last_line, last_col);
    }

    fn ensure_selection(&mut self) {
        if self.buffer.selection().is_none() {
            let pos = self.buffer.cursor();
            self.buffer
                .set_selection(Some(Selection::new(pos, Granularity::Char)));
        }
    }

    fn extend_selection_left(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let new_pos = if col > 0 {
            (row, col - 1)
        } else if row > 0 {
            let prev_len = self.buffer.line_grapheme_len(row - 1);
            (row - 1, prev_len)
        } else {
            (row, col)
        };
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn extend_selection_right(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);
        let new_pos = if col < line_len {
            (row, col + 1)
        } else if row + 1 < self.buffer.len_lines() {
            (row + 1, 0)
        } else {
            (row, col)
        };
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn extend_selection_up(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        if row == 0 {
            return;
        }
        let new_len = self.buffer.line_grapheme_len(row - 1);
        let new_pos = (row - 1, col.min(new_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn extend_selection_down(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        if row + 1 >= self.buffer.len_lines() {
            return;
        }
        let new_len = self.buffer.line_grapheme_len(row + 1);
        let new_pos = (row + 1, col.min(new_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn extend_selection_to_line_start(&mut self) {
        self.ensure_selection();
        let (row, _) = self.buffer.cursor();
        let new_pos = (row, 0);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn extend_selection_to_line_end(&mut self) {
        self.ensure_selection();
        let (row, _) = self.buffer.cursor();
        let len = self.buffer.line_grapheme_len(row);
        let new_pos = (row, len);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn extend_selection_word_left(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();

        if col == 0 {
            if row > 0 {
                let prev_len = self.buffer.line_grapheme_len(row - 1);
                let new_pos = (row - 1, prev_len);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            return;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();

        let mut pos = col.min(graphemes.len());

        while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }

        while pos > 0
            && !graphemes[pos - 1]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos -= 1;
        }

        let new_pos = (row, pos);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn extend_selection_word_right(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            if row + 1 < self.buffer.len_lines() {
                let new_pos = (row + 1, 0);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            return;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let len = graphemes.len();

        let mut pos = col;

        while pos < len
            && !graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        while pos < len
            && graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        let new_pos = (row, pos.min(line_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn insert_char(&mut self, c: char) {
        let parent = self.history.head();
        let op = self.buffer.insert_char_op(c, parent);
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
    }

    fn delete_backward(&mut self) {
        let parent = self.history.head();
        let op = self.buffer.delete_backward_op(parent);
        if let Some(op) = op {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
        }
    }

    fn delete_forward(&mut self) {
        let parent = self.history.head();
        let op = self.buffer.delete_forward_op(parent);
        if let Some(op) = op {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
        }
    }

    fn delete_selection(&mut self) -> bool {
        let parent = self.history.head();
        if let Some(op) = self.buffer.delete_selection_op(parent) {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
            return true;
        }
        self.buffer.clear_selection();
        false
    }

    pub(super) fn undo(&mut self) {
        if let Some((rope, cursor)) = self.history.undo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.dirty = self.history.is_dirty();
        }
    }

    pub(super) fn redo(&mut self) {
        if let Some((rope, cursor)) = self.history.redo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.dirty = self.history.is_dirty();
        }
    }

    fn copy(&mut self) {
        if let Some(text) = self.buffer.get_selection_text() {
            let _ = self.clipboard.set_text(&text);
        }
    }

    fn cut(&mut self) {
        if let Some(text) = self.buffer.get_selection_text() {
            if self.clipboard.set_text(&text).is_ok() {
                self.delete_selection();
            }
        }
    }

    fn paste(&mut self) {
        if let Ok(text) = self.clipboard.get_text() {
            if !text.is_empty() {
                self.delete_selection();
                self.insert_str(&text);
            }
        }
    }

    fn insert_str(&mut self, s: &str) {
        let parent = self.history.head();
        let op = self.buffer.insert_str_op(s, parent);
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
    }

    pub(super) fn handle_paste(&mut self, text: &str) {
        const PASTE_MAX_SIZE: usize = 10 * 1024 * 1024; // 10MB
        if text.len() > PASTE_MAX_SIZE || text.is_empty() {
            return;
        }
        self.delete_selection();
        self.insert_str(text);
    }
}
