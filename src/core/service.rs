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
        if self.type_id() == TypeId::of::<T>() {
            // 数据地址永远放在胖指针的前8个字节 -> [数据地址 | vtable 地址]
            // self是 dyn Service
            // self as *const dyn Service 将胖指针转换为裸指针：本质只是编译器的视角变了，但数据地址并没有变  
            // as *const T 将裸指针转换为 T 指针，只取前8个字节，直接放弃后面的vtable 地址
            unsafe { Some(&*(self as *const dyn Service as *const T)) }
        } else {
            None
        }
    }

    #[inline]
    pub fn downcast_mut<T: Service>(&mut self) -> Option<&mut T> {
        if (*self).type_id() == TypeId::of::<T>() {
            //阶段,表达式类型,内存宽度,含义
            //开始,&mut dyn Service,16 字节,安全的胖引用
            //中转,*mut dyn Service,16 字节,原始胖指针（解开枷锁）
            //关键,*mut T,8 字节,原始瘦指针（丢弃虚表）
            //结束,&mut T,8 字节,重生后的安全瘦引用
            unsafe { Some(&mut *(self as *mut dyn Service as *mut T)) }
        } else {
            None
        }
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
mod tests {
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
}
