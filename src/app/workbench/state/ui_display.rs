//! UI 表现状态：hover/completion 浮窗的渲染快照。纯展示层状态，不含业务决策。

use super::super::{CompletionDocState, HoverPopupRenderState};

#[derive(Debug)]
pub(in crate::app::workbench) struct UiDisplayState {
    pub(in crate::app::workbench) hover_popup: HoverPopupRenderState,
    pub(in crate::app::workbench) completion_doc: CompletionDocState,
}

impl Default for UiDisplayState {
    fn default() -> Self {
        Self::new()
    }
}

impl UiDisplayState {
    pub(in crate::app::workbench) fn new() -> Self {
        Self {
            hover_popup: HoverPopupRenderState::default(),
            completion_doc: CompletionDocState::default(),
        }
    }
}
