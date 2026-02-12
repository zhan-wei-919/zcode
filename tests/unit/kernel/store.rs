use super::completion_strategy;
use super::*;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::{
    LspCompletionTriggerKind, LspPosition, LspRange, LspTextEdit, LspWorkspaceEdit,
    LspWorkspaceFileEdit,
};
use crate::kernel::state::{
    CompletionRequestContext, ContextMenuRequest, PendingAction, PendingEditorNavigation,
    PendingEditorNavigationTarget,
};
use crate::models::{FileTree, Granularity, Selection};
use std::ffi::OsString;
use std::time::Instant;
use tempfile::tempdir;

fn new_store() -> Store {
    let root = std::env::temp_dir();
    let tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    Store::new(AppState::new(root, tree, EditorConfig::default()))
}

fn test_completion_item(id: u64, label: &str) -> LspCompletionItem {
    LspCompletionItem {
        id,
        label: label.to_string(),
        detail: None,
        kind: None,
        documentation: None,
        insert_text: label.to_string(),
        insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
        insert_range: None,
        replace_range: None,
        sort_text: None,
        filter_text: None,
        additional_text_edits: Vec::new(),
        command: None,
        data: None,
    }
}

fn seed_visible_completion_for_active_tab(
    store: &mut Store,
    pane: usize,
    path: std::path::PathBuf,
    label: &str,
) {
    let version = store
        .state
        .editor
        .pane(pane)
        .unwrap()
        .active_tab()
        .unwrap()
        .edit_version;
    let item = test_completion_item(1, label);

    store.state.ui.completion.visible = true;
    store.state.ui.completion.selected = 0;
    store.state.ui.completion.all_items = vec![item];
    store.state.ui.completion.visible_indices = vec![0];
    store.state.ui.completion.request = Some(CompletionRequestContext {
        pane,
        path,
        version,
    });
    store.state.ui.completion.pending_request = None;
    store.state.ui.completion.is_incomplete = false;
}

#[test]
fn escape_opens_settings_when_idle_in_editor() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;

    let result = store.dispatch(Action::RunCommand(Command::Escape));

    assert!(matches!(result.effects.as_slice(), [Effect::OpenSettings]));
    assert!(!result.state_changed);
}

#[test]
fn escape_closes_palette_first() {
    let mut store = new_store();
    store.state.ui.command_palette.visible = true;
    store.state.ui.command_palette.query = "x".to_string();
    store.state.ui.command_palette.selected = 1;
    store.state.ui.focus = FocusTarget::CommandPalette;

    let result = store.dispatch(Action::RunCommand(Command::Escape));

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(!store.state.ui.command_palette.visible);
    assert!(store.state.ui.command_palette.query.is_empty());
    assert_eq!(store.state.ui.command_palette.selected, 0);
    assert_eq!(store.state.ui.focus, FocusTarget::Editor);
}

#[test]
fn escape_focuses_editor_when_in_other_panel() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Explorer;

    let result = store.dispatch(Action::RunCommand(Command::Escape));

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert_eq!(store.state.ui.focus, FocusTarget::Editor);
}

#[test]
fn escape_closes_editor_search_bar() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    store.state.editor.pane_mut(0).unwrap().search_bar.visible = true;

    let result = store.dispatch(Action::RunCommand(Command::Escape));

    assert!(matches!(
        result.effects.as_slice(),
        [Effect::CancelEditorSearch { pane: 0 }]
    ));
    assert!(result.state_changed);
    assert!(!store.state.editor.pane(0).unwrap().search_bar.visible);
}

#[test]
fn escape_clears_editor_selection_before_opening_settings() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("test.txt");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "hello".to_string(),
    }));
    let tab = store
        .state
        .editor
        .pane_mut(0)
        .unwrap()
        .active_tab_mut()
        .unwrap();
    tab.buffer
        .set_selection(Some(Selection::new((0, 0), Granularity::Char)));

    let result = store.dispatch(Action::RunCommand(Command::Escape));

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(store
        .state
        .editor
        .pane(0)
        .unwrap()
        .active_tab()
        .unwrap()
        .buffer
        .selection()
        .is_none());
}

#[test]
fn terminal_tab_spawns_session_on_activation() {
    let mut store = new_store();
    let result = store.dispatch(Action::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });

    assert!(store.state.ui.bottom_panel.visible);
    assert_eq!(
        store.state.ui.bottom_panel.active_tab,
        BottomPanelTab::Terminal
    );
    assert_eq!(store.state.terminal.active, Some(1));
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::TerminalSpawn { id: 1, .. }]
    ));
}

#[test]
fn bottom_panel_height_ratio_clamps_to_valid_range() {
    let mut store = new_store();

    let result = store.dispatch(Action::BottomPanelSetHeightRatio { ratio: 1 });
    assert!(result.state_changed);
    assert_eq!(store.state.ui.bottom_panel.height_ratio, 100);

    let result = store.dispatch(Action::BottomPanelSetHeightRatio { ratio: 999 });
    assert!(result.state_changed);
    assert_eq!(store.state.ui.bottom_panel.height_ratio, 900);

    let result = store.dispatch(Action::BottomPanelSetHeightRatio { ratio: 900 });
    assert!(!result.state_changed);
    assert_eq!(store.state.ui.bottom_panel.height_ratio, 900);
}

#[test]
fn terminal_write_requires_session() {
    let mut store = new_store();
    let result = store.dispatch(Action::TerminalWrite {
        id: 42,
        bytes: b"ls\n".to_vec(),
    });
    assert!(result.effects.is_empty());

    let _ = store.dispatch(Action::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });
    let id = store.state.terminal.active.expect("session");
    let result = store.dispatch(Action::TerminalWrite {
        id,
        bytes: b"pwd\n".to_vec(),
    });
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::TerminalWrite { id: _, .. }]
    ));
}

#[test]
fn terminal_resize_is_idempotent() {
    let mut store = new_store();
    let _ = store.dispatch(Action::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });
    let id = store.state.terminal.active.expect("session");

    let result = store.dispatch(Action::TerminalResize {
        id,
        cols: 80,
        rows: 24,
    });
    assert!(result.effects.is_empty());

    let result = store.dispatch(Action::TerminalResize {
        id,
        cols: 100,
        rows: 40,
    });
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::TerminalResize { id: _, .. }]
    ));
    let session = store.state.terminal.session_mut(id).unwrap();
    assert_eq!(session.cols, 100);
    assert_eq!(session.rows, 40);
}

