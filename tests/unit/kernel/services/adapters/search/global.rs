use super::*;
use std::fs;
use std::sync::mpsc;
use tempfile::tempdir;

fn create_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn test_global_search_literal() {
    let rt = create_runtime();
    let service = GlobalSearchService::new(rt.handle().clone());
    let (tx, rx) = mpsc::sync_channel(64);

    let dir = tempdir().unwrap();
    let file1 = dir.path().join("test1.txt");
    let file2 = dir.path().join("test2.txt");
    let file3 = dir.path().join("other.txt");

    fs::write(&file1, "hello world").unwrap();
    fs::write(&file2, "hello rust").unwrap();
    fs::write(&file3, "goodbye world").unwrap();

    let _task = service.search_in_dir(
        dir.path().to_path_buf(),
        "hello".to_string(),
        true,
        false,
        tx,
    );

    let mut file_matches = Vec::new();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(GlobalSearchMessage::FileMatches {
                file_matches: fm, ..
            }) => {
                file_matches.push(fm);
            }
            Ok(GlobalSearchMessage::Complete { total_matches, .. }) => {
                assert_eq!(total_matches, 2);
                break;
            }
            Ok(GlobalSearchMessage::Progress { .. }) => continue,
            Ok(GlobalSearchMessage::Cancelled { .. }) => panic!("Unexpected cancel"),
            Ok(GlobalSearchMessage::Error { message, .. }) => panic!("Error: {}", message),
            Err(_) => panic!("Timeout"),
        }
    }

    assert_eq!(file_matches.len(), 2);
}

#[test]
fn test_global_search_regex() {
    let rt = create_runtime();
    let service = GlobalSearchService::new(rt.handle().clone());
    let (tx, rx) = mpsc::sync_channel(64);

    let dir = tempdir().unwrap();
    let file1 = dir.path().join("test1.txt");
    let file2 = dir.path().join("test2.txt");

    fs::write(&file1, "hello123 world").unwrap();
    fs::write(&file2, "hello456 rust").unwrap();

    let _task = service.search_in_dir(
        dir.path().to_path_buf(),
        r"hello\d+".to_string(),
        true,
        true,
        tx,
    );

    let mut total = 0;
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(GlobalSearchMessage::FileMatches { file_matches, .. }) => {
                total += file_matches.matches.len();
            }
            Ok(GlobalSearchMessage::Complete { total_matches, .. }) => {
                assert_eq!(total_matches, 2);
                assert_eq!(total, 2);
                break;
            }
            Ok(GlobalSearchMessage::Progress { .. }) => continue,
            Ok(GlobalSearchMessage::Cancelled { .. }) => panic!("Unexpected cancel"),
            Ok(GlobalSearchMessage::Error { message, .. }) => panic!("Error: {}", message),
            Err(_) => panic!("Timeout"),
        }
    }
}

#[test]
fn test_skip_binary_files() {
    let rt = create_runtime();
    let service = GlobalSearchService::new(rt.handle().clone());
    let (tx, rx) = mpsc::sync_channel(64);

    let dir = tempdir().unwrap();
    let text_file = dir.path().join("text.txt");
    let binary_file = dir.path().join("binary.bin");

    fs::write(&text_file, "hello world").unwrap();
    fs::write(&binary_file, b"hello\x00world").unwrap();

    let _task = service.search_in_dir(
        dir.path().to_path_buf(),
        "hello".to_string(),
        true,
        false,
        tx,
    );

    let mut found_files = Vec::new();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(GlobalSearchMessage::FileMatches { file_matches, .. }) => {
                found_files.push(file_matches.path);
            }
            Ok(GlobalSearchMessage::Complete { .. }) => break,
            Ok(_) => continue,
            Err(_) => panic!("Timeout"),
        }
    }

    assert_eq!(found_files.len(), 1);
    assert!(found_files[0].ends_with("text.txt"));
}

#[test]
fn test_cancel_search() {
    let rt = create_runtime();
    let service = GlobalSearchService::new(rt.handle().clone());
    let (tx, rx) = mpsc::sync_channel(64);

    let dir = tempdir().unwrap();
    for i in 0..100 {
        let file = dir.path().join(format!("file{}.txt", i));
        fs::write(&file, "hello world").unwrap();
    }

    let task = service.search_in_dir(
        dir.path().to_path_buf(),
        "hello".to_string(),
        true,
        false,
        tx,
    );

    task.cancel();

    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(GlobalSearchMessage::Cancelled { .. }) => break,
            Ok(GlobalSearchMessage::Complete { .. }) => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}
