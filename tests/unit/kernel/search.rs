use super::*;
use std::time::Instant;

fn match_item(file_index: usize, match_index: usize) -> Match {
    let base = file_index * 10_000 + match_index * 8;
    Match::new(base, base + 3, match_index, match_index)
}

fn seeded_state(files: usize, matches_per_file: usize) -> SearchState {
    let mut state = SearchState::default();
    state.sidebar_view.view_height = 20;
    state.panel_view.view_height = 20;

    for file_index in 0..files {
        let matches: Vec<Match> = (0..matches_per_file)
            .map(|match_index| match_item(file_index, match_index))
            .collect();
        state.files.push(SearchFileResult {
            path: PathBuf::from(format!("src/file_{file_index:04}.rs")),
            matches,
            expanded: true,
        });
        state
            .items
            .push(SearchResultItem::FileHeader { file_index });
        for match_index in 0..matches_per_file {
            state.items.push(SearchResultItem::MatchLine {
                file_index,
                match_index,
            });
        }
    }

    state
}

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

#[test]
fn test_toggle_file_expanded_collapses_and_restores_rows() {
    let mut state = seeded_state(3, 3);
    let baseline_len = state.items.len();
    state.selected_index = 2;

    assert!(state.toggle_file_expanded(0));
    assert!(!state.files[0].expanded);
    assert_eq!(state.selected_index, 0);
    assert_eq!(state.items.len(), baseline_len - 3);
    assert!(!state
        .items
        .iter()
        .any(|item| matches!(item, SearchResultItem::MatchLine { file_index: 0, .. })));

    assert!(state.toggle_file_expanded(0));
    assert!(state.files[0].expanded);
    assert_eq!(state.items.len(), baseline_len);

    let restored = state
        .items
        .iter()
        .filter(|item| matches!(item, SearchResultItem::MatchLine { file_index: 0, .. }))
        .count();
    assert_eq!(restored, 3);
}

#[test]
fn test_toggle_file_expanded_shifts_following_selection_index() {
    let mut state = seeded_state(4, 2);
    let before = state.items.len();
    state.selected_index = 7;

    assert!(state.toggle_file_expanded(1));
    assert_eq!(state.items.len(), before - 2);
    assert_eq!(state.selected_index, 5);
}

#[test]
fn experiment_toggle_file_expanded_scale_baseline() {
    let files = 600usize;
    let matches_per_file = 12usize;
    let loops = 400usize;
    let mut state = seeded_state(files, matches_per_file);
    let target = files / 2;

    let start = Instant::now();
    let mut changed_count = 0usize;
    for _ in 0..loops {
        changed_count += usize::from(state.toggle_file_expanded(target));
        changed_count += usize::from(state.toggle_file_expanded(target));
    }
    let elapsed = start.elapsed();
    let toggles = loops * 2;
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / toggles as f64;

    eprintln!(
        "[experiment] search_toggle_expand loops={} files={} matches_per_file={} items={} elapsed_ms={} avg_us={:.2} changed_count={}",
        loops,
        files,
        matches_per_file,
        state.items.len(),
        elapsed.as_millis(),
        avg_us,
        changed_count
    );

    assert_eq!(changed_count, toggles);
    assert!(state.files[target].expanded);
}
