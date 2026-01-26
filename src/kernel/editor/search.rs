use crate::kernel::services::ports::SearchMessage;
use crate::kernel::Effect;
use unicode_segmentation::UnicodeSegmentation;

use super::state::{EditorPaneState, SearchBarField, SearchBarMode};

impl super::state::SearchBarState {
    pub fn height(&self) -> u16 {
        if !self.visible {
            0
        } else {
            match self.mode {
                SearchBarMode::Search => 1,
                SearchBarMode::Replace => 2,
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

    fn prev_grapheme_boundary(text: &str, cursor: usize) -> usize {
        let mut prev = 0;
        for (pos, _) in text.grapheme_indices(true) {
            if pos >= cursor {
                break;
            }
            prev = pos;
        }
        prev
    }

    fn next_grapheme_boundary(text: &str, cursor: usize) -> usize {
        for (pos, g) in text.grapheme_indices(true) {
            if pos == cursor {
                return pos + g.len();
            }
            if pos > cursor {
                return pos;
            }
        }
        text.len()
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
        let prev_pos = Self::prev_grapheme_boundary(text, cursor_pos);
        if prev_pos == cursor_pos {
            return false;
        }
        let next_pos = Self::next_grapheme_boundary(text, prev_pos);
        text.drain(prev_pos..next_pos);
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
        let next_pos = Self::next_grapheme_boundary(text, cursor_pos);
        if next_pos == cursor_pos {
            return false;
        }
        text.drain(cursor_pos..next_pos);
        true
    }

    pub fn cursor_left(&mut self) -> bool {
        if self.cursor_pos == 0 {
            return false;
        }

        let text = self.current_text();
        let new_pos = Self::prev_grapheme_boundary(text, self.cursor_pos);
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
        let new_pos = Self::next_grapheme_boundary(text, self.cursor_pos);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::editor::SearchBarState;
    use crate::kernel::services::ports::{Match, SearchMessage};

    #[test]
    fn search_bar_backspace_deletes_grapheme_cluster() {
        let mut state = SearchBarState::default();
        state.show(SearchBarMode::Search);

        state.insert_char('e');
        state.insert_char('\u{301}');
        assert_eq!(state.search_text, "e\u{301}");
        assert_eq!(state.cursor_pos, state.search_text.len());

        assert!(state.delete_backward());
        assert_eq!(state.search_text, "");
        assert_eq!(state.cursor_pos, 0);
    }

    #[test]
    fn search_bar_cursor_moves_by_graphemes() {
        let mut state = SearchBarState::default();
        state.show(SearchBarMode::Search);

        state.insert_char('👍');
        state.insert_char('🏽');
        state.insert_char('a');
        assert_eq!(state.search_text, "👍🏽a");

        let cluster_len = "👍🏽".len();
        assert_eq!(state.cursor_pos, state.search_text.len());

        assert!(state.cursor_left());
        assert_eq!(state.cursor_pos, cluster_len);
        assert!(state.cursor_left());
        assert_eq!(state.cursor_pos, 0);

        assert!(state.cursor_right());
        assert_eq!(state.cursor_pos, cluster_len);
        assert!(state.cursor_right());
        assert_eq!(state.cursor_pos, state.search_text.len());
    }

    #[test]
    fn search_bar_apply_message_ignores_other_search_ids() {
        let mut state = SearchBarState::default();
        state.show(SearchBarMode::Search);
        state.search_text = "foo".to_string();
        state.cursor_pos = state.search_text.len();
        state.searching = true;
        state.active_search_id = Some(1);

        let ignored = SearchMessage::Matches {
            search_id: 2,
            matches: vec![Match::new(0, 1, 0, 0)],
            is_final: false,
        };
        assert!(!state.apply_message(ignored));
        assert!(state.matches.is_empty());

        let accepted = SearchMessage::Matches {
            search_id: 1,
            matches: vec![Match::new(0, 1, 0, 0)],
            is_final: true,
        };
        assert!(state.apply_message(accepted));
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.current_match_index, Some(0));
        assert!(!state.searching);
    }
}