#[test]
fn terminal_output_marks_dirty_and_exit_kills() {
    let mut store = new_store();
    let _ = store.dispatch(Action::BottomPanelSetActiveTab {
        tab: BottomPanelTab::Terminal,
    });
    let id = store.state.terminal.active.expect("session");

    let result = store.dispatch(Action::TerminalOutput {
        id,
        bytes: b"hi\n".to_vec(),
    });
    assert!(result.state_changed);
    assert!(store.state.terminal.session_mut(id).unwrap().dirty);

    let result = store.dispatch(Action::TerminalExited { id, code: Some(0) });
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::TerminalKill { id: _ }]
    ));
    assert!(store.state.terminal.session_mut(id).unwrap().exited);
}

#[test]
fn explorer_new_file_flow_creates_effect() {
    let mut store = new_store();
    let result = store.dispatch(Action::RunCommand(Command::ExplorerNewFile));
    assert!(result.effects.is_empty());
    assert!(store.state.ui.input_dialog.visible);

    let _ = store.dispatch(Action::InputDialogAppend('x'));
    let result = store.dispatch(Action::InputDialogAccept);
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::CreateFile(path)] if path.ends_with("x")
    ));
    assert!(!store.state.ui.input_dialog.visible);
}

#[test]
fn explorer_move_path_rejects_out_of_workspace_paths() {
    let ws = tempdir().unwrap();
    let outside = tempdir().unwrap();

    let root = ws.path().to_path_buf();
    let tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));

    let result = store.dispatch(Action::ExplorerMovePath {
        from: outside.path().join("from.txt"),
        to: root.join("to.txt"),
    });
    assert!(result.effects.is_empty());

    let result = store.dispatch(Action::ExplorerMovePath {
        from: root.join("from2.txt"),
        to: outside.path().join("to2.txt"),
    });
    assert!(result.effects.is_empty());
}

#[test]
fn confirm_dialog_rejects_out_of_workspace_file_operations() {
    let ws = tempdir().unwrap();
    let outside = tempdir().unwrap();

    let root = ws.path().to_path_buf();
    let tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));

    let outside_path = outside.path().join("x.txt");
    let _ = store.dispatch(Action::ShowConfirmDialog {
        message: "delete".to_string(),
        on_confirm: PendingAction::DeletePath {
            path: outside_path.clone(),
            is_dir: false,
        },
    });
    let result = store.dispatch(Action::ConfirmDialogAccept);
    assert!(result.effects.is_empty());

    let _ = store.dispatch(Action::ShowConfirmDialog {
        message: "rename".to_string(),
        on_confirm: PendingAction::RenamePath {
            from: outside_path,
            to: root.join("dst.txt"),
            overwrite: true,
        },
    });
    let result = store.dispatch(Action::ConfirmDialogAccept);
    assert!(result.effects.is_empty());
}

#[test]
fn auto_closes_split_when_second_pane_becomes_empty_after_move_tab() {
    let mut store = new_store();

    let a = store.state.workspace_root.join("a.rs");
    let b = store.state.workspace_root.join("b.rs");

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: a,
        content: "a".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::SplitEditorVertical));
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 1,
        path: b,
        content: "b".to_string(),
    }));

    assert_eq!(store.state.ui.editor_layout.panes, 2);
    assert_eq!(store.state.editor.panes.len(), 2);

    let tab_id = store.state.editor.pane(1).unwrap().tabs[0].id;
    let result = store.dispatch(Action::Editor(EditorAction::MoveTab {
        tab_id,
        from_pane: 1,
        to_pane: 0,
        to_index: 1,
    }));

    assert!(result.state_changed);
    assert_eq!(store.state.ui.editor_layout.panes, 1);
    assert_eq!(store.state.editor.panes.len(), 1);

    let titles: Vec<_> = store
        .state
        .editor
        .pane(0)
        .unwrap()
        .tabs
        .iter()
        .map(|t| t.title.as_str())
        .collect();
    assert!(titles.contains(&"a.rs"));
    assert!(titles.contains(&"b.rs"));
}

#[test]
fn auto_closes_split_when_first_pane_becomes_empty_after_move_tab() {
    let mut store = new_store();

    let a = store.state.workspace_root.join("a.rs");
    let b = store.state.workspace_root.join("b.rs");

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: a,
        content: "a".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::SplitEditorVertical));
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 1,
        path: b,
        content: "b".to_string(),
    }));

    let tab_id = store.state.editor.pane(0).unwrap().tabs[0].id;
    let to_index = store.state.editor.pane(1).unwrap().tabs.len();
    let result = store.dispatch(Action::Editor(EditorAction::MoveTab {
        tab_id,
        from_pane: 0,
        to_pane: 1,
        to_index,
    }));

    assert!(result.state_changed);
    assert_eq!(store.state.ui.editor_layout.panes, 1);
    assert_eq!(store.state.ui.editor_layout.active_pane, 0);
    assert_eq!(
        store.state.ui.editor_layout.split_direction,
        SplitDirection::Vertical
    );
    assert_eq!(store.state.editor.panes.len(), 1);

    let pane = store.state.editor.pane(0).unwrap();
    assert_eq!(pane.tabs.len(), 2);
    assert!(pane.active < pane.tabs.len());

    let titles: Vec<_> = pane.tabs.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"a.rs"));
    assert!(titles.contains(&"b.rs"));
}

#[test]
fn auto_closes_split_when_last_tab_in_second_pane_is_closed() {
    let mut store = new_store();

    let a = store.state.workspace_root.join("a.rs");
    let b = store.state.workspace_root.join("b.rs");

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: a,
        content: "a".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::SplitEditorVertical));
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 1,
        path: b,
        content: "b".to_string(),
    }));

    let result = store.dispatch(Action::Editor(EditorAction::CloseTabAt {
        pane: 1,
        index: 0,
    }));
    assert!(result.state_changed);
    assert_eq!(store.state.ui.editor_layout.panes, 1);
    assert_eq!(store.state.editor.panes.len(), 1);
    assert_eq!(store.state.editor.pane(0).unwrap().tabs.len(), 1);
    assert_eq!(store.state.editor.pane(0).unwrap().tabs[0].title, "a.rs");
}

