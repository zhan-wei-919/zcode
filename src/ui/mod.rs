//! UI layer (deep wrapper over `ratatui`).
//!
//! The goal is to keep all `ratatui` types behind a backend adapter and expose
//! a stable UI runtime (hit-test, drag/drop, overlays) to the app.

pub mod core;

pub mod backend;

pub mod widgets;
