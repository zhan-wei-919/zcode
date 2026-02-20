use crate::core::Command;
use crate::kernel::services::ports::SearchMessage;
use crate::kernel::Effect;

use super::action::EditorAction;
use super::state::{
    DiskState, EditorPaneState, EditorState, ReloadCause, ReloadRequest, SearchBarMode, TabId,
};
use super::viewport;

impl EditorState {
    pub fn dispatch_action(&mut self, action: EditorAction) -> (bool, Vec<Effect>) {
        match action {
            EditorAction::OpenFile {
                pane,
                path,
                content,
            } => self.open_file(pane, path, content),
            EditorAction::GotoByteOffset { pane, byte_offset } => {
                self.goto_byte_offset(pane, byte_offset)
            }
            EditorAction::SetActiveTab { pane, index } => self.set_active_tab(pane, index),
            EditorAction::SetViewportSize {
                pane,
                width,
                height,
            } => self.set_viewport_size(pane, width, height),
            EditorAction::InsertText { pane, text } => self.insert_text(pane, &text),
            EditorAction::ApplyTextEdit {
                pane,
                start_byte,
                end_byte,
                text,
            } => self.apply_text_edit(pane, start_byte, end_byte, &text),
            EditorAction::ApplyTextEditToTab {
                pane,
                tab_index,
                start_byte,
                end_byte,
                text,
            } => self.apply_text_edit_to_tab(pane, tab_index, start_byte, end_byte, &text),
            EditorAction::ReplaceRangeChars {
                pane,
                start_char,
                end_char,
                text,
            } => self.replace_range_chars(pane, start_char, end_char, &text),
            EditorAction::PlaceCursor {
                pane,
                row,
                col,
                granularity,
            } => self.place_cursor(pane, row, col, granularity),
            EditorAction::ExtendSelection { pane, row, col } => {
                self.extend_selection(pane, row, col)
            }
            EditorAction::EndSelectionGesture { pane } => self.end_selection_gesture(pane),
            EditorAction::Scroll { pane, delta_lines } => self.scroll(pane, delta_lines),
            EditorAction::ScrollHorizontal {
                pane,
                delta_columns,
            } => self.scroll_horizontal(pane, delta_columns),
            EditorAction::SearchBarAppend { pane, ch } => self.search_bar_append(pane, ch),
            EditorAction::SearchBarBackspace { pane } => self.search_bar_backspace(pane),
            EditorAction::SearchBarDeleteForward { pane } => self.search_bar_delete_forward(pane),
            EditorAction::SearchBarCursorLeft { pane } => self.search_bar_cursor_left(pane),
            EditorAction::SearchBarCursorRight { pane } => self.search_bar_cursor_right(pane),
            EditorAction::SearchBarCursorHome { pane } => self.search_bar_cursor_home(pane),
            EditorAction::SearchBarCursorEnd { pane } => self.search_bar_cursor_end(pane),
            EditorAction::SearchBarSwitchField { pane } => self.search_bar_switch_field(pane),
            EditorAction::SearchBarToggleCaseSensitive { pane } => {
                self.search_bar_toggle_case_sensitive(pane)
            }
            EditorAction::SearchBarToggleRegex { pane } => self.search_bar_toggle_regex(pane),
            EditorAction::SearchBarToggleReplaceMode { pane } => {
                self.search_bar_toggle_replace_mode(pane)
            }
            EditorAction::ReplaceCurrent { pane } => self.replace_current(pane),
            EditorAction::ReplaceAll { pane } => self.replace_all(pane),
            EditorAction::SearchStarted { pane, search_id } => self.search_started(pane, search_id),
            EditorAction::SearchMessage { pane, message } => self.search_message(pane, message),
            EditorAction::Saved {
                pane,
                path,
                success,
                version,
            } => self.saved(pane, path, success, version),
            EditorAction::CloseTabAt { pane, index } => self.close_tab_at(pane, index),
            EditorAction::CloseTabsById { pane, tab_ids } => self.close_tabs_by_id(pane, &tab_ids),
            EditorAction::MoveTab {
                tab_id,
                from_pane,
                to_pane,
                to_index,
            } => self.move_tab(tab_id, from_pane, to_pane, to_index),
            EditorAction::FileReloaded { content, request } => self.file_reloaded(request, content),
            EditorAction::FileExternallyModified { path } => self.file_externally_modified(path),
            EditorAction::FileExternallyDeleted { path } => self.file_externally_deleted(path),
            EditorAction::AcceptDiskVersion {
                pane,
                path,
                content,
            } => self.accept_disk_version(pane, path, content),
            EditorAction::KeepMemoryVersion { pane } => self.keep_memory_version(pane),
        }
    }

