use super::*;
use ropey::Rope;
use std::path::Path;

#[test]
fn test_highlight_comment_range_rust() {
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

#[test]
fn test_highlight_go_comment_string_keyword_and_in_string_or_comment() {
    let src = "package main\n// hi\nfunc main() { println(\"x\") }\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.go"), &rope).expect("go syntax");

    let spans = doc.highlight_lines(&rope, 1, 2);
    let line = "// hi";
    let idx = line.find("//").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end));

    let spans = doc.highlight_lines(&rope, 2, 3);
    let line = "func main() { println(\"x\") }";
    let idx_func = line.find("func").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_func && idx_func < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));

    let in_comment = src.find("// hi").unwrap() + 3;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("package").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_python_comment_string_keyword_and_in_string_or_comment() {
    let src = "# hi\ndef f():\n    return \"x\"\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.py"), &rope).expect("python syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "# hi";
    let idx = line.find('#').unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Comment && s.start <= idx && idx < s.end));

    let spans = doc.highlight_lines(&rope, 1, 2);
    let line = "def f():";
    let idx_def = line.find("def").unwrap();
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_def && idx_def < s.end));

    let spans = doc.highlight_lines(&rope, 2, 3);
    let line = "    return \"x\"";
    let idx_return = line.find("return").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_return && idx_return < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));

    let in_comment = src.find("# hi").unwrap() + 2;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("def").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_javascript_comment_string_keyword_and_in_string_or_comment() {
    let src = "function f() { return \"x\" } // hi\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.js"), &rope).expect("js syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "function f() { return \"x\" } // hi";
    let idx_func = line.find("function").unwrap();
    let idx_return = line.find("return").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;
    let idx_comment = line.find("//").unwrap();

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_func && idx_func < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_return && idx_return < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));
    assert!(spans[0].iter().any(|s| s.kind == HighlightKind::Comment
        && s.start <= idx_comment
        && idx_comment < s.end));

    let in_comment = src.find("// hi").unwrap() + 3;
    assert!(doc.is_in_string_or_comment(in_comment));
    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("function").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}

#[test]
fn test_highlight_tsx_string_and_in_string_or_comment() {
    let src = "const x = <div>{\"x\"}</div>\n";
    let rope = Rope::from_str(src);
    let doc = SyntaxDocument::for_path(Path::new("test.tsx"), &rope).expect("tsx syntax");

    let spans = doc.highlight_lines(&rope, 0, 1);
    let line = "const x = <div>{\"x\"}</div>";
    let idx_const = line.find("const").unwrap();
    let idx_str = line.find("\"x\"").unwrap() + 1;

    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::Keyword && s.start <= idx_const && idx_const < s.end));
    assert!(spans[0]
        .iter()
        .any(|s| s.kind == HighlightKind::String && s.start <= idx_str && idx_str < s.end));

    let in_string = src.find("\"x\"").unwrap() + 1;
    assert!(doc.is_in_string_or_comment(in_string));
    let in_code = src.find("const").unwrap();
    assert!(!doc.is_in_string_or_comment(in_code));
}
