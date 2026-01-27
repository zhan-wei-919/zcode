use std::any::{Any, TypeId};
use std::collections::HashMap;

pub type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Debug)]
pub enum ServiceError {
    NotFound(String),
    InitializationFailed(String),
    AlreadyRegistered(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::NotFound(name) => write!(f, "Service not found: {}", name),
            ServiceError::InitializationFailed(msg) => {
                write!(f, "Service initialization failed: {}", msg)
            }
            ServiceError::AlreadyRegistered(name) => {
                write!(f, "Service already registered: {}", name)
            }
        }
    }
}

impl std::error::Error for ServiceError {}

pub trait Service: Any {
    fn name(&self) -> &'static str;
}

impl dyn Service {
    #[inline]
    pub fn downcast_ref<T: Service>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }

    #[inline]
    pub fn downcast_mut<T: Service>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }
}

pub struct ServiceRegistry {
    services: HashMap<TypeId, Box<dyn Service>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    pub fn register<S: Service + 'static>(&mut self, service: S) -> Result<()> {
        let type_id = TypeId::of::<S>();
        let name = service.name();
        if self.services.contains_key(&type_id) {
            return Err(ServiceError::AlreadyRegistered(name.to_string()));
        }
        self.services.insert(type_id, Box::new(service));
        Ok(())
    }

    pub fn get<S: Service + 'static>(&self) -> Option<&S> {
        self.services
            .get(&TypeId::of::<S>())
            .and_then(|s| s.downcast_ref::<S>())
    }

    pub fn get_mut<S: Service + 'static>(&mut self) -> Option<&mut S> {
        self.services
            .get_mut(&TypeId::of::<S>())
            .and_then(|s| s.downcast_mut::<S>())
    }

    pub fn contains<S: Service + 'static>(&self) -> bool {
        self.services.contains_key(&TypeId::of::<S>())
    }

    pub fn remove<S: Service + 'static>(&mut self) -> Option<Box<dyn Service>> {
        self.services.remove(&TypeId::of::<S>())
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/core/service.rs"]
mod tests;
