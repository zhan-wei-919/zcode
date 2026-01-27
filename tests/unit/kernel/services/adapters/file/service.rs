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
