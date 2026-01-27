//! 本地文件系统 Provider
//!
//! 实现 FileProvider trait，操作本地文件系统

use crate::kernel::services::ports::file::{
    DirEntry, FileError, FileMetadata, FileProvider, Result,
};
use std::fs;
use std::path::{Path, PathBuf};

pub struct LocalFileProvider;

impl LocalFileProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LocalFileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FileProvider for LocalFileProvider {
    fn scheme(&self) -> &'static str {
        "file"
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let mut entries = Vec::new();

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let dir_entry = DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path,
                is_dir: metadata.is_dir(),
                is_file: metadata.is_file(),
                is_symlink: file_type.is_symlink(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
            };

            entries.push(dir_entry);
        }

        entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));

        Ok(entries)
    }

    fn read_file(&self, path: &Path) -> Result<String> {
        if !path.exists() {
            return Err(FileError::NotFound(path.to_path_buf()));
        }
        if !path.is_file() {
            return Err(FileError::NotAFile(path.to_path_buf()));
        }
        Ok(fs::read_to_string(path)?)
    }

    fn read_file_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        if !path.exists() {
            return Err(FileError::NotFound(path.to_path_buf()));
        }
        if !path.is_file() {
            return Err(FileError::NotAFile(path.to_path_buf()));
        }
        Ok(fs::read(path)?)
    }

    fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        Ok(fs::write(path, content)?)
    }

    fn write_file_bytes(&self, path: &Path, content: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        Ok(fs::write(path, content)?)
    }

    fn create_dir(&self, path: &Path) -> Result<()> {
        if path.exists() {
            return Err(FileError::AlreadyExists(path.to_path_buf()));
        }
        Ok(fs::create_dir(path)?)
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        Ok(fs::create_dir_all(path)?)
    }

    fn delete_file(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(FileError::NotFound(path.to_path_buf()));
        }
        if !path.is_file() {
            return Err(FileError::NotAFile(path.to_path_buf()));
        }
        Ok(fs::remove_file(path)?)
    }

    fn delete_dir(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(FileError::NotFound(path.to_path_buf()));
        }
        if !path.is_dir() {
            return Err(FileError::NotADirectory(path.to_path_buf()));
        }
        Ok(fs::remove_dir(path)?)
    }

    fn delete_dir_all(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(FileError::NotFound(path.to_path_buf()));
        }
        if !path.is_dir() {
            return Err(FileError::NotADirectory(path.to_path_buf()));
        }
        Ok(fs::remove_dir_all(path)?)
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        if !from.exists() {
            return Err(FileError::NotFound(from.to_path_buf()));
        }
        if to.exists() {
            return Err(FileError::AlreadyExists(to.to_path_buf()));
        }
        Ok(fs::rename(from, to)?)
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<()> {
        if !from.exists() {
            return Err(FileError::NotFound(from.to_path_buf()));
        }
        if !from.is_file() {
            return Err(FileError::NotAFile(from.to_path_buf()));
        }
        if to.exists() {
            return Err(FileError::AlreadyExists(to.to_path_buf()));
        }
        if let Some(parent) = to.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::copy(from, to)?;
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        if !path.exists() {
            return Err(FileError::NotFound(path.to_path_buf()));
        }
        let meta = fs::metadata(path)?;
        let is_symlink = fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let mut out = FileMetadata::from_std(meta);
        out.is_symlink = is_symlink;
        Ok(out)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        Ok(fs::canonicalize(path)?)
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/kernel/services/adapters/file/local.rs"]
mod tests;
