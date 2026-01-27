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
