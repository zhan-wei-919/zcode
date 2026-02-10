use super::geom::Pos;
use super::id::Id;
use crate::core::event::MouseButton;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DragPayload {
    Tab { from_pane: usize, tab_id: u64 },
    ExplorerNode { node_id: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    HoverChanged {
        from: Option<Id>,
        to: Option<Id>,
        pos: Pos,
    },
    Click {
        id: Id,
        button: MouseButton,
        pos: Pos,
    },
    ContextMenu {
        id: Id,
        pos: Pos,
    },
    DragStart {
        id: Id,
        pos: Pos,
    },
    DragMove {
        id: Id,
        pos: Pos,
        delta: (i16, i16),
    },
    DragEnd {
        id: Id,
        pos: Pos,
    },
    Drop {
        payload: DragPayload,
        target: Id,
        pos: Pos,
    },
}