    pub fn apply_command(&mut self, pane: usize, command: Command) -> (bool, Vec<Effect>) {
        match command {
            Command::NextTab => self.next_tab(pane),
            Command::PrevTab => self.prev_tab(pane),
            Command::CloseTab => self.close_tab(pane),
            Command::Find => self.toggle_search_bar(pane),
            Command::Replace => self.show_replace(pane),
            Command::FindNext => self.find_next(pane),
            Command::FindPrev => self.find_prev(pane),
            Command::Save => self.save(pane),
            Command::EditorSearchBarClose => self.close_search_bar(pane),
            Command::EditorSearchBarSwitchField => {
                self.dispatch_action(EditorAction::SearchBarSwitchField { pane })
            }
            Command::EditorSearchBarToggleCaseSensitive => {
                self.dispatch_action(EditorAction::SearchBarToggleCaseSensitive { pane })
            }
            Command::EditorSearchBarToggleRegex => {
                self.dispatch_action(EditorAction::SearchBarToggleRegex { pane })
            }
            Command::EditorSearchBarToggleReplaceMode => {
                self.dispatch_action(EditorAction::SearchBarToggleReplaceMode { pane })
            }
            Command::EditorSearchBarCursorLeft => {
                self.dispatch_action(EditorAction::SearchBarCursorLeft { pane })
            }
            Command::EditorSearchBarCursorRight => {
                self.dispatch_action(EditorAction::SearchBarCursorRight { pane })
            }
            Command::EditorSearchBarCursorHome => {
                self.dispatch_action(EditorAction::SearchBarCursorHome { pane })
            }
            Command::EditorSearchBarCursorEnd => {
                self.dispatch_action(EditorAction::SearchBarCursorEnd { pane })
            }
            Command::EditorSearchBarBackspace => {
                self.dispatch_action(EditorAction::SearchBarBackspace { pane })
            }
            Command::EditorSearchBarDeleteForward => {
                self.dispatch_action(EditorAction::SearchBarDeleteForward { pane })
            }
            Command::EditorSearchBarReplaceCurrent => {
                self.dispatch_action(EditorAction::ReplaceCurrent { pane })
            }
            Command::EditorSearchBarReplaceAll => {
                self.dispatch_action(EditorAction::ReplaceAll { pane })
            }
            cmd => self.forward_to_active_tab(pane, cmd),
        }
    }

    fn close_search_bar(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }

