use super::*;

#[test]
fn test_query_editing() {
    let mut state = SearchState::default();
    assert!(state.append_query_char('h'));
    assert!(state.append_query_char('i'));
    assert_eq!(state.query, "hi");
    assert_eq!(state.query_cursor, 2);

    assert!(state.cursor_left());
    assert_eq!(state.query_cursor, 1);

    assert!(state.backspace_query());
    assert_eq!(state.query, "i");
    assert_eq!(state.query_cursor, 0);
}

#[test]
fn test_selection_wraps() {
    let mut state = SearchState::default();
    state.files.push(SearchFileResult {
        path: PathBuf::from("a"),
        matches: vec![Match {
            start: 0,
            end: 1,
            line: 0,
            col: 0,
        }],
        expanded: true,
    });
    state
        .items
        .push(SearchResultItem::FileHeader { file_index: 0 });
    state.items.push(SearchResultItem::MatchLine {
        file_index: 0,
        match_index: 0,
    });

    assert_eq!(state.selected_index, 0);
    state.sidebar_view.view_height = 1;
    assert!(state.move_selection(-1, SearchViewport::Sidebar));
    assert_eq!(state.selected_index, 1);
    assert!(state.move_selection(1, SearchViewport::Sidebar));
    assert_eq!(state.selected_index, 0);
}
