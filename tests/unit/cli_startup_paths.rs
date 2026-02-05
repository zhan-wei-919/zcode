use tempfile::tempdir;

#[test]
fn resolve_startup_paths_defaults_to_cwd() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();

    let startup = super::resolve_startup_paths(cwd, None).unwrap();
    assert_eq!(startup.root, cwd);
    assert!(startup.open_file.is_none());
}

#[test]
fn resolve_startup_paths_accepts_directory_arg() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();

    let workspace = cwd.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let startup = super::resolve_startup_paths(cwd, Some("workspace")).unwrap();
    assert_eq!(startup.root, workspace);
    assert!(startup.open_file.is_none());
}

#[test]
fn resolve_startup_paths_accepts_file_arg_and_uses_cwd_as_root_when_file_is_inside_cwd() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();

    let workspace = cwd.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let file = workspace.join("a.txt");
    std::fs::write(&file, "hello\n").unwrap();

    let startup = super::resolve_startup_paths(cwd, Some("workspace/a.txt")).unwrap();
    assert_eq!(startup.root, cwd);
    assert_eq!(startup.open_file, Some(file));
}

#[test]
fn resolve_startup_paths_errors_for_missing_path() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();

    let err = super::resolve_startup_paths(cwd, Some("nope")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn resolve_startup_paths_keeps_absolute_paths() {
    let dir = tempdir().unwrap();
    let cwd = dir.path();

    let workspace = cwd.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let abs = workspace.canonicalize().unwrap();
    let raw = abs.to_string_lossy().to_string();

    let startup = super::resolve_startup_paths(cwd, Some(&raw)).unwrap();
    assert_eq!(startup.root, abs);
    assert!(startup.open_file.is_none());
}
