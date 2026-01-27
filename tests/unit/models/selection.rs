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
