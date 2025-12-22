//! 视图层模块
//!
//! 所有 UI 视图组件：
//! - ExplorerView: 文件浏览器
//! - EditorView: 编辑器
//! - EditorGroup: 多 Tab 管理

pub mod editor;
pub mod explorer;

pub use editor::{EditorGroup, EditorTab, EditorView, Viewport};
pub use explorer::ExplorerView;