        let changed = pane_state.search_bar.hide();
        (changed, vec![Effect::CancelEditorSearch { pane }])
    }

    fn open_file(
        &mut self,
        pane: usize,
        path: std::path::PathBuf,
        content: String,
    ) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let default_height = self.config.default_viewport_height;
        let config = self.config.clone();
        let already_open = self
            .panes
            .iter()
            .flat_map(|pane| pane.tabs.iter())
            .any(|tab| tab.path.as_ref() == Some(&path));

        let tab_id = self.alloc_tab_id();
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };

        let (vw, vh) = pane_state
            .active_tab()
            .map(|t| (t.viewport.width, t.viewport.height))
            .unwrap_or((80, default_height));

        let changed = pane_state.open_file(tab_id, path, &content, &config);
        if changed && !already_open {
            self.open_paths_version = self.open_paths_version.saturating_add(1);
        }

        if let Some(active) = pane_state.active_tab_mut() {
            active.viewport.width = vw;
            active.viewport.height = vh;
            viewport::clamp_and_follow(&mut active.viewport, &active.buffer, tab_size);
        }

        let mut effects = Vec::new();
        if changed && pane_state.search_bar.visible {
            let before = pane_state.search_bar.begin_search();
            if before {
                if let Some(effect) = pane_state.trigger_search(pane) {
                    effects.push(effect);
                }
            }
        }

        (changed, effects)
    }

    fn goto_byte_offset(&mut self, pane: usize, byte_offset: usize) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };

        goto_byte_offset(tab, byte_offset, tab_size);
        (true, Vec::new())
    }

    fn set_active_tab(&mut self, pane: usize, index: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };

        let changed = pane_state.set_active(index);
        if !changed {
            return (false, Vec::new());
        }

        let mut effects = Vec::new();
        if pane_state.search_bar.visible {
            let before = pane_state.search_bar.begin_search();
            if before {
                if let Some(effect) = pane_state.trigger_search(pane) {
                    effects.push(effect);
                }
            }
        }

        (true, effects)
    }

    fn set_viewport_size(
        &mut self,
        pane: usize,
        width: usize,
        height: usize,
    ) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };

        let mut changed = pane_state.set_viewport_size(width, height);
        for tab in &mut pane_state.tabs {
            let prev_offset = tab.viewport.line_offset;
            let prev_h = tab.viewport.horiz_offset;
            viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
            changed |=
                tab.viewport.line_offset != prev_offset || tab.viewport.horiz_offset != prev_h;
        }

        (changed, Vec::new())
    }

    fn next_tab(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let changed = pane_state.next_tab();
        if !changed {
            return (false, Vec::new());
        }
        let mut effects = Vec::new();
        if pane_state.search_bar.visible && pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (true, effects)
    }

    fn prev_tab(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let changed = pane_state.prev_tab();
        if !changed {
            return (false, Vec::new());
        }
        let mut effects = Vec::new();
        if pane_state.search_bar.visible && pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (true, effects)
    }

    fn close_tab(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let should_bump = self
            .panes
            .get(pane)
            .and_then(|pane_state| pane_state.active_tab())
            .and_then(|tab| tab.path.as_ref())
            .is_some_and(|path| {
                let count = self
                    .panes
                    .iter()
                    .flat_map(|pane_state| pane_state.tabs.iter())
                    .filter(|tab| tab.path.as_ref() == Some(path))
                    .count();
                count <= 1
            });

        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let changed = pane_state.close_active_tab();
        if !changed {
            return (false, Vec::new());
        }
        if should_bump {
            self.open_paths_version = self.open_paths_version.saturating_add(1);
        }
        let mut effects = Vec::new();
        if pane_state.search_bar.visible && pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (true, effects)
    }

    pub fn close_tab_at(&mut self, pane: usize, index: usize) -> (bool, Vec<Effect>) {
        let should_bump = self
            .panes
            .get(pane)
            .and_then(|pane_state| pane_state.tabs.get(index))
            .and_then(|tab| tab.path.as_ref())
            .is_some_and(|path| {
                let count = self
                    .panes
                    .iter()
                    .flat_map(|pane_state| pane_state.tabs.iter())
                    .filter(|tab| tab.path.as_ref() == Some(path))
                    .count();
                count <= 1
            });

        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let changed = pane_state.close_tab_at(index);
        if !changed {
            return (false, Vec::new());
        }
        if should_bump {
            self.open_paths_version = self.open_paths_version.saturating_add(1);
        }
        let mut effects = Vec::new();
        if pane_state.search_bar.visible && pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (true, effects)
    }

    fn close_tabs_by_id(&mut self, pane: usize, tab_ids: &[u64]) -> (bool, Vec<Effect>) {
        if tab_ids.is_empty() {
            return (false, Vec::new());
        }

        let Some(pane_state) = self.panes.get(pane) else {
            return (false, Vec::new());
        };

        let mut indices = pane_state
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| tab_ids.contains(&tab.id.raw()).then_some(index))
            .collect::<Vec<_>>();
        if indices.is_empty() {
            return (false, Vec::new());
        }

        indices.sort_unstable();

        let mut changed = false;
        let mut effects = Vec::new();
        for index in indices.into_iter().rev() {
            let (closed, mut close_effects) = self.close_tab_at(pane, index);
            changed |= closed;
            effects.append(&mut close_effects);
        }

        (changed, effects)
    }

    fn move_tab(
        &mut self,
        tab_id: TabId,
        from_pane: usize,
        to_pane: usize,
        to_index: usize,
    ) -> (bool, Vec<Effect>) {
        if to_pane >= self.panes.len() {
            return (false, Vec::new());
        }

        let Some((from_pane, from_index)) = self
            .panes
            .get(from_pane)
            .and_then(|pane_state| {
                pane_state
                    .tabs
                    .iter()
                    .position(|t| t.id == tab_id)
                    .map(|idx| (from_pane, idx))
            })
            .or_else(|| {
                self.panes
                    .iter()
                    .enumerate()
                    .find_map(|(pane, pane_state)| {
                        pane_state
                            .tabs
                            .iter()
                            .position(|t| t.id == tab_id)
                            .map(|idx| (pane, idx))
                    })
            })
        else {
            return (false, Vec::new());
        };

        if from_pane == to_pane {
            let pane_state = &mut self.panes[from_pane];
            let len = pane_state.tabs.len();
            if len == 0 || from_index >= len {
                return (false, Vec::new());
            }

            let mut to_index = to_index.min(len);
            if to_index > from_index {
                to_index = to_index.saturating_sub(1);
            }
            if to_index == from_index {
                return (false, Vec::new());
            }

            let moving_active = pane_state.active == from_index;

            let tab = pane_state.tabs.remove(from_index);
            if !moving_active && pane_state.active > from_index {
                pane_state.active = pane_state.active.saturating_sub(1);
            }

            to_index = to_index.min(pane_state.tabs.len());
            pane_state.tabs.insert(to_index, tab);

            if moving_active {
                pane_state.active = to_index;
            } else if pane_state.active >= to_index {
                pane_state.active = pane_state.active.saturating_add(1);
            }

            let mut effects = Vec::new();
            if pane_state.search_bar.visible && pane_state.search_bar.begin_search() {
                if let Some(effect) = pane_state.trigger_search(from_pane) {
                    effects.push(effect);
                }
            }
            return (true, effects);
        }

        let tab = {
            let from_state = &mut self.panes[from_pane];
            if from_index >= from_state.tabs.len() {
                return (false, Vec::new());
            }

            let moving_active = from_state.active == from_index;
            let tab = from_state.tabs.remove(from_index);

            if moving_active {
                if from_state.tabs.is_empty() {
                    from_state.active = 0;
                } else if from_state.active >= from_state.tabs.len() {
                    from_state.active = from_state.tabs.len().saturating_sub(1);
                }
            } else if from_state.active > from_index {
                from_state.active = from_state.active.saturating_sub(1);
            }

            tab
        };

        {
            let to_state = &mut self.panes[to_pane];
            let idx = to_index.min(to_state.tabs.len());
            to_state.tabs.insert(idx, tab);
            to_state.active = idx;
        }

        let mut effects = Vec::new();
        for pane in [from_pane, to_pane] {
            let Some(pane_state) = self.panes.get_mut(pane) else {
                continue;
            };
            if pane_state.search_bar.visible && pane_state.search_bar.begin_search() {
                if let Some(effect) = pane_state.trigger_search(pane) {
                    effects.push(effect);
                }
            }
        }
        (true, effects)
    }

    fn toggle_search_bar(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };

        if pane_state.search_bar.visible {
            let changed = pane_state.search_bar.hide();
            let effects = vec![Effect::CancelEditorSearch { pane }];
            return (changed, effects);
        }

        let changed = pane_state.search_bar.show(SearchBarMode::Search);
        let mut effects = Vec::new();
        if pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (changed, effects)
    }

    fn show_replace(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };

        let changed = pane_state.search_bar.show(SearchBarMode::Replace);
        let mut effects = Vec::new();
        if pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (changed, effects)
    }

    fn find_next(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }

        let Some(len) = (!pane_state.search_bar.matches.is_empty())
            .then_some(pane_state.search_bar.matches.len())
        else {
            return (false, Vec::new());
        };

        let next = match pane_state.search_bar.current_match_index {
            Some(i) => (i + 1) % len,
            None => 0,
        };
        pane_state.search_bar.current_match_index = Some(next);

        let Some(m) = pane_state.search_bar.matches.get(next).copied() else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };

        goto_match(tab, &m, tab_size);
        (true, Vec::new())
    }

    fn find_prev(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }

        let Some(len) = (!pane_state.search_bar.matches.is_empty())
            .then_some(pane_state.search_bar.matches.len())
        else {
            return (false, Vec::new());
        };

        let prev = match pane_state.search_bar.current_match_index {
            Some(i) => {
                if i == 0 {
                    len - 1
                } else {
                    i - 1
                }
            }
            None => len - 1,
        };
        pane_state.search_bar.current_match_index = Some(prev);

        let Some(m) = pane_state.search_bar.matches.get(prev).copied() else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };

        goto_match(tab, &m, tab_size);
        (true, Vec::new())
    }

    fn save(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab() else {
            return (false, Vec::new());
        };
        let Some(path) = tab.path.clone() else {
            return (false, Vec::new());
        };
        let version = tab.edit_version;
        (
            false,
            vec![Effect::WriteFile {
                pane,
                path,
                version,
            }],
        )
    }

    fn forward_to_active_tab(&mut self, pane: usize, command: Command) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        tab.apply_command(command, pane, &self.config)
    }

    fn insert_text(&mut self, pane: usize, text: &str) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        let changed = tab.insert_text(text, tab_size);
        (changed, Vec::new())
    }

    fn apply_text_edit(
        &mut self,
        pane: usize,
        start_byte: usize,
        end_byte: usize,
        text: &str,
    ) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        Self::apply_text_edit_to_tab_state(tab_size, tab, start_byte, end_byte, text)
    }

    fn apply_text_edit_to_tab(
        &mut self,
        pane: usize,
        tab_index: usize,
        start_byte: usize,
        end_byte: usize,
        text: &str,
    ) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.tabs.get_mut(tab_index) else {
            return (false, Vec::new());
        };
        Self::apply_text_edit_to_tab_state(tab_size, tab, start_byte, end_byte, text)
    }

    fn apply_text_edit_to_tab_state(
        tab_size: u8,
        tab: &mut super::state::EditorTabState,
        start_byte: usize,
        end_byte: usize,
        text: &str,
    ) -> (bool, Vec<Effect>) {
        if text.is_empty() && start_byte == end_byte {
            return (false, Vec::new());
        }

        let rope = tab.buffer.rope();
        let len_bytes = rope.len_bytes();
        let start_byte = start_byte.min(len_bytes);
        let end_byte = end_byte.min(len_bytes);
        let mut start_char = rope.byte_to_char(start_byte);
        let mut end_char = rope.byte_to_char(end_byte);
        if start_char > end_char {
            std::mem::swap(&mut start_char, &mut end_char);
        }

        let parent = tab.history.head();
        let op = tab
            .buffer
            .replace_range_op_adjust_cursor(start_char, end_char, text, parent);
        tab.apply_edit_op(op, tab_size);
        (true, Vec::new())
    }

    fn replace_range_chars(
        &mut self,
        pane: usize,
        start_char: usize,
        end_char: usize,
        text: &str,
    ) -> (bool, Vec<Effect>) {
        if text.is_empty() && start_char == end_char {
            return (false, Vec::new());
        }

        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };

        let len_chars = tab.buffer.len_chars();
        let mut start_char = start_char.min(len_chars);
        let mut end_char = end_char.min(len_chars);
        if start_char > end_char {
            std::mem::swap(&mut start_char, &mut end_char);
        }

        let parent = tab.history.head();
        let op = tab
            .buffer
            .replace_range_op_adjust_cursor(start_char, end_char, text, parent);
        tab.apply_edit_op(op, tab_size);
        tab.buffer.clear_selection();
        (true, Vec::new())
    }

    fn place_cursor(
        &mut self,
        pane: usize,
        row: usize,
        col: usize,
        granularity: crate::models::Granularity,
    ) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        let changed = tab.place_cursor(row, col, granularity, tab_size);
        (changed, Vec::new())
    }

    fn extend_selection(&mut self, pane: usize, row: usize, col: usize) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        let changed = tab.extend_selection(row, col, tab_size);
        (changed, Vec::new())
    }

    fn end_selection_gesture(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        let changed = tab.end_selection_gesture();
        (changed, Vec::new())
    }

    fn scroll(&mut self, pane: usize, delta_lines: isize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };

        if delta_lines == 0 {
            return (false, Vec::new());
        }

        tab.viewport.follow_cursor = false;
        let prev = tab.viewport.line_offset;
        let total_lines = tab.buffer.len_lines();
        let height = tab.viewport.height.max(1);
        let max_offset = total_lines.saturating_sub(height);

        if delta_lines > 0 {
            tab.viewport.line_offset =
                (tab.viewport.line_offset + delta_lines as usize).min(max_offset);
        } else {
            tab.viewport.line_offset = tab
                .viewport
                .line_offset
                .saturating_sub((-delta_lines) as usize);
        }

        (tab.viewport.line_offset != prev, Vec::new())
    }

    fn scroll_horizontal(&mut self, pane: usize, delta_columns: isize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };

        if delta_columns == 0 {
            return (false, Vec::new());
        }

        tab.viewport.follow_cursor = false;
        let prev = tab.viewport.horiz_offset;
        let visible_lines =
            tab.visible_lines_in_viewport(tab.viewport.line_offset, tab.viewport.height.max(1));
        let max_visible_width = visible_lines
            .iter()
            .map(|&row| viewport::line_display_width(&tab.buffer, row, self.config.tab_size))
            .max()
            .unwrap_or(0);
        let width = tab.viewport.width.max(1) as u32;
        let max_offset = max_visible_width.saturating_sub(width);

        if delta_columns > 0 {
            tab.viewport.horiz_offset =
                (tab.viewport.horiz_offset + delta_columns as u32).min(max_offset);
        } else {
            tab.viewport.horiz_offset = tab
                .viewport
                .horiz_offset
                .saturating_sub((-delta_columns) as u32);
        }

        (tab.viewport.horiz_offset != prev, Vec::new())
    }

    fn search_started(&mut self, pane: usize, search_id: u64) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        if pane_state.search_bar.active_search_id == Some(search_id) {
            return (false, Vec::new());
        }
        pane_state.search_bar.active_search_id = Some(search_id);
        pane_state.search_bar.searching = true;
        (true, Vec::new())
    }

    fn search_message(&mut self, pane: usize, message: SearchMessage) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        let changed = pane_state.search_bar.apply_message(message);
        (changed, Vec::new())
    }

    fn saved(
        &mut self,
        pane: usize,
        path: std::path::PathBuf,
        success: bool,
        version: u64,
    ) -> (bool, Vec<Effect>) {
        if !success {
            return (false, Vec::new());
        }
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state
            .tabs
            .iter_mut()
            .find(|t| t.path.as_ref() == Some(&path))
        else {
            return (false, Vec::new());
        };
        if tab.edit_version != version {
            return (false, Vec::new());
        }
        tab.on_saved();
        (true, Vec::new())
    }

    fn search_bar_append(&mut self, pane: usize, ch: char) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }

        let old_search = pane_state.search_bar.search_text.clone();
        let old_case = pane_state.search_bar.case_sensitive;
        let old_regex = pane_state.search_bar.use_regex;

        let changed = pane_state.search_bar.insert_char(ch);
        if !changed {
            return (false, Vec::new());
        }

        Self::restart_search_if_needed(pane, pane_state, old_search, old_case, old_regex)
    }

    fn search_bar_backspace(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        let old_search = pane_state.search_bar.search_text.clone();
        let old_case = pane_state.search_bar.case_sensitive;
        let old_regex = pane_state.search_bar.use_regex;

        let changed = pane_state.search_bar.delete_backward();
        if !changed {
            return (false, Vec::new());
        }

        Self::restart_search_if_needed(pane, pane_state, old_search, old_case, old_regex)
    }

    fn search_bar_delete_forward(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        let old_search = pane_state.search_bar.search_text.clone();
        let old_case = pane_state.search_bar.case_sensitive;
        let old_regex = pane_state.search_bar.use_regex;

        let changed = pane_state.search_bar.delete_forward();
        if !changed {
            return (false, Vec::new());
        }

        Self::restart_search_if_needed(pane, pane_state, old_search, old_case, old_regex)
    }

    fn search_bar_cursor_left(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        (pane_state.search_bar.cursor_left(), Vec::new())
    }

    fn search_bar_cursor_right(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        (pane_state.search_bar.cursor_right(), Vec::new())
    }

    fn search_bar_cursor_home(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        (pane_state.search_bar.cursor_home(), Vec::new())
    }

    fn search_bar_cursor_end(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        (pane_state.search_bar.cursor_end(), Vec::new())
    }

    fn search_bar_switch_field(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        (pane_state.search_bar.switch_field(), Vec::new())
    }

    fn search_bar_toggle_case_sensitive(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }

        let old_search = pane_state.search_bar.search_text.clone();
        let old_case = pane_state.search_bar.case_sensitive;
        let old_regex = pane_state.search_bar.use_regex;

        pane_state.search_bar.case_sensitive = !pane_state.search_bar.case_sensitive;
        Self::restart_search_if_needed(pane, pane_state, old_search, old_case, old_regex)
    }

    fn search_bar_toggle_regex(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }

        let old_search = pane_state.search_bar.search_text.clone();
        let old_case = pane_state.search_bar.case_sensitive;
        let old_regex = pane_state.search_bar.use_regex;

        pane_state.search_bar.use_regex = !pane_state.search_bar.use_regex;
        Self::restart_search_if_needed(pane, pane_state, old_search, old_case, old_regex)
    }

    fn search_bar_toggle_replace_mode(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        (pane_state.search_bar.toggle_replace_mode(), Vec::new())
    }

    fn restart_search_if_needed(
        pane: usize,
        pane_state: &mut EditorPaneState,
        old_search: String,
        old_case: bool,
        old_regex: bool,
    ) -> (bool, Vec<Effect>) {
        let search_changed = pane_state.search_bar.search_text != old_search
            || pane_state.search_bar.case_sensitive != old_case
            || pane_state.search_bar.use_regex != old_regex;

        if !search_changed {
            return (true, Vec::new());
        }

        let mut effects = Vec::new();
        if pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (true, effects)
    }

    fn replace_current(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        let Some(index) = pane_state.search_bar.current_match_index else {
            return (false, Vec::new());
        };
        let Some(m) = pane_state.search_bar.matches.get(index).copied() else {
            return (false, Vec::new());
        };
        let replace_text = pane_state.search_bar.replace_text.clone();

        let changed = if let Some(tab) = pane_state.active_tab_mut() {
            tab.replace_current_match(&m, &replace_text, tab_size)
        } else {
            false
        };
        if !changed {
            return (false, Vec::new());
        }
        let mut effects = Vec::new();
        if pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (true, effects)
    }

    fn replace_all(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        if !pane_state.search_bar.visible {
            return (false, Vec::new());
        }
        if pane_state.search_bar.search_text.is_empty() {
            return (false, Vec::new());
        }
        let matches = pane_state.search_bar.matches.clone();
        let replace_text = pane_state.search_bar.replace_text.clone();
        if matches.is_empty() {
            return (false, Vec::new());
        }

        let multi = matches.len() > 1;
        {
            let Some(tab) = pane_state.active_tab_mut() else {
                return (false, Vec::new());
            };
            for m in matches.iter().rev() {
                let _ = tab.replace_current_match(m, &replace_text, tab_size);
            }
            if multi {
                tab.last_edit_op_id = None;
            }
        }

        let mut effects = Vec::new();
        if pane_state.search_bar.begin_search() {
            if let Some(effect) = pane_state.trigger_search(pane) {
                effects.push(effect);
            }
        }
        (true, effects)
    }
    fn file_reloaded(&mut self, request: ReloadRequest, content: String) -> (bool, Vec<Effect>) {
        let config = self.config.clone();
        let Some(pane_state) = self.panes.get_mut(request.pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state
            .tabs
            .iter_mut()
            .find(|t| t.path.as_ref() == Some(&request.path))
        else {
            return (false, Vec::new());
        };
        if !tab.can_apply_reload(&request) {
            return (false, Vec::new());
        }
        tab.reload_from_content(&content, &config);
        self.open_paths_version = self.open_paths_version.saturating_add(1);
        (true, Vec::new())
    }

    fn file_externally_modified(&mut self, path: std::path::PathBuf) -> (bool, Vec<Effect>) {
        let mut changed = false;
        let mut effects = Vec::new();
        for (pane, pane_state) in self.panes.iter_mut().enumerate() {
            for tab in &mut pane_state.tabs {
                let Some(tab_path) = tab.path.as_ref() else {
                    continue;
                };
                if !paths_equivalent(tab_path.as_path(), path.as_path()) {
                    continue;
                }
                if !tab.dirty {
                    if let Some(request) = tab.issue_reload_request(pane, ReloadCause::ExternalSync)
                    {
                        effects.push(Effect::ReloadFile(request));
                    }
                } else {
                    tab.disk_state = DiskState::ConflictExternalModified;
                }
                changed = true;
            }
        }
        (changed, effects)
    }

    fn file_externally_deleted(&mut self, path: std::path::PathBuf) -> (bool, Vec<Effect>) {
        let mut changed = false;
        for pane_state in &mut self.panes {
            for tab in &mut pane_state.tabs {
                if tab
                    .path
                    .as_ref()
                    .is_some_and(|tab_path| paths_equivalent(tab_path.as_path(), path.as_path()))
                {
                    tab.disk_state = DiskState::MissingOnDisk;
                    changed = true;
                }
            }
        }
        (changed, Vec::new())
    }

    fn accept_disk_version(
        &mut self,
        pane: usize,
        path: std::path::PathBuf,
        content: String,
    ) -> (bool, Vec<Effect>) {
        let config = self.config.clone();
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state
            .tabs
            .iter_mut()
            .find(|t| t.path.as_ref() == Some(&path))
        else {
            return (false, Vec::new());
        };
        tab.next_reload_request_id();
        tab.reload_from_content(&content, &config);
        self.open_paths_version = self.open_paths_version.saturating_add(1);
        (true, Vec::new())
    }

    fn keep_memory_version(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        tab.disk_state = DiskState::InSync;
        (true, Vec::new())
    }
}

