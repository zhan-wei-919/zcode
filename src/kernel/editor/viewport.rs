use crate::models::{slice_to_cow, TextBuffer};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::state::EditorViewportState;

pub fn cursor_display_x_abs(buffer: &TextBuffer, tab_size: u8) -> u32 {
    let (row, col) = buffer.cursor();
    let Some(slice) = buffer.line_slice(row) else {
        return 0;
    };
    if slice.len_bytes() == slice.len_chars() {
        return cursor_display_x_abs_ascii(&slice, col, tab_size);
    }

    let mut display_col = 0u32;
    let line = slice_to_cow(slice);
    for (i, g) in line.graphemes(true).enumerate() {
        if i >= col {
            break;
        }
        if g == "\t" {
            let tab = tab_size as u32;
            let rem = display_col % tab;
            display_col += if rem == 0 { tab } else { tab - rem };
        } else if g == "\n" || g == "\r" {
            break;
        } else {
            display_col += g.width() as u32;
        }
    }

    display_col
}

fn cursor_display_x_abs_ascii(slice: &ropey::RopeSlice<'_>, col: usize, tab_size: u8) -> u32 {
    if col == 0 {
        return 0;
    }

    let mut remaining = col;
    let mut display_col = 0u32;
    let tab = tab_size.max(1) as u32;

    for chunk in slice.chunks() {
        for &b in chunk.as_bytes() {
            if b == b'\n' || b == b'\r' {
                return display_col;
            }
            if remaining == 0 {
                return display_col;
            }

            if b == b'\t' {
                let rem = display_col % tab;
                display_col += if rem == 0 { tab } else { tab - rem };
            } else {
                display_col = display_col.saturating_add(1);
            }
            remaining -= 1;
        }
    }

    display_col
}

pub fn clamp_and_follow(viewport: &mut EditorViewportState, buffer: &TextBuffer, tab_size: u8) {
    let total_lines = buffer.len_lines().max(1);
    let height = viewport.height.max(1);

    let max_offset = total_lines.saturating_sub(height);
    viewport.line_offset = viewport.line_offset.min(max_offset);

    if !viewport.follow_cursor {
        return;
    }

    let (row, _) = buffer.cursor();

    if row < viewport.line_offset {
        viewport.line_offset = row;
    } else if row >= viewport.line_offset + height {
        viewport.line_offset = row.saturating_sub(height.saturating_sub(1));
    }

    let cursor_x = cursor_display_x_abs(buffer, tab_size);
    let width = viewport.width.max(1) as u32;

    if cursor_x < viewport.horiz_offset {
        viewport.horiz_offset = cursor_x;
    } else if cursor_x >= viewport.horiz_offset + width {
        viewport.horiz_offset = cursor_x.saturating_sub(width.saturating_sub(1));
    }
}

#[cfg(test)]
pub fn screen_to_pos(
    viewport: &EditorViewportState,
    buffer: &TextBuffer,
    tab_size: u8,
    x: u16,
    y: u16,
) -> Option<(usize, usize)> {
    if viewport.width == 0 || viewport.height == 0 {
        return None;
    }

    if x as usize >= viewport.width || y as usize >= viewport.height {
        return None;
    }

    let row = (viewport.line_offset + y as usize).min(buffer.len_lines().saturating_sub(1));

    let col = screen_to_col(viewport, buffer, tab_size, row, x)?;
    Some((row, col))
}

pub fn screen_to_col(
    viewport: &EditorViewportState,
    buffer: &TextBuffer,
    tab_size: u8,
    row: usize,
    x: u16,
) -> Option<usize> {
    if viewport.width == 0 || viewport.height == 0 {
        return None;
    }
    if x as usize >= viewport.width {
        return None;
    }
    if row >= buffer.len_lines().max(1) {
        return None;
    }

    let slice = buffer.line_slice(row)?;
    let line = slice_to_cow(slice);
    let target_x = viewport.horiz_offset + x as u32;
    let mut display_col = 0u32;
    let mut col = 0usize;

    for (i, g) in line.graphemes(true).enumerate() {
        if g == "\n" || g == "\r" {
            break;
        }

        let w = if g == "\t" {
            let tab = tab_size.max(1) as u32;
            let rem = display_col % tab;
            if rem == 0 {
                tab
            } else {
                tab - rem
            }
        } else {
            g.width() as u32
        };

        if display_col + w / 2 >= target_x {
            col = i;
            break;
        }

        display_col += w;
        col = i + 1;
    }

    Some(col)
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/viewport.rs"]
mod tests;
