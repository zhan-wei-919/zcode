use tempfile::tempdir;

#[test]
fn resolve_startup_paths_defaults_to_cwd() {
    let dir = tempdir().unwrap();
    let cwd = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());

    let startup = super::resolve_startup_paths(&cwd, None).unwrap();
    assert_eq!(startup.root, cwd);
    assert!(startup.open_file.is_none());
}

#[test]
fn resolve_startup_paths_accepts_directory_arg() {
    let dir = tempdir().unwrap();
    let cwd = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());

    let workspace = cwd.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let startup = super::resolve_startup_paths(&cwd, Some("workspace")).unwrap();
    assert_eq!(startup.root, workspace);
    assert!(startup.open_file.is_none());
}

#[test]
fn resolve_startup_paths_accepts_file_arg_and_uses_cwd_as_root_when_file_is_inside_cwd() {
    let dir = tempdir().unwrap();
    let cwd = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());

    let workspace = cwd.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let file = workspace.join("a.txt");
    std::fs::write(&file, "hello\n").unwrap();

    let startup = super::resolve_startup_paths(&cwd, Some("workspace/a.txt")).unwrap();
    assert_eq!(startup.root, cwd);
    assert_eq!(startup.open_file, Some(file));
}

#[test]
fn resolve_startup_paths_errors_for_missing_path() {
    let dir = tempdir().unwrap();
    let cwd = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());

    let err = super::resolve_startup_paths(&cwd, Some("nope")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn resolve_startup_paths_keeps_absolute_paths() {
    let dir = tempdir().unwrap();
    let cwd = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf());

    let workspace = cwd.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let abs = workspace.canonicalize().unwrap();
    let raw = abs.to_string_lossy().to_string();

    let startup = super::resolve_startup_paths(&cwd, Some(&raw)).unwrap();
    assert_eq!(startup.root, abs);
    assert!(startup.open_file.is_none());
}

#[test]
fn poll_events_with_maps_revents_to_wakeup_ready() {
    let result = super::poll_events_with(
        std::time::Duration::from_millis(1),
        123,
        |fds, _timeout_ms| {
            fds[0].revents = libc::POLLIN;
            fds[1].revents = libc::POLLIN;
            Ok(())
        },
    )
    .unwrap();

    assert!(result.wakeup_ready);
}

#[test]
fn poll_events_returns_error_for_invalid_wakeup_fd() {
    let mut fds = [0; 2];
    // SAFETY: `fds` points to a valid 2-element array for `pipe`.
    let pipe_ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
    assert_eq!(pipe_ret, 0, "failed to create test pipe");

    let read_fd = fds[0];
    let write_fd = fds[1];

    // SAFETY: `read_fd` was returned by `pipe` and is valid to close once.
    unsafe { libc::close(read_fd) };

    let result = super::poll_events(std::time::Duration::from_millis(1), read_fd);

    // SAFETY: `write_fd` was returned by `pipe` and is still open here.
    unsafe { libc::close(write_fd) };

    match result {
        Ok(_) => panic!("expected poll_events to return EBADF for closed wakeup fd"),
        Err(err) => assert_eq!(err.raw_os_error(), Some(libc::EBADF)),
    }
}

#[test]
fn poll_events_propagates_poller_errors() {
    let result = super::poll_events_with(
        std::time::Duration::from_millis(1),
        123,
        |_fds, _timeout_ms| Err(std::io::Error::from_raw_os_error(libc::EBADF)),
    );

    match result {
        Ok(_) => panic!("expected poll_events_with to return error"),
        Err(err) => assert_eq!(err.raw_os_error(), Some(libc::EBADF)),
    }
}

#[test]
fn coalesce_scroll_events_separates_different_modifiers() {
    use crossterm::event::{Event, KeyModifiers, MouseEvent, MouseEventKind};

    let events = vec![
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 3,
            row: 4,
            modifiers: KeyModifiers::NONE,
        }),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 3,
            row: 4,
            modifiers: KeyModifiers::SHIFT,
        }),
    ];

    let result = super::coalesce_scroll_events(events);

    assert_eq!(result.len(), 2);
    match result[0] {
        Event::Mouse(mouse) => {
            assert_eq!(mouse.kind, MouseEventKind::ScrollDown);
            assert_eq!(mouse.modifiers, KeyModifiers::NONE);
        }
        _ => panic!("expected mouse event"),
    }
    match result[1] {
        Event::Mouse(mouse) => {
            assert_eq!(mouse.kind, MouseEventKind::ScrollUp);
            assert_eq!(mouse.modifiers, KeyModifiers::SHIFT);
        }
        _ => panic!("expected mouse event"),
    }
}

#[test]
fn coalesce_scroll_events_keeps_net_scroll_with_same_modifiers() {
    use crossterm::event::{Event, KeyModifiers, MouseEvent, MouseEventKind};

    let events = vec![
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 6,
            modifiers: KeyModifiers::SHIFT,
        }),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 6,
            modifiers: KeyModifiers::SHIFT,
        }),
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 6,
            modifiers: KeyModifiers::SHIFT,
        }),
    ];

    let result = super::coalesce_scroll_events(events);

    assert_eq!(result.len(), 1);
    match result[0] {
        Event::Mouse(mouse) => {
            assert_eq!(mouse.kind, MouseEventKind::ScrollDown);
            assert_eq!(mouse.modifiers, KeyModifiers::SHIFT);
        }
        _ => panic!("expected mouse event"),
    }
}
