use crate::models::{cursor_set, Granularity, SecondaryCursor, Selection};

use super::state::EditorTabState;
use super::viewport;

impl EditorTabState {
    pub fn place_cursor(
        &mut self,
        row: usize,
        col: usize,
        granularity: Granularity,
        tab_size: u8,
    ) -> bool {
        self.cancel_snippet_session();
        self.clear_secondary_cursors();
        self.viewport.follow_cursor = true;
        self.buffer.set_cursor(row, col);
        self.reset_cursor_goal_col();
        let selection = Selection::from_pos((row, col), granularity, self.buffer.rope());
        self.buffer.set_selection(Some(selection));
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn add_cursor_at(&mut self, row: usize, col: usize, tab_size: u8) -> bool {
        self.cancel_snippet_session();

        let old_primary_pos = self.buffer.cursor();
        if old_primary_pos == (row, col) {
            return false;
        }

        self.viewport.follow_cursor = true;

        let old_primary = SecondaryCursor {
            pos: old_primary_pos,
            selection: self.buffer.selection().cloned(),
            goal_col: self.cursor_goal_col,
        };
        self.secondary_cursors.push(old_primary);

        self.buffer.set_cursor(row, col);
        self.buffer.clear_selection();
        self.reset_cursor_goal_col();

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn extend_selection(&mut self, row: usize, col: usize, tab_size: u8) -> bool {
        self.cancel_snippet_session();
        self.viewport.follow_cursor = true;
        self.buffer.update_selection_cursor((row, col));
        self.buffer.set_cursor(row, col);
        self.set_cursor_goal_col(col);
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn end_selection_gesture(&mut self) -> bool {
        self.cancel_snippet_session();
        if self
            .buffer
            .selection()
            .is_some_and(|s| s.granularity() == Granularity::Char && s.is_empty())
        {
            self.buffer.clear_selection();
        }
        true
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/mouse.rs"]
mod tests;
