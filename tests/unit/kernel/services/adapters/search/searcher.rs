use super::*;
use std::io::Cursor;

#[test]
fn test_literal_search() {
    let data = b"hello world hello";
    let config = SearchConfig::literal("hello", true);
    let reader = Cursor::new(data);
    let matches = StreamSearcher::new(reader, &config).search().unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[1].start, 12);
}

#[test]
fn test_case_insensitive() {
    let data = b"Hello HELLO hello";
    let config = SearchConfig::literal("hello", false);
    let reader = Cursor::new(data);
    let matches = StreamSearcher::new(reader, &config).search().unwrap();

    assert_eq!(matches.len(), 3);
}

#[test]
fn test_line_numbers() {
    let data = b"line1\nline2 hello\nline3";
    let config = SearchConfig::literal("hello", true);
    let reader = Cursor::new(data);
    let matches = StreamSearcher::new(reader, &config).search().unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 1);
    assert_eq!(matches[0].col, 6);
}

#[test]
fn test_regex_search() {
    let data = b"hello123 world456";
    let config = SearchConfig::regex(r"\w+\d+", true).unwrap();
    let SearchConfig::Regex { regex } = &config else {
        panic!("expected Regex config");
    };
    let matches = search_regex_in_slice(data, regex);

    assert_eq!(matches.len(), 2);
}

#[test]
fn test_rope_reader() {
    let rope = ropey::Rope::from_str("hello world hello");
    let config = SearchConfig::literal("hello", true);
    let reader = RopeReader::new(&rope);
    let matches = StreamSearcher::new(reader, &config).search().unwrap();

    assert_eq!(matches.len(), 2);
}

#[test]
fn test_empty_pattern() {
    let data = b"hello";
    let config = SearchConfig::literal("", true);
    let reader = Cursor::new(data);
    let matches = StreamSearcher::new(reader, &config).search().unwrap();

    assert!(matches.is_empty());
}

#[test]
fn test_long_pattern_over_buffer_size() {
    let pattern = "a".repeat(BUFFER_SIZE + 10);
    let config = SearchConfig::literal(&pattern, true);
    let reader = Cursor::new(pattern.into_bytes());
    let matches = StreamSearcher::new(reader, &config).search().unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[0].end, BUFFER_SIZE + 10);
}
