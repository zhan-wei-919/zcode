use super::*;
use crate::core::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::ffi::{OsStr, OsString};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn create_test_runtime() -> (AsyncRuntime, mpsc::Receiver<AppMessage>) {
    let (tx, rx) = mpsc::channel();
    (AsyncRuntime::new(tx).unwrap(), rx)
}

fn drain_runtime_messages(workbench: &mut Workbench, rx: &mpsc::Receiver<AppMessage>) -> bool {
    let mut changed = false;
    while let Ok(msg) = rx.try_recv() {
        workbench.handle_message(msg);
        changed = true;
    }
    changed
}

fn drive_until(
    workbench: &mut Workbench,
    rx: &mpsc::Receiver<AppMessage>,
    timeout: Duration,
    mut done: impl FnMut(&Workbench) -> bool,
) {
    let start = Instant::now();
    loop {
        drain_runtime_messages(workbench, rx);
        workbench.tick();
        if done(workbench) {
            return;
        }
        if start.elapsed() > timeout {
            panic!("timeout waiting for condition");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn test_workbench_new() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert_eq!(workbench.focus(), FocusTarget::Editor);
    assert!(workbench.sidebar_visible());
}

#[test]
fn test_toggle_sidebar() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert!(workbench.sidebar_visible());

    let key_event = KeyEvent {
        code: KeyCode::Char('b'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(!workbench.sidebar_visible());
}

#[test]
fn test_toggle_bottom_panel() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert!(!workbench.bottom_panel_visible());

    let key_event = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(workbench.bottom_panel_visible());
}

#[test]
fn test_focus_bottom_panel() {
    let dir = tempdir().unwrap();
    let (runtime, _rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    assert_eq!(workbench.focus(), FocusTarget::Editor);

    let key_event = KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
    };
    let result = workbench.handle_input(&InputEvent::Key(key_event));

    assert!(result.is_consumed());
    assert!(workbench.bottom_panel_visible());
    assert_eq!(workbench.focus(), FocusTarget::BottomPanel);
}

#[test]
fn test_open_file_and_save_runs_async_runtime() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, "hello\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::OpenPath(file_path.clone()));
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p == &file_path)
    });

    let insert = KeyEvent {
        code: KeyCode::Char('X'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(insert));

    let save = KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(save));

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        let Some(tab) = w.store.state().editor.pane(0).and_then(|p| p.active_tab()) else {
            return false;
        };
        !tab.dirty
    });

    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "Xhello\n");
}

#[test]
fn test_editor_search_runs_async_task_and_updates_matches() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a.txt");
    std::fs::write(&file_path, "hello world\nhello again\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::OpenPath(file_path.clone()));
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p == &file_path)
    });

    let find = KeyEvent {
        code: KeyCode::Char('f'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(find));

    for ch in "hello".chars() {
        let ev = KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        };
        let _ = workbench.handle_input(&InputEvent::Key(ev));
    }

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .is_some_and(|p| !p.search_bar.searching && !p.search_bar.matches.is_empty())
    });

    let pane = workbench.store.state().editor.pane(0).unwrap();
    assert!(pane.search_bar.visible);
    assert!(!pane.search_bar.matches.is_empty());
}

#[test]
fn test_global_search_runs_async_task_and_populates_results() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    std::fs::write(&a, "needle\n").unwrap();
    std::fs::write(&b, "x needle y\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::FocusSearch));
    for ch in "needle".chars() {
        let ev = KeyEvent {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        };
        let _ = workbench.handle_input(&InputEvent::Key(ev));
    }

    let start = KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(start));

    drive_until(&mut workbench, &rx, Duration::from_secs(3), |w| {
        let s = &w.store.state().search;
        !s.searching && s.total_matches > 0 && !s.items.is_empty()
    });

    let s = &workbench.store.state().search;
    assert!(s.total_matches >= 2);
    assert!(!s.items.is_empty());
}

#[test]
fn test_explorer_create_file_runs_async_fs_and_updates_tree() {
    let dir = tempdir().unwrap();
    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerNewFile));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAppend('x'));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAccept);

    let path = dir.path().join("x");
    let file_name = OsString::from("x");
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        path.is_file()
            && w.store
                .state()
                .explorer
                .rows
                .iter()
                .any(|row| row.name == file_name)
    });

    assert!(path.is_file());
}

