use super::*;
use crate::ui::core::id::Id;
use crate::ui::core::tree::{Node, Sense};

fn node(kind: crate::ui::core::tree::NodeKind) -> Node {
    Node {
        id: Id::raw(1),
        rect: crate::ui::core::geom::Rect::new(0, 0, 1, 1),
        layer: 0,
        z: 0,
        sense: Sense::NONE,
        kind,
    }
}

#[test]
fn payload_for_source_maps_tab_and_explorer_row() {
    assert_eq!(
        drag_payload_for_source(crate::ui::core::tree::NodeKind::Tab {
            pane: 2,
            tab_id: 99,
        }),
        Some(crate::ui::core::input::DragPayload::Tab {
            from_pane: 2,
            tab_id: 99,
        })
    );

    assert_eq!(
        drag_payload_for_source(crate::ui::core::tree::NodeKind::ExplorerRow { node_id: 42 }),
        Some(crate::ui::core::input::DragPayload::ExplorerNode { node_id: 42 })
    );

    assert_eq!(
        drag_payload_for_source(crate::ui::core::tree::NodeKind::EditorArea { pane: 0 }),
        None
    );
}

#[test]
fn drop_intent_maps_supported_pairs() {
    let tab = crate::ui::core::input::DragPayload::Tab {
        from_pane: 0,
        tab_id: 7,
    };
    let explorer = crate::ui::core::input::DragPayload::ExplorerNode { node_id: 3 };

    assert_eq!(
        drop_intent(&tab, crate::ui::core::tree::NodeKind::TabBar { pane: 1 }),
        Some(DropIntent::TabToTabBar { to_pane: 1 })
    );
    assert_eq!(
        drop_intent(
            &tab,
            crate::ui::core::tree::NodeKind::EditorSplitDrop {
                pane: 0,
                drop: crate::ui::core::tree::SplitDrop::Down,
            },
        ),
        Some(DropIntent::TabToSplit {
            drop: crate::ui::core::tree::SplitDrop::Down
        })
    );

    assert_eq!(
        drop_intent(
            &explorer,
            crate::ui::core::tree::NodeKind::EditorArea { pane: 4 }
        ),
        Some(DropIntent::ExplorerToEditorArea { pane: 4 })
    );
    assert_eq!(
        drop_intent(
            &explorer,
            crate::ui::core::tree::NodeKind::ExplorerFolderDrop { node_id: 11 },
        ),
        Some(DropIntent::ExplorerToExplorerFolder { to_dir_id: 11 })
    );
    assert_eq!(
        drop_intent(
            &explorer,
            crate::ui::core::tree::NodeKind::ExplorerRow { node_id: 12 },
        ),
        Some(DropIntent::ExplorerToExplorerRow { to_row_id: 12 })
    );
}

#[test]
fn drop_intent_rejects_incompatible_pairs() {
    let tab = crate::ui::core::input::DragPayload::Tab {
        from_pane: 0,
        tab_id: 7,
    };
    let explorer = crate::ui::core::input::DragPayload::ExplorerNode { node_id: 3 };

    assert_eq!(
        drop_intent(
            &tab,
            crate::ui::core::tree::NodeKind::ExplorerFolderDrop { node_id: 11 },
        ),
        None
    );
    assert_eq!(
        drop_intent(
            &explorer,
            crate::ui::core::tree::NodeKind::TabBar { pane: 1 }
        ),
        None
    );
}

#[test]
fn rules_adapter_delegates_to_shared_logic() {
    let rules = WORKBENCH_DND_RULES;

    let source = node(crate::ui::core::tree::NodeKind::Tab { pane: 1, tab_id: 8 });
    assert_eq!(
        rules.payload_for_source(&source),
        Some(crate::ui::core::input::DragPayload::Tab {
            from_pane: 1,
            tab_id: 8
        })
    );

    let payload = crate::ui::core::input::DragPayload::Tab {
        from_pane: 1,
        tab_id: 8,
    };
    let ok_target = node(crate::ui::core::tree::NodeKind::TabBar { pane: 0 });
    let bad_target = node(crate::ui::core::tree::NodeKind::ExplorerRow { node_id: 22 });
    assert!(rules.can_drop(&payload, &ok_target));
    assert!(!rules.can_drop(&payload, &bad_target));
}
