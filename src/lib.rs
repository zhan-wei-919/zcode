//! zcode - TUI 文本编辑器库
//!
//! 模块结构：
//! - core: 核心框架（Service, View, Command, Event）
//! - services: 服务层（FileService, KeybindingService, ConfigService）
//! - models: 数据模型（FileTree, TextBuffer, Selection）
//! - views: 视图层（ExplorerView, EditorView, EditorGroup）
//! - app: 应用层（Workbench）

pub mod app;
pub mod core;
pub mod models;
pub mod services;
pub mod views;
