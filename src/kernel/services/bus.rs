use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

#[derive(Debug)]
pub enum KernelMessage {
    Action(crate::kernel::Action),
}

#[derive(Clone)]
pub struct KernelBusSender {
    tx: Sender<KernelMessage>,
}

pub struct KernelBusReceiver {
    rx: Receiver<KernelMessage>,
}

pub fn kernel_bus() -> (KernelBusSender, KernelBusReceiver) {
    let (tx, rx) = mpsc::channel();
    (
        KernelBusSender { tx },
        KernelBusReceiver { rx },
    )
}

impl KernelBusSender {
    pub fn send(&self, msg: KernelMessage) -> Result<(), mpsc::SendError<KernelMessage>> {
        self.tx.send(msg)
    }

    pub fn send_action(
        &self,
        action: crate::kernel::Action,
    ) -> Result<(), mpsc::SendError<KernelMessage>> {
        self.send(KernelMessage::Action(action))
    }
}

impl KernelBusReceiver {
    pub fn try_recv(&mut self) -> Result<KernelMessage, TryRecvError> {
        self.rx.try_recv()
    }
}
