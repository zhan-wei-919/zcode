//! Services layer (ports + adapters).
//!
//! - `ports`: pure contracts/types used across the app (kernel-facing).
//! - `adapters`: OS/runtime specific implementations (IO/async).

#[cfg(feature = "tui")]
pub mod adapters;
pub mod bus;
pub mod host;
pub mod ports;

pub use bus::{kernel_bus, KernelBusReceiver, KernelBusSender, KernelMessage};
pub use host::{KernelServiceContext, KernelServiceHost};
