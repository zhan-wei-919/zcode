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
mod tests {
    use super::*;

    #[test]
    fn window_handles_empty_and_zero_width() {
        assert_eq!(window("", 0, 0), (0, 0));
        assert_eq!(window("", 10, 0), (0, 0));

        let text = "abc";
        assert_eq!(window(text, 0, 0), (0, 0));
        assert_eq!(window(text, 2, 0), (2, 2));
        assert_eq!(window(text, 10, 0), (3, 3));
    }

    #[test]
    fn window_ascii_keeps_cursor_visible() {
        let text = "abcdefghij";
        assert_eq!(window(text, 0, 5), (0, 5));
        assert_eq!(window(text, 3, 5), (0, 5));
        assert_eq!(window(text, 6, 5), (2, 7));
        assert_eq!(window(text, 10, 5), (5, 10));
    }

    #[test]
    fn window_respects_char_boundaries_for_wide_chars() {
        let text = "你好世界";
        let (start, end) = window(text, text.len(), 4);
        assert_eq!((start, end), (6, 12));
        assert!(text.is_char_boundary(start));
        assert!(text.is_char_boundary(end));
        assert_eq!(&text[start..end], "世界");
    }

    #[test]
    fn truncate_to_width_does_not_split_utf8() {
        let text = "éé";
        let end = truncate_to_width(text, 1);
        assert_eq!(end, "é".len());
        assert!(text.is_char_boundary(end));
        assert_eq!(&text[..end], "é");
    }

    #[test]
    fn compute_window_start_with_combining_marks() {
        let text = "e\u{301}e\u{301}e\u{301}";
        let cursor = text.len();
        let start = compute_window_start(text, cursor, 1);
        assert!(text.is_char_boundary(start));
        let (s, e) = window(text, cursor, 1);
        assert!(text.is_char_boundary(s));
        assert!(text.is_char_boundary(e));
        assert!(s <= cursor && cursor <= text.len());
    }
}
