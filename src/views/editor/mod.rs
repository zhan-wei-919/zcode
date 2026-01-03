//! Editor UI: pure render + hit-test over kernel state.

mod hit_test;
mod layout;
mod render;

pub use hit_test::{hit_test_editor_mouse, hit_test_editor_tab};
pub use layout::{compute_editor_pane_layout, EditorPaneLayout};
pub use render::{cursor_position_editor, render_editor_pane};

