//! zcode - TUI 文本编辑器库

pub mod core;
pub mod kernel;
pub mod models;

#[cfg(feature = "tui")]
pub mod app;
#[cfg(feature = "tui")]
pub mod tui;
#[cfg(feature = "tui")]
pub mod ui;
#[cfg(feature = "tui")]
pub mod views;
