use crate::models::{slice_to_cow, TextBuffer};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::state::EditorViewportState;

pub fn cursor_display_x_abs(buffer: &TextBuffer, tab_size: u8) -> u32 {
    let (row, col) = buffer.cursor();
    let Some(slice) = buffer.line_slice(row) else {
        return 0;
    };
    let line = slice_to_cow(slice);
    let graphemes = line.graphemes(true);

    let mut display_col = 0u32;
    for (i, g) in graphemes.enumerate() {
        if i >= col {
            break;
        }
        if g == "\t" {
            let tab = tab_size as u32;
            let rem = display_col % tab;
            display_col += if rem == 0 { tab } else { tab - rem };
        } else if g == "\n" {
            break;
        } else {
            display_col += g.width() as u32;
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

pub fn expand_tabs(line: &str, tab_size: u8) -> String {
    let mut expanded = String::new();
    let mut display_col = 0u32;
    let tab_size = tab_size as u32;

    for ch in line.chars() {
        if ch == '\t' {
            let remainder = display_col % tab_size;
            let spaces = if remainder == 0 {
                tab_size
            } else {
                tab_size - remainder
            };
            for _ in 0..spaces {
                expanded.push(' ');
            }
            display_col += spaces;
        } else if ch == '\n' {
            break;
        } else {
            expanded.push(ch);
            display_col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as u32;
        }
    }

    expanded
}

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

    let slice = buffer.line_slice(row)?;
    let line = slice_to_cow(slice);
    let expanded = expand_tabs(&line, tab_size);
    let graphemes: Vec<&str> = expanded.graphemes(true).collect();

    let target_x = viewport.horiz_offset + x as u32;
    let mut accumulated_x = 0u32;
    let mut col = 0usize;

    for (i, g) in graphemes.iter().enumerate() {
        let w = g.width() as u32;
        if accumulated_x + w / 2 >= target_x {
            col = i;
            break;
        }
        accumulated_x += w;
        col = i + 1;
    }

    Some((row, col.min(graphemes.len())))
}
