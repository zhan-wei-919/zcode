use super::*;
use crate::core::event::{InputEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::ui::core::geom::Rect;
use crate::ui::core::id::Id;
use crate::ui::core::tree::{Node, NodeKind, Sense, UiTree};
use slotmap::Key;

fn node(id: u64, rect: Rect, sense: Sense) -> Node {
    Node {
        id: Id::raw(id),
        rect,
        layer: 0,
        z: 0,
        sense,
        kind: NodeKind::Unknown,
    }
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct TestRules;

impl DragDropRules for TestRules {
    fn payload_for_source(&self, source: &Node) -> Option<DragPayload> {
        match source.kind {
            NodeKind::Tab { pane, tab_id } => Some(DragPayload::Tab {
                from_pane: pane,
                tab_id,
            }),
            NodeKind::ExplorerRow { node_id } => Some(DragPayload::ExplorerNode { node_id }),
            _ => None,
        }
    }

    fn can_drop(&self, payload: &DragPayload, target: &Node) -> bool {
        matches!(
            (payload, target.kind),
            (DragPayload::Tab { .. }, NodeKind::TabBar { .. })
                | (DragPayload::Tab { .. }, NodeKind::EditorSplitDrop { .. })
                | (
                    DragPayload::ExplorerNode { .. },
                    NodeKind::EditorArea { .. }
                )
                | (
                    DragPayload::ExplorerNode { .. },
                    NodeKind::ExplorerRow { .. }
                )
                | (
                    DragPayload::ExplorerNode { .. },
                    NodeKind::ExplorerFolderDrop { .. }
                )
        )
    }
}

const TEST_RULES: TestRules = TestRules;

fn on_input(rt: &mut UiRuntime, input: &InputEvent, tree: &UiTree) -> UiRuntimeOutput {
    rt.on_input(input, tree, &TEST_RULES)
}

#[test]
fn hover_change_triggers_redraw() {
    let mut tree = UiTree::new();
    tree.push(node(1, Rect::new(0, 0, 10, 10), Sense::HOVER));

    let mut rt = UiRuntime::new();
    let out = on_input(&mut rt, &mouse(MouseEventKind::Moved, 5, 5), &tree);

    assert!(out.needs_redraw);
    assert!(matches!(
        out.events.as_slice(),
        [UiEvent::HoverChanged {
            from: None,
            to: Some(_),
            ..
        }]
    ));
    assert_eq!(rt.hovered(), Some(Id::raw(1)));
}

#[test]
fn left_click_emits_click() {
    let mut tree = UiTree::new();
    tree.push(node(
        1,
        Rect::new(0, 0, 10, 10),
        Sense::CLICK | Sense::HOVER,
    ));

    let mut rt = UiRuntime::new();
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Down(MouseButton::Left), 1, 1),
        &tree,
    );
    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Up(MouseButton::Left), 1, 1),
        &tree,
    );

    assert!(out.events.iter().any(|e| matches!(
        e,
        UiEvent::Click {
            id,
            button: MouseButton::Left,
            ..
        } if *id == Id::raw(1)
    )));
}

#[test]
fn right_click_emits_context_menu_when_supported() {
    let mut tree = UiTree::new();
    tree.push(node(
        1,
        Rect::new(0, 0, 10, 10),
        Sense::CONTEXT_MENU | Sense::HOVER,
    ));

    let mut rt = UiRuntime::new();
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Down(MouseButton::Right), 2, 2),
        &tree,
    );
    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Up(MouseButton::Right), 2, 2),
        &tree,
    );

    assert!(out.needs_redraw);
    assert!(out.events.iter().any(|e| matches!(
        e,
        UiEvent::ContextMenu { id, .. } if *id == Id::raw(1)
    )));
}

#[test]
fn drag_threshold_prevents_accidental_drag() {
    let mut tree = UiTree::new();
    tree.push(node(
        1,
        Rect::new(0, 0, 20, 20),
        Sense::CLICK | Sense::HOVER | Sense::DRAG_SOURCE,
    ));

    let mut rt = UiRuntime::new();
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Down(MouseButton::Left), 0, 0),
        &tree,
    );

    // Small move: dist == 1 -> no drag.
    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 1, 0),
        &tree,
    );
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, UiEvent::DragStart { .. })));

    // Move >= threshold: dist == 2 -> drag start.
    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 2, 0),
        &tree,
    );
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, UiEvent::DragStart { .. })));
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, UiEvent::DragMove { .. })));
    assert_eq!(rt.capture(), Some(Id::raw(1)));

    // Release ends drag and clears capture.
    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Up(MouseButton::Left), 2, 0),
        &tree,
    );
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, UiEvent::DragEnd { .. })));
    assert_eq!(rt.capture(), None);
}

