//! Editor UI: pure render + hit-test over kernel state.

pub mod coord;
mod hit_test;
mod layout;
pub mod markdown;
pub mod markdown_cache;
mod render;
mod tab_row;

pub use hit_test::{
    hit_test_editor_mouse, hit_test_editor_mouse_drag, hit_test_editor_tab,
    hit_test_editor_vertical_scrollbar, hit_test_search_bar, hit_test_tab_hover,
    tab_insertion_index, tab_insertion_x, DragHitResult, EditorVerticalScrollbarHitResult,
    SearchBarHitResult, TabHitResult,
};
pub use layout::{
    compute_editor_pane_layout, vertical_scrollbar_metrics, EditorPaneLayout,
    VerticalScrollbarMetrics,
};
pub use render::{
    cursor_position_editor, paint_editor_pane, EditorPaneRenderOptions, TransientRowHighlight,
};
pub use tab_row::{compute_tab_row_layout, ellipsize_title, TabRowLayout, TabRowSlot};
