use super::super::search_bar::SearchBarMode;
use super::EditorGroup;

impl EditorGroup {
    pub fn toggle_search(&mut self) {
        self.search_bar.toggle();
        if self.search_bar.is_visible() {
            self.trigger_search();
        }
    }

    pub fn show_search(&mut self) {
        self.search_bar.show(SearchBarMode::Search);
    }

    pub fn show_replace(&mut self) {
        self.search_bar.show(SearchBarMode::Replace);
    }

    pub fn hide_search(&mut self) {
        self.search_bar.hide();
    }

    pub(super) fn trigger_search(&mut self) {
        if let Some(editor) = self.active_editor() {
            let rope = editor.buffer().rope().clone();
            self.search_bar.search(&rope);
        }
    }

    fn goto_current_match(&mut self) {
        if let Some(m) = self.search_bar.current_match() {
            let line = m.line;
            let col = m.col;
            if let Some(editor) = self.active_editor_mut() {
                editor.buffer_mut().set_cursor(line, col);
            }
        }
    }

    pub(super) fn find_next(&mut self) {
        self.search_bar.next_match();
        self.goto_current_match();
    }

    pub(super) fn find_prev(&mut self) {
        self.search_bar.prev_match();
        self.goto_current_match();
    }

    pub(super) fn replace_current(&mut self) {
        let replace_text = self.search_bar.replace_text().to_string();
        if replace_text.is_empty() {
            return;
        }

        if let Some(m) = self.search_bar.current_match() {
            let start_char = m.start;
            let end_char = m.end;

            if let Some(editor) = self.active_editor_mut() {
                editor
                    .buffer_mut()
                    .replace_range(start_char, end_char, &replace_text);
                editor.set_dirty(true);
            }

            self.trigger_search();
        }
    }

    pub(super) fn replace_all(&mut self) {
        let search_text = self.search_bar.search_text().to_string();
        let replace_text = self.search_bar.replace_text().to_string();
        let case_sensitive = self.search_bar.case_sensitive();

        if search_text.is_empty() {
            return;
        }

        if let Some(editor) = self.active_editor_mut() {
            let text = editor.buffer().text();
            let new_text = if case_sensitive {
                text.replace(&search_text, &replace_text)
            } else {
                let mut result = text.clone();
                let lower_search = search_text.to_lowercase();
                let mut offset = 0;

                while let Some(pos) = result[offset..].to_lowercase().find(&lower_search) {
                    let actual_pos = offset + pos;
                    result.replace_range(actual_pos..actual_pos + search_text.len(), &replace_text);
                    offset = actual_pos + replace_text.len();
                }
                result
            };

            editor.set_content(&new_text);
            editor.set_dirty(true);
        }

        self.trigger_search();
    }
}
