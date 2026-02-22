use super::*;
use compact_str::CompactString;
use ropey::Rope;

#[test]
fn test_insert_apply() {
    let mut rope = Rope::from_str("hello");
    let op = EditOp::insert(
        OpId::root(),
        5,
        CompactString::new(" world"),
        (0, 5),
        (0, 11),
    );
    op.apply(&mut rope);
    assert_eq!(rope.to_string(), "hello world");
}

#[test]
fn test_delete_apply() {
    let mut rope = Rope::from_str("hello world");
    let op = EditOp::delete(
        OpId::root(),
        5,
        11,
        CompactString::new(" world"),
        (0, 11),
        (0, 5),
    );
    op.apply(&mut rope);
    assert_eq!(rope.to_string(), "hello");
}

#[test]
fn test_inverse() {
    let insert_op = EditOp::insert(OpId::root(), 0, CompactString::new("hello"), (0, 0), (0, 5));
    let delete_kind = insert_op.inverse();

    let mut rope = Rope::new();
    insert_op.apply(&mut rope);
    assert_eq!(rope.to_string(), "hello");

    delete_kind.apply(&mut rope);
    assert_eq!(rope.to_string(), "");
}

#[test]
fn test_replace_apply() {
    let mut rope = Rope::from_str("hello world");
    let op = EditOp::replace(
        OpId::root(),
        6,
        11,
        CompactString::new("world"),
        CompactString::new("rust"),
        (0, 11),
        (0, 10),
    );
    op.apply(&mut rope);
    assert_eq!(rope.to_string(), "hello rust");
}

#[test]
fn test_serialization() {
    let op = EditOp::insert(OpId::root(), 0, CompactString::new("hello"), (0, 0), (0, 5));
    let json = op.to_json_line().unwrap();
    let restored = EditOp::from_json_line(&json).unwrap();

    assert_eq!(restored.cursor_after(), (0, 5));
    assert_eq!(restored.parent, OpId::root());
}

#[test]
fn test_opid_uniqueness() {
    let id1 = OpId::new();
    let id2 = OpId::new();
    assert_ne!(id1, id2);
}

#[test]
fn test_batch_apply() {
    let mut rope = Rope::from_str("0123456789");
    let op = EditOp {
        id: OpId::root(),
        parent: OpId::root(),
        kind: OpKind::Batch {
            edits: vec![
                // Must be applied in descending start order.
                BatchEdit {
                    start: 7,
                    end: 9,
                    deleted: CompactString::new("78"),
                    inserted: CompactString::new("ab"),
                },
                BatchEdit {
                    start: 1,
                    end: 3,
                    deleted: CompactString::new("12"),
                    inserted: CompactString::new("CD"),
                },
            ],
        },
        cursor_before: (0, 0),
        cursor_after: (0, 0),
        extra_cursors_before: None,
        extra_cursors_after: None,
    };

    op.apply(&mut rope);
    assert_eq!(rope.to_string(), "0CD3456ab9");
}

#[test]
fn test_batch_inverse_roundtrip() {
    let mut rope = Rope::from_str("hello world");
    let forward = EditOp {
        id: OpId::root(),
        parent: OpId::root(),
        kind: OpKind::Batch {
            edits: vec![
                BatchEdit {
                    start: 6,
                    end: 11,
                    deleted: CompactString::new("world"),
                    inserted: CompactString::new("rust"),
                },
                BatchEdit {
                    start: 0,
                    end: 5,
                    deleted: CompactString::new("hello"),
                    inserted: CompactString::new("hey"),
                },
            ],
        },
        cursor_before: (0, 0),
        cursor_after: (0, 0),
        extra_cursors_before: None,
        extra_cursors_after: None,
    };

    forward.apply(&mut rope);
    assert_eq!(rope.to_string(), "hey rust");

    let inverse = forward.inverse();
    inverse.apply(&mut rope);
    assert_eq!(rope.to_string(), "hello world");
}
