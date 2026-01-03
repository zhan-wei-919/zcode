use crate::core::Command;
use crate::kernel::Effect;
use crate::kernel::services::ports::SearchMessage;

use super::action::EditorAction;
use super::state::{EditorPaneState, EditorState, SearchBarMode};
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
            EditorAction::MouseDown { pane, x, y, now } => self.mouse_down(pane, x, y, now),
            EditorAction::MouseDrag { pane, x, y } => self.mouse_drag(pane, x, y),
            EditorAction::MouseUp { pane } => self.mouse_up(pane),
            EditorAction::Scroll { pane, delta_lines } => self.scroll(pane, delta_lines),
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
            EditorAction::SearchStarted { pane, search_id } => {
                self.search_started(pane, search_id)
            }
            EditorAction::SearchMessage { pane, message } => self.search_message(pane, message),
            EditorAction::Saved { pane, path, success } => self.saved(pane, path, success),
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
            Command::EditorSearchBarSwitchField => self.dispatch_action(EditorAction::SearchBarSwitchField { pane }),
            Command::EditorSearchBarToggleCaseSensitive => {
                self.dispatch_action(EditorAction::SearchBarToggleCaseSensitive { pane })
            }
            Command::EditorSearchBarToggleRegex => self.dispatch_action(EditorAction::SearchBarToggleRegex { pane }),
            Command::EditorSearchBarToggleReplaceMode => {
                self.dispatch_action(EditorAction::SearchBarToggleReplaceMode { pane })
            }
            Command::EditorSearchBarCursorLeft => self.dispatch_action(EditorAction::SearchBarCursorLeft { pane }),
            Command::EditorSearchBarCursorRight => {
                self.dispatch_action(EditorAction::SearchBarCursorRight { pane })
            }
            Command::EditorSearchBarCursorHome => self.dispatch_action(EditorAction::SearchBarCursorHome { pane }),
            Command::EditorSearchBarCursorEnd => self.dispatch_action(EditorAction::SearchBarCursorEnd { pane }),
            Command::EditorSearchBarBackspace => self.dispatch_action(EditorAction::SearchBarBackspace { pane }),
            Command::EditorSearchBarDeleteForward => {
                self.dispatch_action(EditorAction::SearchBarDeleteForward { pane })
            }
            Command::EditorSearchBarReplaceCurrent => self.dispatch_action(EditorAction::ReplaceCurrent { pane }),
            Command::EditorSearchBarReplaceAll => self.dispatch_action(EditorAction::ReplaceAll { pane }),
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

        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };

        let (vw, vh) = pane_state
            .active_tab()
            .map(|t| (t.viewport.width, t.viewport.height))
            .unwrap_or((80, default_height));

        let changed = pane_state.open_file(path, &content, &config);

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

    fn set_viewport_size(&mut self, pane: usize, width: usize, height: usize) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };

        let mut changed = pane_state.set_viewport_size(width, height);
        for tab in &mut pane_state.tabs {
            let prev_offset = tab.viewport.line_offset;
            let prev_h = tab.viewport.horiz_offset;
            viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
            changed |= tab.viewport.line_offset != prev_offset || tab.viewport.horiz_offset != prev_h;
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
        if pane_state.search_bar.visible {
            if pane_state.search_bar.begin_search() {
                if let Some(effect) = pane_state.trigger_search(pane) {
                    effects.push(effect);
                }
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
        if pane_state.search_bar.visible {
            if pane_state.search_bar.begin_search() {
                if let Some(effect) = pane_state.trigger_search(pane) {
                    effects.push(effect);
                }
            }
        }
        (true, effects)
    }

    fn close_tab(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let changed = pane_state.close_active_tab();
        if !changed {
            return (false, Vec::new());
        }
        let mut effects = Vec::new();
        if pane_state.search_bar.visible {
            if pane_state.search_bar.begin_search() {
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

        let Some(len) = (!pane_state.search_bar.matches.is_empty()).then_some(pane_state.search_bar.matches.len()) else {
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

        let Some(len) = (!pane_state.search_bar.matches.is_empty()).then_some(pane_state.search_bar.matches.len()) else {
            return (false, Vec::new());
        };

        let prev = match pane_state.search_bar.current_match_index {
            Some(i) => if i == 0 { len - 1 } else { i - 1 },
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
        (false, vec![Effect::WriteFile { pane, path }])
    }

    fn forward_to_active_tab(&mut self, pane: usize, command: Command) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        tab.apply_command(command, pane, tab_size)
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

    fn mouse_down(&mut self, pane: usize, x: u16, y: u16, now: std::time::Instant) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let click_slop = self.config.click_slop;
        let triple_click_ms = self.config.triple_click_ms;

        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        let changed = tab.mouse_down(
            x,
            y,
            now,
            tab_size,
            click_slop,
            triple_click_ms,
        );
        (changed, Vec::new())
    }

    fn mouse_drag(&mut self, pane: usize, x: u16, y: u16) -> (bool, Vec<Effect>) {
        let tab_size = self.config.tab_size;
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        let changed = tab.mouse_drag(x, y, tab_size);
        (changed, Vec::new())
    }

    fn mouse_up(&mut self, pane: usize) -> (bool, Vec<Effect>) {
        let Some(pane_state) = self.panes.get_mut(pane) else {
            return (false, Vec::new());
        };
        let Some(tab) = pane_state.active_tab_mut() else {
            return (false, Vec::new());
        };
        let changed = tab.mouse_up();
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
            tab.viewport.line_offset = (tab.viewport.line_offset + delta_lines as usize).min(max_offset);
        } else {
            tab.viewport.line_offset = tab.viewport.line_offset.saturating_sub((-delta_lines) as usize);
        }

        (tab.viewport.line_offset != prev, Vec::new())
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

    fn saved(&mut self, pane: usize, path: std::path::PathBuf, success: bool) -> (bool, Vec<Effect>) {
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
        if pane_state.search_bar.replace_text.is_empty() {
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
        if pane_state.search_bar.replace_text.is_empty() {
            return (false, Vec::new());
        }
        let matches = pane_state.search_bar.matches.clone();
        let replace_text = pane_state.search_bar.replace_text.clone();
        if matches.is_empty() {
            return (false, Vec::new());
        }

        {
            let Some(tab) = pane_state.active_tab_mut() else {
                return (false, Vec::new());
            };
            for m in matches.iter().rev() {
                let _ = tab.replace_current_match(m, &replace_text, tab_size);
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
    tab.buffer.clear_selection();
    viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
}

fn goto_byte_offset(tab: &mut super::state::EditorTabState, byte_offset: usize, tab_size: u8) {
    let rope = tab.buffer.rope();
    if rope.len_bytes() == 0 {
        tab.buffer.set_cursor(0, 0);
        tab.buffer.clear_selection();
        viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
        return;
    }

    let byte_offset = byte_offset.min(rope.len_bytes().saturating_sub(1));
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
    tab.buffer.clear_selection();
    viewport::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
}
