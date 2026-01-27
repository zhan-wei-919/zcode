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
