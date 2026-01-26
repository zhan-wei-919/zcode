//! TUI integration layer (crossterm + ratatui).
//!
//! This module is intentionally separate from `kernel`/`models` so the core can be reused by
//! other frontends (e.g. Web) without depending on terminal crates.

pub mod crossterm;
pub mod view;
