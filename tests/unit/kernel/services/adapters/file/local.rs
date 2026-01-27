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
