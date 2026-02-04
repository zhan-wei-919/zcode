use super::*;

#[test]
fn inset_shrinks_rect() {
    let r = Rect::new(0, 0, 10, 5);
    assert_eq!(r.inset(Insets::all(1)), Rect::new(1, 1, 8, 3));
}

#[test]
fn inset_saturates_to_empty() {
    let r = Rect::new(0, 0, 2, 2);
    assert_eq!(r.inset(Insets::all(3)), Rect::new(3, 3, 0, 0));
}

#[test]
fn intersect_returns_overlap() {
    let a = Rect::new(0, 0, 5, 5);
    let b = Rect::new(3, 3, 5, 5);
    assert_eq!(a.intersect(b), Rect::new(3, 3, 2, 2));
}

#[test]
fn intersect_disjoint_is_empty() {
    let a = Rect::new(0, 0, 2, 2);
    let b = Rect::new(5, 5, 2, 2);
    assert_eq!(a.intersect(b), Rect::new(5, 5, 0, 0));
    assert!(a.intersect(b).is_empty());
}

#[test]
fn split_top_and_bottom() {
    let r = Rect::new(0, 0, 10, 5);
    let (top, rest) = r.split_top(2);
    assert_eq!(top, Rect::new(0, 0, 10, 2));
    assert_eq!(rest, Rect::new(0, 2, 10, 3));

    let (rest, bottom) = r.split_bottom(2);
    assert_eq!(rest, Rect::new(0, 0, 10, 3));
    assert_eq!(bottom, Rect::new(0, 3, 10, 2));
}

#[test]
fn split_left_and_right() {
    let r = Rect::new(0, 0, 10, 5);
    let (left, rest) = r.split_left(4);
    assert_eq!(left, Rect::new(0, 0, 4, 5));
    assert_eq!(rest, Rect::new(4, 0, 6, 5));

    let (rest, right) = r.split_right(4);
    assert_eq!(rest, Rect::new(0, 0, 6, 5));
    assert_eq!(right, Rect::new(6, 0, 4, 5));
}

#[test]
fn centered_clamps_to_bounds() {
    let r = Rect::new(0, 0, 10, 5);
    assert_eq!(r.centered(4, 1), Rect::new(3, 2, 4, 1));
    assert_eq!(r.centered(100, 100), r);
}
