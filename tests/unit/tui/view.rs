use super::*;

#[test]
fn test_event_result() {
    assert!(EventResult::Consumed.is_consumed());
    assert!(EventResult::Ignored.is_ignored());
    assert!(EventResult::Quit.is_quit());
}

#[test]
fn test_active_area_default() {
    let area = ActiveArea::default();
    assert_eq!(area, ActiveArea::Editor);
}
