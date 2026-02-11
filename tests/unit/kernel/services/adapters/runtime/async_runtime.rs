use super::*;
use crate::kernel::services::ports::{LspPosition, LspRange};
use tempfile::tempdir;

#[test]
fn apply_text_edits_to_path_rewrites_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.rs");
    std::fs::write(&path, "hello\nworld\n").unwrap();

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 5,
            },
        },
        new_text: "rust".to_string(),
    }];

    apply_text_edits_to_path(&path, &edits, LspPositionEncoding::Utf16).unwrap();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert_eq!(updated, "hello\nrust\n");
}

#[test]
fn apply_text_edits_to_rope_utf16_handles_emoji_columns() {
    let mut rope = Rope::from_str("a😀b\n");

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 3,
            },
            end: LspPosition {
                line: 0,
                character: 4,
            },
        },
        new_text: "c".to_string(),
    }];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf16);

    assert_eq!(rope.to_string(), "a😀c\n");
}

#[test]
fn apply_text_edits_to_rope_utf8_handles_emoji_columns() {
    let mut rope = Rope::from_str("a😀b\n");

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 5,
            },
            end: LspPosition {
                line: 0,
                character: 6,
            },
        },
        new_text: "c".to_string(),
    }];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf8);

    assert_eq!(rope.to_string(), "a😀c\n");
}

#[test]
fn apply_text_edits_to_rope_crlf_inserts_before_line_break() {
    let mut rope = Rope::from_str("a\r\nb\r\n");

    let edits = vec![LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 1,
            },
            end: LspPosition {
                line: 0,
                character: 1,
            },
        },
        new_text: "X".to_string(),
    }];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf16);

    assert_eq!(rope.to_string(), "aX\r\nb\r\n");
}

#[test]
fn apply_text_edits_to_rope_sorts_edits_from_bottom_to_top() {
    let mut rope = Rope::from_str("abcdef\n");

    let edits = vec![
        LspTextEdit {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 0,
                },
                end: LspPosition {
                    line: 0,
                    character: 2,
                },
            },
            new_text: "Y".to_string(),
        },
        LspTextEdit {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 2,
                },
                end: LspPosition {
                    line: 0,
                    character: 4,
                },
            },
            new_text: "X".to_string(),
        },
    ];

    apply_text_edits_to_rope(&mut rope, &edits, LspPositionEncoding::Utf16);

    assert_eq!(rope.to_string(), "YXef\n");
}

#[test]
fn move_path_rejects_overwrite_by_default() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    std::fs::write(&from, "FROM").unwrap();
    std::fs::write(&to, "TO").unwrap();

    let err = move_path(&from, &to, false).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);

    assert!(from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "TO");
}

#[test]
fn move_path_overwrite_replaces_destination() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    std::fs::write(&from, "FROM").unwrap();
    std::fs::write(&to, "TO").unwrap();

    move_path(&from, &to, true).unwrap();

    assert!(!from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "FROM");
}

#[test]
fn move_path_cross_device_falls_back_to_copy_remove() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    std::fs::write(&from, "FROM").unwrap();

    move_path_impl(&from, &to, false, |_from, _to| {
        Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices))
    })
    .unwrap();

    assert!(!from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "FROM");
}

#[test]
fn move_dir_cross_device_falls_back_to_copy_remove() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("src");
    let to = dir.path().join("dst");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join("a.txt"), "hello").unwrap();

    move_path_impl(&from, &to, false, |_from, _to| {
        Err(std::io::Error::from(std::io::ErrorKind::CrossesDevices))
    })
    .unwrap();

    assert!(!from.exists());
    assert_eq!(std::fs::read_to_string(to.join("a.txt")).unwrap(), "hello");
}

#[test]
fn copy_path_rejects_overwrite_by_default() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    std::fs::write(&from, "FROM").unwrap();
    std::fs::write(&to, "TO").unwrap();

    let err = copy_path(&from, &to, false).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);

    assert!(from.exists());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "TO");
}

#[test]
fn copy_path_overwrite_replaces_destination_and_keeps_source() {
    let dir = tempdir().unwrap();
    let from = dir.path().join("from.txt");
    let to = dir.path().join("to.txt");
    std::fs::write(&from, "FROM").unwrap();
    std::fs::write(&to, "TO").unwrap();

    copy_path(&from, &to, true).unwrap();

    assert!(from.exists());
    assert_eq!(std::fs::read_to_string(&from).unwrap(), "FROM");
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "FROM");
}

#[cfg(feature = "terminal")]
#[test]
fn terminal_ops_do_not_panic_when_terminals_lock_is_poisoned() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let runtime = AsyncRuntime::new(tx).expect("runtime");

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = runtime.terminals.lock().expect("terminals lock");
        panic!("poison terminals lock");
    }));

    assert!(runtime.terminals.lock().is_err(), "lock should be poisoned");

    let write_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.terminal_write(1, b"echo hi".to_vec());
    }));
    assert!(write_result.is_ok(), "terminal_write should not panic");

    let resize_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.terminal_resize(1, 80, 24);
    }));
    assert!(resize_result.is_ok(), "terminal_resize should not panic");

    let kill_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.terminal_kill(1);
    }));
    assert!(kill_result.is_ok(), "terminal_kill should not panic");
}
