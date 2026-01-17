//! Async runtime adapter: executes IO effects and sends messages back to the UI layer.

mod message;
mod runtime;

pub use message::AppMessage;
pub use runtime::AsyncRuntime;

use crate::kernel::services::ports::{AsyncExecutor, BoxFuture};

impl AsyncExecutor for tokio::runtime::Handle {
    fn spawn(&self, task: BoxFuture) {
        let _ = tokio::runtime::Handle::spawn(self, task);
    }
}
