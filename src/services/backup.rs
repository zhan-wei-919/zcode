//! 备份路径管理
//!
//! 跨平台的备份文件路径生成，类似 VS Code 的逻辑：
//! - macOS: ~/Library/Application Support/zcode/backups/<hash>.ops
//! - Linux: ~/.local/share/zcode/backups/<hash>.ops
//! - Windows: %APPDATA%\zcode\backups\<hash>.ops

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

const APP_NAME: &str = "zcode";
const BACKUP_DIR: &str = "backups";
const LOG_DIR: &str = "logs";

/// 获取应用数据目录
fn get_app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs_path_macos()
    }

    #[cfg(target_os = "linux")]
    {
        dirs_path_linux()
    }

    #[cfg(target_os = "windows")]
    {
        dirs_path_windows()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn dirs_path_macos() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join("Library/Application Support").join(APP_NAME))
}

#[cfg(target_os = "linux")]
fn dirs_path_linux() -> Option<PathBuf> {
    // 优先使用 XDG_DATA_HOME，否则使用 ~/.local/share
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        Some(PathBuf::from(xdg).join(APP_NAME))
    } else {
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".local/share").join(APP_NAME))
    }
}

#[cfg(target_os = "windows")]
fn dirs_path_windows() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|appdata| PathBuf::from(appdata).join(APP_NAME))
}

/// 计算文件路径的哈希值（用于生成备份文件名）
fn hash_path(path: &std::path::Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 获取备份目录路径
pub fn get_backup_dir() -> Option<PathBuf> {
    get_app_data_dir().map(|p| p.join(BACKUP_DIR))
}

/// 获取日志目录路径
pub fn get_log_dir() -> Option<PathBuf> {
    get_app_data_dir().map(|p| p.join(LOG_DIR))
}

/// 获取指定文件的 .ops 备份文件路径
pub fn get_ops_file_path(file_path: &std::path::Path) -> Option<PathBuf> {
    // 获取绝对路径
    let abs_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        std::env::current_dir()
            .ok()?
            .join(file_path)
            .canonicalize()
            .ok()?
    };

    let hash = hash_path(&abs_path);
    get_backup_dir().map(|dir| dir.join(format!("{}.ops", hash)))
}

/// 确保备份目录存在
pub fn ensure_backup_dir() -> std::io::Result<PathBuf> {
    let dir = get_backup_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Cannot determine backup directory")
    })?;

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    Ok(dir)
}

/// 确保日志目录存在
pub fn ensure_log_dir() -> std::io::Result<PathBuf> {
    let dir = get_log_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "Cannot determine log directory")
    })?;

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_path() {
        let path1 = std::path::Path::new("/Users/test/file.txt");
        let path2 = std::path::Path::new("/Users/test/file.txt");
        let path3 = std::path::Path::new("/Users/test/other.txt");

        assert_eq!(hash_path(path1), hash_path(path2));
        assert_ne!(hash_path(path1), hash_path(path3));
    }

    #[test]
    fn test_get_backup_dir() {
        let dir = get_backup_dir();
        // 在测试环境中应该能获取到目录
        assert!(dir.is_some());
        let dir = dir.unwrap();
        assert!(dir.to_string_lossy().contains(APP_NAME));
        assert!(dir.to_string_lossy().contains(BACKUP_DIR));
    }

    #[test]
    fn test_get_log_dir() {
        let dir = get_log_dir();
        assert!(dir.is_some());
        let dir = dir.unwrap();
        assert!(dir.to_string_lossy().contains(APP_NAME));
        assert!(dir.to_string_lossy().contains(LOG_DIR));
    }

    #[test]
    fn test_get_ops_file_path() {
        let file_path = std::path::Path::new("/tmp/test.txt");
        let ops_path = get_ops_file_path(file_path);

        assert!(ops_path.is_some());
        let ops_path = ops_path.unwrap();
        assert!(ops_path.to_string_lossy().ends_with(".ops"));
    }
}
