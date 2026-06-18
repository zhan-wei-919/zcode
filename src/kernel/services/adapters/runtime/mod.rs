//! Async runtime adapter: executes IO effects and sends messages back to the UI layer.

mod async_runtime;
mod message;

pub use async_runtime::AsyncRuntime;
pub use message::AppMessage;
