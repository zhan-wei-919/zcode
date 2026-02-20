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
