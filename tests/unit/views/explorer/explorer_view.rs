use super::*;

#[test]
fn test_explorer_view_new() {
    let view = ExplorerView::new();
    assert!(view.area.is_none());
}