#[test]
fn explorer_context_menu_root_items_include_disabled_actions() {
    let mut store = new_store();

    let result = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Explorer { tree_row: None },
        x: 10,
        y: 5,
    });

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(store.state.ui.context_menu.visible);

    let items = &store.state.ui.context_menu.items;
    assert!(matches!(
        items.first(),
        Some(item) if item.label == "New File" && item.is_selectable()
    ));
    assert!(matches!(
        items.get(1),
        Some(item) if item.label == "New Folder" && item.is_selectable()
    ));
    assert!(items.iter().any(|item| item.is_separator()));
    assert!(items
        .iter()
        .any(|item| item.label == "Rename" && !item.is_selectable()));
}

#[test]
fn explorer_context_menu_confirm_rename_opens_rename_dialog() {
    let root = std::env::temp_dir();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let file_id = tree
        .insert_child(
            tree.root(),
            OsString::from("a.txt"),
            crate::models::NodeKind::File,
        )
        .unwrap();

    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));
    let tree_row = store
        .state
        .explorer
        .rows
        .iter()
        .position(|row| row.id == file_id)
        .unwrap();

    let _ = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Explorer {
            tree_row: Some(tree_row),
        },
        x: 10,
        y: 5,
    });

    let rename_index = store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .position(|item| item.label == "Rename")
        .expect("rename item exists");

    let _ = store.dispatch(Action::ContextMenuSetSelected {
        index: rename_index,
    });
    let result = store.dispatch(Action::ContextMenuConfirm);
    assert!(result.effects.is_empty());
    assert!(store.state.ui.input_dialog.visible);
    assert!(matches!(
        store.state.ui.input_dialog.kind,
        Some(InputDialogKind::ExplorerRename { .. })
    ));

    store.state.ui.input_dialog.value = "b.txt".to_string();
    store.state.ui.input_dialog.cursor = store.state.ui.input_dialog.value.len();
    let result = store.dispatch(Action::InputDialogAccept);
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::RenamePath {
            from,
            to,
            overwrite: false
        }]
            if from == &root.join("a.txt") && to == &root.join("b.txt")
    ));
}

#[test]
fn explorer_context_menu_confirm_copy_path_sets_clipboard_text() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let file_id = tree
        .insert_child(
            tree.root(),
            OsString::from("a.txt"),
            crate::models::NodeKind::File,
        )
        .unwrap();

    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));
    let tree_row = store
        .state
        .explorer
        .rows
        .iter()
        .position(|row| row.id == file_id)
        .unwrap();

    let _ = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Explorer {
            tree_row: Some(tree_row),
        },
        x: 10,
        y: 5,
    });

    let copy_path_index = store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .position(|item| item.label == "Copy Path")
        .expect("copy path item exists");

    let _ = store.dispatch(Action::ContextMenuSetSelected {
        index: copy_path_index,
    });
    let result = store.dispatch(Action::ContextMenuConfirm);

    assert!(result.state_changed);
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::SetClipboardText(text)] if text == &root.join("a.txt").to_string_lossy()
    ));
}

#[test]
fn explorer_context_menu_confirm_copy_relative_path_sets_clipboard_text() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let file_id = tree
        .insert_child(
            tree.root(),
            OsString::from("a.txt"),
            crate::models::NodeKind::File,
        )
        .unwrap();

    let mut store = Store::new(AppState::new(root, tree, EditorConfig::default()));
    let tree_row = store
        .state
        .explorer
        .rows
        .iter()
        .position(|row| row.id == file_id)
        .unwrap();

    let _ = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Explorer {
            tree_row: Some(tree_row),
        },
        x: 10,
        y: 5,
    });

    let copy_rel_index = store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .position(|item| item.label == "Copy Relative Path")
        .expect("copy relative path item exists");

    let _ = store.dispatch(Action::ContextMenuSetSelected {
        index: copy_rel_index,
    });
    let result = store.dispatch(Action::ContextMenuConfirm);

    assert!(result.state_changed);
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::SetClipboardText(text)] if text == "a.txt"
    ));
}

#[test]
fn tab_context_menu_open_sets_state() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;

    let path = store.state.workspace_root.join("tab.txt");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "hello".to_string(),
    }));

    let result = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Tab { pane: 0, index: 0 },
        x: 10,
        y: 5,
    });

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(store.state.ui.context_menu.visible);
    assert!(store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .any(|item| item.label == "Close" && item.is_selectable()));
    assert_eq!(
        store.state.ui.context_menu.request,
        Some(ContextMenuRequest::Tab { pane: 0, index: 0 })
    );
}

#[test]
fn editor_area_context_menu_open_sets_items() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;

    let result = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::EditorArea { pane: 0 },
        x: 10,
        y: 5,
    });

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(store.state.ui.context_menu.visible);
    let labels = store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .map(|item| item.label)
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Copy"));
    assert!(labels.contains(&"Paste"));
    assert!(labels.contains(&"Go to Definition"));
    assert_eq!(
        store.state.ui.context_menu.request,
        Some(ContextMenuRequest::EditorArea { pane: 0 })
    );
}

#[test]
fn tab_context_menu_confirm_close_closes_tab_when_clean() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;

    let path = store.state.workspace_root.join("tab.txt");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "hello".to_string(),
    }));
    assert!(!store.state.editor.pane(0).unwrap().is_tab_dirty(0));

    let _ = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Tab { pane: 0, index: 0 },
        x: 10,
        y: 5,
    });

    let close_index = store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .position(|item| item.label == "Close")
        .expect("close item exists");
    let _ = store.dispatch(Action::ContextMenuSetSelected { index: close_index });
    let result = store.dispatch(Action::ContextMenuConfirm);

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(!store.state.ui.context_menu.visible);
    assert_eq!(store.state.editor.pane(0).unwrap().tabs.len(), 0);
}

