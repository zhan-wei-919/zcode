use super::*;

struct DummyService {
    value: i32,
}

impl Service for DummyService {
    fn name(&self) -> &'static str {
        "DummyService"
    }
}

#[test]
fn test_context_register_and_get() {
    let mut ctx = AppContext::new();
    ctx.register(DummyService { value: 100 }).unwrap();

    let service = ctx.get::<DummyService>().unwrap();
    assert_eq!(service.value, 100);
}

#[test]
fn test_context_get_mut() {
    let mut ctx = AppContext::new();
    ctx.register(DummyService { value: 1 }).unwrap();

    {
        let service = ctx.get_mut::<DummyService>().unwrap();
        service.value = 2;
    }

    assert_eq!(ctx.get::<DummyService>().unwrap().value, 2);
}
