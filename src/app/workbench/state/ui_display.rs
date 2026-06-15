//! UI 表现状态：hover/completion 浮窗的渲染快照、终端光标闪烁、点击节流。
//! 纯展示层状态，不含业务决策。

use super::super::{ClickTracker, CompletionDocState, HoverPopupRenderState};
use std::time::Instant;

#[derive(Debug)]
pub(in crate::app::workbench) struct UiDisplayState {
    pub(in crate::app::workbench) hover_popup: HoverPopupRenderState,
    pub(in crate::app::workbench) completion_doc: CompletionDocState,
    pub(in crate::app::workbench) terminal_cursor_visible: bool,
    pub(in crate::app::workbench) terminal_cursor_last_blink: Instant,
    pub(in crate::app::workbench) click_tracker: ClickTracker,
}

impl UiDisplayState {
    pub(in crate::app::workbench) fn new(now: Instant) -> Self {
        Self {
            hover_popup: HoverPopupRenderState::default(),
            completion_doc: CompletionDocState::default(),
            terminal_cursor_visible: true,
            terminal_cursor_last_blink: now,
            click_tracker: ClickTracker::default(),
        }
    }
}
