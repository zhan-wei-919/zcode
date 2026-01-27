use super::*;

#[test]
fn test_clipboard_service_creation() {
    let service = ClipboardService::new();
    assert_eq!(service.name(), "ClipboardService");
}