fn paths_equivalent(left: &std::path::Path, right: &std::path::Path) -> bool {
    if left == right {
        return true;
    }

    let left = left.canonicalize();
    let right = right.canonicalize();
    match (left, right) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn goto_match(
    tab: &mut super::state::EditorTabState,
    m: &crate::kernel::services::ports::Match,
    tab_size: u8,
) {
    let rope = tab.buffer.rope();
    let char_offset = rope.byte_to_char(m.start);
    let row = rope.char_to_line(char_offset);
    let line_char_start = rope.line_to_char(row);
    let col_chars = char_offset.saturating_sub(line_char_start);

    let slice = rope.line(row);
    let line = crate::models::slice_to_cow(slice);

    let mut taken_chars = 0usize;
    let mut col_graphemes = 0usize;
    for g in unicode_segmentation::UnicodeSegmentation::graphemes(line.as_ref(), true) {
        let g_chars = g.chars().count();
        if taken_chars + g_chars > col_chars {
            break;
        }
        taken_chars += g_chars;
        col_graphemes += 1;
    }

    tab.buffer.set_cursor(row, col_graphemes);
    tab.reset_cursor_goal_col();
    tab.buffer.clear_selection();
    viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
}

fn goto_byte_offset(tab: &mut super::state::EditorTabState, byte_offset: usize, tab_size: u8) {
    let rope = tab.buffer.rope();
    if rope.len_bytes() == 0 {
        tab.buffer.set_cursor(0, 0);
        tab.reset_cursor_goal_col();
        tab.buffer.clear_selection();
        viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
        return;
    }

    let byte_offset = byte_offset.min(rope.len_bytes());
    let char_offset = rope.byte_to_char(byte_offset);
    let row = rope.char_to_line(char_offset);
    let line_char_start = rope.line_to_char(row);
    let col_chars = char_offset.saturating_sub(line_char_start);

    let slice = rope.line(row);
    let line = crate::models::slice_to_cow(slice);

    let mut taken_chars = 0usize;
    let mut col_graphemes = 0usize;
    for g in unicode_segmentation::UnicodeSegmentation::graphemes(line.as_ref(), true) {
        let g_chars = g.chars().count();
        if taken_chars + g_chars > col_chars {
            break;
        }
        taken_chars += g_chars;
        col_graphemes += 1;
    }

    tab.buffer.set_cursor(row, col_graphemes);
    tab.reset_cursor_goal_col();
    tab.buffer.clear_selection();
    viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/reducer.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/move_tab.rs"]
mod move_tab_tests;
