//! 文件系统 Provider trait
//!
//! 抽象文件系统操作，支持本地、SSH、FTP 等多种后端

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub type Result<T> = std::result::Result<T, FileError>;

#[derive(Debug)]
pub enum FileError {
    Io(io::Error),
    NotFound(PathBuf),
    PermissionDenied(PathBuf),
    AlreadyExists(PathBuf),
    NotADirectory(PathBuf),
    NotAFile(PathBuf),
    InvalidPath(String),
    ProviderNotFound(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::Io(e) => write!(f, "IO error: {}", e),
            FileError::NotFound(p) => write!(f, "Not found: {}", p.display()),
            FileError::PermissionDenied(p) => write!(f, "Permission denied: {}", p.display()),
            FileError::AlreadyExists(p) => write!(f, "Already exists: {}", p.display()),
            FileError::NotADirectory(p) => write!(f, "Not a directory: {}", p.display()),
            FileError::NotAFile(p) => write!(f, "Not a file: {}", p.display()),
            FileError::InvalidPath(s) => write!(f, "Invalid path: {}", s),
            FileError::ProviderNotFound(s) => write!(f, "Provider not found: {}", s),
        }
    }
}

impl std::error::Error for FileError {}

impl From<io::Error> for FileError {
    fn from(e: io::Error) -> Self {
        FileError::Io(e)
    }
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl DirEntry {
    pub fn new(path: PathBuf, is_dir: bool) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            path,
            name,
            is_dir,
            is_file: !is_dir,
            is_symlink: false,
            size: 0,
            modified: None,
        }
    }
}

pub trait FileProvider: Send + Sync {
    fn scheme(&self) -> &'static str;

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;

    fn read_file(&self, path: &Path) -> Result<String>;

    fn read_file_bytes(&self, path: &Path) -> Result<Vec<u8>>;

    fn write_file(&self, path: &Path, content: &str) -> Result<()>;

    fn write_file_bytes(&self, path: &Path, content: &[u8]) -> Result<()>;

    fn create_dir(&self, path: &Path) -> Result<()>;

    fn create_dir_all(&self, path: &Path) -> Result<()>;

    fn delete_file(&self, path: &Path) -> Result<()>;

    fn delete_dir(&self, path: &Path) -> Result<()>;

    fn delete_dir_all(&self, path: &Path) -> Result<()>;

    fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    fn copy(&self, from: &Path, to: &Path) -> Result<()>;

    fn exists(&self, path: &Path) -> bool;

    fn is_dir(&self, path: &Path) -> bool;

    fn is_file(&self, path: &Path) -> bool;

    fn metadata(&self, path: &Path) -> Result<FileMetadata>;

    fn canonicalize(&self, path: &Path) -> Result<PathBuf>;
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub readonly: bool,
}

impl FileMetadata {
    pub fn from_std(meta: std::fs::Metadata) -> Self {
        Self {
            size: meta.len(),
            is_dir: meta.is_dir(),
            is_file: meta.is_file(),
            is_symlink: meta.is_symlink(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
            accessed: meta.accessed().ok(),
            readonly: meta.permissions().readonly(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dir_entry_new() {
        let entry = DirEntry::new(PathBuf::from("/test/file.txt"), false);
        assert_eq!(entry.name, "file.txt");
        assert!(!entry.is_dir);
        assert!(entry.is_file);
    }

    #[test]
    fn test_file_error_display() {
        let err = FileError::NotFound(PathBuf::from("/test"));
        assert!(err.to_string().contains("/test"));
    }
}
