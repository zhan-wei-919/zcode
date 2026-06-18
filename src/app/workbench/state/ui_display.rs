//! UI 表现状态：hover/completion 浮窗的渲染快照、终端光标闪烁、点击节流。
//! 纯展示层状态，不含业务决策。

use super::super::{CompletionDocState, HoverPopupRenderState};
use std::time::Instant;

#[derive(Debug)]
pub(in crate::app::workbench) struct UiDisplayState {
    pub(in crate::app::workbench) hover_popup: HoverPopupRenderState,
    pub(in crate::app::workbench) completion_doc: CompletionDocState,
}

impl UiDisplayState {
    pub(in crate::app::workbench) fn new(_now: Instant) -> Self {
        Self {
            hover_popup: HoverPopupRenderState::default(),
            completion_doc: CompletionDocState::default(),
        }
    }
}
