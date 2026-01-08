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
            let metadata = entry.metadata()?;

            let dir_entry = DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path,
                is_dir: metadata.is_dir(),
                is_file: metadata.is_file(),
                is_symlink: metadata.is_symlink(),
                size: metadata.len(),
                modified: metadata.modified().ok(),
            };

            entries.push(dir_entry);
        }

        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

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
        Ok(FileMetadata::from_std(meta))
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        Ok(fs::canonicalize(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_read_write_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let provider = LocalFileProvider::new();

        provider.write_file(&file_path, "Hello, World!").unwrap();
        assert!(provider.exists(&file_path));
        assert!(provider.is_file(&file_path));

        let content = provider.read_file(&file_path).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[test]
    fn test_read_dir() {
        let dir = tempdir().unwrap();

        fs::create_dir(dir.path().join("subdir")).unwrap();
        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(dir.path().join("file2.txt")).unwrap();

        let provider = LocalFileProvider::new();
        let entries = provider.read_dir(dir.path()).unwrap();

        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "subdir");
    }

    #[test]
    fn test_create_delete_dir() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("newdir");

        let provider = LocalFileProvider::new();

        provider.create_dir(&subdir).unwrap();
        assert!(provider.is_dir(&subdir));

        provider.delete_dir(&subdir).unwrap();
        assert!(!provider.exists(&subdir));
    }

    #[test]
    fn test_rename() {
        let dir = tempdir().unwrap();
        let old_path = dir.path().join("old.txt");
        let new_path = dir.path().join("new.txt");

        let provider = LocalFileProvider::new();

        provider.write_file(&old_path, "content").unwrap();
        provider.rename(&old_path, &new_path).unwrap();

        assert!(!provider.exists(&old_path));
        assert!(provider.exists(&new_path));
    }

    #[test]
    fn test_copy() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");

        let provider = LocalFileProvider::new();

        provider.write_file(&src, "content").unwrap();
        provider.copy(&src, &dst).unwrap();

        assert!(provider.exists(&src));
        assert!(provider.exists(&dst));
        assert_eq!(provider.read_file(&dst).unwrap(), "content");
    }

    #[test]
    fn test_metadata() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let provider = LocalFileProvider::new();
        provider.write_file(&file_path, "Hello").unwrap();

        let meta = provider.metadata(&file_path).unwrap();
        assert!(meta.is_file);
        assert!(!meta.is_dir);
        assert_eq!(meta.size, 5);
    }

    #[test]
    fn test_not_found_error() {
        let provider = LocalFileProvider::new();
        let result = provider.read_file(Path::new("/nonexistent/file.txt"));
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }
}
