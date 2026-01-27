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
