use super::*;
use crate::kernel::editor::{EditorPaneState, EditorTabState, TabId};
use crate::kernel::services::ports::EditorConfig;
use crate::ui::core::geom::Rect;

fn pane_with_tabs(config: &EditorConfig, tabs: &[(&str, bool)]) -> EditorPaneState {
    let mut pane = EditorPaneState::new(config);
    pane.tabs = tabs
        .iter()
        .enumerate()
        .map(|(i, (title, dirty))| {
            let mut tab = EditorTabState::untitled(TabId::new(i as u64 + 1), config);
            tab.title = (*title).to_string();
            tab.dirty = *dirty;
            tab
        })
        .collect();
    pane
}

#[test]
fn tab_insertion_index_none_outside_tab_row_or_area() {
    let config = EditorConfig::default();
    let pane = pane_with_tabs(&config, &[("a", false)]);
    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 20, 10), &pane, &config);

    // Different row.
    assert_eq!(
        tab_insertion_index(
            &layout,
            &pane,
            layout.tab_area.x,
            layout.tab_area.y + 1,
            None
        ),
        None
    );
    // Outside column.
    assert_eq!(
        tab_insertion_index(&layout, &pane, 999, layout.tab_area.y, None),
        None
    );
}

#[test]
fn tab_insertion_index_empty_tabs_returns_zero() {
    let config = EditorConfig::default();
    let pane = pane_with_tabs(&config, &[]);
    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 20, 10), &pane, &config);

    assert_eq!(
        tab_insertion_index(&layout, &pane, layout.tab_area.x, layout.tab_area.y, None),
        Some(0)
    );
}

#[test]
fn tab_insertion_index_returns_between_tabs_on_divider() {
    let config = EditorConfig::default();
    let pane = pane_with_tabs(&config, &[("a", false), ("b", false)]);
    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 20, 10), &pane, &config);

    // For the current tab layout algorithm: each tab " a " is width 3, divider is width 1.
    // First tab: [0..3), divider: [3..4), second tab starts at 4.
    assert_eq!(
        tab_insertion_index(&layout, &pane, 3, layout.tab_area.y, None),
        Some(1)
    );
}

#[test]
fn tab_insertion_index_is_monotonic() {
    let config = EditorConfig::default();
    let pane = pane_with_tabs(&config, &[("a", false), ("bbbb", true), ("c", false)]);
    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 10), &pane, &config);

    let y = layout.tab_area.y;
    let mut prev = 0usize;
    for x in layout.tab_area.x..layout.tab_area.x + layout.tab_area.w {
        let idx = tab_insertion_index(&layout, &pane, x, y, None).unwrap();
        assert!(idx <= pane.tabs.len());
        assert!(idx >= prev);
        prev = idx;
    }
}