#[test]
fn drag_drop_emits_drop_for_supported_targets() {
    let mut tree = UiTree::new();

    let file = Node {
        id: Id::raw(1),
        rect: Rect::new(0, 0, 5, 5),
        layer: 0,
        z: 0,
        sense: Sense::HOVER | Sense::DRAG_SOURCE,
        kind: NodeKind::ExplorerRow {
            node_id: crate::models::NodeId::null().to_raw(),
        },
    };
    let editor = Node {
        id: Id::raw(2),
        rect: Rect::new(10, 0, 10, 10),
        layer: 0,
        z: 0,
        sense: Sense::DROP_TARGET,
        kind: NodeKind::EditorArea { pane: 0 },
    };

    tree.push(file);
    tree.push(editor);

    let mut rt = UiRuntime::new();
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Down(MouseButton::Left), 1, 1),
        &tree,
    );

    // Start drag.
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 3, 1),
        &tree,
    );
    // Move over editor drop target.
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 12, 1),
        &tree,
    );

    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Up(MouseButton::Left), 12, 1),
        &tree,
    );
    assert!(out.events.iter().any(|e| matches!(
        e,
        UiEvent::Drop { target, payload, .. }
            if *target == Id::raw(2) && matches!(payload, DragPayload::ExplorerNode { .. })
    )));
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, UiEvent::DragEnd { .. })));
}

#[test]
fn drag_drop_prefers_topmost_compatible_target_when_overlapping() {
    let mut tree = UiTree::new();

    let file = Node {
        id: Id::raw(1),
        rect: Rect::new(0, 0, 5, 5),
        layer: 0,
        z: 0,
        sense: Sense::HOVER | Sense::DRAG_SOURCE,
        kind: NodeKind::ExplorerRow {
            node_id: crate::models::NodeId::null().to_raw(),
        },
    };

    // Overlapping drop targets: the TabBar is above the EditorArea in z-order, but it is not
    // compatible with ExplorerNode payloads.
    let editor = Node {
        id: Id::raw(2),
        rect: Rect::new(10, 0, 10, 10),
        layer: 0,
        z: 0,
        sense: Sense::DROP_TARGET,
        kind: NodeKind::EditorArea { pane: 0 },
    };
    let tabbar = Node {
        id: Id::raw(3),
        rect: Rect::new(10, 0, 10, 10),
        layer: 0,
        z: 0,
        sense: Sense::DROP_TARGET,
        kind: NodeKind::TabBar { pane: 0 },
    };

    tree.push(file);
    tree.push(editor);
    tree.push(tabbar);

    let mut rt = UiRuntime::new();
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Down(MouseButton::Left), 1, 1),
        &tree,
    );

    // Start drag.
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 3, 1),
        &tree,
    );
    // Move over the overlapping drop targets.
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 12, 1),
        &tree,
    );

    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Up(MouseButton::Left), 12, 1),
        &tree,
    );
    assert!(out.events.iter().any(|e| matches!(
        e,
        UiEvent::Drop { target, payload, .. }
            if *target == Id::raw(2) && matches!(payload, DragPayload::ExplorerNode { .. })
    )));
}

#[test]
fn drag_drop_emits_drop_for_tab_split_targets() {
    let mut tree = UiTree::new();

    let tab = Node {
        id: Id::raw(1),
        rect: Rect::new(0, 0, 5, 5),
        layer: 0,
        z: 0,
        sense: Sense::HOVER | Sense::DRAG_SOURCE,
        kind: NodeKind::Tab {
            pane: 0,
            tab_id: 42u64,
        },
    };
    let split = Node {
        id: Id::raw(2),
        rect: Rect::new(10, 0, 10, 10),
        layer: 0,
        z: 0,
        sense: Sense::DROP_TARGET,
        kind: NodeKind::EditorSplitDrop {
            pane: 0,
            drop: crate::ui::core::tree::SplitDrop::Right,
        },
    };

    tree.push(tab);
    tree.push(split);

    let mut rt = UiRuntime::new();
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Down(MouseButton::Left), 1, 1),
        &tree,
    );

    // Start drag.
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 3, 1),
        &tree,
    );
    // Move over editor split drop target.
    let _ = on_input(
        &mut rt,
        &mouse(MouseEventKind::Drag(MouseButton::Left), 12, 1),
        &tree,
    );

    let out = on_input(
        &mut rt,
        &mouse(MouseEventKind::Up(MouseButton::Left), 12, 1),
        &tree,
    );
    assert!(out.events.iter().any(|e| matches!(
        e,
        UiEvent::Drop { target, payload, .. }
            if *target == Id::raw(2)
                && matches!(payload, DragPayload::Tab { from_pane: 0, tab_id } if *tab_id == 42u64)
    )));
}
