//! Services layer (ports + adapters).
//!
//! - `ports`: pure contracts/types used across the app (kernel-facing).
//! - `adapters`: OS/runtime specific implementations (IO/async).

pub mod adapters;
pub mod ports;

