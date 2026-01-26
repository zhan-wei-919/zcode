//! 核心框架模块
//!
//! 提供可扩展编辑器框架的核心抽象：
//! - Service: 服务注册与依赖注入
//! - Event: 统一事件定义
//! - Command: 命令系统
//! - Context: 应用上下文

pub mod command;
pub mod context;
pub mod event;
pub mod service;
pub mod text_window;

pub use command::Command;
pub use context::AppContext;
pub use event::{InputEvent, Key, MouseAction, MousePosition};
pub use service::{Service, ServiceError, ServiceRegistry};
