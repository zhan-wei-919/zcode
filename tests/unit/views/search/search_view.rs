use super::*;
use crate::core::event::{KeyModifiers, MouseButton, MouseEventKind};

#[test]
fn test_hit_test_results_row() {
    let mut view = SearchView::new();
    view.results_area = Some(Rect::new(0, 2, 10, 3));

    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 3,
        modifiers: KeyModifiers::NONE,
    };

    assert_eq!(view.hit_test_results_row(&ev, 0), Some(1));
    assert_eq!(view.hit_test_results_row(&ev, 2), Some(3));
}