#[test]
fn tab_context_menu_confirm_close_shows_confirm_dialog_when_dirty() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;

    let path = store.state.workspace_root.join("tab.txt");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "hello".to_string(),
    }));
    let _ = store.dispatch(Action::Editor(EditorAction::InsertText {
        pane: 0,
        text: "x".to_string(),
    }));
    assert!(store.state.editor.pane(0).unwrap().is_tab_dirty(0));

    let _ = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Tab { pane: 0, index: 0 },
        x: 10,
        y: 5,
    });

    let close_index = store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .position(|item| item.label == "Close")
        .expect("close item exists");
    let _ = store.dispatch(Action::ContextMenuSetSelected { index: close_index });
    let result = store.dispatch(Action::ContextMenuConfirm);

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(store.state.ui.confirm_dialog.visible);
    assert!(matches!(
        store.state.ui.confirm_dialog.on_confirm,
        Some(PendingAction::CloseTabsBatch { pane: 0, ref tab_ids }) if tab_ids.len() == 1
    ));
    assert_eq!(store.state.editor.pane(0).unwrap().tabs.len(), 1);
}

#[test]
fn explorer_delete_confirm_produces_delete_effect() {
    let root = std::env::temp_dir();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let file_id = tree
        .insert_child(
            tree.root(),
            OsString::from("to_delete.txt"),
            crate::models::NodeKind::File,
        )
        .unwrap();
    tree.set_selected(Some(file_id));

    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));
    let result = store.dispatch(Action::RunCommand(Command::ExplorerDelete));
    assert!(result.effects.is_empty());
    assert!(store.state.ui.confirm_dialog.visible);

    let result = store.dispatch(Action::ConfirmDialogAccept);
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::DeletePath { path, is_dir: false }] if path.ends_with("to_delete.txt")
    ));
}

#[test]
fn open_file_applies_pending_editor_nav_byte_offset_and_clears_it() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("test.txt");
    let content = "a😀b".to_string();
    let byte_offset_after_emoji = "a😀".len();
    store.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
        pane: 0,
        path: path.clone(),
        target: PendingEditorNavigationTarget::ByteOffset {
            byte_offset: byte_offset_after_emoji,
        },
    });

    let result = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content,
    }));

    assert!(result.state_changed);
    assert!(result.effects.is_empty());
    assert!(store.state.ui.pending_editor_nav.is_none());
    assert_eq!(
        store
            .state
            .editor
            .pane(0)
            .unwrap()
            .active_tab()
            .unwrap()
            .buffer
            .cursor(),
        (0, 2)
    );
}

#[test]
fn open_file_applies_pending_editor_nav_line_column_utf16_and_clears_it() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("test.txt");
    store.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
        pane: 0,
        path: path.clone(),
        target: PendingEditorNavigationTarget::LineColumn { line: 0, column: 3 },
    });

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "a😀b".to_string(),
    }));

    assert!(store.state.ui.pending_editor_nav.is_none());
    assert_eq!(
        store
            .state
            .editor
            .pane(0)
            .unwrap()
            .active_tab()
            .unwrap()
            .buffer
            .cursor(),
        (0, 2)
    );
}

#[test]
fn open_file_does_not_consume_pending_nav_for_other_path() {
    let mut store = new_store();
    let pending_path = store.state.workspace_root.join("pending.txt");
    store.state.ui.pending_editor_nav = Some(PendingEditorNavigation {
        pane: 0,
        path: pending_path,
        target: PendingEditorNavigationTarget::ByteOffset { byte_offset: 1 },
    });

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: store.state.workspace_root.join("actual.txt"),
        content: "hello".to_string(),
    }));

    assert!(store.state.ui.pending_editor_nav.is_some());
    assert_eq!(
        store
            .state
            .editor
            .pane(0)
            .unwrap()
            .active_tab()
            .unwrap()
            .buffer
            .cursor(),
        (0, 0)
    );
}

#[test]
fn insert_dot_in_rust_triggers_lsp_completion_request() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() {}\n".to_string(),
    }));

    let result = store.dispatch(Action::RunCommand(Command::InsertChar('.')));

    assert!(result.effects.iter().any(|e| {
        matches!(
            e,
            Effect::LspCompletionRequest {
                path: p,
                trigger,
                ..
            } if p == &path
                && trigger.kind == LspCompletionTriggerKind::TriggerCharacter
                && trigger.character == Some('.')
        )
    }));
    let req = store
        .state
        .ui
        .completion
        .pending_request
        .as_ref()
        .expect("request set");
    assert_eq!(req.path, path);
}

#[test]
fn insert_double_colon_in_rust_triggers_lsp_completion_request() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() {}\n".to_string(),
    }));

    let first = store.dispatch(Action::RunCommand(Command::InsertChar(':')));
    assert!(!first
        .effects
        .iter()
        .any(|e| matches!(e, Effect::LspCompletionRequest { .. })));
    assert!(store.state.ui.completion.pending_request.is_none());
    assert!(store.state.ui.completion.request.is_none());

    let second = store.dispatch(Action::RunCommand(Command::InsertChar(':')));
    assert!(second.effects.iter().any(|e| {
        matches!(
            e,
            Effect::LspCompletionRequest {
                path: p,
                trigger,
                ..
            } if p == &path
                && trigger.kind == LspCompletionTriggerKind::TriggerCharacter
                && trigger.character == Some(':')
        )
    }));
    let req = store
        .state
        .ui
        .completion
        .pending_request
        .as_ref()
        .expect("request set");
    assert_eq!(req.path, path);
}

#[test]
fn insert_dot_in_non_rust_does_not_trigger_lsp_completion_request() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.txt");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "hello\n".to_string(),
    }));

    let result = store.dispatch(Action::RunCommand(Command::InsertChar('.')));

    assert!(!result
        .effects
        .iter()
        .any(|e| matches!(e, Effect::LspCompletionRequest { .. })));
    assert!(store.state.ui.completion.pending_request.is_none());
    assert!(store.state.ui.completion.request.is_none());
}

#[test]
fn command_lsp_completion_uses_invoked_trigger_context() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() { pri }\n".to_string(),
    }));

    let result = store.dispatch(Action::RunCommand(Command::LspCompletion));

    assert!(result.effects.iter().any(|e| {
        matches!(
            e,
            Effect::LspCompletionRequest {
                path: p,
                trigger,
                ..
            } if p == &path
                && trigger.kind == LspCompletionTriggerKind::Invoked
                && trigger.character.is_none()
        )
    }));
}

