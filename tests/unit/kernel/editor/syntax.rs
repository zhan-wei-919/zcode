use super::*;
use ropey::Rope;
use std::path::Path;

#[test]
fn test_highlight_comment_range() {
    let rope = Rope::from_str("fn main() { // hi\n}\n");
    let doc = SyntaxDocument::for_path(Path::new("test.rs"), &rope).expect("rust syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    assert_eq!(spans.len(), 1);

    let line = "fn main() { // hi";
    let idx = line.find("//").unwrap();

    assert!(spans[0]
        .iter()
        .any(|s| { s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end }));
}
