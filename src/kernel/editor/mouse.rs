use crate::models::{Granularity, Selection};

use super::state::{EditorMouseState, EditorTabState};
use super::viewport;

fn click_granularity(
    mouse: &mut EditorMouseState,
    x: u16,
    y: u16,
    now: std::time::Instant,
    slop: u16,
    triple_click_ms: u64,
) -> Granularity {
    if let Some((lx, ly, lt)) = mouse.last_click {
        let dx = (x as i32 - lx as i32).abs();
        let dy = (y as i32 - ly as i32).abs();
        let dt = now.duration_since(lt).as_millis() as u64;

        if dx <= slop as i32 && dy <= slop as i32 && dt < triple_click_ms {
            mouse.click_count = (mouse.click_count % 3) + 1;
        } else {
            mouse.click_count = 1;
        }
    } else {
        mouse.click_count = 1;
    }

    mouse.last_click = Some((x, y, now));
    mouse.dragging = true;

    match mouse.click_count {
        1 => Granularity::Char,
        2 => Granularity::Word,
        _ => Granularity::Line,
    }
}

impl EditorTabState {
    pub fn mouse_down(
        &mut self,
        x: u16,
        y: u16,
        now: std::time::Instant,
        tab_size: u8,
        slop: u16,
        triple_click_ms: u64,
    ) -> bool {
        self.viewport.follow_cursor = true;

        let granularity = click_granularity(&mut self.mouse, x, y, now, slop, triple_click_ms);
        self.mouse.granularity = granularity;

        let visible_lines =
            self.visible_lines_in_viewport(self.viewport.line_offset, self.viewport.height.max(1));
        let Some(row) = visible_lines.get(y as usize).copied() else {
            return false;
        };

        let Some(col) = viewport::screen_to_col(&self.viewport, &self.buffer, tab_size, row, x)
        else {
            return false;
        };
        let pos = (row, col);

        self.buffer.set_cursor(pos.0, pos.1);

        let selection = Selection::from_pos(pos, granularity, self.buffer.rope());
        self.buffer.set_selection(Some(selection));

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn mouse_drag(
        &mut self,
        x: u16,
        y: u16,
        tab_size: u8,
        overflow_y: i16,
        past_right: bool,
    ) -> bool {
        if !self.mouse.dragging {
            return false;
        }

        if overflow_y != 0 {
            self.viewport.follow_cursor = true;
            let max_offset = self
                .buffer
                .len_lines()
                .max(1)
                .saturating_sub(self.viewport.height.max(1));
            if overflow_y < 0 {
                self.viewport.line_offset = self
                    .viewport
                    .line_offset
                    .saturating_sub((-overflow_y) as usize);
            } else {
                self.viewport.line_offset =
                    (self.viewport.line_offset + overflow_y as usize).min(max_offset);
            }
        }

        let visible_lines =
            self.visible_lines_in_viewport(self.viewport.line_offset, self.viewport.height.max(1));
        let Some(row) = visible_lines.get(y as usize).copied() else {
            return false;
        };

        let col = if past_right {
            self.buffer.line_grapheme_len(row)
        } else {
            let Some(col) = viewport::screen_to_col(&self.viewport, &self.buffer, tab_size, row, x)
            else {
                return false;
            };
            col
        };

        let pos = (row, col);
        self.buffer.update_selection_cursor(pos);
        self.buffer.set_cursor(pos.0, pos.1);
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn mouse_up(&mut self) -> bool {
        if !self.mouse.dragging {
            return false;
        }
        self.mouse.dragging = false;
        true
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/mouse.rs"]
mod tests;
