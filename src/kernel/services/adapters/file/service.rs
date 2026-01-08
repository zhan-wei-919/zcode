//! 文件服务：管理多个 FileProvider
//!
//! 根据 URI scheme 选择对应的 Provider

use super::local::LocalFileProvider;
use crate::core::Service;
use crate::kernel::services::ports::file::{
    DirEntry, FileError, FileMetadata, FileProvider, Result,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct FileService {
    providers: HashMap<String, Box<dyn FileProvider>>,
    default_scheme: String,
}

impl FileService {
    pub fn new() -> Self {
        let mut service = Self {
            providers: HashMap::new(),
            default_scheme: "file".to_string(),
        };
        service.register_provider(Box::new(LocalFileProvider::new()));
        service
    }

    pub fn register_provider(&mut self, provider: Box<dyn FileProvider>) {
        let scheme = provider.scheme().to_string();
        self.providers.insert(scheme, provider);
    }

    pub fn set_default_scheme(&mut self, scheme: &str) {
        self.default_scheme = scheme.to_string();
    }

    fn get_provider(&self, scheme: &str) -> Result<&dyn FileProvider> {
        self.providers
            .get(scheme)
            .map(|p| p.as_ref())
            .ok_or_else(|| FileError::ProviderNotFound(scheme.to_string()))
    }

    fn default_provider(&self) -> Result<&dyn FileProvider> {
        self.get_provider(&self.default_scheme)
    }

    pub fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        self.default_provider()?.read_dir(path)
    }

    pub fn read_file(&self, path: &Path) -> Result<String> {
        self.default_provider()?.read_file(path)
    }

    pub fn read_file_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        self.default_provider()?.read_file_bytes(path)
    }

    pub fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        self.default_provider()?.write_file(path, content)
    }

    pub fn write_file_bytes(&self, path: &Path, content: &[u8]) -> Result<()> {
        self.default_provider()?.write_file_bytes(path, content)
    }

    pub fn create_dir(&self, path: &Path) -> Result<()> {
        self.default_provider()?.create_dir(path)
    }

    pub fn create_dir_all(&self, path: &Path) -> Result<()> {
        self.default_provider()?.create_dir_all(path)
    }

    pub fn delete_file(&self, path: &Path) -> Result<()> {
        self.default_provider()?.delete_file(path)
    }

    pub fn delete_dir(&self, path: &Path) -> Result<()> {
        self.default_provider()?.delete_dir(path)
    }

    pub fn delete_dir_all(&self, path: &Path) -> Result<()> {
        self.default_provider()?.delete_dir_all(path)
    }

    pub fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.default_provider()?.rename(from, to)
    }

    pub fn copy(&self, from: &Path, to: &Path) -> Result<()> {
        self.default_provider()?.copy(from, to)
    }

    pub fn exists(&self, path: &Path) -> bool {
        self.default_provider()
            .map(|p| p.exists(path))
            .unwrap_or(false)
    }

    pub fn is_dir(&self, path: &Path) -> bool {
        self.default_provider()
            .map(|p| p.is_dir(path))
            .unwrap_or(false)
    }

    pub fn is_file(&self, path: &Path) -> bool {
        self.default_provider()
            .map(|p| p.is_file(path))
            .unwrap_or(false)
    }

    pub fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        self.default_provider()?.metadata(path)
    }

    pub fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        self.default_provider()?.canonicalize(path)
    }

    pub fn has_provider(&self, scheme: &str) -> bool {
        self.providers.contains_key(scheme)
    }

    pub fn available_schemes(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for FileService {
    fn default() -> Self {
        Self::new()
    }
}

impl Service for FileService {
    fn name(&self) -> &'static str {
        "FileService"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_file_service_new() {
        let service = FileService::new();
        assert!(service.has_provider("file"));
    }

    #[test]
    fn test_read_write() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let service = FileService::new();

        service.write_file(&file_path, "Hello").unwrap();
        let content = service.read_file(&file_path).unwrap();
        assert_eq!(content, "Hello");
    }

    #[test]
    fn test_available_schemes() {
        let service = FileService::new();
        let schemes = service.available_schemes();
        assert!(schemes.contains(&"file"));
    }

    #[test]
    fn test_service_trait() {
        let service = FileService::new();
        assert_eq!(service.name(), "FileService");
    }
}
