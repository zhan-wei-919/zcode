use super::*;

#[test]
fn compute_split_ratio_vertical_clamps_edges() {
    let area = crate::ui::core::geom::Rect::new(10, 5, 11, 10); // total = 10

    // Left edge would be 0 -> clamped to 1.
    assert_eq!(
        compute_split_ratio(SplitDirection::Vertical, area, 10, 5),
        Some(100)
    );
    // Right edge would be 10 -> clamped to 9.
    assert_eq!(
        compute_split_ratio(SplitDirection::Vertical, area, 20, 5),
        Some(900)
    );
}

#[test]
fn compute_split_ratio_vertical_returns_none_for_too_small_width() {
    let area = crate::ui::core::geom::Rect::new(0, 0, 2, 10);
    assert_eq!(
        compute_split_ratio(SplitDirection::Vertical, area, 0, 0),
        None
    );
}

#[test]
fn compute_split_ratio_horizontal_clamps_edges() {
    let area = crate::ui::core::geom::Rect::new(10, 5, 10, 11); // total = 10

    // Top edge would be 0 -> clamped to 1.
    assert_eq!(
        compute_split_ratio(SplitDirection::Horizontal, area, 10, 5),
        Some(100)
    );
    // Bottom edge would be 10 -> clamped to 9.
    assert_eq!(
        compute_split_ratio(SplitDirection::Horizontal, area, 10, 15),
        Some(900)
    );
}

#[test]
fn compute_split_ratio_horizontal_returns_none_for_too_small_height() {
    let area = crate::ui::core::geom::Rect::new(0, 0, 10, 2);
    assert_eq!(
        compute_split_ratio(SplitDirection::Horizontal, area, 0, 0),
        None
    );
}