#[test]
fn lsp_completion_items_are_filtered_and_sorted() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "prin\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let items = vec![
        LspCompletionItem {
            id: 1,
            label: "self::".to_string(),
            detail: None,
            kind: None,
            documentation: None,
            insert_text: "self::".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("2".to_string()),
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
        LspCompletionItem {
            id: 2,
            label: "Alignment".to_string(),
            detail: None,
            kind: None,
            documentation: None,
            insert_text: "Alignment".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("1".to_string()),
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
        LspCompletionItem {
            id: 3,
            label: "println!".to_string(),
            detail: None,
            kind: None,
            documentation: None,
            insert_text: "println!".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("0".to_string()),
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
        LspCompletionItem {
            id: 4,
            label: "Print".to_string(),
            detail: None,
            kind: None,
            documentation: None,
            insert_text: "Print".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("9".to_string()),
            filter_text: Some("Print".to_string()),
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
    ];

    let _ = store.dispatch(Action::LspCompletion {
        items,
        is_incomplete: false,
    });

    assert!(store.state.ui.completion.visible);
    assert_eq!(store.state.ui.completion.visible_len(), 2);
    assert_eq!(
        store
            .state
            .ui
            .completion
            .visible_item(0)
            .map(|item| item.label.as_str()),
        Some("println!")
    );
    assert_eq!(
        store
            .state
            .ui
            .completion
            .visible_item(1)
            .map(|item| item.label.as_str()),
        Some("Print")
    );
}

#[test]
fn completion_selected_id_stays_stable_when_visible_indices_change() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "pri\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let items = vec![
        LspCompletionItem {
            id: 1,
            label: "print".to_string(),
            detail: None,
            kind: None,
            documentation: Some("doc".to_string()),
            insert_text: "print".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("2".to_string()),
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
        LspCompletionItem {
            id: 2,
            label: "println!".to_string(),
            detail: None,
            kind: None,
            documentation: Some("doc".to_string()),
            insert_text: "println!".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("1".to_string()),
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
        LspCompletionItem {
            id: 3,
            label: "probe".to_string(),
            detail: None,
            kind: None,
            documentation: Some("doc".to_string()),
            insert_text: "probe".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("0".to_string()),
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
        LspCompletionItem {
            id: 4,
            label: "prio".to_string(),
            detail: None,
            kind: None,
            documentation: Some("doc".to_string()),
            insert_text: "prio".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: Some("3".to_string()),
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        },
    ];

    let _ = store.dispatch(Action::LspCompletion {
        items,
        is_incomplete: false,
    });

    let selected = store
        .state
        .ui
        .completion
        .visible_indices
        .iter()
        .position(|idx| store.state.ui.completion.all_items[*idx].id == 1)
        .expect("print should be visible");
    store.state.ui.completion.selected = selected;
    let before_id = store
        .state
        .ui
        .completion
        .selected_item()
        .map(|item| item.id);
    assert_eq!(before_id, Some(1));

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('n')));

    assert_eq!(
        store
            .state
            .ui
            .completion
            .selected_item()
            .map(|item| item.id),
        Some(1)
    );
    assert_eq!(store.state.ui.completion.visible_len(), 2);
}

#[test]
fn lsp_completion_resolve_updates_insert_payload_fields() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() { pri }\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let _ = store.dispatch(Action::LspCompletion {
        items: vec![LspCompletionItem {
            id: 1,
            label: "print".to_string(),
            detail: None,
            kind: None,
            documentation: None,
            insert_text: "print".to_string(),
            insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        }],
        is_incomplete: false,
    });

    let insert_range = LspRange {
        start: LspPosition {
            line: 0,
            character: 11,
        },
        end: LspPosition {
            line: 0,
            character: 14,
        },
    };
    let replace_range = LspRange {
        start: LspPosition {
            line: 0,
            character: 11,
        },
        end: LspPosition {
            line: 0,
            character: 18,
        },
    };

    let _ = store.dispatch(Action::LspCompletionResolved {
        id: 1,
        detail: None,
        documentation: None,
        insert_text: Some("print(${1:value})$0".to_string()),
        insert_text_format: Some(crate::kernel::services::ports::LspInsertTextFormat::Snippet),
        insert_range: Some(insert_range),
        replace_range: Some(replace_range),
        additional_text_edits: Vec::new(),
        command: None,
    });

    let item = store
        .state
        .ui
        .completion
        .visible_item(0)
        .expect("completion item");
    assert_eq!(item.insert_text, "print(${1:value})$0");
    assert_eq!(
        item.insert_text_format,
        crate::kernel::services::ports::LspInsertTextFormat::Snippet
    );
    assert!(item.insert_range.is_some());
    assert!(item.replace_range.is_some());
}

#[test]
fn completion_does_not_close_on_viewport_resize() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() { pri }\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let items = vec![LspCompletionItem {
        id: 1,
        label: "println!".to_string(),
        detail: None,
        kind: None,
        documentation: None,
        insert_text: "println!".to_string(),
        insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
        insert_range: None,
        replace_range: None,
        sort_text: None,
        filter_text: None,
        additional_text_edits: Vec::new(),
        command: None,
        data: None,
    }];

    let _ = store.dispatch(Action::LspCompletion {
        items,
        is_incomplete: false,
    });

    assert!(store.state.ui.completion.visible);
    assert!(store.state.ui.completion.visible_len() > 0);

    let _ = store.dispatch(Action::Editor(EditorAction::SetViewportSize {
        pane: 0,
        width: 80,
        height: 20,
    }));

    assert!(store.state.ui.completion.visible);
    assert!(store.state.ui.completion.visible_len() > 0);
}

