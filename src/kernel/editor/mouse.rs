use crate::models::{Granularity, Selection};

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
        self.viewport.follow_cursor = true;
        self.buffer.set_cursor(row, col);
        self.reset_cursor_goal_col();
        let selection = Selection::from_pos((row, col), granularity, self.buffer.rope());
        self.buffer.set_selection(Some(selection));
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn extend_selection(&mut self, row: usize, col: usize, tab_size: u8) -> bool {
        self.viewport.follow_cursor = true;
        self.buffer.update_selection_cursor((row, col));
        self.buffer.set_cursor(row, col);
        self.set_cursor_goal_col(col);
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn end_selection_gesture(&mut self) -> bool {
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
