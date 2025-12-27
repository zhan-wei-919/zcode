//! 服务层模块
//!
//! 提供可扩展的服务实现：
//! - FileService: 文件系统服务（支持多 Provider）
//! - KeybindingService: 快捷键服务
//! - ConfigService: 配置服务
//! - ClipboardService: 剪贴板服务
//! - backup: 备份路径管理
//! - search: 搜索服务（单文件 + 全局搜索）

pub mod backup;
pub mod clipboard;
pub mod config;
pub mod file;
pub mod keybinding;
pub mod search;

pub use backup::{ensure_backup_dir, get_backup_dir, get_ops_file_path};
pub use clipboard::{ClipboardError, ClipboardService};
pub use config::{ConfigService, EditorConfig};
pub use file::{DirEntry, FileError, FileMetadata, FileProvider, FileService, LocalFileProvider};
pub use keybinding::KeybindingService;
pub use search::{
    FileMatches, GlobalSearchMessage, GlobalSearchService, GlobalSearchTask,
    Match, SearchConfig, SearchMessage, SearchService, SearchTask, StreamSearcher,
};
