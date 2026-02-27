use crate::ui::core::input::DragPayload;
use crate::ui::core::runtime::DragDropRules;
use crate::ui::core::tree::{Node, NodeKind, SplitDrop};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DropIntent {
    TabToTabBar { to_pane: usize },
    TabToSplit { drop: SplitDrop },
    ExplorerToEditorArea { pane: usize },
    ExplorerToExplorerRow { to_row_id: u64 },
    ExplorerToExplorerFolder { to_dir_id: u64 },
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct WorkbenchDragDropRules;

pub(super) const WORKBENCH_DND_RULES: WorkbenchDragDropRules = WorkbenchDragDropRules;

pub(super) fn drag_payload_for_source(kind: NodeKind) -> Option<DragPayload> {
    match kind {
        NodeKind::Tab { pane, tab_id } => Some(DragPayload::Tab {
            from_pane: pane,
            tab_id,
        }),
        NodeKind::ExplorerRow { node_id } => Some(DragPayload::ExplorerNode { node_id }),
        _ => None,
    }
}

pub(super) fn drop_intent(payload: &DragPayload, target_kind: NodeKind) -> Option<DropIntent> {
    match (payload, target_kind) {
        (DragPayload::Tab { .. }, NodeKind::TabBar { pane }) => {
            Some(DropIntent::TabToTabBar { to_pane: pane })
        }
        (DragPayload::Tab { .. }, NodeKind::EditorSplitDrop { drop, .. }) => {
            Some(DropIntent::TabToSplit { drop })
        }
        (DragPayload::ExplorerNode { .. }, NodeKind::EditorArea { pane }) => {
            Some(DropIntent::ExplorerToEditorArea { pane })
        }
        (DragPayload::ExplorerNode { .. }, NodeKind::ExplorerRow { node_id }) => {
            Some(DropIntent::ExplorerToExplorerRow { to_row_id: node_id })
        }
        (DragPayload::ExplorerNode { .. }, NodeKind::ExplorerFolderDrop { node_id }) => {
            Some(DropIntent::ExplorerToExplorerFolder { to_dir_id: node_id })
        }
        _ => None,
    }
}

impl DragDropRules for WorkbenchDragDropRules {
    fn payload_for_source(&self, source: &Node) -> Option<DragPayload> {
        drag_payload_for_source(source.kind)
    }

    fn can_drop(&self, payload: &DragPayload, target: &Node) -> bool {
        drop_intent(payload, target.kind).is_some()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app/workbench/dnd_rules.rs"]
mod tests;
