//! Editor UI: pure render + hit-test over kernel state.

mod hit_test;
mod layout;
mod render;

pub use hit_test::{
    hit_test_editor_mouse, hit_test_editor_mouse_drag, hit_test_editor_tab, hit_test_search_bar,
    hit_test_tab_hover, tab_insertion_index, tab_insertion_x, DragHitResult, SearchBarHitResult,
    TabHitResult,
};
pub use layout::{compute_editor_pane_layout, EditorPaneLayout};
pub use render::{cursor_position_editor, paint_editor_pane};
