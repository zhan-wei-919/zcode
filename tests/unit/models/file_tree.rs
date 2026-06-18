use super::*;
use slotmap::Key;

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

#[test]
fn test_full_path_ro_builds_paths_without_cache_mutation() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();

    let dir_id = tree
        .insert_child(root, "subdir".into(), NodeKind::Dir)
        .unwrap();
    let file_id = tree
        .insert_child(dir_id, "file.txt".into(), NodeKind::File)
        .unwrap();

    assert_eq!(tree.full_path_ro(root), Some(PathBuf::from("/root")));
    assert_eq!(
        tree.full_path_ro(dir_id),
        Some(PathBuf::from("/root/subdir"))
    );
    assert_eq!(
        tree.full_path_ro(file_id),
        Some(PathBuf::from("/root/subdir/file.txt"))
    );

    assert_eq!(tree.full_path_ro(NodeId::null()), None);
}

#[test]
fn test_find_node_by_path_ro_returns_matching_node_id() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();
    let src_id = tree
        .insert_child(root, "src".into(), NodeKind::Dir)
        .unwrap();
    let main_id = tree
        .insert_child(src_id, "main.rs".into(), NodeKind::File)
        .unwrap();

    assert_eq!(
        tree.find_node_by_path_ro(&PathBuf::from("/root")),
        Some(root)
    );
    assert_eq!(
        tree.find_node_by_path_ro(&PathBuf::from("/root/src")),
        Some(src_id)
    );
    assert_eq!(
        tree.find_node_by_path_ro(&PathBuf::from("/root/src/main.rs")),
        Some(main_id)
    );
}

#[test]
fn test_find_node_by_path_ro_returns_none_for_unknown_or_outside_paths() {
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();
    let _ = tree
        .insert_child(root, "src".into(), NodeKind::Dir)
        .unwrap();

    assert_eq!(
        tree.find_node_by_path_ro(&PathBuf::from("/root/missing.txt")),
        None
    );
    assert_eq!(tree.find_node_by_path_ro(&PathBuf::from("/tmp")), None);
}

#[test]
fn test_flatten_for_view_hides_root_so_top_level_depth_is_zero() {
    // 隐藏的 root 不应计入缩进：顶层条目 depth=0、嵌套逐层 +1，与 demo 的文件列表对齐。
    let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
    let root = tree.root();
    let src = tree
        .insert_child(root, "src".into(), NodeKind::Dir)
        .unwrap();
    let app = tree.insert_child(src, "app".into(), NodeKind::Dir).unwrap();
    tree.insert_child(app, "main.rs".into(), NodeKind::File)
        .unwrap();
    tree.expand(src);
    tree.expand(app);

    let rows = tree.flatten_for_view();
    let depth_of = |name: &str| {
        rows.iter()
            .find(|r| r.name.as_os_str() == std::ffi::OsStr::new(name))
            .map(|r| r.depth)
    };

    assert_eq!(depth_of("src"), Some(0), "顶层目录应为 depth 0");
    assert_eq!(depth_of("app"), Some(1), "二层目录应为 depth 1");
    assert_eq!(depth_of("main.rs"), Some(2), "三层文件应为 depth 2");
}
