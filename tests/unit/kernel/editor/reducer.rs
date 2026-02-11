use super::goto_byte_offset;
use crate::core::Command;
use crate::kernel::editor::action::EditorAction;
use crate::kernel::editor::EditorState;
use crate::kernel::editor::EditorTabState;
use crate::kernel::editor::TabId;
use crate::kernel::editor::{DiskState, ReloadCause, ReloadRequest};
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::Effect;
use ropey::Rope;
use std::path::PathBuf;

#[test]
fn test_goto_byte_offset_eof_multibyte() {
    let config = EditorConfig::default();
    let mut tab = EditorTabState::untitled(TabId::new(1), &config);
    tab.buffer.set_rope(Rope::from_str("é"));

    let byte_offset = tab.buffer.rope().len_bytes();
    goto_byte_offset(&mut tab, byte_offset, config.tab_size);

    assert_eq!(tab.buffer.cursor(), (0, 1));
}

#[test]
fn test_saved_version_mismatch_does_not_clear_dirty() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("test.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "hello".to_string(),
    });

    let (changed, _) = editor.apply_command(0, Command::InsertChar('x'));
    assert!(changed);

    let (_, effects) = editor.apply_command(0, Command::Save);
    let version1 = match effects.as_slice() {
        [Effect::WriteFile { version, .. }] => *version,
        other => panic!("expected WriteFile effect, got {other:?}"),
    };

    let (changed, _) = editor.apply_command(0, Command::InsertChar('y'));
    assert!(changed);

    let (dirty, version2) = {
        let tab = editor.pane(0).unwrap().active_tab().unwrap();
        (tab.dirty, tab.edit_version)
    };
    assert!(dirty);
    assert!(version2 > version1);

    let _ = editor.dispatch_action(EditorAction::Saved {
        pane: 0,
        path: path.clone(),
        success: true,
        version: version1,
    });
    assert!(editor.pane(0).unwrap().active_tab().unwrap().dirty);

    let _ = editor.dispatch_action(EditorAction::Saved {
        pane: 0,
        path,
        success: true,
        version: version2,
    });
    assert!(!editor.pane(0).unwrap().active_tab().unwrap().dirty);
}

#[test]
fn test_apply_text_edit_swaps_and_clamps_byte_ranges() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("test.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "hello".to_string(),
    });

    let (changed, _) = editor.dispatch_action(EditorAction::ApplyTextEdit {
        pane: 0,
        start_byte: 999,
        end_byte: 0,
        text: "x".to_string(),
    });
    assert!(changed);
    assert_eq!(
        editor.pane(0).unwrap().active_tab().unwrap().buffer.text(),
        "x"
    );
}

#[test]
fn test_apply_text_edit_noop_on_empty_edit() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("test.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "hello".to_string(),
    });

    let (changed, effects) = editor.dispatch_action(EditorAction::ApplyTextEdit {
        pane: 0,
        start_byte: 2,
        end_byte: 2,
        text: String::new(),
    });
    assert!(!changed);
    assert!(effects.is_empty());
    assert_eq!(
        editor.pane(0).unwrap().active_tab().unwrap().buffer.text(),
        "hello"
    );
}

#[test]
fn test_apply_text_edit_inserts_at_eof_multibyte() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("test.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "é".to_string(),
    });

    let (changed, _) = editor.dispatch_action(EditorAction::ApplyTextEdit {
        pane: 0,
        start_byte: 100,
        end_byte: 100,
        text: "x".to_string(),
    });
    assert!(changed);
    assert_eq!(
        editor.pane(0).unwrap().active_tab().unwrap().buffer.text(),
        "éx"
    );
}

#[test]
fn test_reload_cause_manual_overwrites_dirty_but_external_sync_does_not() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("test.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "base".to_string(),
    });

    let (changed, _) = editor.apply_command(0, Command::InsertChar('x'));
    assert!(changed);

    let before = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists")
        .buffer
        .text();

    let (changed, effects) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "disk-external".to_string(),
        request: ReloadRequest {
            pane: 0,
            path: path.clone(),
            cause: ReloadCause::ExternalSync,
            request_id: 1,
        },
    });
    assert!(!changed);
    assert!(effects.is_empty());

    let tab_after_external = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert!(tab_after_external.dirty);
    assert_eq!(tab_after_external.buffer.text(), before);

    let (changed, effects) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "disk-manual".to_string(),
        request: ReloadRequest {
            pane: 0,
            path,
            cause: ReloadCause::ManualCommand,
            request_id: 2,
        },
    });
    assert!(changed);
    assert!(effects.is_empty());

    let tab_after_manual = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert!(!tab_after_manual.dirty);
    assert_eq!(tab_after_manual.buffer.text(), "disk-manual");
}

