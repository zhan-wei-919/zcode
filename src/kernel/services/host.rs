use std::future::Future;
use std::sync::Arc;

use crate::core::service::Result as ServiceResult;
use crate::core::{Service, ServiceRegistry};

use super::bus::{kernel_bus, KernelBusReceiver, KernelBusSender, KernelMessage};
use super::ports::{AsyncExecutor, BoxFuture};
use std::sync::mpsc::TryRecvError;

use crate::tui::wakeup::WakeupSender;

pub struct KernelServiceHost {
    registry: ServiceRegistry,
    bus: KernelBusSender,
    rx: KernelBusReceiver,
    executor: Arc<dyn AsyncExecutor>,
}

#[derive(Clone)]
pub struct KernelServiceContext {
    bus: KernelBusSender,
    executor: Arc<dyn AsyncExecutor>,
}

impl KernelServiceHost {
    pub fn new(executor: Arc<dyn AsyncExecutor>) -> Self {
        let (bus, rx) = kernel_bus();
        Self {
            registry: ServiceRegistry::new(),
            bus,
            rx,
            executor,
        }
    }

    pub fn context(&self) -> KernelServiceContext {
        KernelServiceContext {
            bus: self.bus.clone(),
            executor: Arc::clone(&self.executor),
        }
    }

    pub fn register<S: Service + 'static>(&mut self, service: S) -> ServiceResult<()> {
        self.registry.register(service)
    }

    pub fn get<S: Service + 'static>(&self) -> Option<&S> {
        self.registry.get::<S>()
    }

    pub fn get_mut<S: Service + 'static>(&mut self) -> Option<&mut S> {
        self.registry.get_mut::<S>()
    }

    pub fn services(&self) -> &ServiceRegistry {
        &self.registry
    }

    pub fn services_mut(&mut self) -> &mut ServiceRegistry {
        &mut self.registry
    }

    pub fn try_recv(&mut self) -> Result<KernelMessage, TryRecvError> {
        self.rx.try_recv()
    }

    pub fn set_wakeup(&mut self, sender: WakeupSender) {
        self.bus.set_wakeup(sender);
    }
}

impl KernelServiceContext {
    pub fn dispatch(&self, action: crate::kernel::Action) {
        let _ = self.bus.send_action(action);
    }

    pub fn spawn(&self, task: BoxFuture) {
        self.executor.spawn(task);
    }

    pub fn spawn_future<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.executor.spawn(Box::pin(task));
    }
}
