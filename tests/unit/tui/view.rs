use super::*;

#[test]
fn test_event_result() {
    assert!(EventResult::Consumed.is_consumed());
    assert!(!EventResult::Ignored.is_consumed());
}
