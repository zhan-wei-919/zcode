use super::*;
use crate::models::{FileTree, NodeKind};
use std::ffi::OsString;

#[test]
fn explorer_move_selection_selects_first_row_when_root_selected() {
    let root = std::env::temp_dir();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root);
    let file_id = tree
        .insert_child(tree.root(), OsString::from("a.txt"), NodeKind::File)
        .unwrap();

    let mut explorer = ExplorerState::new(tree);
    assert!(explorer.selected().is_some());
    assert!(explorer.move_selection(1));
    assert_eq!(explorer.selected(), Some(file_id));
}