#[test]
fn test_file_reloaded_same_request_id_is_idempotent() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("test.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "base".to_string(),
    });

    let (changed, effects) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "disk-v1".to_string(),
        request: ReloadRequest {
            pane: 0,
            path: path.clone(),
            cause: ReloadCause::ExternalSync,
            request_id: 1,
        },
    });
    assert!(changed);
    assert!(effects.is_empty());

    let after_first = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    let version_after_first = after_first.edit_version;
    assert_eq!(after_first.buffer.text(), "disk-v1");

    let (changed, effects) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "disk-v2-should-be-ignored".to_string(),
        request: ReloadRequest {
            pane: 0,
            path,
            cause: ReloadCause::ExternalSync,
            request_id: 1,
        },
    });
    assert!(!changed);
    assert!(effects.is_empty());

    let after_second = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert_eq!(after_second.buffer.text(), "disk-v1");
    assert_eq!(after_second.edit_version, version_after_first);
}

#[test]
fn test_file_reloaded_lower_request_id_is_ignored() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("test.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "base".to_string(),
    });

    let (changed, _) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "disk-newer".to_string(),
        request: ReloadRequest {
            pane: 0,
            path: path.clone(),
            cause: ReloadCause::ExternalSync,
            request_id: 2,
        },
    });
    assert!(changed);

    let after_newer = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    let version_after_newer = after_newer.edit_version;
    assert_eq!(after_newer.buffer.text(), "disk-newer");

    let (changed, effects) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "disk-older-ignored".to_string(),
        request: ReloadRequest {
            pane: 0,
            path,
            cause: ReloadCause::ExternalSync,
            request_id: 1,
        },
    });
    assert!(!changed);
    assert!(effects.is_empty());

    let after_older = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert_eq!(after_older.buffer.text(), "disk-newer");
    assert_eq!(after_older.edit_version, version_after_newer);
}

#[test]
fn test_file_reloaded_ignores_mismatched_pane_or_path() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path_a = PathBuf::from("a.txt");
    let path_b = PathBuf::from("b.txt");

    assert!(editor.ensure_panes(2));
    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path_a.clone(),
        content: "base".to_string(),
    });

    let (changed, effects) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "ignored-by-pane".to_string(),
        request: ReloadRequest {
            pane: 1,
            path: path_a.clone(),
            cause: ReloadCause::ExternalSync,
            request_id: 1,
        },
    });
    assert!(!changed);
    assert!(effects.is_empty());

    let (changed, effects) = editor.dispatch_action(EditorAction::FileReloaded {
        content: "ignored-by-path".to_string(),
        request: ReloadRequest {
            pane: 0,
            path: path_b,
            cause: ReloadCause::ExternalSync,
            request_id: 2,
        },
    });
    assert!(!changed);
    assert!(effects.is_empty());

    let tab = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert_eq!(tab.buffer.text(), "base");
}

#[test]
fn test_file_externally_modified_emits_reload_for_clean_and_marks_dirty_conflict() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    let path = PathBuf::from("shared.txt");

    assert!(editor.ensure_panes(2));
    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "pane0".to_string(),
    });
    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 1,
        path: path.clone(),
        content: "pane1".to_string(),
    });

    let (changed, _) = editor.apply_command(0, Command::InsertChar('x'));
    assert!(changed);

    let (changed, effects) =
        editor.dispatch_action(EditorAction::FileExternallyModified { path: path.clone() });
    assert!(changed);

    let request = match effects.as_slice() {
        [Effect::ReloadFile(request)] => request,
        other => panic!("expected one ReloadFile effect, got {other:?}"),
    };
    assert_eq!(request.pane, 1);
    assert_eq!(request.path, path);
    assert_eq!(request.cause, ReloadCause::ExternalSync);
    assert_eq!(request.request_id, 1);

    let pane0_tab = editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("pane0 tab");
    assert!(matches!(
        pane0_tab.disk_state,
        DiskState::ConflictExternalModified
    ));

    let pane1_tab = editor
        .pane(1)
        .and_then(|pane| pane.active_tab())
        .expect("pane1 tab");
    assert!(matches!(pane1_tab.disk_state, DiskState::InSync));
}

#[test]
fn test_close_tabs_by_id_removes_requested_tabs() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);

    let path_a = PathBuf::from("a.txt");
    let path_b = PathBuf::from("b.txt");
    let path_c = PathBuf::from("c.txt");

    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path_a,
        content: "a".to_string(),
    });
    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path_b,
        content: "b".to_string(),
    });
    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane: 0,
        path: path_c,
        content: "c".to_string(),
    });

    let pane = editor.pane(0).expect("pane exists");
    assert_eq!(pane.tabs.len(), 3);
    let close_ids = vec![pane.tabs[0].id.raw(), pane.tabs[2].id.raw()];

    let (changed, effects) = editor.dispatch_action(EditorAction::CloseTabsById {
        pane: 0,
        tab_ids: close_ids,
    });

    assert!(changed);
    assert!(effects.is_empty());

    let pane = editor.pane(0).expect("pane exists");
    assert_eq!(pane.tabs.len(), 1);
    assert_eq!(pane.tabs[0].title, "b.txt");
}
