use super::*;

#[test]
fn test_new_tree() {
    let tree = FileTree::new_with_root("test".into(), PathBuf::from("/test"));
    assert!(tree.is_dir(tree.root()));
    assert!(tree.is_expanded(tree.root()));
}

#[test]
fn test_insert_child() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();

    let file_id = tree
        .insert_child(root, "file.txt".into(), NodeKind::File)
        .unwrap();
    let dir_id = tree
        .insert_child(root, "subdir".into(), NodeKind::Dir)
        .unwrap();

    assert!(!tree.is_dir(file_id));
    assert!(tree.is_dir(dir_id));
}

#[test]
fn test_rename() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();

    let file_id = tree
        .insert_child(root, "old.txt".into(), NodeKind::File)
        .unwrap();
    tree.rename(file_id, "new.txt".into()).unwrap();

    assert_eq!(tree.get_name(file_id), Some(&OsString::from("new.txt")));
}

#[test]
fn test_delete() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();

    let file_id = tree
        .insert_child(root, "file.txt".into(), NodeKind::File)
        .unwrap();
    tree.delete(file_id).unwrap();

    assert!(tree.get_name(file_id).is_none());
}

#[test]
fn test_toggle_expand() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();

    let dir_id = tree
        .insert_child(root, "subdir".into(), NodeKind::Dir)
        .unwrap();

    assert!(!tree.is_expanded(dir_id));
    tree.toggle_expand(dir_id);
    assert!(tree.is_expanded(dir_id));
    tree.toggle_expand(dir_id);
    assert!(!tree.is_expanded(dir_id));
}

#[test]
fn test_flatten_for_view() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();

    tree.insert_child(root, "file1.txt".into(), NodeKind::File)
        .unwrap();
    let dir_id = tree
        .insert_child(root, "subdir".into(), NodeKind::Dir)
        .unwrap();
    tree.insert_child(dir_id, "file2.txt".into(), NodeKind::File)
        .unwrap();

    let rows = tree.flatten_for_view();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].is_dir);

    tree.expand(dir_id);
    let rows = tree.flatten_for_view();
    assert_eq!(rows.len(), 3);
}
