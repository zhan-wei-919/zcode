use super::*;

#[test]
fn test_text_buffer_basic() {
    let mut buffer = TextBuffer::from_text("hello\nworld");

    assert_eq!(buffer.len_lines(), 2);
    assert_eq!(buffer.cursor(), (0, 0));

    buffer.set_cursor(1, 2);
    assert_eq!(buffer.cursor(), (1, 2));
}

#[test]
fn test_pos_to_char() {
    let buffer = TextBuffer::from_text("hello\nworld");

    assert_eq!(buffer.pos_to_char((0, 0)), 0);
    assert_eq!(buffer.pos_to_char((1, 0)), 6);
}

#[test]
fn test_insert_char_op() {
    let mut buffer = TextBuffer::new();
    let op = buffer.insert_char_op('a', OpId::root());

    assert_eq!(buffer.text(), "a");
    assert_eq!(buffer.cursor(), (0, 1));
    assert_eq!(op.cursor_before(), (0, 0));
    assert_eq!(op.cursor_after(), (0, 1));
}

#[test]
fn test_insert_combining_mark_keeps_cursor_grapheme_index() {
    let mut buffer = TextBuffer::new();

    buffer.insert_char_op('e', OpId::root());
    buffer.insert_char_op('\u{301}', OpId::root());

    assert_eq!(buffer.text(), "e\u{301}");
    assert_eq!(buffer.cursor(), (0, 1));

    let _ = buffer.delete_backward_op(OpId::root()).expect("delete");
    assert_eq!(buffer.text(), "");
    assert_eq!(buffer.cursor(), (0, 0));
}

#[test]
fn test_insert_str_combining_mark_keeps_cursor_grapheme_index() {
    let mut buffer = TextBuffer::new();

    buffer.insert_str_op("e", OpId::root());
    buffer.insert_str_op("\u{301}", OpId::root());

    assert_eq!(buffer.text(), "e\u{301}");
    assert_eq!(buffer.cursor(), (0, 1));
}

#[test]
fn test_delete_backward_removes_single_emoji_grapheme() {
    let mut buffer = TextBuffer::from_text("👍🏽a");
    buffer.set_cursor(0, 1);

    let _ = buffer
        .delete_backward_op(OpId::root())
        .expect("delete backward");

    assert_eq!(buffer.text(), "a");
    assert_eq!(buffer.cursor(), (0, 0));
}

#[test]
fn test_delete_forward_removes_single_combining_grapheme() {
    let mut buffer = TextBuffer::from_text("e\u{301}x");
    buffer.set_cursor(0, 0);

    let _ = buffer
        .delete_forward_op(OpId::root())
        .expect("delete forward");

    assert_eq!(buffer.text(), "x");
    assert_eq!(buffer.cursor(), (0, 0));
}

#[test]
fn test_delete_backward_keeps_char_offset_cache_for_next_insert() {
    let mut buffer = TextBuffer::from_text("e\u{301}x");
    buffer.set_cursor(0, 1);

    let _ = buffer
        .delete_backward_op(OpId::root())
        .expect("delete backward");
    let _ = buffer.insert_char_op('a', OpId::root());

    assert_eq!(buffer.text(), "ax");
    assert_eq!(buffer.cursor(), (0, 1));
}

#[test]
fn test_delete_forward_keeps_char_offset_cache_for_next_insert() {
    let mut buffer = TextBuffer::from_text("ab\ncd");
    buffer.set_cursor(0, 2);

    let _ = buffer
        .delete_forward_op(OpId::root())
        .expect("delete forward");
    let _ = buffer.insert_char_op('X', OpId::root());

    assert_eq!(buffer.text(), "abXcd");
    assert_eq!(buffer.cursor(), (0, 3));
}

#[test]
fn test_line_grapheme_len() {
    let buffer = TextBuffer::from_text("hello\nworld\n");

    assert_eq!(buffer.line_grapheme_len(0), 5);
    assert_eq!(buffer.line_grapheme_len(1), 5);
}

#[test]
fn test_line_grapheme_len_crlf() {
    let buffer = TextBuffer::from_text("hello\r\nworld\r\n");

    assert_eq!(buffer.line_grapheme_len(0), 5);
    assert_eq!(buffer.line_grapheme_len(1), 5);
}

#[test]
fn test_line_non_contiguous_slice_returns_string() {
    let long = "a".repeat(5000);
    let buffer = TextBuffer::from_text(&long);
    assert_eq!(buffer.line(0).unwrap(), long);
}

#[test]
fn test_has_selection() {
    let mut buffer = TextBuffer::new();
    assert!(!buffer.has_selection());

    buffer.set_selection(Some(Selection::new(
        (0, 0),
        super::super::selection::Granularity::Char,
    )));
    assert!(!buffer.has_selection());
}

#[test]
fn replace_range_op_adjust_cursor_keeps_cursor_when_edit_after_cursor() {
    let mut buffer = TextBuffer::from_text("abcdef");
    buffer.set_cursor(0, 0);

    let _ = buffer.replace_range_op_adjust_cursor(4, 6, "XY", OpId::root());

    assert_eq!(buffer.text(), "abcdXY");
    assert_eq!(buffer.cursor(), (0, 0));
}

#[test]
fn replace_range_op_adjust_cursor_shifts_cursor_when_edit_before_cursor() {
    let mut buffer = TextBuffer::from_text("abcdef");
    buffer.set_cursor(0, 4);

    let _ = buffer.replace_range_op_adjust_cursor(0, 2, "XYZ", OpId::root());

    assert_eq!(buffer.text(), "XYZcdef");
    assert_eq!(buffer.cursor(), (0, 5));
}

#[test]
fn replace_range_op_adjust_cursor_moves_cursor_to_end_of_insert_when_cursor_inside_range() {
    let mut buffer = TextBuffer::from_text("abcdef");
    buffer.set_cursor(0, 1);

    let _ = buffer.replace_range_op_adjust_cursor(0, 2, "XYZ", OpId::root());

    assert_eq!(buffer.text(), "XYZcdef");
    assert_eq!(buffer.cursor(), (0, 3));
}

#[test]
fn replace_range_op_adjust_cursor_moves_cursor_after_insertion_at_cursor() {
    let mut buffer = TextBuffer::from_text("abc");
    buffer.set_cursor(0, 1);

    let _ = buffer.replace_range_op_adjust_cursor(1, 1, "XYZ", OpId::root());

    assert_eq!(buffer.text(), "aXYZbc");
    assert_eq!(buffer.cursor(), (0, 4));
}
