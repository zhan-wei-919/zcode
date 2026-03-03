use super::*;

#[test]
fn test_screen_to_pos_tab_mapping() {
    let buffer = TextBuffer::from_text("\tabc\n");
    let mut viewport = EditorViewportState {
        width: 100,
        height: 1,
        line_offset: 0,
        horiz_offset: 0,
        ..Default::default()
    };

    assert_eq!(screen_to_pos(&viewport, &buffer, 4, 0, 0), Some((0, 0)));
    assert_eq!(screen_to_pos(&viewport, &buffer, 4, 3, 0), Some((0, 1)));
    assert_eq!(screen_to_pos(&viewport, &buffer, 4, 4, 0), Some((0, 1)));
    assert_eq!(screen_to_pos(&viewport, &buffer, 4, 5, 0), Some((0, 2)));

    viewport.horiz_offset = 4;
    assert_eq!(screen_to_pos(&viewport, &buffer, 4, 0, 0), Some((0, 1)));
}

#[test]
fn test_clamp_and_follow_allows_scroll_beyond_last_line() {
    let buffer = TextBuffer::from_text("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n");
    let mut viewport = EditorViewportState {
        width: 80,
        height: 5,
        line_offset: 9,
        horiz_offset: 0,
        follow_cursor: false,
    };

    clamp_and_follow(&mut viewport, &buffer, 4);
    assert_eq!(viewport.line_offset, 9);
}

#[test]
fn test_clamp_and_follow_does_not_snap_manual_offset_when_cursor_visible() {
    let content = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut buffer = TextBuffer::from_text(&content);
    buffer.set_cursor(99, 0);

    let mut viewport = EditorViewportState {
        width: 80,
        height: 30,
        line_offset: 90,
        horiz_offset: 0,
        follow_cursor: true,
    };

    clamp_and_follow(&mut viewport, &buffer, 4);
    assert_eq!(viewport.line_offset, 90);
}
