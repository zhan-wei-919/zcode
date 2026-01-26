//! Async runtime adapter: executes IO effects and sends messages back to the UI layer.

mod async_runtime;
mod message;

pub use async_runtime::AsyncRuntime;
pub use message::AppMessage;

use crate::kernel::services::ports::{AsyncExecutor, BoxFuture};

impl AsyncExecutor for tokio::runtime::Handle {
    fn spawn(&self, task: BoxFuture) {
        drop(tokio::runtime::Handle::spawn(self, task));
    }
}
