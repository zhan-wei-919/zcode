use crate::models::{Granularity, Selection};

use super::markdown::MarkdownDocument;
use super::state::{EditorMouseState, EditorTabState};
use super::viewport;

/// For a markdown WYSIWYG line, convert a screen x position to a source column.
fn screen_to_col_markdown(
    md: &MarkdownDocument,
    tab: &EditorTabState,
    row: usize,
    x: u16,
    horiz_offset: u32,
    viewport_width: usize,
) -> Option<usize> {
    let rendered = md.render_line(row, tab.buffer.rope(), viewport_width);

    // Walk the display text graphemes to find which display byte offset corresponds to screen x
    let target_x = horiz_offset + x as u32;
    let mut display_col: u32 = 0;
    let mut display_byte: usize = 0;

    for g in rendered.text.graphemes(true) {
        let w = g.width() as u32;
        if display_col + w / 2 >= target_x {
            break;
        }
        display_col += w;
        display_byte += g.len();
    }

    // Map display byte to source byte
    let src_byte_in_line =
        super::markdown::display_to_source_byte(&rendered.offset_map, display_byte);

    // Convert source byte to column (grapheme index within the line)
    let rope = tab.buffer.rope();
    let line_start_byte = rope.line_to_byte(row);
    let line_end_byte = if row + 1 < rope.len_lines() {
        rope.line_to_byte(row + 1)
    } else {
        rope.len_bytes()
    };
    let line_len_bytes = line_end_byte.saturating_sub(line_start_byte);
    let clamped_src_byte_in_line = src_byte_in_line.min(line_len_bytes);
    let abs_src_byte = line_start_byte + clamped_src_byte_in_line;

    if abs_src_byte >= rope.len_bytes() {
        return Some(tab.buffer.line_grapheme_len(row));
    }
    let src_char = rope.byte_to_char(abs_src_byte);
    let line_start_char = rope.line_to_char(row);
    Some(src_char.saturating_sub(line_start_char))
}

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

        let cursor_row = self.buffer.cursor().0;
        let col = if self.is_markdown() && row != cursor_row {
            if let Some(md) = self.markdown() {
                screen_to_col_markdown(
                    md,
                    self,
                    row,
                    x,
                    self.viewport.horiz_offset,
                    self.viewport.width,
                )
            } else {
                viewport::screen_to_col(&self.viewport, &self.buffer, tab_size, row, x)
            }
        } else {
            viewport::screen_to_col(&self.viewport, &self.buffer, tab_size, row, x)
        };

        let Some(col) = col else {
            return false;
        };
        let pos = (row, col);

        self.buffer.set_cursor(pos.0, pos.1);
        self.reset_cursor_goal_col();

        let selection = Selection::from_pos(pos, granularity, self.buffer.rope());
        self.buffer.set_selection(Some(selection));

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn mouse_select_word(&mut self, x: u16, y: u16, tab_size: u8) -> bool {
        self.viewport.follow_cursor = true;

        let visible_lines =
            self.visible_lines_in_viewport(self.viewport.line_offset, self.viewport.height.max(1));
        let Some(row) = visible_lines.get(y as usize).copied() else {
            return false;
        };

        let cursor_row = self.buffer.cursor().0;
        let col = if self.is_markdown() && row != cursor_row {
            if let Some(md) = self.markdown() {
                screen_to_col_markdown(
                    md,
                    self,
                    row,
                    x,
                    self.viewport.horiz_offset,
                    self.viewport.width,
                )
            } else {
                viewport::screen_to_col(&self.viewport, &self.buffer, tab_size, row, x)
            }
        } else {
            viewport::screen_to_col(&self.viewport, &self.buffer, tab_size, row, x)
        };

        let Some(col) = col else {
            return false;
        };

        let pos = (row, col);
        self.buffer.set_cursor(pos.0, pos.1);
        self.reset_cursor_goal_col();
        let selection = Selection::from_pos(pos, Granularity::Word, self.buffer.rope());
        self.buffer.set_selection(Some(selection));

        self.mouse.dragging = false;
        self.mouse.granularity = Granularity::Word;

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn mouse_context_menu(&mut self, x: u16, y: u16, tab_size: u8) -> bool {
        self.viewport.follow_cursor = true;

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
        if self
            .buffer
            .selection()
            .is_some_and(|selection| !selection.is_empty() && selection.contains(pos))
        {
            self.buffer.set_cursor(pos.0, pos.1);
            self.reset_cursor_goal_col();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            return true;
        }

        self.mouse_select_word(x, y, tab_size)
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
        self.set_cursor_goal_col(pos.1);
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn mouse_up(&mut self) -> bool {
        if !self.mouse.dragging {
            return false;
        }

        self.mouse.dragging = false;

        if self.buffer.selection().is_some_and(|selection| {
            selection.granularity() == Granularity::Char && selection.is_empty()
        }) {
            self.buffer.clear_selection();
        }

        true
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/mouse.rs"]
mod tests;
