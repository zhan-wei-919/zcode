use super::*;

fn node(id: u64, rect: Rect, layer: u8, z: u32, sense: Sense) -> Node {
    Node {
        id: Id::raw(id),
        rect,
        layer,
        z,
        sense,
        kind: NodeKind::Unknown,
    }
}

#[test]
fn hit_test_prefers_higher_layer() {
    let mut tree = UiTree::new();
    let r = Rect::new(0, 0, 10, 10);
    tree.push(node(1, r, 0, 1, Sense::CLICK));
    tree.push(node(2, r, 1, 0, Sense::CLICK));

    let hit = tree.hit_test(Pos::new(5, 5)).unwrap();
    assert_eq!(hit.id, Id::raw(2));
}

#[test]
fn hit_test_prefers_higher_z_within_same_layer() {
    let mut tree = UiTree::new();
    let r = Rect::new(0, 0, 10, 10);
    tree.push(node(1, r, 0, 1, Sense::CLICK));
    tree.push(node(2, r, 0, 2, Sense::CLICK));

    let hit = tree.hit_test(Pos::new(5, 5)).unwrap();
    assert_eq!(hit.id, Id::raw(2));
}

#[test]
fn hit_test_with_sense_filters_nodes() {
    let mut tree = UiTree::new();
    let r = Rect::new(0, 0, 10, 10);
    tree.push(node(1, r, 0, 1, Sense::CLICK));
    tree.push(node(2, r, 0, 2, Sense::HOVER));

    assert_eq!(
        tree.hit_test_with_sense(Pos::new(5, 5), Sense::CLICK)
            .unwrap()
            .id,
        Id::raw(1)
    );
    assert_eq!(
        tree.hit_test_with_sense(Pos::new(5, 5), Sense::HOVER)
            .unwrap()
            .id,
        Id::raw(2)
    );
    assert!(tree
        .hit_test_with_sense(Pos::new(5, 5), Sense::CONTEXT_MENU)
        .is_none());
}

#[test]
fn node_lookup_finds_by_id() {
    let mut tree = UiTree::new();
    tree.push(node(1, Rect::new(0, 0, 1, 1), 0, 0, Sense::CLICK));
    tree.push(node(2, Rect::new(0, 0, 1, 1), 0, 0, Sense::CLICK));
    assert_eq!(tree.node(Id::raw(2)).unwrap().id, Id::raw(2));
    assert!(tree.node(Id::raw(3)).is_none());
}
