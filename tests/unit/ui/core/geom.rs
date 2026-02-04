use super::*;

#[test]
fn rect_contains_is_inclusive_exclusive() {
    let r = Rect::new(10, 20, 3, 2); // x:10..13, y:20..22
    assert!(r.contains(Pos::new(10, 20)));
    assert!(r.contains(Pos::new(12, 21)));

    // Right/bottom edges are exclusive.
    assert!(!r.contains(Pos::new(13, 20)));
    assert!(!r.contains(Pos::new(12, 22)));

    // Outside.
    assert!(!r.contains(Pos::new(9, 20)));
    assert!(!r.contains(Pos::new(10, 19)));
}

#[test]
fn rect_empty_never_contains() {
    let r = Rect::new(0, 0, 0, 10);
    assert!(!r.contains(Pos::new(0, 0)));
    let r = Rect::new(0, 0, 10, 0);
    assert!(!r.contains(Pos::new(0, 0)));
}
