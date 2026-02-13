use super::*;
use crate::kernel::editor::{EditorPaneState, EditorTabState, SearchBarMode, TabId};
use crate::kernel::services::ports::EditorConfig;
use crate::ui::core::geom::Rect;
use crate::ui::core::painter::{PaintCmd, Painter};
use crate::ui::core::theme::Theme;

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

fn layout_with_content_area(content_area: Rect) -> crate::views::EditorPaneLayout {
    crate::views::EditorPaneLayout {
        area: Rect::new(0, 0, 80, 24),
        tab_area: Rect::new(0, 0, 80, 1),
        search_area: None,
        editor_area: Rect::new(0, 1, 80, 23),
        gutter_area: Rect::new(0, 1, content_area.x, content_area.h),
        content_area,
        gutter_width: content_area.x,
    }
}

fn nav_buttons_origin(
    pane: &EditorPaneState,
    layout: &crate::views::EditorPaneLayout,
    config: &EditorConfig,
) -> (u16, u16) {
    let mut painter = Painter::new();
    crate::views::paint_editor_pane(
        &mut painter,
        layout,
        pane,
        config,
        &Theme::default(),
        None,
        false,
    );

    painter
        .cmds()
        .iter()
        .find_map(|cmd| match cmd {
            PaintCmd::Text { pos, text, .. } if text == " ▲ ▼ ✕" => Some((pos.x, pos.y)),
            _ => None,
        })
        .expect("search nav buttons")
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

// --- hit_test_editor_mouse_drag tests ---

#[test]
fn drag_hit_test_inside_matches_normal_hit_test() {
    // content_area at (5, 2) with size 40x20
    let layout = layout_with_content_area(Rect::new(5, 2, 40, 20));
    let col = 10;
    let row = 5;

    let normal = hit_test_editor_mouse(&layout, col, row).unwrap();
    let drag = hit_test_editor_mouse_drag(&layout, col, row).unwrap();

    assert_eq!(drag.x, normal.0);
    assert_eq!(drag.y, normal.1);
    assert_eq!(drag.overflow_y, 0);
    assert!(!drag.past_right);
}

#[test]
fn drag_hit_test_left_of_content_clamps_to_zero() {
    let layout = layout_with_content_area(Rect::new(5, 2, 40, 20));
    // Column 2 is left of content_area.x=5 (in the gutter)
    let drag = hit_test_editor_mouse_drag(&layout, 2, 5).unwrap();

    assert_eq!(drag.x, 0);
    assert_eq!(drag.overflow_y, 0);
    assert!(!drag.past_right);
}

#[test]
fn drag_hit_test_right_of_content_sets_past_right() {
    let layout = layout_with_content_area(Rect::new(5, 2, 40, 20));
    // Column 50 is at content_area.right()=45, so past right
    let drag = hit_test_editor_mouse_drag(&layout, 50, 5).unwrap();

    assert_eq!(drag.x, 39); // clamped to width-1
    assert!(drag.past_right);
    assert_eq!(drag.overflow_y, 0);
}

#[test]
fn drag_hit_test_above_content_negative_overflow() {
    let layout = layout_with_content_area(Rect::new(5, 2, 40, 20));
    // Row 0 is 2 rows above content_area.y=2
    let drag = hit_test_editor_mouse_drag(&layout, 10, 0).unwrap();

    assert_eq!(drag.y, 0);
    assert_eq!(drag.overflow_y, -2);
    assert!(!drag.past_right);
}

#[test]
fn drag_hit_test_below_content_positive_overflow() {
    let layout = layout_with_content_area(Rect::new(5, 2, 40, 20));
    // content_area bottom = 2+20 = 22, row 25 is 4 rows below
    let drag = hit_test_editor_mouse_drag(&layout, 10, 25).unwrap();

    assert_eq!(drag.y, 19); // clamped to height-1
    assert_eq!(drag.overflow_y, 4);
    assert!(!drag.past_right);
}

#[test]
fn drag_hit_test_diagonal_clamps_both() {
    let layout = layout_with_content_area(Rect::new(5, 2, 40, 20));
    // Bottom-right corner: past right and below
    let drag = hit_test_editor_mouse_drag(&layout, 60, 30).unwrap();

    assert_eq!(drag.x, 39);
    assert_eq!(drag.y, 19);
    assert!(drag.past_right);
    assert!(drag.overflow_y > 0);
}

#[test]
fn drag_hit_test_empty_content_area_returns_none() {
    let layout = layout_with_content_area(Rect::new(0, 0, 0, 0));
    assert!(hit_test_editor_mouse_drag(&layout, 5, 5).is_none());
}

#[test]
fn search_bar_hit_test_detects_prev_next_and_close_buttons() {
    let config = EditorConfig::default();
    let mut pane = pane_with_tabs(&config, &[("a", false)]);
    pane.search_bar.show(SearchBarMode::Search);
    pane.search_bar.search_text = "hello".to_string();
    pane.search_bar.cursor_pos = pane.search_bar.search_text.len();
    pane.search_bar.current_match_index = Some(0);
    pane.search_bar.matches = vec![
        crate::kernel::services::ports::Match::new(0, 5, 0, 0),
        crate::kernel::services::ports::Match::new(12, 17, 0, 12),
    ];

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 80, 12), &pane, &config);
    let (nav_x, nav_y) = nav_buttons_origin(&pane, &layout, &config);

    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 1, nav_y),
        Some(SearchBarHitResult::PrevMatch)
    );
    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 3, nav_y),
        Some(SearchBarHitResult::NextMatch)
    );
    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 5, nav_y),
        Some(SearchBarHitResult::Close)
    );
}

#[test]
fn search_bar_hit_test_ignores_spaces_and_non_search_rows() {
    let config = EditorConfig::default();
    let mut pane = pane_with_tabs(&config, &[("a", false)]);
    pane.search_bar.show(SearchBarMode::Search);

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 80, 12), &pane, &config);
    let (nav_x, nav_y) = nav_buttons_origin(&pane, &layout, &config);

    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x, nav_y),
        None
    );
    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 2, nav_y),
        None
    );
    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 1, nav_y + 1),
        None
    );
}

#[test]
fn search_bar_hit_test_works_in_replace_mode_top_row_buttons() {
    let config = EditorConfig::default();
    let mut pane = pane_with_tabs(&config, &[("a", false)]);
    pane.search_bar.show(SearchBarMode::Replace);

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 80, 12), &pane, &config);
    let (nav_x, nav_y) = nav_buttons_origin(&pane, &layout, &config);

    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 1, nav_y),
        Some(SearchBarHitResult::PrevMatch)
    );
    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 3, nav_y),
        Some(SearchBarHitResult::NextMatch)
    );
    assert_eq!(
        hit_test_search_bar(&layout, &pane.search_bar, nav_x + 5, nav_y),
        Some(SearchBarHitResult::Close)
    );
}
