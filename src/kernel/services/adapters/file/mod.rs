//! 文件服务模块
//!
//! 提供文件系统抽象，支持多种后端（本地、SSH、FTP 等）

pub mod local;
pub mod service;

pub use local::LocalFileProvider;
pub use service::FileService;
