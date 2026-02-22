use super::*;
use compact_str::CompactString;

#[test]
fn test_undo_redo() {
    let base = Rope::from_str("hello");
    let mut history = EditHistory::new(base.clone());

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
    history.push(op, &rope);

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
    let base = Rope::from_str("");
    let mut history = EditHistory::new(base.clone());

    // 插入 "a"
    let mut rope = Rope::from_str("a");
    let op_a = EditOp::insert(history.head(), 0, CompactString::new("a"), (0, 0), (0, 1));
    history.push(op_a.clone(), &rope);

    // 插入 "b"
    rope = Rope::from_str("ab");
    let op_b = EditOp::insert(history.head(), 1, CompactString::new("b"), (0, 1), (0, 2));
    history.push(op_b.clone(), &rope);

    // Undo 一次（回到 "a"）
    let _ = history.undo(&rope).unwrap();

    // 插入 "c"（创建分支）
    rope = Rope::from_str("ac");
    let op_c = EditOp::insert(history.head(), 1, CompactString::new("c"), (0, 1), (0, 2));
    history.push(op_c.clone(), &rope);

    // 验证分支存在
    let children = history.children_of(&op_a.id);
    assert_eq!(children.len(), 2); // b 和 c 都是 a 的子节点

    // checkout 到旧分支 b，再 undo 回 a，然后 redo 应该回到 b（而不是最近创建的 c）
    let (checkout_rope, _) = history.checkout(op_b.id).unwrap();
    assert_eq!(checkout_rope.to_string(), "ab");
    rope = checkout_rope;

    let undo = history.undo(&rope).unwrap();
    assert_eq!(undo.rope.to_string(), "a");
    rope = undo.rope;

    let redo = history.redo(&rope).unwrap();
    assert_eq!(redo.rope.to_string(), "ab");
}

#[test]
fn test_log() {
    let base = Rope::from_str("");
    let mut history = EditHistory::new(base.clone());

    // 插入 "a", "b", "c"
    let mut rope = Rope::from_str("a");
    let op_a = EditOp::insert(history.head(), 0, CompactString::new("a"), (0, 0), (0, 1));
    history.push(op_a, &rope);

    rope = Rope::from_str("ab");
    let op_b = EditOp::insert(history.head(), 1, CompactString::new("b"), (0, 1), (0, 2));
    history.push(op_b, &rope);

    rope = Rope::from_str("abc");
    let op_c = EditOp::insert(history.head(), 2, CompactString::new("c"), (0, 2), (0, 3));
    history.push(op_c, &rope);

    // log 应该返回 c, b, a
    let log = history.log();
    assert_eq!(log.len(), 3);
}

#[test]
fn test_checkout() {
    let base = Rope::from_str("");
    let mut history = EditHistory::new(base.clone());

    // 插入 "a"
    let mut rope = Rope::from_str("a");
    let op_a = EditOp::insert(history.head(), 0, CompactString::new("a"), (0, 0), (0, 1));
    let op_a_id = op_a.id;
    history.push(op_a, &rope);

    // 插入 "b"
    rope = Rope::from_str("ab");
    let op_b = EditOp::insert(history.head(), 1, CompactString::new("b"), (0, 1), (0, 2));
    history.push(op_b, &rope);

    // checkout 到 op_a
    let (checkout_rope, _) = history.checkout(op_a_id).unwrap();
    assert_eq!(checkout_rope.to_string(), "a");
    assert_eq!(history.head(), op_a_id);
}

#[test]
fn test_is_dirty() {
    let base = Rope::from_str("hello");
    let mut history = EditHistory::new(base.clone());

    assert!(!history.is_dirty());

    let rope = Rope::from_str("hello world");
    let op = EditOp::insert(
        history.head(),
        5,
        CompactString::new(" world"),
        (0, 5),
        (0, 11),
    );
    history.push(op, &rope);

    assert!(history.is_dirty());
}

#[test]
fn test_dirty_save_point() {
    let base = Rope::from_str("hello");
    let mut history = EditHistory::new(base.clone());

    let mut rope = base.clone();
    let op = EditOp::insert(
        history.head(),
        5,
        CompactString::new(" world"),
        (0, 5),
        (0, 11),
    );
    op.apply(&mut rope);
    history.push(op, &rope);

    assert!(history.is_dirty());
    history.on_save(&rope);
    assert!(!history.is_dirty());

    let undo = history.undo(&rope).unwrap();
    assert!(history.is_dirty());

    let redo = history.redo(&undo.rope).unwrap();
    assert_eq!(redo.rope.to_string(), "hello world");
    assert!(!history.is_dirty());
}

#[test]
fn push_without_backup_does_not_queue_pending_ops() {
    let base = Rope::from_str("");
    let mut history = EditHistory::new(base.clone());

    let mut rope = base.clone();
    let op = EditOp::insert(history.head(), 0, CompactString::new("a"), (0, 0), (0, 1));
    op.apply(&mut rope);
    history.push(op, &rope);

    assert!(history.pending_ops.is_empty());
}

#[test]
fn push_with_backup_queues_pending_ops() {
    let base = Rope::from_str("");
    let mut path = std::env::temp_dir();
    path.push(format!(
        "zcode-edit-history-{}.ops",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    let mut history = EditHistory::with_backup(base.clone(), path.clone())
        .expect("history with backup should be created")
        .with_config(EditHistoryConfig {
            flush_interval_ms: 60_000,
            flush_threshold: 1_000_000,
            checkpoint_interval: 0,
        });

    let mut rope = base.clone();
    let op = EditOp::insert(history.head(), 0, CompactString::new("a"), (0, 0), (0, 1));
    op.apply(&mut rope);
    history.push(op, &rope);

    assert_eq!(history.pending_ops.len(), 1);

    drop(history);
    let _ = std::fs::remove_file(path);
}
