use crate::kernel::Effect;
use crate::kernel::services::ports::SearchMessage;

use super::state::{EditorPaneState, SearchBarField, SearchBarMode};

impl super::state::SearchBarState {
    pub fn height(&self) -> u16 {
        if !self.visible {
            0
        } else {
            match self.mode {
                SearchBarMode::Search => 2,
                SearchBarMode::Replace => 3,
            }
        }
    }

    pub fn show(&mut self, mode: SearchBarMode) -> bool {
        let mut changed = false;
        if !self.visible {
            self.visible = true;
            changed = true;
        }
        if self.mode != mode {
            self.mode = mode;
            changed = true;
        }
        if self.focused_field != SearchBarField::Search {
            self.focused_field = SearchBarField::Search;
            changed = true;
        }
        if self.cursor_pos != self.search_text.len() {
            self.cursor_pos = self.search_text.len();
            changed = true;
        }
        changed
    }

    pub fn hide(&mut self) -> bool {
        if !self.visible {
            return false;
        }
        self.visible = false;
        self.matches.clear();
        self.current_match_index = None;
        self.searching = false;
        self.active_search_id = None;
        self.last_error = None;
        true
    }

    pub fn toggle_replace_mode(&mut self) -> bool {
        let prev = self.mode;
        self.mode = match self.mode {
            SearchBarMode::Search => SearchBarMode::Replace,
            SearchBarMode::Replace => SearchBarMode::Search,
        };
        if self.mode == prev {
            return false;
        }
        if self.mode == SearchBarMode::Search && self.focused_field == SearchBarField::Replace {
            self.focused_field = SearchBarField::Search;
            self.cursor_pos = self.search_text.len();
        }
        true
    }

    fn current_text(&self) -> &str {
        match self.focused_field {
            SearchBarField::Search => &self.search_text,
            SearchBarField::Replace => &self.replace_text,
        }
    }

    fn current_text_mut(&mut self) -> &mut String {
        match self.focused_field {
            SearchBarField::Search => &mut self.search_text,
            SearchBarField::Replace => &mut self.replace_text,
        }
    }

    pub fn switch_field(&mut self) -> bool {
        if self.mode != SearchBarMode::Replace {
            return false;
        }
        self.focused_field = match self.focused_field {
            SearchBarField::Search => SearchBarField::Replace,
            SearchBarField::Replace => SearchBarField::Search,
        };
        self.cursor_pos = self.current_text().len();
        true
    }

    pub fn insert_char(&mut self, c: char) -> bool {
        let cursor_pos = self.cursor_pos;
        let text = self.current_text_mut();
        if cursor_pos >= text.len() {
            text.push(c);
        } else {
            text.insert(cursor_pos, c);
        }
        self.cursor_pos += c.len_utf8();
        true
    }

    pub fn delete_backward(&mut self) -> bool {
        if self.cursor_pos == 0 {
            return false;
        }

        let cursor_pos = self.cursor_pos;
        let text = self.current_text_mut();
        let mut char_indices = text.char_indices();
        let mut prev_pos = 0;

        while let Some((pos, _)) = char_indices.next() {
            if pos >= cursor_pos {
                break;
            }
            prev_pos = pos;
        }

        text.remove(prev_pos);
        self.cursor_pos = prev_pos;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        let text_len = self.current_text().len();
        if self.cursor_pos >= text_len {
            return false;
        }

        let cursor_pos = self.cursor_pos;
        let text = self.current_text_mut();
        let mut next_pos = cursor_pos + 1;
        while next_pos < text.len() && !text.is_char_boundary(next_pos) {
            next_pos += 1;
        }

        text.drain(cursor_pos..next_pos);
        true
    }

    pub fn cursor_left(&mut self) -> bool {
        if self.cursor_pos == 0 {
            return false;
        }

        let text = self.current_text();
        let mut new_pos = self.cursor_pos.saturating_sub(1);
        while new_pos > 0 && !text.is_char_boundary(new_pos) {
            new_pos -= 1;
        }
        if new_pos == self.cursor_pos {
            return false;
        }
        self.cursor_pos = new_pos;
        true
    }

    pub fn cursor_right(&mut self) -> bool {
        let text = self.current_text();
        if self.cursor_pos >= text.len() {
            return false;
        }
        let mut new_pos = self.cursor_pos + 1;
        while new_pos < text.len() && !text.is_char_boundary(new_pos) {
            new_pos += 1;
        }
        if new_pos == self.cursor_pos {
            return false;
        }
        self.cursor_pos = new_pos;
        true
    }

    pub fn cursor_home(&mut self) -> bool {
        if self.cursor_pos == 0 {
            return false;
        }
        self.cursor_pos = 0;
        true
    }

    pub fn cursor_end(&mut self) -> bool {
        let end = self.current_text().len();
        if self.cursor_pos == end {
            return false;
        }
        self.cursor_pos = end;
        true
    }

    pub fn begin_search(&mut self) -> bool {
        if self.search_text.is_empty() {
            if self.searching || !self.matches.is_empty() || self.current_match_index.is_some() {
                self.searching = false;
                self.matches.clear();
                self.current_match_index = None;
                self.active_search_id = None;
                self.last_error = None;
                return true;
            }
            return false;
        }

        self.searching = true;
        self.active_search_id = None;
        self.matches.clear();
        self.current_match_index = None;
        self.last_error = None;
        true
    }

    pub fn apply_message(&mut self, msg: SearchMessage) -> bool {
        let search_id = match &msg {
            SearchMessage::Matches { search_id, .. }
            | SearchMessage::Complete { search_id, .. }
            | SearchMessage::Cancelled { search_id }
            | SearchMessage::Error { search_id, .. } => *search_id,
        };

        if self.active_search_id.is_some() && self.active_search_id != Some(search_id) {
            return false;
        }

        match msg {
            SearchMessage::Matches {
                matches, is_final, ..
            } => {
                self.matches.extend(matches);
                if self.current_match_index.is_none() && !self.matches.is_empty() {
                    self.current_match_index = Some(0);
                }
                if is_final {
                    self.searching = false;
                }
                true
            }
            SearchMessage::Complete { total, .. } => {
                self.searching = false;
                if total == 0 {
                    self.current_match_index = None;
                } else if self.current_match_index.is_none() && !self.matches.is_empty() {
                    self.current_match_index = Some(0);
                }
                true
            }
            SearchMessage::Cancelled { .. } => {
                self.searching = false;
                true
            }
            SearchMessage::Error { message, .. } => {
                self.searching = false;
                self.last_error = Some(message);
                true
            }
        }
    }
}

impl EditorPaneState {
    pub fn trigger_search(&mut self, pane: usize) -> Option<Effect> {
        if !self.search_bar.visible {
            return None;
        }
        if self.search_bar.search_text.is_empty() {
            return None;
        }
        let tab = self.active_tab()?;
        Some(Effect::StartEditorSearch {
            pane,
            rope: tab.buffer.rope().clone(),
            pattern: self.search_bar.search_text.clone(),
            case_sensitive: self.search_bar.case_sensitive,
            use_regex: self.search_bar.use_regex,
        })
    }
}
