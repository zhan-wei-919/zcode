//! Utilities for horizontal text windowing in fixed-width areas (nvim-like).
//!
//! All indices are byte offsets into UTF-8 strings. The window boundaries always
//! land on valid UTF-8 character boundaries.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn clamp_to_char_boundary(text: &str, idx: usize) -> usize {
    let mut idx = idx.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn cursor_visible_end(text: &str, cursor: usize) -> usize {
    let cursor = clamp_to_char_boundary(text, cursor);
    if cursor >= text.len() {
        return cursor;
    }

    cursor
        + text[cursor..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(0)
}

/// Compute a window `[start, end)` for `text` such that:
/// - `end - start` fits into `available_width` (terminal cells),
/// - `cursor` (byte offset) is visible within the window.
pub fn window(text: &str, cursor: usize, available_width: usize) -> (usize, usize) {
    let cursor = clamp_to_char_boundary(text, cursor);
    if available_width == 0 || text.is_empty() {
        return (cursor, cursor);
    }

    let start = compute_window_start(text, cursor, available_width);
    let end = start + truncate_to_width(&text[start..], available_width);
    (start, end.min(text.len()))
}

/// Computes the window start (byte offset) that keeps `cursor` visible.
pub fn compute_window_start(text: &str, cursor: usize, available_width: usize) -> usize {
    let cursor = clamp_to_char_boundary(text, cursor);
    if available_width == 0 {
        return cursor;
    }

    let prefix_end = cursor_visible_end(text, cursor);
    let prefix = &text[..prefix_end];
    if UnicodeWidthStr::width(prefix) <= available_width {
        return 0;
    }

    // Walk backwards from the cursor until we fill `available_width`.
    let mut start = cursor;
    let mut used = 0usize;
    let indices: Vec<(usize, char)> = prefix.char_indices().collect();
    for (idx, ch) in indices.into_iter().rev() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > available_width {
            break;
        }
        used += w;
        start = idx;
    }

    start
}

/// Returns how many bytes from the start of `s` fit into `max_width` cells.
pub fn truncate_to_width(s: &str, max_width: usize) -> usize {
    if max_width == 0 || s.is_empty() {
        return 0;
    }

    let mut used = 0usize;
    let mut end = 0usize;
    for (idx, ch) in s.char_indices() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max_width {
            break;
        }
        used += w;
        end = idx + ch.len_utf8();
    }

    end
}

#[cfg(test)]
#[path = "../../tests/unit/core/text_window.rs"]
mod tests;
