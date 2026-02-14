//! 视图层模块
//!
//! 所有 UI 视图组件：
//! - ExplorerView: 文件浏览器
//! - Editor: 纯渲染/命中测试
//! - SearchView: 全局搜索面板（纯渲染）

pub mod editor;
pub mod explorer;
pub mod search;
pub mod theme_editor;

pub use editor::{
    compute_editor_pane_layout, compute_tab_row_layout, cursor_position_editor, ellipsize_title,
    hit_test_editor_mouse, hit_test_editor_mouse_drag, hit_test_editor_tab,
    hit_test_editor_vertical_scrollbar, hit_test_search_bar, hit_test_tab_hover, paint_editor_pane,
    tab_insertion_index, tab_insertion_x, vertical_scrollbar_metrics, DragHitResult,
    EditorPaneLayout, EditorVerticalScrollbarHitResult, SearchBarHitResult, TabHitResult,
    TabRowLayout, TabRowSlot, VerticalScrollbarMetrics,
};
pub use explorer::{ExplorerPaintCtx, ExplorerView};
pub use search::SearchView;
