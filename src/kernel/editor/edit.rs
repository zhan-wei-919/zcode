use crate::core::Command;
use crate::models::{slice_to_cow, Granularity, Selection};
use crate::kernel::services::ports::Match;
use unicode_segmentation::UnicodeSegmentation;

use super::state::EditorTabState;
use super::viewport;

fn is_word_boundary_char(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '（' | '）' | '【' | '】' | '「' | '」' | '，' | '。' | '：' | '；'
        )
}

impl EditorTabState {
    pub fn apply_command(
        &mut self,
        command: Command,
        pane: usize,
        tab_size: u8,
    ) -> (bool, Vec<crate::kernel::Effect>) {
        use crate::kernel::Effect;

        self.viewport.follow_cursor = true;

        match command {
            Command::Undo => {
                let changed = self.undo(tab_size);
                (changed, Vec::new())
            }
            Command::Redo => {
                let changed = self.redo(tab_size);
                (changed, Vec::new())
            }
            Command::Copy => self.copy(),
            Command::Cut => self.cut(tab_size),
            Command::Paste => (
                false,
                vec![Effect::RequestClipboardText { pane }],
            ),
            Command::ExtendSelectionLeft => {
                let changed = self.extend_selection_left(tab_size);
                (changed, Vec::new())
            }
            Command::ExtendSelectionRight => {
                let changed = self.extend_selection_right(tab_size);
                (changed, Vec::new())
            }
            Command::ExtendSelectionUp => {
                let changed = self.extend_selection_up(tab_size);
                (changed, Vec::new())
            }
            Command::ExtendSelectionDown => {
                let changed = self.extend_selection_down(tab_size);
                (changed, Vec::new())
            }
            Command::ExtendSelectionLineStart => {
                let changed = self.extend_selection_to_line_start(tab_size);
                (changed, Vec::new())
            }
            Command::ExtendSelectionLineEnd => {
                let changed = self.extend_selection_to_line_end(tab_size);
                (changed, Vec::new())
            }
            Command::ExtendSelectionWordLeft => {
                let changed = self.extend_selection_word_left(tab_size);
                (changed, Vec::new())
            }
            Command::ExtendSelectionWordRight => {
                let changed = self.extend_selection_word_right(tab_size);
                (changed, Vec::new())
            }
            cmd if cmd.is_cursor_command() || cmd.is_selection_command() || cmd.is_edit_command() => {
                let changed = self.execute(cmd, tab_size);
                (changed, Vec::new())
            }
            _ => (false, Vec::new()),
        }
    }

    fn execute(&mut self, command: Command, tab_size: u8) -> bool {
        match command {
            Command::CursorLeft => self.cursor_left(tab_size),
            Command::CursorRight => self.cursor_right(tab_size),
            Command::CursorUp => self.cursor_up(tab_size),
            Command::CursorDown => self.cursor_down(tab_size),
            Command::CursorLineStart => {
                let (row, _) = self.buffer.cursor();
                self.buffer.set_cursor(row, 0);
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                true
            }
            Command::CursorLineEnd => {
                let (row, _) = self.buffer.cursor();
                let len = self.buffer.line_grapheme_len(row);
                self.buffer.set_cursor(row, len);
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                true
            }
            Command::CursorFileStart => {
                self.buffer.set_cursor(0, 0);
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                true
            }
            Command::CursorFileEnd => {
                let last = self.buffer.len_lines().saturating_sub(1);
                let len = self.buffer.line_grapheme_len(last);
                self.buffer.set_cursor(last, len);
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                true
            }
            Command::PageUp => {
                let height = self.viewport.height.max(1);
                self.viewport.follow_cursor = false;
                let total = self.buffer.len_lines();
                self.viewport.line_offset = self.viewport.line_offset.saturating_sub(height);
                self.viewport.line_offset = self
                    .viewport
                    .line_offset
                    .min(total.saturating_sub(height));
                let (row, col) = self.buffer.cursor();
                self.buffer.set_cursor(row.saturating_sub(height), col);
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                true
            }
            Command::PageDown => {
                let height = self.viewport.height.max(1);
                self.viewport.follow_cursor = false;
                let total = self.buffer.len_lines();
                let max_offset = total.saturating_sub(height);
                self.viewport.line_offset = (self.viewport.line_offset + height).min(max_offset);
                let (row, col) = self.buffer.cursor();
                let new_row = (row + height).min(total.saturating_sub(1));
                self.buffer.set_cursor(new_row, col);
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                true
            }
            Command::InsertChar(c) => {
                let mut changed = self.delete_selection(tab_size);
                changed |= self.insert_char(c, tab_size);
                changed
            }
            Command::InsertNewline => {
                let mut changed = self.delete_selection(tab_size);
                changed |= self.insert_char('\n', tab_size);
                changed
            }
            Command::InsertTab => {
                let mut changed = self.delete_selection(tab_size);
                changed |= self.insert_char('\t', tab_size);
                changed
            }
            Command::DeleteBackward => {
                if !self.delete_selection(tab_size) {
                    self.delete_backward(tab_size)
                } else {
                    true
                }
            }
            Command::DeleteForward => {
                if !self.delete_selection(tab_size) {
                    self.delete_forward(tab_size)
                } else {
                    true
                }
            }
            Command::ClearSelection => {
                if self.buffer.selection().is_some() {
                    self.buffer.clear_selection();
                    true
                } else {
                    false
                }
            }
            Command::CursorWordLeft => self.cursor_word_left(tab_size),
            Command::CursorWordRight => self.cursor_word_right(tab_size),
            Command::SelectAll => self.select_all(tab_size),
            _ => false,
        }
    }