#[test]
fn test_explorer_create_dir_then_expand_loads_entries() {
    let dir = tempdir().unwrap();
    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerNewFolder));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAppend('d'));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAccept);

    let dir_path = dir.path().join("d");
    let dir_name = OsString::from("d");
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        dir_path.is_dir()
            && w.store
                .state()
                .explorer
                .rows
                .iter()
                .any(|row| row.name == dir_name)
    });

    let child_path = dir_path.join("child.txt");
    std::fs::write(&child_path, "hello\n").unwrap();

    let dir_row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|row| row.name.as_os_str() == OsStr::new("d"))
        .expect("directory row exists");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row: dir_row,
        now: Instant::now(),
    });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerActivate);

    let child_name = OsString::from("child.txt");
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .explorer
            .rows
            .iter()
            .any(|row| row.name == child_name)
    });

    assert!(child_path.is_file());
}

#[test]
fn test_explorer_expand_dir_load_error_collapses_node() {
    let dir = tempdir().unwrap();
    let gone_path = dir.path().join("gone");
    std::fs::create_dir_all(&gone_path).unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    std::fs::remove_dir_all(&gone_path).unwrap();

    let row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.name.as_os_str() == OsStr::new("gone"))
        .expect("gone dir row exists");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row,
        now: Instant::now(),
    });
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerActivate);

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        let Some(row) = w
            .store
            .state()
            .explorer
            .rows
            .iter()
            .find(|r| r.name.as_os_str() == OsStr::new("gone"))
        else {
            return false;
        };
        matches!(row.load_state, crate::models::LoadState::NotLoaded) && !row.is_expanded
    });
}

#[test]
fn test_explorer_delete_file_runs_async_fs_and_updates_tree() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("to_delete.txt");
    std::fs::write(&file_path, "x\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let row = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .position(|r| r.name.as_os_str() == OsStr::new("to_delete.txt"))
        .expect("file row exists");
    let _ = workbench.dispatch_kernel(KernelAction::ExplorerClickRow {
        row,
        now: Instant::now(),
    });

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerDelete));
    let _ = workbench.dispatch_kernel(KernelAction::ConfirmDialogAccept);

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        !file_path.exists()
            && !w
                .store
                .state()
                .explorer
                .rows
                .iter()
                .any(|r| r.name.as_os_str() == OsStr::new("to_delete.txt"))
    });

    assert!(!file_path.exists());
}

#[test]
fn test_explorer_create_file_error_is_logged() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("x");
    std::fs::write(&file_path, "exists\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::RunCommand(Command::ExplorerNewFile));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAppend('x'));
    let _ = workbench.dispatch_kernel(KernelAction::InputDialogAccept);

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.logs
            .iter()
            .any(|line| line.contains("[fs:create_file]") && line.contains("x"))
    });

    let count = workbench
        .store
        .state()
        .explorer
        .rows
        .iter()
        .filter(|r| r.name.as_os_str() == OsStr::new("x"))
        .count();
    assert_eq!(count, 1);
}

#[test]
fn test_save_failure_is_logged_and_does_not_clear_dirty() {
    let dir = tempdir().unwrap();
    let subdir = dir.path().join("sub");
    std::fs::create_dir_all(&subdir).unwrap();
    let file_path = subdir.join("a.txt");
    std::fs::write(&file_path, "hello\n").unwrap();

    let (runtime, rx) = create_test_runtime();
    let mut workbench = Workbench::new(dir.path(), runtime, None).unwrap();

    let _ = workbench.dispatch_kernel(KernelAction::OpenPath(file_path.clone()));
    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.store
            .state()
            .editor
            .pane(0)
            .and_then(|p| p.active_tab())
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p == &file_path)
    });

    std::fs::remove_dir_all(&subdir).unwrap();

    let insert = KeyEvent {
        code: KeyCode::Char('X'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(insert));

    let save = KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
    };
    let _ = workbench.handle_input(&InputEvent::Key(save));

    drive_until(&mut workbench, &rx, Duration::from_secs(2), |w| {
        w.logs
            .iter()
            .any(|line| line.contains("[fs:write_file]") && line.contains("a.txt"))
    });

    let tab = workbench
        .store
        .state()
        .editor
        .pane(0)
        .and_then(|p| p.active_tab())
        .expect("tab exists");
    assert!(tab.dirty);
}
