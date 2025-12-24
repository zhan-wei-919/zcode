//! 服务层模块
//!
//! 提供可扩展的服务实现：
//! - FileService: 文件系统服务（支持多 Provider）
//! - KeybindingService: 快捷键服务
//! - ConfigService: 配置服务
//! - backup: 备份路径管理

pub mod backup;
pub mod config;
pub mod file;
pub mod keybinding;

pub use backup::{ensure_backup_dir, get_backup_dir, get_ops_file_path};
pub use config::{ConfigService, EditorConfig};
pub use file::{DirEntry, FileError, FileMetadata, FileProvider, FileService, LocalFileProvider};
pub use keybinding::KeybindingService;
