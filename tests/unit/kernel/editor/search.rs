use super::*;
use crate::kernel::editor::SearchBarState;
use crate::kernel::services::ports::{Match, SearchMessage};

#[test]
fn search_bar_backspace_deletes_grapheme_cluster() {
    let mut state = SearchBarState::default();
    state.show(SearchBarMode::Search);

    state.insert_char('e');
    state.insert_char('\u{301}');
    assert_eq!(state.search_text, "e\u{301}");
    assert_eq!(state.cursor_pos, state.search_text.len());

    assert!(state.delete_backward());
    assert_eq!(state.search_text, "");
    assert_eq!(state.cursor_pos, 0);
}

#[test]
fn search_bar_cursor_moves_by_graphemes() {
    let mut state = SearchBarState::default();
    state.show(SearchBarMode::Search);

    state.insert_char('👍');
    state.insert_char('🏽');
    state.insert_char('a');
    assert_eq!(state.search_text, "👍🏽a");

    let cluster_len = "👍🏽".len();
    assert_eq!(state.cursor_pos, state.search_text.len());

    assert!(state.cursor_left());
    assert_eq!(state.cursor_pos, cluster_len);
    assert!(state.cursor_left());
    assert_eq!(state.cursor_pos, 0);

    assert!(state.cursor_right());
    assert_eq!(state.cursor_pos, cluster_len);
    assert!(state.cursor_right());
    assert_eq!(state.cursor_pos, state.search_text.len());
}

#[test]
fn search_bar_apply_message_ignores_other_search_ids() {
    let mut state = SearchBarState::default();
    state.show(SearchBarMode::Search);
    state.search_text = "foo".to_string();
    state.cursor_pos = state.search_text.len();
    state.searching = true;
    state.active_search_id = Some(1);

    let ignored = SearchMessage::Matches {
        search_id: 2,
        matches: vec![Match::new(0, 1, 0, 0)],
        is_final: false,
    };
    assert!(!state.apply_message(ignored));
    assert!(state.matches.is_empty());

    let accepted = SearchMessage::Matches {
        search_id: 1,
        matches: vec![Match::new(0, 1, 0, 0)],
        is_final: true,
    };
    assert!(state.apply_message(accepted));
    assert_eq!(state.matches.len(), 1);
    assert_eq!(state.current_match_index, Some(0));
    assert!(!state.searching);
}
