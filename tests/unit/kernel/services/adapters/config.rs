use super::*;

#[test]
fn test_config_service() {
    let mut service = ConfigService::new();
    assert_eq!(service.editor().tab_size, 4);

    service.set_tab_size(2);
    assert_eq!(service.editor().tab_size, 2);
}

#[test]
fn test_service_trait() {
    let service = ConfigService::new();
    assert_eq!(service.name(), "ConfigService");
}
