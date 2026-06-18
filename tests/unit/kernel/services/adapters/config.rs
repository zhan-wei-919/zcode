use super::*;

#[test]
fn test_service_trait() {
    let service = ConfigService::new();
    assert_eq!(service.name(), "ConfigService");
}
