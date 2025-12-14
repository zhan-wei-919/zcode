//zcode/src/editor/input/mod.rs
//! 输入系统
//! 
//! 包含：
//! - input: 键盘和鼠标事件处理
//! - mouse: 鼠标状态管理（点击计数、拖拽）
//! - command: 语义命令定义
//! - keybinding: 按键到命令的映射
//! - selection: 选区模型（字符/词/行粒度）
//! - executor: 命令执行器

pub mod input;
pub mod mouse;
pub mod command;
pub mod keybinding;
pub mod selection;
pub mod executor;

// 重新导出常用类型
pub use mouse::MouseController;
pub use command::{Command, Key};
pub use keybinding::Keybindings;
pub use selection::{Selection, Granularity};

