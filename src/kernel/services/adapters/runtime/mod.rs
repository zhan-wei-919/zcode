//! Async runtime adapter: executes IO effects and sends messages back to the UI layer.

mod message;
mod runtime;

pub use message::AppMessage;
pub use runtime::AsyncRuntime;
