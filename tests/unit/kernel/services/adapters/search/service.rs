use super::*;
use std::sync::mpsc;

fn create_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn test_search_sync_literal() {
    let rope = Rope::from_str("hello world hello");
    let matches = SearchService::search_sync(&rope, "hello", true, false).unwrap();
    assert_eq!(matches.len(), 2);
}

#[test]
fn test_search_sync_regex() {
    let rope = Rope::from_str("hello123 world456");
    let matches = SearchService::search_sync(&rope, r"\w+\d+", true, true).unwrap();
    assert_eq!(matches.len(), 2);
}

#[test]
fn test_find_next() {
    let rope = Rope::from_str("hello world hello");
    let m = SearchService::find_next(&rope, "hello", 0, true, false).unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().start, 0);

    let m = SearchService::find_next(&rope, "hello", 1, true, false).unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().start, 12);
}

#[test]
fn test_find_prev() {
    let rope = Rope::from_str("hello world hello");
    let m = SearchService::find_prev(&rope, "hello", 17, true, false).unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().start, 12);

    let m = SearchService::find_prev(&rope, "hello", 12, true, false).unwrap();
    assert!(m.is_some());
    assert_eq!(m.unwrap().start, 0);
}

#[test]
fn test_async_search() {
    let rt = create_runtime();
    let service = SearchService::new(rt.handle().clone());
    let (tx, rx) = mpsc::sync_channel(64);

    let rope = Rope::from_str("hello world hello");
    let _task = service.search_in_rope(rope, "hello".to_string(), true, false, tx);

    let mut total_matches = 0;
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(SearchMessage::Matches { matches, .. }) => {
                total_matches += matches.len();
            }
            Ok(SearchMessage::Complete { total, .. }) => {
                assert_eq!(total, 2);
                assert_eq!(total_matches, 2);
                break;
            }
            Ok(SearchMessage::Cancelled { .. }) => {
                panic!("Search was cancelled unexpectedly");
            }
            Ok(SearchMessage::Error { message, .. }) => {
                panic!("Error: {}", message);
            }
            Err(_) => {
                panic!("Timeout waiting for search results");
            }
        }
    }
}

#[test]
fn test_cancel_search() {
    let rt = create_runtime();
    let service = SearchService::new(rt.handle().clone());
    let (tx, rx) = mpsc::sync_channel(64);

    let text = "hello ".repeat(10000);
    let rope = Rope::from_str(&text);

    let task = service.search_in_rope(rope, "hello".to_string(), true, false, tx);
    task.cancel();

    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(SearchMessage::Cancelled { .. }) => break,
            Ok(SearchMessage::Complete { .. }) => break,
            Ok(SearchMessage::Matches { .. }) => continue,
            Ok(SearchMessage::Error { .. }) => break,
            Err(_) => break,
        }
    }
}

#[test]
fn test_invalid_regex() {
    let rope = Rope::from_str("hello world");
    let result = SearchService::search_sync(&rope, "[invalid", true, true);
    assert!(result.is_err());
}
