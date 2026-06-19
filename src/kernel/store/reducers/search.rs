use crate::kernel::Action;
use crate::kernel::SearchResultItem;
use std::path::PathBuf;

pub(super) fn search_open_target(
    search: &crate::kernel::SearchState,
    item: SearchResultItem,
) -> Option<(PathBuf, usize)> {
    match item {
        SearchResultItem::FileHeader { file_index } => {
            let file = search.files.get(file_index)?;
            let byte_offset = file.matches.first().map(|m| m.start).unwrap_or(0);
            Some((file.path.clone(), byte_offset))
        }
        SearchResultItem::MatchLine {
            file_index,
            match_index,
        } => {
            let file = search.files.get(file_index)?;
            let m = file.matches.get(match_index)?;
            Some((file.path.clone(), m.start))
        }
    }
}

impl super::Store {
    pub(super) fn reduce_search_action(&mut self, action: Action) -> super::DispatchResult {
        match action {
            Action::SearchSetViewHeight { height } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.set_view_height(height),
            },
            Action::SearchAppend(ch) => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.append_query_char(ch),
            },
            Action::SearchBackspace => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.backspace_query(),
            },
            Action::SearchClickRow { row } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.click_row(row),
            },
            Action::SearchStarted { search_id } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.set_active_search_id(search_id),
            },
            Action::SearchMessage(msg) => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.apply_message(msg),
            },
            _ => unreachable!("non-search action passed to reduce_search_action"),
        }
    }
}
