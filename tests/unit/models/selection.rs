use super::*;

#[test]
fn test_word_bounds() {
    let line = "hello_world foo bar";

    assert_eq!(Selection::word_bounds_at(line, 5), (0, 11));
    assert_eq!(Selection::word_bounds_at(line, 11), (11, 12));
    assert_eq!(Selection::word_bounds_at(line, 12), (12, 15));
}

#[test]
fn test_word_bounds_unicode() {
    let line = "let 变量名 = value;";
    assert_eq!(Selection::word_bounds_at(line, 4), (4, 7));

    let line = "café";
    assert_eq!(Selection::word_bounds_at(line, 0), (0, 4));

    let line = "a+b-c";
    assert_eq!(Selection::word_bounds_at(line, 0), (0, 1));
    assert_eq!(Selection::word_bounds_at(line, 1), (1, 2));
    assert_eq!(Selection::word_bounds_at(line, 2), (2, 3));
}

#[test]
fn test_selection_range() {
    let sel = Selection::new((1, 5), Granularity::Char);
    assert_eq!(sel.range(), ((1, 5), (1, 5)));

    let mut sel = Selection::new((2, 3), Granularity::Char);
    sel.cursor = (5, 7);
    assert_eq!(sel.range(), ((2, 3), (5, 7)));

    let mut sel = Selection::new((5, 7), Granularity::Char);
    sel.cursor = (2, 3);
    assert_eq!(sel.range(), ((2, 3), (5, 7)));
}

#[test]
fn test_word_selection_out_of_bounds_cursor() {
    let rope = Rope::from_str("hello");
    let mut sel = Selection::new((0, 0), Granularity::Word);
    sel.update_cursor((0, 100), &rope);
    assert_eq!(sel.cursor(), (0, 5));
}

#[test]
fn test_word_selection_from_pos_expands_to_whole_word() {
    let line = "zcode::kernel::services::adapters::ensure";
    let rope = Rope::from_str(&format!("{line}\n"));
    let sel = Selection::from_pos((0, 29), Granularity::Word, &rope);
    let start = line.find("adapters").expect("line should contain adapters");
    let end = start + "adapters".len();

    assert_eq!(sel.range(), ((0, start), (0, end)));
}

#[test]
fn test_line_selection_from_pos_expands_to_whole_line() {
    let rope = Rope::from_str("hello world\nnext\n");
    let sel = Selection::from_pos((0, 4), Granularity::Line, &rope);

    assert_eq!(sel.range(), ((0, 0), (0, 11)));
}
