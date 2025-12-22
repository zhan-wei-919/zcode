//! 应用上下文：服务访问入口
//!
//! 提供对 ServiceRegistry 的访问，作为依赖注入的容器

use super::service::{Service, ServiceRegistry};

pub struct AppContext {
    services: ServiceRegistry,
}

impl AppContext {
    pub fn new() -> Self {
        Self {
            services: ServiceRegistry::new(),
        }
    }

    pub fn with_services(services: ServiceRegistry) -> Self {
        Self { services }
    }

    pub fn register<S: Service + 'static>(&mut self, service: S) -> super::service::Result<()> {
        self.services.register(service)
    }

    pub fn get<S: Service + 'static>(&self) -> Option<&S> {
        self.services.get::<S>()
    }

    pub fn get_mut<S: Service + 'static>(&mut self) -> Option<&mut S> {
        self.services.get_mut::<S>()
    }

    pub fn services(&self) -> &ServiceRegistry {
        &self.services
    }

    pub fn services_mut(&mut self) -> &mut ServiceRegistry {
        &mut self.services
    }
}

impl Default for AppContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
}
