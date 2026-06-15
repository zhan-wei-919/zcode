//! 工作台状态分组：把原本平铺在 `Workbench` 上的字段按职责收进子结构，
//! 降低上帝结构体的字段数，并让 render / interaction 各自依赖清晰的状态簇。
//!
//! 字段统一用 `pub(in crate::app::workbench)`，保持与原先"私有字段、由后代模块
//! （render/interaction/...）直接访问"等价的可见性，不对外泄漏。

mod frame_layout;
mod interaction;
mod lsp_sync;
mod render_cache;
mod theme;
mod ui_display;

pub(in crate::app::workbench) use frame_layout::FrameLayout;
pub(in crate::app::workbench) use interaction::InteractionState;
pub(in crate::app::workbench) use lsp_sync::LspSyncState;
pub(in crate::app::workbench) use render_cache::RenderCache;
pub(in crate::app::workbench) use theme::ThemeState;
pub(in crate::app::workbench) use ui_display::UiDisplayState;
