use super::*;

struct TestService {
    value: i32,
}

impl Service for TestService {
    fn name(&self) -> &'static str {
        "TestService"
    }
}

struct AnotherService {
    data: String,
}

impl Service for AnotherService {
    fn name(&self) -> &'static str {
        "AnotherService"
    }
}

#[test]
fn test_register_and_get() {
    let mut registry = ServiceRegistry::new();
    let service = TestService { value: 42 };

    registry.register(service).unwrap();

    let retrieved = registry.get::<TestService>().unwrap();
    assert_eq!(retrieved.value, 42);
}

#[test]
fn test_get_mut() {
    let mut registry = ServiceRegistry::new();
    registry.register(TestService { value: 10 }).unwrap();

    {
        let service = registry.get_mut::<TestService>().unwrap();
        service.value = 20;
    }

    let service = registry.get::<TestService>().unwrap();
    assert_eq!(service.value, 20);
}

#[test]
fn test_multiple_services() {
    let mut registry = ServiceRegistry::new();
    registry.register(TestService { value: 1 }).unwrap();
    registry
        .register(AnotherService {
            data: "hello".to_string(),
        })
        .unwrap();

    assert_eq!(registry.get::<TestService>().unwrap().value, 1);
    assert_eq!(registry.get::<AnotherService>().unwrap().data, "hello");
}

#[test]
fn test_duplicate_registration() {
    let mut registry = ServiceRegistry::new();
    registry.register(TestService { value: 1 }).unwrap();

    let result = registry.register(TestService { value: 2 });
    assert!(matches!(result, Err(ServiceError::AlreadyRegistered(_))));
}

#[test]
fn test_get_nonexistent() {
    let registry = ServiceRegistry::new();
    assert!(registry.get::<TestService>().is_none());
}

#[test]
fn test_contains() {
    let mut registry = ServiceRegistry::new();
    assert!(!registry.contains::<TestService>());

    registry.register(TestService { value: 1 }).unwrap();
    assert!(registry.contains::<TestService>());
}

#[test]
fn test_remove() {
    let mut registry = ServiceRegistry::new();
    registry.register(TestService { value: 1 }).unwrap();

    assert!(registry.contains::<TestService>());
    registry.remove::<TestService>();
    assert!(!registry.contains::<TestService>());
}
