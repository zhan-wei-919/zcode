//! Screen-to-source coordinate mapping for the editor.
//!
//! All display-coordinate logic lives here, keeping the kernel free of
//! screen/viewport concerns.

use crate::kernel::editor::{EditorTabState, EditorViewportState};
use crate::models::{slice_to_cow, TextBuffer};
use crate::views::editor::markdown::{self, MarkdownDocument};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Convert a screen x position to a source column for a regular (non-markdown) line.
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

    let src_byte_in_line = markdown::display_to_source_byte(&rendered.offset_map, display_byte);

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

/// Unified entry point: automatically dispatches between markdown and regular
/// coordinate conversion based on file type and cursor position.
pub fn resolve_source_col(
    tab: &EditorTabState,
    md: Option<&MarkdownDocument>,
    row: usize,
    x: u16,
    tab_size: u8,
) -> Option<usize> {
    let cursor_row = tab.buffer.cursor().0;
    if let Some(md) = md.filter(|_| row != cursor_row) {
        screen_to_col_markdown(
            md,
            tab,
            row,
            x,
            tab.viewport.horiz_offset,
            tab.viewport.width,
        )
    } else {
        screen_to_col(&tab.viewport, &tab.buffer, tab_size, row, x)
    }
}