#[test]
fn completion_does_not_close_on_editor_search_messages() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() { pri }\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let items = vec![LspCompletionItem {
        id: 1,
        label: "println!".to_string(),
        detail: None,
        kind: None,
        documentation: None,
        insert_text: "println!".to_string(),
        insert_text_format: crate::kernel::services::ports::LspInsertTextFormat::PlainText,
        insert_range: None,
        replace_range: None,
        sort_text: None,
        filter_text: None,
        additional_text_edits: Vec::new(),
        command: None,
        data: None,
    }];

    let _ = store.dispatch(Action::LspCompletion {
        items,
        is_incomplete: false,
    });

    assert!(store.state.ui.completion.visible);
    assert!(store.state.ui.completion.visible_len() > 0);

    let search_id = 7u64;
    let _ = store.dispatch(Action::Editor(EditorAction::SearchStarted {
        pane: 0,
        search_id,
    }));
    let _ = store.dispatch(Action::Editor(EditorAction::SearchMessage {
        pane: 0,
        message: crate::kernel::services::ports::SearchMessage::Complete {
            search_id,
            total: 0,
        },
    }));

    assert!(store.state.ui.completion.visible);
    assert!(store.state.ui.completion.visible_len() > 0);
}

#[test]
fn cpp_include_path_context_allows_completion() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#include <vec".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: content.clone(),
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer.set_cursor(0, content.chars().count());
    }

    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let strategy = completion_strategy::strategy_for_tab(tab);
    assert!(strategy.context_allows_completion(tab));
}

#[test]
fn cpp_include_trailing_comment_context_is_blocked() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#include <vector> // trailing note".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: content.clone(),
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer.set_cursor(0, content.chars().count());
    }

    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let strategy = completion_strategy::strategy_for_tab(tab);
    assert!(!strategy.context_allows_completion(tab));
}

#[test]
fn cpp_include_trailing_comment_does_not_keep_completion_open() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#include <vector> // trailing note".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: content.clone(),
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer.set_cursor(0, content.chars().count());
    }

    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let strategy = completion_strategy::strategy_for_tab(tab);
    assert!(!strategy.completion_should_keep_open(tab));
}

#[test]
fn cpp_insert_slash_outside_include_closes_completion_popup() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "int value = lhs rhs;".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content,
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer.set_cursor(0, "int value = lhs ".chars().count());
    }

    seed_visible_completion_for_active_tab(&mut store, 0, path, "lhs");

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('/')));

    assert!(!store.state.ui.completion.visible);
    assert_eq!(store.state.ui.completion.visible_len(), 0);
    assert!(store.state.ui.completion.request.is_none());
}

#[test]
fn non_cpp_languages_insert_slash_close_completion_popup() {
    let cases = [
        ("main.rs", "let value = lhs rhs;", "let value = lhs "),
        ("main.py", "value = lhs rhs", "value = lhs "),
        ("main.ts", "const value = lhs rhs;", "const value = lhs "),
    ];

    for (filename, content, before_rhs) in cases {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let path = store.state.workspace_root.join(filename);

        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane: 0,
            path: path.clone(),
            content: content.to_string(),
        }));

        {
            let tab = store
                .state
                .editor
                .pane_mut(0)
                .unwrap()
                .active_tab_mut()
                .unwrap();
            tab.buffer.set_cursor(0, before_rhs.chars().count());
        }

        seed_visible_completion_for_active_tab(&mut store, 0, path, "lhs");

        let _ = store.dispatch(Action::RunCommand(Command::InsertChar('/')));

        assert!(
            !store.state.ui.completion.visible,
            "completion popup should close on '/' for {filename}"
        );
        assert!(
            store.state.ui.completion.request.is_none(),
            "completion request should clear for {filename}"
        );
    }
}

#[test]
fn cpp_insert_gt_after_dash_keeps_open_and_triggers() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "auto value = obj-".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: content.clone(),
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer.set_cursor(0, content.chars().count());
    }

    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let strategy = completion_strategy::strategy_for_tab(tab);

    assert!(!strategy.should_close_on_command(&Command::InsertChar('>'), Some(tab)));

    let result = store.dispatch(Action::RunCommand(Command::InsertChar('>')));
    assert!(result.effects.iter().any(|e| {
        matches!(
            e,
            Effect::LspCompletionRequest { trigger, .. }
                if trigger.kind == LspCompletionTriggerKind::TriggerCharacter
                    && trigger.character == Some('>')
        )
    }));
}

#[test]
fn cpp_insert_gt_in_comparison_closes_and_does_not_trigger_by_default() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "auto value = a ".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: content.clone(),
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer.set_cursor(0, content.chars().count());
    }

    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let strategy = completion_strategy::strategy_for_tab(tab);

    assert!(strategy.should_close_on_command(&Command::InsertChar('>'), Some(tab)));
    assert!(!strategy.triggered_by_insert(tab, '>', &[]));
}

#[test]
fn experiment_completion_filtering_scale_baseline() {
    use crate::kernel::store::completion::{
        filtered_completion_indices, sync_completion_items_from_cache,
    };

    let mut store = new_store();
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "pr".to_string(),
    }));

    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let strategy = completion_strategy::strategy_for_tab(tab);

    let items: Vec<LspCompletionItem> = (0..10_000)
        .map(|i| test_completion_item(i, &format!("item_{i:05}")))
        .collect();

    let warm = filtered_completion_indices(tab, &items, strategy);
    assert!(!warm.is_empty());

    let start = Instant::now();
    let mut total = 0usize;
    for _ in 0..50 {
        total = total.saturating_add(filtered_completion_indices(tab, &items, strategy).len());
    }
    let elapsed = start.elapsed();

    let mut popup = crate::kernel::state::CompletionPopupState {
        all_items: items,
        ..Default::default()
    };
    let start_sync = Instant::now();
    let mut changed_count = 0usize;
    for _ in 0..50 {
        if sync_completion_items_from_cache(&mut popup, tab, strategy) {
            changed_count += 1;
        }
    }
    let elapsed_sync = start_sync.elapsed();

    eprintln!(
        "[experiment] filtering_scale loops=50 items=10000 filtered_total={} elapsed_ms={} sync_elapsed_ms={} changed_count={}",
        total,
        elapsed.as_millis(),
        elapsed_sync.as_millis(),
        changed_count
    );
}

