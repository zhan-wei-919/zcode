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
