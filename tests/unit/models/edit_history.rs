use super::*;
use compact_str::CompactString;

#[test]
fn test_undo_redo() {
    let base = Rope::from_str("hello");
    let mut history = EditHistory::new();

    // 插入 " world"
    let mut rope = base.clone();
    let op = EditOp::insert(
        history.head(),
        5,
        CompactString::new(" world"),
        (0, 5),
        (0, 11),
    );
    op.apply(&mut rope);
    history.push(op);

    assert!(history.can_undo());
    assert!(!history.can_redo());

    // Undo
    let undo = history.undo(&rope).unwrap();
    assert_eq!(undo.rope.to_string(), "hello");
    assert_eq!(undo.cursor, (0, 5));
    rope = undo.rope;

    assert!(!history.can_undo());
    assert!(history.can_redo());

    // Redo
    let redo = history.redo(&rope).unwrap();
    assert_eq!(redo.rope.to_string(), "hello world");
    assert_eq!(redo.cursor, (0, 11));
}

#[test]
fn test_branch_on_edit_after_undo() {
    let mut history = EditHistory::new();

    // 插入 "a"
    let op_a = EditOp::insert(history.head(), 0, CompactString::new("a"), (0, 0), (0, 1));
    history.push(op_a.clone());

    // 插入 "b"
    let op_b = EditOp::insert(history.head(), 1, CompactString::new("b"), (0, 1), (0, 2));
    history.push(op_b.clone());
    let mut rope = Rope::from_str("ab");

    // Undo 一次（回到 "a"），HEAD 落到 op_a
    let undo = history.undo(&rope).unwrap();
    assert_eq!(undo.rope.to_string(), "a");
    assert_eq!(history.head(), op_a.id);

    // 在 "a" 上插入 "c"，创建第二条分支
    let op_c = EditOp::insert(history.head(), 1, CompactString::new("c"), (0, 1), (0, 2));
    history.push(op_c.clone());
    rope = Rope::from_str("ac");
    assert_eq!(history.head(), op_c.id);

    // 两条分支的 op 都仍在历史中
    assert!(history.get_op(&op_b.id).is_some());
    assert!(history.get_op(&op_c.id).is_some());

    // Undo 回 "a" 后 redo 跟随最近创建的分支 c
    let undo = history.undo(&rope).unwrap();
    assert_eq!(undo.rope.to_string(), "a");
    let redo = history.redo(&undo.rope).unwrap();
    assert_eq!(redo.rope.to_string(), "ac");
    assert_eq!(history.head(), op_c.id);
}

#[test]
fn test_is_dirty() {
    let mut history = EditHistory::new();

    assert!(!history.is_dirty());

    let op = EditOp::insert(
        history.head(),
        5,
        CompactString::new(" world"),
        (0, 5),
        (0, 11),
    );
    history.push(op);

    assert!(history.is_dirty());
}

#[test]
fn test_dirty_save_point() {
    let base = Rope::from_str("hello");
    let mut history = EditHistory::new();

    let mut rope = base.clone();
    let op = EditOp::insert(
        history.head(),
        5,
        CompactString::new(" world"),
        (0, 5),
        (0, 11),
    );
    op.apply(&mut rope);
    history.push(op);

    assert!(history.is_dirty());
    history.on_save();
    assert!(!history.is_dirty());

    let undo = history.undo(&rope).unwrap();
    assert!(history.is_dirty());

    let redo = history.redo(&undo.rope).unwrap();
    assert_eq!(redo.rope.to_string(), "hello world");
    assert!(!history.is_dirty());
}

#[test]
fn clear_resets_history() {
    let mut history = EditHistory::new();
    let op = EditOp::insert(history.head(), 0, CompactString::new("a"), (0, 0), (0, 1));
    history.push(op);
    assert!(history.can_undo());

    history.clear();
    assert!(!history.can_undo());
    assert!(!history.is_dirty());
    assert!(history.head().is_root());
}