#[test]
fn experiment_cpp_include_context_lookup_counts() {
    completion_strategy::reset_include_context_perf_counter();

    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#include <vector>".to_string();
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: content.clone(),
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .unwrap()
            .active_tab_mut()
            .unwrap();
        tab.buffer.set_cursor(0, content.chars().count());
    }

    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let strategy = completion_strategy::strategy_for_tab(tab);

    for _ in 0..1000 {
        let _ = strategy.context_allows_completion(tab);
        let _ = strategy.prefix_bounds(tab);
        let _ = strategy.completion_should_keep_open(tab);
    }

    let calls = completion_strategy::include_context_perf_counter();
    eprintln!(
        "[experiment] cpp_include_context loops=1000 include_context_bounds_calls={}",
        calls
    );

    assert!(
        calls >= 1,
        "expected at least one include lookup, calls={calls}"
    );
}

#[test]
fn experiment_insert_char_signature_help_capability_lookup_counts() {
    use crate::kernel::store::lsp::{
        lsp_capability_lookup_perf_counter, reset_lsp_capability_lookup_perf_counter,
    };

    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() { foo".to_string(),
    }));

    reset_lsp_capability_lookup_perf_counter();
    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('x')));
    let calls_for_x = lsp_capability_lookup_perf_counter();

    reset_lsp_capability_lookup_perf_counter();
    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('(')));
    let calls_for_lparen = lsp_capability_lookup_perf_counter();

    eprintln!(
        "[experiment] insert_char_capability_lookup x_calls={} lparen_calls={}",
        calls_for_x, calls_for_lparen
    );

    assert!(
        calls_for_x >= 1,
        "expected at least one capability lookup, got {calls_for_x}"
    );
    assert!(
        calls_for_lparen >= 1,
        "expected at least one capability lookup, got {calls_for_lparen}"
    );
}

#[test]
fn lsp_workspace_edit_applies_to_all_open_tabs_for_path() {
    let mut store = new_store();
    store.state.editor.ensure_panes(2);

    let path = store.state.workspace_root.join("test.rs");
    let content = "hello\nworld\n".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: content.clone(),
    }));
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 1,
        path: path.clone(),
        content,
    }));

    let edit = LspWorkspaceEdit {
        changes: vec![LspWorkspaceFileEdit {
            path: path.clone(),
            edits: vec![LspTextEdit {
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
            }],
        }],
        ..Default::default()
    };

    let _ = store.dispatch(Action::LspApplyWorkspaceEdit { edit });

    assert_eq!(
        store
            .state
            .editor
            .pane(0)
            .unwrap()
            .active_tab()
            .unwrap()
            .buffer
            .text(),
        "hello\nrust\n"
    );
    assert_eq!(
        store
            .state
            .editor
            .pane(1)
            .unwrap()
            .active_tab()
            .unwrap()
            .buffer
            .text(),
        "hello\nrust\n"
    );
}

#[test]
fn lsp_workspace_edit_schedules_file_edits_when_not_open() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("test.rs");

    let edit = LspWorkspaceEdit {
        changes: vec![LspWorkspaceFileEdit {
            path: path.clone(),
            edits: vec![LspTextEdit {
                range: LspRange {
                    start: LspPosition {
                        line: 0,
                        character: 0,
                    },
                    end: LspPosition {
                        line: 0,
                        character: 0,
                    },
                },
                new_text: "x".to_string(),
            }],
        }],
        ..Default::default()
    };

    let result = store.dispatch(Action::LspApplyWorkspaceEdit { edit });

    assert!(!result.state_changed);
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::ApplyFileEdits { position_encoding, resource_ops, edits }]
            if *position_encoding == LspPositionEncoding::Utf16
                && resource_ops.is_empty()
                && edits.len() == 1
                && edits[0].path == path
    ));
}

#[test]
fn expand_snippet_strips_tabstops_and_keeps_placeholder_text() {
    let out = expand_snippet("foo$1bar$0");
    assert_eq!(out.text, "foobar");
    assert_eq!(out.cursor, Some(3));
    assert!(out.selection.is_none());

    let out = expand_snippet("fn ${1:name}($2)$0");
    assert_eq!(out.text, "fn name()");
    assert_eq!(out.selection, Some((3, 7)));
    assert_eq!(out.cursor, Some(7));

    let out = expand_snippet("x${1|a,b,c|}y");
    assert_eq!(out.text, "xay");
    assert_eq!(out.selection, Some((1, 2)));
    assert_eq!(out.cursor, Some(2));

    let out = expand_snippet("\\$\\{not_a_placeholder\\}");
    assert_eq!(out.text, "${not_a_placeholder}");
    assert!(out.cursor.is_none());
    assert!(out.selection.is_none());
}

#[test]
fn completion_plain_text_moves_cursor_inside_trailing_parens() {
    let insertion = CompletionInsertion::from_plain_text("println!()".to_string());
    assert_eq!(insertion.text, "println!()");
    assert_eq!(insertion.cursor, Some("println!(".chars().count()));
    assert!(insertion.selection.is_none());

    let insertion = CompletionInsertion::from_plain_text("no_parens".to_string());
    assert_eq!(insertion.text, "no_parens");
    assert!(insertion.cursor.is_none());
    assert!(insertion.selection.is_none());
}

#[test]
fn lsp_position_to_byte_offset_handles_emoji_crlf_and_out_of_bounds() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "a😀b\r\nc".to_string(),
    }));
    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();

    let after_emoji = "a😀".len();
    assert_eq!(
        lsp_position_to_byte_offset(tab, 0, 3, LspPositionEncoding::Utf16),
        after_emoji
    );
    assert_eq!(
        lsp_position_to_byte_offset(tab, 0, 5, LspPositionEncoding::Utf8),
        after_emoji
    );

    assert_eq!(
        lsp_position_to_byte_offset(tab, 0, 1, LspPositionEncoding::Utf16),
        1
    );
    assert_eq!(
        lsp_position_to_byte_offset(tab, 1, 0, LspPositionEncoding::Utf16),
        "a😀b\r\n".len()
    );

    assert_eq!(
        lsp_position_to_byte_offset(tab, u32::MAX, u32::MAX, LspPositionEncoding::Utf16),
        "a😀b\r\nc".len()
    );
}

