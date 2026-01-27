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
#[path = "../../tests/unit/core/context.rs"]
mod tests;
