use super::*;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::{
    LspPosition, LspRange, LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
};
use crate::kernel::state::{
    ExplorerContextMenuItem, PendingEditorNavigation, PendingEditorNavigationTarget,
};
use crate::models::{FileTree, Granularity, Selection};
use std::ffi::OsString;

fn new_store() -> Store {
    let root = std::env::temp_dir();
    let tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    Store::new(AppState::new(root, tree, EditorConfig::default()))
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
fn explorer_context_menu_root_only_shows_create_items() {
    let mut store = new_store();

    let result = store.dispatch(Action::ExplorerContextMenuOpen {
        tree_row: None,
        x: 10,
        y: 5,
    });

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(store.state.ui.explorer_context_menu.visible);
    assert_eq!(
        store.state.ui.explorer_context_menu.items,
        vec![
            ExplorerContextMenuItem::NewFile,
            ExplorerContextMenuItem::NewFolder
        ]
    );
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

    let _ = store.dispatch(Action::ExplorerContextMenuOpen {
        tree_row: Some(tree_row),
        x: 10,
        y: 5,
    });

    assert_eq!(
        store.state.ui.explorer_context_menu.items,
        vec![
            ExplorerContextMenuItem::NewFile,
            ExplorerContextMenuItem::NewFolder,
            ExplorerContextMenuItem::Rename,
            ExplorerContextMenuItem::Delete,
        ]
    );

    let _ = store.dispatch(Action::ExplorerContextMenuSetSelected { index: 2 });
    let result = store.dispatch(Action::ExplorerContextMenuConfirm);
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
        [Effect::RenamePath { from, to }]
            if from == &root.join("a.txt") && to == &root.join("b.txt")
    ));
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

    assert!(result
        .effects
        .iter()
        .any(|e| { matches!(e, Effect::LspCompletionRequest { path: p, .. } if p == &path) }));
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
    assert!(second
        .effects
        .iter()
        .any(|e| { matches!(e, Effect::LspCompletionRequest { path: p, .. } if p == &path) }));
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
    assert_eq!(store.state.ui.completion.items.len(), 2);
    assert_eq!(store.state.ui.completion.items[0].label, "println!");
    assert_eq!(store.state.ui.completion.items[1].label, "Print");
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
    assert!(!store.state.ui.completion.items.is_empty());

    let _ = store.dispatch(Action::Editor(EditorAction::SetViewportSize {
        pane: 0,
        width: 80,
        height: 20,
    }));

    assert!(store.state.ui.completion.visible);
    assert!(!store.state.ui.completion.items.is_empty());
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
    assert!(!store.state.ui.completion.items.is_empty());

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
    assert!(!store.state.ui.completion.items.is_empty());
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

    let mut rng = Rng::new(0xBAD5_EED);

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
