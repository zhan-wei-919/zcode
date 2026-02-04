use super::goto_byte_offset;
use crate::core::Command;
use crate::kernel::editor::action::EditorAction;
use crate::kernel::editor::EditorState;
use crate::kernel::editor::EditorTabState;
use crate::kernel::editor::TabId;
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