#[test]
fn lsp_range_for_full_lines_keeps_end_position_within_document() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "a😀".to_string(),
    }));
    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let total_lines = tab.buffer.len_lines().max(1);
    let range = lsp_range_for_full_lines(tab, 0, total_lines, LspPositionEncoding::Utf16).unwrap();
    assert_eq!(range.end.line, 0);
    assert_eq!(range.end.character, 3);

    let path = store.state.workspace_root.join("newline.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "abc\n".to_string(),
    }));
    let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
    let total_lines = tab.buffer.len_lines().max(1);
    let range = lsp_range_for_full_lines(tab, 0, total_lines, LspPositionEncoding::Utf16).unwrap();
    assert_eq!(range.end.line, 1);
    assert_eq!(range.end.character, 0);
}

#[test]
fn fuzz_workspace_edit_application_does_not_break_cursor_invariants() {
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next_u32(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 & 0xFFFF_FFFF) as u32
        }

        fn gen_range(&mut self, upper: u32) -> u32 {
            if upper == 0 {
                return 0;
            }
            self.next_u32() % upper
        }
    }

    fn assert_cursor_invariants(tab: &crate::kernel::editor::EditorTabState) {
        let (row, col) = tab.buffer.cursor();
        let total_lines = tab.buffer.len_lines().max(1);
        assert!(row < total_lines);
        assert!(col <= tab.buffer.line_grapheme_len(row));
    }

    let mut store = new_store();
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "line0\nline1\nline2\n".to_string(),
    }));

    let mut rng = Rng::new(0x0BAD_5EED);

    const STEPS: usize = 300;
    for _ in 0..STEPS {
        let edits_len = 1 + rng.gen_range(3) as usize;
        let mut edits = Vec::with_capacity(edits_len);
        for _ in 0..edits_len {
            let huge = rng.gen_range(10) == 0;
            let start_line = if huge { u32::MAX } else { rng.gen_range(8) };
            let start_col = if huge { u32::MAX } else { rng.gen_range(32) };
            let end_line = if huge { 0 } else { rng.gen_range(8) };
            let end_col = if huge { 0 } else { rng.gen_range(32) };

            let new_text = match rng.gen_range(7) {
                0 => String::new(),
                1 => "x".to_string(),
                2 => "😀".to_string(),
                3 => "y\n".to_string(),
                4 => "中".to_string(),
                5 => "\r\n".to_string(),
                _ => "_".to_string(),
            };

            edits.push(LspTextEdit {
                range: LspRange {
                    start: LspPosition {
                        line: start_line,
                        character: start_col,
                    },
                    end: LspPosition {
                        line: end_line,
                        character: end_col,
                    },
                },
                new_text,
            });
        }

        let _ = store.dispatch(Action::LspApplyWorkspaceEdit {
            edit: LspWorkspaceEdit {
                changes: vec![LspWorkspaceFileEdit {
                    path: path.clone(),
                    edits,
                }],
                ..Default::default()
            },
        });

        let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
        assert_cursor_invariants(tab);
    }
}

#[test]
fn reload_from_disk_emits_reload_request_from_active_tab() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("reload_target.rs");

    let _ = store.dispatch(Action::RunCommand(Command::SplitEditorVertical));
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() {}\n".to_string(),
    }));
    let _ = store.dispatch(Action::EditorSetActivePane { pane: 0 });

    let result = store.dispatch(Action::RunCommand(Command::ReloadFromDisk));

    let request = match result.effects.as_slice() {
        [Effect::ReloadFile(request)] => request,
        other => panic!("expected one ReloadFile effect, got {other:?}"),
    };
    assert_eq!(request.pane, 0);
    assert_eq!(request.path, path);
    assert_eq!(
        request.cause,
        crate::kernel::editor::ReloadCause::ManualCommand
    );
    assert_eq!(request.request_id, 1);
    assert!(!result.state_changed);
}

#[test]
fn explorer_context_menu_paste_emits_copy_effect() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let source_id = tree
        .insert_child(
            tree.root(),
            OsString::from("a.txt"),
            crate::models::NodeKind::File,
        )
        .unwrap();
    let target_dir_id = tree
        .insert_child(
            tree.root(),
            OsString::from("dir"),
            crate::models::NodeKind::Dir,
        )
        .unwrap();

    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));
    let source_path = root.join("a.txt");
    assert!(store.state.explorer.set_clipboard(
        source_path.clone(),
        false,
        crate::kernel::state::ExplorerClipboardMode::Copy
    ));

    let tree_row = store
        .state
        .explorer
        .rows
        .iter()
        .position(|row| row.id == target_dir_id)
        .unwrap();

    let _ = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Explorer {
            tree_row: Some(tree_row),
        },
        x: 10,
        y: 5,
    });

    let paste_index = store
        .state
        .ui
        .context_menu
        .items
        .iter()
        .position(|item| item.label == "Paste")
        .expect("paste item exists");
    assert!(store.state.ui.context_menu.items[paste_index].is_selectable());

    let _ = store.dispatch(Action::ContextMenuSetSelected { index: paste_index });
    let result = store.dispatch(Action::ContextMenuConfirm);

    assert!(result.state_changed);
    assert!(matches!(
        result.effects.as_slice(),
        [Effect::CopyPath {
            from,
            to,
            overwrite: false
        }] if from == &source_path && to == &root.join("dir").join("a.txt")
    ));

    // Keep source node alive in tree setup to avoid accidental dead-code warnings.
    assert!(store
        .state
        .explorer
        .rows
        .iter()
        .any(|row| row.id == source_id));
}

#[test]
fn context_menu_move_selection_skips_disabled_entries() {
    let mut store = new_store();

    let _ = store.dispatch(Action::ContextMenuOpen {
        request: ContextMenuRequest::Explorer { tree_row: None },
        x: 10,
        y: 5,
    });

    assert_eq!(store.state.ui.context_menu.selected, 0);
    let _ = store.dispatch(Action::ContextMenuMoveSelection { delta: 1 });
    assert_eq!(store.state.ui.context_menu.selected, 1);

    let _ = store.dispatch(Action::ContextMenuMoveSelection { delta: 1 });
    let selected = store.state.ui.context_menu.selected;
    assert!(store.state.ui.context_menu.items[selected].is_selectable());
    assert!(
        selected > 2,
        "selection should skip separator/disabled rows"
    );
}