    fn cursor_left(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        if col > 0 {
            self.buffer.set_cursor(row, col - 1);
        } else if row > 0 {
            let prev_len = self.buffer.line_grapheme_len(row - 1);
            self.buffer.set_cursor(row - 1, prev_len);
        }
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_right(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let line_len = self.buffer.line_grapheme_len(row);
        if col < line_len {
            self.buffer.set_cursor(row, col + 1);
        } else if row + 1 < self.buffer.len_lines() {
            self.buffer.set_cursor(row + 1, 0);
        }
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_up(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        if row == 0 {
            return false;
        }
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(row - 1);
        self.buffer.set_cursor(row - 1, col.min(new_len));
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_down(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        if row + 1 >= self.buffer.len_lines() {
            return false;
        }
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(row + 1);
        self.buffer.set_cursor(row + 1, col.min(new_len));
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_word_left(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);

        if col == 0 {
            if row > 0 {
                let prev_len = self.buffer.line_grapheme_len(row - 1);
                self.buffer.set_cursor(row - 1, prev_len);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                self.buffer.update_selection_cursor(self.buffer.cursor());
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
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
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_word_right(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            if row + 1 < self.buffer.len_lines() {
                self.buffer.set_cursor(row + 1, 0);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                self.buffer.update_selection_cursor(self.buffer.cursor());
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
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
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn select_all(&mut self, tab_size: u8) -> bool {
        let last_line = self.buffer.len_lines().saturating_sub(1);
        let last_col = self.buffer.line_grapheme_len(last_line);

        let mut selection = Selection::new((0, 0), Granularity::Char);
        selection.update_cursor((last_line, last_col), self.buffer.rope());
        self.buffer.set_selection(Some(selection));
        self.buffer.set_cursor(last_line, last_col);
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    fn ensure_selection(&mut self) {
        if self.buffer.selection().is_none() {
            let pos = self.buffer.cursor();
            self.buffer
                .set_selection(Some(Selection::new(pos, Granularity::Char)));
        }
    }

    fn extend_selection_left(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
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
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_right(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
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
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_up(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        if row == 0 {
            return false;
        }
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(row - 1);
        let new_pos = (row - 1, col.min(new_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_down(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        if row + 1 >= self.buffer.len_lines() {
            return false;
        }
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(row + 1);
        let new_pos = (row + 1, col.min(new_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_to_line_start(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let new_pos = (row, 0);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_to_line_end(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let len = self.buffer.line_grapheme_len(row);
        let new_pos = (row, len);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_word_left(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);

        if col == 0 {
            if row > 0 {
                let prev_len = self.buffer.line_grapheme_len(row - 1);
                let new_pos = (row - 1, prev_len);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
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
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_word_right(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            if row + 1 < self.buffer.len_lines() {
                let new_pos = (row + 1, 0);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
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
        let changed = self.buffer.cursor() != prev;
        if changed {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn insert_char(&mut self, c: char, tab_size: u8) -> bool {
        let parent = self.history.head();
        let op = self.buffer.insert_char_op(c, parent);
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn insert_text(&mut self, text: &str, tab_size: u8) -> bool {
        const PASTE_MAX_SIZE: usize = 10 * 1024 * 1024;
        if text.is_empty() || text.len() > PASTE_MAX_SIZE {
            return false;
        }
        let _ = self.delete_selection(tab_size);
        let parent = self.history.head();
        let op = self.buffer.insert_str_op(text, parent);
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    fn delete_backward(&mut self, tab_size: u8) -> bool {
        let parent = self.history.head();
        let op = self.buffer.delete_backward_op(parent);
        if let Some(op) = op {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            true
        } else {
            false
        }
    }

    fn delete_forward(&mut self, tab_size: u8) -> bool {
        let parent = self.history.head();
        let op = self.buffer.delete_forward_op(parent);
        if let Some(op) = op {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            true
        } else {
            false
        }
    }

    fn delete_selection(&mut self, tab_size: u8) -> bool {
        let parent = self.history.head();
        if let Some(op) = self.buffer.delete_selection_op(parent) {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            return true;
        }
        self.buffer.clear_selection();
        false
    }

    fn undo(&mut self, tab_size: u8) -> bool {
        if let Some((rope, cursor)) = self.history.undo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.dirty = self.history.is_dirty();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            return true;
        }
        false
    }

    fn redo(&mut self, tab_size: u8) -> bool {
        if let Some((rope, cursor)) = self.history.redo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.dirty = self.history.is_dirty();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            return true;
        }
        false
    }

    fn copy(&mut self) -> (bool, Vec<crate::kernel::Effect>) {
        use crate::kernel::Effect;
        let Some(text) = self.buffer.get_selection_text() else {
            return (false, Vec::new());
        };
        (false, vec![Effect::SetClipboardText(text)])
    }

    fn cut(&mut self, tab_size: u8) -> (bool, Vec<crate::kernel::Effect>) {
        use crate::kernel::Effect;
        let Some(text) = self.buffer.get_selection_text() else {
            return (false, Vec::new());
        };
        let changed = self.delete_selection(tab_size);
        (changed, vec![Effect::SetClipboardText(text)])
    }

    pub fn replace_current_match(&mut self, m: &Match, replace: &str, tab_size: u8) -> bool {
        if replace.is_empty() {
            return false;
        }
        let rope = self.buffer.rope();
        let start_char = rope.byte_to_char(m.start);
        let end_char = rope.byte_to_char(m.end);
        self.buffer.replace_range(start_char, end_char, replace);
        self.dirty = true;
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn on_saved(&mut self) {
        self.history.on_save(self.buffer.rope());
        self.dirty = false;
    }
}
