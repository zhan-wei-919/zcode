use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Instant;

use crate::tui::wakeup::WakeupSender;

pub struct KernelMessage {
    pub payload: KernelMessagePayload,
    pub enqueued_at: Instant,
}

#[derive(Debug)]
pub enum KernelMessagePayload {
    Action(crate::kernel::Action),
}

#[derive(Clone)]
pub struct KernelBusSender {
    tx: Sender<KernelMessage>,
    wakeup: Option<WakeupSender>,
}

pub struct KernelBusReceiver {
    rx: Receiver<KernelMessage>,
}

pub fn kernel_bus() -> (KernelBusSender, KernelBusReceiver) {
    let (tx, rx) = mpsc::channel();
    (
        KernelBusSender { tx, wakeup: None },
        KernelBusReceiver { rx },
    )
}

impl KernelBusSender {
    pub fn set_wakeup(&mut self, sender: WakeupSender) {
        self.wakeup = Some(sender);
    }

    pub fn send(&self, msg: KernelMessage) -> Result<(), mpsc::SendError<()>> {
        self.tx.send(msg).map_err(|_| mpsc::SendError(()))?;
        if let Some(w) = &self.wakeup {
            w.wake();
        }
        Ok(())
    }

    pub fn send_action(&self, action: crate::kernel::Action) -> Result<(), mpsc::SendError<()>> {
        self.send(KernelMessage {
            payload: KernelMessagePayload::Action(action),
            enqueued_at: Instant::now(),
        })
    }
}

impl KernelBusReceiver {
    pub fn try_recv(&mut self) -> Result<KernelMessage, TryRecvError> {
        self.rx.try_recv()
    }
}
