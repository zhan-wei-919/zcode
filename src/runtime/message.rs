//! 异步消息定义

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
}

pub enum AppMessage {
    DirLoaded {
        path: PathBuf,
        entries: Vec<DirEntryInfo>,
    },
    DirLoadError {
        path: PathBuf,
        error: String,
    },

    FileLoaded {
        path: PathBuf,
        content: String,
    },
    FileError {
        path: PathBuf,
        error: String,
    },
}
