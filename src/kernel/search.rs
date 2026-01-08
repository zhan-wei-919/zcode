use crate::kernel::services::ports::{FileMatches, GlobalSearchMessage, Match};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchViewport {
    Sidebar,
    BottomPanel,
}

#[derive(Debug, Clone)]
pub struct SearchViewportState {
    pub view_height: usize,
    pub scroll_offset: usize,
}

impl Default for SearchViewportState {
    fn default() -> Self {
        Self {
            view_height: 10,
            scroll_offset: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchFileResult {
    pub path: PathBuf,
    pub matches: Vec<Match>,
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResultItem {
    FileHeader {
        file_index: usize,
    },
    MatchLine {
        file_index: usize,
        match_index: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
    pub query_cursor: usize,
    pub case_sensitive: bool,
    pub use_regex: bool,
    pub searching: bool,
    pub active_search_id: Option<u64>,
    pub files_searched: usize,
    pub files_with_matches: usize,
    pub total_matches: usize,
    pub file_count: usize,
    pub files: Vec<SearchFileResult>,
    pub items: Vec<SearchResultItem>,
    pub selected_index: usize,
    pub sidebar_view: SearchViewportState,
    pub panel_view: SearchViewportState,
    pub last_error: Option<String>,
}

pub struct SearchResultsSnapshot<'a> {
    pub search_text: &'a str,
    pub searching: bool,
    pub total_matches: usize,
    pub file_count: usize,
    pub files_searched: usize,
    pub files_with_matches: usize,
    pub items: &'a [SearchResultItem],
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub last_error: Option<&'a str>,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            query_cursor: 0,
            case_sensitive: false,
            use_regex: false,
            searching: false,
            active_search_id: None,
            files_searched: 0,
            files_with_matches: 0,
            total_matches: 0,
            file_count: 0,
            files: Vec::new(),
            items: Vec::new(),
            selected_index: 0,
            sidebar_view: SearchViewportState::default(),
            panel_view: SearchViewportState::default(),
            last_error: None,
        }
    }
}

impl SearchState {
    pub fn snapshot(&self, viewport: SearchViewport) -> SearchResultsSnapshot<'_> {
        let scroll_offset = self.viewport(viewport).scroll_offset;
        SearchResultsSnapshot {
            search_text: &self.query,
            searching: self.searching,
            total_matches: self.total_matches,
            file_count: self.file_count,
            files_searched: self.files_searched,
            files_with_matches: self.files_with_matches,
            items: &self.items,
            selected_index: self.selected_index,
            scroll_offset,
            last_error: self.last_error.as_deref(),
        }
    }

    pub fn begin_search(&mut self) -> bool {
        if self.query.is_empty() {
            return false;
        }

        self.searching = true;
        self.active_search_id = None;
        self.files_searched = 0;
        self.files_with_matches = 0;
        self.total_matches = 0;
        self.file_count = 0;
        self.files.clear();
        self.items.clear();
        self.selected_index = 0;
        self.sidebar_view.scroll_offset = 0;
        self.panel_view.scroll_offset = 0;
        self.last_error = None;

        true
    }

    pub fn set_active_search_id(&mut self, search_id: u64) -> bool {
        if self.active_search_id == Some(search_id) {
            return false;
        }
        self.active_search_id = Some(search_id);
        true
    }

    pub fn append_query_char(&mut self, ch: char) -> bool {
        if self.query_cursor >= self.query.len() {
            self.query.push(ch);
        } else {
            self.query.insert(self.query_cursor, ch);
        }
        self.query_cursor += ch.len_utf8();
        true
    }

    pub fn backspace_query(&mut self) -> bool {
        if self.query_cursor == 0 {
            return false;
        }
        let prev = self.query[..self.query_cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.query.remove(prev);
        self.query_cursor = prev;
        true
    }

    pub fn cursor_left(&mut self) -> bool {
        if self.query_cursor == 0 {
            return false;
        }
        let prev = self.query[..self.query_cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        if prev == self.query_cursor {
            return false;
        }
        self.query_cursor = prev;
        true
    }

    pub fn cursor_right(&mut self) -> bool {
        if self.query_cursor >= self.query.len() {
            return false;
        }
        let slice = &self.query[self.query_cursor..];
        let mut iter = slice.char_indices();
        iter.next();
        let next = iter
            .next()
            .map(|(i, _)| self.query_cursor + i)
            .unwrap_or(self.query.len());
        if next == self.query_cursor {
            return false;
        }
        self.query_cursor = next;
        true
    }

    pub fn toggle_case_sensitive(&mut self) -> bool {
        self.case_sensitive = !self.case_sensitive;
        true
    }

    pub fn toggle_regex(&mut self) -> bool {
        self.use_regex = !self.use_regex;
        true
    }

    pub fn set_view_height(&mut self, viewport: SearchViewport, height: usize) -> bool {
        let height = height.max(1);
        let view = self.viewport_mut(viewport);
        if view.view_height == height {
            return false;
        }
        view.view_height = height;
        self.clamp_scroll(viewport);
        true
    }

    pub fn move_selection(&mut self, delta: isize, viewport: SearchViewport) -> bool {
        if self.items.is_empty() || delta == 0 {
            return false;
        }

        let prev = self.selected_index;
        let len = self.items.len();

        if delta < 0 {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                self.selected_index = len - 1;
            }
        } else if self.selected_index + 1 < len {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }

        self.keep_row_visible(self.selected_index, viewport);
        self.selected_index != prev
    }

    pub fn click_row(&mut self, row: usize, viewport: SearchViewport) -> bool {
        if row >= self.items.len() {
            return false;
        }
        if self.selected_index == row {
            return false;
        }
        self.selected_index = row;
        self.keep_row_visible(self.selected_index, viewport);
        true
    }

    pub fn scroll(&mut self, delta: isize, viewport: SearchViewport) -> bool {
        if self.items.is_empty() || delta == 0 {
            return false;
        }

        let view_height = self.viewport(viewport).view_height.max(1);
        let max_scroll = self.items.len().saturating_sub(view_height);
        let prev = self.viewport(viewport).scroll_offset;

        if delta > 0 {
            let view = self.viewport_mut(viewport);
            view.scroll_offset = (view.scroll_offset + delta as usize).min(max_scroll);
        } else {
            let view = self.viewport_mut(viewport);
            view.scroll_offset = view.scroll_offset.saturating_sub((-delta) as usize);
        }

        self.viewport(viewport).scroll_offset != prev
    }

    pub fn apply_message(&mut self, msg: GlobalSearchMessage) -> bool {
        let search_id = match &msg {
            GlobalSearchMessage::FileMatches { search_id, .. }
            | GlobalSearchMessage::Progress { search_id, .. }
            | GlobalSearchMessage::Complete { search_id, .. }
            | GlobalSearchMessage::Cancelled { search_id }
            | GlobalSearchMessage::Error { search_id, .. } => *search_id,
        };

        if self.active_search_id != Some(search_id) {
            return false;
        }

        match msg {
            GlobalSearchMessage::FileMatches {
                file_matches: FileMatches { path, matches },
                ..
            } => {
                let had_items = !self.items.is_empty();
                self.total_matches += matches.len();
                self.file_count += 1;

                let file_index = self.files.len();
                self.files.push(SearchFileResult {
                    path: path.clone(),
                    matches,
                    expanded: true,
                });

                self.items.push(SearchResultItem::FileHeader { file_index });
                let match_len = self
                    .files
                    .get(file_index)
                    .map(|f| f.matches.len())
                    .unwrap_or(0);
                self.items.reserve(match_len);
                for match_index in 0..match_len {
                    self.items.push(SearchResultItem::MatchLine {
                        file_index,
                        match_index,
                    });
                }

                if !had_items {
                    self.selected_index = 0;
                    self.sidebar_view.scroll_offset = 0;
                    self.panel_view.scroll_offset = 0;
                }

                true
            }
            GlobalSearchMessage::Progress {
                files_searched,
                files_with_matches,
                ..
            } => {
                let changed = self.files_searched != files_searched
                    || self.files_with_matches != files_with_matches;
                self.files_searched = files_searched;
                self.files_with_matches = files_with_matches;
                changed
            }
            GlobalSearchMessage::Complete {
                total_files,
                total_matches,
                ..
            } => {
                let changed = self.searching
                    || self.files_searched != total_files
                    || self.total_matches != total_matches;
                self.searching = false;
                self.files_searched = total_files;
                self.total_matches = total_matches;
                changed
            }
            GlobalSearchMessage::Cancelled { .. } => {
                let changed = self.searching;
                self.searching = false;
                changed
            }
            GlobalSearchMessage::Error { message, .. } => {
                let changed =
                    self.searching || self.last_error.as_deref() != Some(message.as_str());
                self.searching = false;
                self.last_error = Some(message);
                changed
            }
        }
    }

    pub fn toggle_selected_file_expanded(&mut self) -> bool {
        let Some(item) = self.items.get(self.selected_index).copied() else {
            return false;
        };
        let SearchResultItem::FileHeader { file_index } = item else {
            return false;
        };
        self.toggle_file_expanded(file_index)
    }

    pub fn toggle_file_expanded(&mut self, file_index: usize) -> bool {
        let Some(file) = self.files.get_mut(file_index) else {
            return false;
        };

        if file.expanded {
            file.expanded = false;
            let Some(header_pos) = self.items.iter().position(|i| {
                matches!(
                    *i,
                    SearchResultItem::FileHeader { file_index: idx } if idx == file_index
                )
            }) else {
                return true;
            };

            let start = header_pos + 1;
            let mut end = start;
            while end < self.items.len() {
                match self.items[end] {
                    SearchResultItem::MatchLine {
                        file_index: idx, ..
                    } if idx == file_index => {
                        end += 1;
                    }
                    _ => break,
                }
            }

            let removed = end.saturating_sub(start);
            if removed > 0 {
                self.items.drain(start..end);

                if (start..end).contains(&self.selected_index) {
                    self.selected_index = header_pos;
                } else if self.selected_index >= end {
                    self.selected_index = self.selected_index.saturating_sub(removed);
                }
            }
        } else {
            file.expanded = true;

            let header_pos = self.items.iter().position(|i| {
                matches!(
                    *i,
                    SearchResultItem::FileHeader { file_index: idx } if idx == file_index
                )
            });
            let Some(header_pos) = header_pos else {
                return true;
            };

            let insert_at = header_pos + 1;
            let count = file.matches.len();
            if count > 0 {
                self.items.splice(
                    insert_at..insert_at,
                    (0..count).map(|match_index| SearchResultItem::MatchLine {
                        file_index,
                        match_index,
                    }),
                );

                if self.selected_index > header_pos {
                    self.selected_index = self.selected_index.saturating_add(count);
                }
            }
        }

        self.keep_row_visible(self.selected_index, SearchViewport::Sidebar);
        self.keep_row_visible(self.selected_index, SearchViewport::BottomPanel);

        true
    }

    fn viewport(&self, viewport: SearchViewport) -> &SearchViewportState {
        match viewport {
            SearchViewport::Sidebar => &self.sidebar_view,
            SearchViewport::BottomPanel => &self.panel_view,
        }
    }

    fn viewport_mut(&mut self, viewport: SearchViewport) -> &mut SearchViewportState {
        match viewport {
            SearchViewport::Sidebar => &mut self.sidebar_view,
            SearchViewport::BottomPanel => &mut self.panel_view,
        }
    }

    fn clamp_scroll(&mut self, viewport: SearchViewport) {
        let view_height = self.viewport(viewport).view_height.max(1);
        let max_scroll = self.items.len().saturating_sub(view_height);
        let view = self.viewport_mut(viewport);
        view.scroll_offset = view.scroll_offset.min(max_scroll);
    }

    fn keep_row_visible(&mut self, row: usize, viewport: SearchViewport) {
        let view_height = self.viewport(viewport).view_height.max(1);
        let scroll_offset = self.viewport(viewport).scroll_offset;

        if row < scroll_offset {
            let view = self.viewport_mut(viewport);
            view.scroll_offset = row;
            self.clamp_scroll(viewport);
            return;
        }

        let end = scroll_offset + view_height;
        if row >= end {
            let view = self.viewport_mut(viewport);
            view.scroll_offset = row + 1 - view_height;
        }

        self.clamp_scroll(viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_editing() {
        let mut state = SearchState::default();
        assert!(state.append_query_char('h'));
        assert!(state.append_query_char('i'));
        assert_eq!(state.query, "hi");
        assert_eq!(state.query_cursor, 2);

        assert!(state.cursor_left());
        assert_eq!(state.query_cursor, 1);

        assert!(state.backspace_query());
        assert_eq!(state.query, "i");
        assert_eq!(state.query_cursor, 0);
    }

    #[test]
    fn test_selection_wraps() {
        let mut state = SearchState::default();
        state.files.push(SearchFileResult {
            path: PathBuf::from("a"),
            matches: vec![Match {
                start: 0,
                end: 1,
                line: 0,
                col: 0,
            }],
            expanded: true,
        });
        state
            .items
            .push(SearchResultItem::FileHeader { file_index: 0 });
        state.items.push(SearchResultItem::MatchLine {
            file_index: 0,
            match_index: 0,
        });

        assert_eq!(state.selected_index, 0);
        state.sidebar_view.view_height = 1;
        assert!(state.move_selection(-1, SearchViewport::Sidebar));
        assert_eq!(state.selected_index, 1);
        assert!(state.move_selection(1, SearchViewport::Sidebar));
        assert_eq!(state.selected_index, 0);
    }
}
