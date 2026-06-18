use crate::core::service::Result as ServiceResult;
use crate::core::{Service, ServiceRegistry};

use super::bus::{kernel_bus, KernelBusReceiver, KernelBusSender, KernelMessage};
use std::sync::mpsc::TryRecvError;

use crate::core::wakeup::WakeupSender;

pub struct KernelServiceHost {
    registry: ServiceRegistry,
    bus: KernelBusSender,
    rx: KernelBusReceiver,
}

#[derive(Clone)]
pub struct KernelServiceContext {
    bus: KernelBusSender,
}

impl KernelServiceHost {
    pub fn new() -> Self {
        let (bus, rx) = kernel_bus();
        Self {
            registry: ServiceRegistry::new(),
            bus,
            rx,
        }
    }

    pub fn context(&self) -> KernelServiceContext {
        KernelServiceContext {
            bus: self.bus.clone(),
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

    pub fn try_recv(&mut self) -> Result<KernelMessage, TryRecvError> {
        self.rx.try_recv()
    }

    pub fn set_wakeup(&mut self, sender: WakeupSender) {
        self.bus.set_wakeup(sender);
    }
}

impl Default for KernelServiceHost {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelServiceContext {
    pub fn dispatch(&self, action: crate::kernel::Action) {
        let _ = self.bus.send_action(action);
    }
}
