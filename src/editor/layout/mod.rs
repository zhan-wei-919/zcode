//zcode/src/editor/layout/mod.rs
//! 布局引擎
//! 
//! 负责文本行的布局计算和缓存：
//! - 字形簇到屏幕坐标的映射
//! - Tab 展开
//! - Unicode 宽度计算
//! - 布局缓存和失效管理

pub mod layout;

// 重新导出
pub use layout::{LayoutEngine, LineLayout};

