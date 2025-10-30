//zcode/src/editor/core/mod.rs
//! 编辑器核心组件
//! 
//! 包含：
//! - Editor: 编辑器主状态和协调器
//! - TextModel: 文本数据模型
//! - EditorView: 视图和渲染逻辑

pub mod state;
pub mod text_model;
pub mod view;

// 重新导出常用类型
pub use state::Editor;
pub use text_model::TextModel;
pub use view::EditorView;

