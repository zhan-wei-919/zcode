use crate::kernel::editor::{EditorTabState, TabId};
use crate::kernel::services::ports::EditorConfig;
use std::path::PathBuf;

fn tab(content: &str) -> EditorTabState {
    let config = EditorConfig::default();
    EditorTabState::from_file(TabId::new(1), PathBuf::from("test.rs"), content, &config)
}

#[test]
fn matches_open_to_close_forward() {
    let mut t = tab("foo(bar)");
    t.buffer.set_cursor(0, 3); // 光标在 '('
    assert_eq!(t.matching_bracket(), Some([(0, 3), (0, 7)]));
}

#[test]
fn matches_close_to_open_backward() {
    let mut t = tab("foo(bar)");
    t.buffer.set_cursor(0, 7); // 光标在 ')'
    assert_eq!(t.matching_bracket(), Some([(0, 7), (0, 3)]));
}

#[test]
fn matches_respect_nesting() {
    let mut t = tab("((x))");
    t.buffer.set_cursor(0, 0); // 外层 '('
    assert_eq!(t.matching_bracket(), Some([(0, 0), (0, 4)]));
    t.buffer.set_cursor(0, 1); // 内层 '('
    assert_eq!(t.matching_bracket(), Some([(0, 1), (0, 3)]));
}

#[test]
fn matches_left_neighbor_when_cursor_past_bracket() {
    let mut t = tab("foo(bar)");
    t.buffer.set_cursor(0, 8); // 行尾，左邻是刚敲完的 ')'
    assert_eq!(t.matching_bracket(), Some([(0, 7), (0, 3)]));
}

#[test]
fn no_match_when_not_on_bracket() {
    let mut t = tab("foo(bar)");
    t.buffer.set_cursor(0, 5); // 在 'a' 上
    assert_eq!(t.matching_bracket(), None);
}

#[test]
fn no_match_when_unbalanced() {
    let mut t = tab("foo(bar");
    t.buffer.set_cursor(0, 3); // '(' 没有配对
    assert_eq!(t.matching_bracket(), None);
}

#[test]
fn matches_across_lines() {
    let mut t = tab("fn f() {\n    x\n}");
    t.buffer.set_cursor(0, 7); // 第一行的 '{'
    assert_eq!(t.matching_bracket(), Some([(0, 7), (2, 0)]));
}
