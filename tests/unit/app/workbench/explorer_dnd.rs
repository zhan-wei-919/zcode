use super::*;
use std::path::PathBuf;

#[test]
fn compute_explorer_move_target_moves_file_into_dir() {
    let from = PathBuf::from("/ws/src/main.rs");
    let to_dir = PathBuf::from("/ws/tests");
    let to = compute_explorer_move_target(&from, false, &to_dir).unwrap();
    assert_eq!(to, PathBuf::from("/ws/tests/main.rs"));
}

#[test]
fn compute_explorer_move_target_noops_when_dropping_into_same_parent() {
    let from = PathBuf::from("/ws/tests/main.rs");
    let to_dir = PathBuf::from("/ws/tests");
    assert!(compute_explorer_move_target(&from, false, &to_dir).is_none());
}

#[test]
fn compute_explorer_move_target_rejects_moving_dir_into_itself_or_descendant() {
    let from = PathBuf::from("/ws/foo");

    // Into itself.
    assert!(compute_explorer_move_target(&from, true, &from).is_none());

    // Into descendant.
    let to_dir = PathBuf::from("/ws/foo/bar");
    assert!(compute_explorer_move_target(&from, true, &to_dir).is_none());
}
