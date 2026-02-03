use crate::kernel::SearchResultItem;
use crate::kernel::{Action, Effect};
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
            Action::SearchSetViewHeight { viewport, height } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.set_view_height(viewport, height),
            },
            Action::SearchAppend(ch) => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.append_query_char(ch),
            },
            Action::SearchBackspace => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.backspace_query(),
            },
            Action::SearchCursorLeft => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.cursor_left(),
            },
            Action::SearchCursorRight => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.cursor_right(),
            },
            Action::SearchToggleCaseSensitive => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.toggle_case_sensitive(),
            },
            Action::SearchToggleRegex => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.toggle_regex(),
            },
            Action::SearchMoveSelection { delta, viewport } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.move_selection(delta, viewport),
            },
            Action::SearchScroll { delta, viewport } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.scroll(delta, viewport),
            },
            Action::SearchClickRow { row, viewport } => super::DispatchResult {
                effects: Vec::new(),
                state_changed: self.state.search.click_row(row, viewport),
            },
            Action::SearchStart => {
                if self.state.search.query.is_empty() {
                    return super::DispatchResult {
                        effects: Vec::new(),
                        state_changed: false,
                    };
                }

                let root = self.state.workspace_root.clone();
                let pattern = self.state.search.query.clone();
                let case_sensitive = self.state.search.case_sensitive;
                let use_regex = self.state.search.use_regex;

                let state_changed = self.state.search.begin_search();
                super::DispatchResult {
                    effects: vec![Effect::StartGlobalSearch {
                        root,
                        pattern,
                        case_sensitive,
                        use_regex,
                    }],
                    state_changed,
                }
            }
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
