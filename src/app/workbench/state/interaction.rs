//! 交互临时状态：分割线拖拽、滚动条拖拽/悬停、终端选区、每 pane 鼠标状态机。
//! 这些都是由输入事件驱动的瞬时状态，与业务状态分开。

use super::super::mouse_tracker::EditorMouseTracker;
use super::super::{EditorScrollbarDragState, TerminalSelection};

#[derive(Debug, Default)]
pub(in crate::app::workbench) struct InteractionState {
    pub(in crate::app::workbench) editor_split_dragging: bool,
    pub(in crate::app::workbench) sidebar_split_dragging: bool,
    pub(in crate::app::workbench) bottom_panel_split_dragging: bool,
    pub(in crate::app::workbench) editor_scrollbar_drag: Option<EditorScrollbarDragState>,
    pub(in crate::app::workbench) editor_scrollbar_hover: Option<usize>,
    pub(in crate::app::workbench) terminal_selection: Option<TerminalSelection>,
    pub(in crate::app::workbench) terminal_selecting: bool,
    pub(in crate::app::workbench) editor_mouse: Vec<EditorMouseTracker>,
}
