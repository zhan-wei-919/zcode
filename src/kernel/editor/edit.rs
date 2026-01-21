use crate::core::Command;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::Match;
use crate::models::{slice_to_cow, EditOp, Granularity, Selection};
use unicode_segmentation::UnicodeSegmentation;

use super::state::EditorTabState;
use super::viewport;
use super::LanguageId;

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
        config: &EditorConfig,
    ) -> (bool, Vec<crate::kernel::Effect>) {
        use crate::kernel::Effect;

        self.viewport.follow_cursor = true;
        let tab_size = config.tab_size;

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
            Command::Paste => (false, vec![Effect::RequestClipboardText { pane }]),
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
            cmd if cmd.is_cursor_command()
                || cmd.is_selection_command()
                || cmd.is_edit_command() =>
            {
                let changed = self.execute(cmd, config);
                (changed, Vec::new())
            }
            _ => (false, Vec::new()),
        }
    }

    fn execute(&mut self, command: Command, config: &EditorConfig) -> bool {
        let tab_size = config.tab_size;
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
                self.viewport.line_offset =
                    self.viewport.line_offset.min(total.saturating_sub(height));
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
                let had_selection = self.buffer.has_selection();
                let mut changed = self.delete_selection(tab_size);
                if !had_selection && self.try_skip_closing(c, tab_size) {
                    return true;
                }
                if config.auto_indent
                    && self.language() == Some(LanguageId::Rust)
                    && !self.in_string_or_comment()
                {
                    match c {
                        '{' => changed |= self.insert_brace_pair(tab_size),
                        '(' => changed |= self.insert_pair('(', ')', tab_size),
                        '[' => changed |= self.insert_pair('[', ']', tab_size),
                        '"' => changed |= self.insert_pair('"', '"', tab_size),
                        '\'' => changed |= self.insert_pair('\'', '\'', tab_size),
                        _ => changed |= self.insert_char(c, tab_size),
                    }
                } else {
                    changed |= self.insert_char(c, tab_size);
                }
                changed
            }
            Command::InsertNewline => {
                if config.auto_indent
                    && self.language() == Some(LanguageId::Rust)
                    && !self.in_string_or_comment()
                    && self.expand_empty_brace_pair(tab_size)
                {
                    return true;
                }
                let mut changed = self.delete_selection(tab_size);
                if config.auto_indent {
                    changed |= self.insert_newline_with_indent(tab_size);
                } else {
                    changed |= self.insert_char('\n', tab_size);
                }
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

    fn in_string_or_comment(&mut self) -> bool {
        let char_offset = self.buffer.cursor_char_offset();
        let byte_offset = self.buffer.rope().char_to_byte(char_offset);
        self.syntax()
            .is_some_and(|syntax| syntax.is_in_string_or_comment(byte_offset))
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

    fn commit_op(&mut self, op: EditOp, tab_size: u8) {
        self.apply_syntax_edit(&op);
        self.last_edit_op = Some(op.clone());
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        self.bump_version();
    }

    fn insert_brace_pair(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let cursor_char_offset = self.buffer.cursor_char_offset();
        let parent = self.history.head();

        let op = self.buffer.insert_str_op_with_cursor_after_char_offset(
            "{}",
            (row, col.saturating_add(1)),
            cursor_char_offset.saturating_add(1),
            parent,
        );

        self.commit_op(op, tab_size);
        true
    }

    fn try_skip_closing(&mut self, c: char, tab_size: u8) -> bool {
        if !matches!(c, ')' | ']' | '}' | '"' | '\'') {
            return false;
        }

        let cursor_char_offset = self.buffer.cursor_char_offset();
        let rope = self.buffer.rope();
        if cursor_char_offset >= rope.len_chars() {
            return false;
        }

        let next = rope.char(cursor_char_offset);
        if next != c {
            return false;
        }

        self.cursor_right(tab_size)
    }

    fn insert_pair(&mut self, open: char, close: char, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let cursor_char_offset = self.buffer.cursor_char_offset();
        let parent = self.history.head();

        let mut text = String::with_capacity(2);
        text.push(open);
        text.push(close);

        let op = self.buffer.insert_str_op_with_cursor_after_char_offset(
            &text,
            (row, col.saturating_add(1)),
            cursor_char_offset.saturating_add(1),
            parent,
        );

        self.commit_op(op, tab_size);
        true
    }

    fn insert_newline_with_indent(&mut self, tab_size: u8) -> bool {
        let row = self.buffer.cursor().0;
        let cursor_char_offset = self.buffer.cursor_char_offset();
        let rope = self.buffer.rope();
        let line_start = rope.line_to_char(row);
        let before_cursor = rope.slice(line_start..cursor_char_offset).to_string();

        let mut indent = String::new();
        for ch in before_cursor.chars() {
            if ch == ' ' || ch == '\t' {
                indent.push(ch);
            } else {
                break;
            }
        }

        let trimmed = before_cursor
            .trim_end_matches(|c| c == ' ' || c == '\t');
        if trimmed.ends_with('{')
            && self.language() == Some(LanguageId::Rust)
            && !self.in_string_or_comment()
        {
            indent.push_str(&" ".repeat(tab_size as usize));
        }

        let mut text = String::with_capacity(1 + indent.len());
        text.push('\n');
        text.push_str(&indent);

        let parent = self.history.head();
        let op = self.buffer.insert_str_op(&text, parent);
        self.commit_op(op, tab_size);
        true
    }

    fn expand_empty_brace_pair(&mut self, tab_size: u8) -> bool {
        if self.buffer.has_selection() {
            return false;
        }

        let (row, col) = self.buffer.cursor();
        let Some(slice) = self.buffer.line_slice(row) else {
            return false;
        };

        let line_cow = slice_to_cow(slice);
        let line = line_cow.strip_suffix('\n').unwrap_or(&line_cow);
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let len = graphemes.len();
        let col = col.min(len);

        let is_ws = |g: &str| g.chars().all(|c| c.is_whitespace());

        let mut left = None;
        for i in (0..col).rev() {
            if !is_ws(graphemes[i]) {
                left = Some(i);
                break;
            }
        }
        let mut right = None;
        for i in col..len {
            if !is_ws(graphemes[i]) {
                right = Some(i);
                break;
            }
        }

        let (left, right) = match (left, right) {
            (Some(l), Some(r)) => (l, r),
            _ => return false,
        };

        if graphemes[left] != "{" || graphemes[right] != "}" || left >= right {
            return false;
        }
        if !(left + 1..right).all(|i| is_ws(graphemes[i])) {
            return false;
        }

        let indent_end = line
            .bytes()
            .position(|b| b != b' ' && b != b'\t')
            .unwrap_or(line.len());
        let base_indent = &line[..indent_end];
        let base_indent_chars = base_indent.chars().count();
        const INDENT_SPACES: usize = 4;

        let mut inserted =
            String::with_capacity(1 + base_indent.len() + INDENT_SPACES + 1 + base_indent.len());
        inserted.push('\n');
        inserted.push_str(base_indent);
        inserted.push_str("    ");
        inserted.push('\n');
        inserted.push_str(base_indent);

        let start_char = self.buffer.pos_to_char((row, left + 1));
        let end_char = self.buffer.pos_to_char((row, right));

        let cursor_after = (row.saturating_add(1), base_indent_chars + INDENT_SPACES);
        let cursor_after_char_offset = start_char + 1 + base_indent_chars + INDENT_SPACES;
        let parent = self.history.head();

        let op = self.buffer.replace_range_op(
            start_char,
            end_char,
            &inserted,
            cursor_after,
            cursor_after_char_offset,
            parent,
        );

        self.commit_op(op, tab_size);
        true
    }

    fn insert_char(&mut self, c: char, tab_size: u8) -> bool {
        let parent = self.history.head();
        let op = self.buffer.insert_char_op(c, parent);
        self.commit_op(op, tab_size);
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
        self.commit_op(op, tab_size);
        true
    }

    fn delete_backward(&mut self, tab_size: u8) -> bool {
        let parent = self.history.head();
        let op = self.buffer.delete_backward_op(parent);
        if let Some(op) = op {
            self.commit_op(op, tab_size);
            true
        } else {
            false
        }
    }

    fn delete_forward(&mut self, tab_size: u8) -> bool {
        let parent = self.history.head();
        let op = self.buffer.delete_forward_op(parent);
        if let Some(op) = op {
            self.commit_op(op, tab_size);
            true
        } else {
            false
        }
    }

    fn delete_selection(&mut self, tab_size: u8) -> bool {
        let parent = self.history.head();
        if let Some(op) = self.buffer.delete_selection_op(parent) {
            self.commit_op(op, tab_size);
            return true;
        }
        self.buffer.clear_selection();
        false
    }

    fn undo(&mut self, tab_size: u8) -> bool {
        if let Some((rope, cursor)) = self.history.undo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.reparse_syntax();
            self.dirty = self.history.is_dirty();
            self.last_edit_op = None;
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            self.bump_version();
            return true;
        }
        false
    }

    fn redo(&mut self, tab_size: u8) -> bool {
        if let Some((rope, cursor)) = self.history.redo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.reparse_syntax();
            self.dirty = self.history.is_dirty();
            self.last_edit_op = None;
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            self.bump_version();
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
        self.reparse_syntax();
        self.dirty = true;
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        self.last_edit_op = None;
        self.bump_version();
        true
    }

    pub fn on_saved(&mut self) {
        self.history.on_save(self.buffer.rope());
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_rust_brace_pair_and_electric_enter() {
        let config = EditorConfig::default();
        let mut tab = EditorTabState::from_file(PathBuf::from("test.rs"), "fn main() ", &config);

        let end = tab.buffer.line_grapheme_len(0);
        tab.buffer.set_cursor(0, end);

        let _ = tab.apply_command(Command::InsertChar('{'), 0, &config);
        assert_eq!(tab.buffer.text(), "fn main() {}");
        assert_eq!(tab.buffer.cursor(), (0, "fn main() {".len()));

        let _ = tab.apply_command(Command::InsertNewline, 0, &config);
        assert_eq!(tab.buffer.text(), "fn main() {\n    \n}");
        assert_eq!(tab.buffer.cursor(), (1, 4));
    }

    #[test]
    fn test_electric_enter_with_whitespace_between_braces() {
        let config = EditorConfig::default();
        let mut tab = EditorTabState::from_file(PathBuf::from("test.rs"), "fn main() ", &config);

        let end = tab.buffer.line_grapheme_len(0);
        tab.buffer.set_cursor(0, end);
        let _ = tab.apply_command(Command::InsertChar('{'), 0, &config);

        let _ = tab.apply_command(Command::InsertChar(' '), 0, &config);
        let _ = tab.apply_command(Command::InsertChar(' '), 0, &config);
        assert_eq!(tab.buffer.text(), "fn main() {  }");

        let _ = tab.apply_command(Command::InsertNewline, 0, &config);
        assert_eq!(tab.buffer.text(), "fn main() {\n    \n}");
        assert_eq!(tab.buffer.cursor(), (1, 4));
    }

    #[test]
    fn test_auto_pair_and_skip_closing() {
        let config = EditorConfig::default();
        let mut tab = EditorTabState::from_file(PathBuf::from("test.rs"), "", &config);

        let _ = tab.apply_command(Command::InsertChar('('), 0, &config);
        assert_eq!(tab.buffer.text(), "()");
        assert_eq!(tab.buffer.cursor(), (0, 1));

        let _ = tab.apply_command(Command::InsertChar(')'), 0, &config);
        assert_eq!(tab.buffer.text(), "()");
        assert_eq!(tab.buffer.cursor(), (0, 2));

        tab = EditorTabState::from_file(PathBuf::from("test.rs"), "", &config);
        let _ = tab.apply_command(Command::InsertChar('"'), 0, &config);
        assert_eq!(tab.buffer.text(), "\"\"");
        assert_eq!(tab.buffer.cursor(), (0, 1));

        let _ = tab.apply_command(Command::InsertChar('"'), 0, &config);
        assert_eq!(tab.buffer.text(), "\"\"");
        assert_eq!(tab.buffer.cursor(), (0, 2));

        tab = EditorTabState::from_file(PathBuf::from("test.rs"), "", &config);
        let _ = tab.apply_command(Command::InsertChar('\''), 0, &config);
        assert_eq!(tab.buffer.text(), "''");
        assert_eq!(tab.buffer.cursor(), (0, 1));

        let _ = tab.apply_command(Command::InsertChar('\''), 0, &config);
        assert_eq!(tab.buffer.text(), "''");
        assert_eq!(tab.buffer.cursor(), (0, 2));
    }
}
