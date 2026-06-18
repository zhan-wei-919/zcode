use super::*;
use crate::kernel::language::adapter::{
    adapter_for_tab, include_context_perf_counter, reset_include_context_perf_counter,
};
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::{
    LspCompletionTriggerKind, LspHoverBlock, LspHoverPayload, LspInsertTextFormat, LspPosition,
    LspRange, LspSemanticToken, LspSemanticTokensLegend, LspServerCapabilities, LspServerKind,
    LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
};
use crate::kernel::state::{
    CompletionRequestContext, ContextMenuRequest, PendingAction, PendingEditorNavigation,
    PendingEditorNavigationTarget,
};
use crate::models::{FileTree, Granularity, LoadState, NodeKind, Selection};
use std::ffi::{OsStr, OsString};
use std::time::Instant;
use tempfile::tempdir;

fn new_store() -> Store {
    let root = std::env::temp_dir();
    let tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    Store::new(AppState::new(root, tree, EditorConfig::default()))
}

fn sem_seg(
    start: usize,
    end: usize,
    kind: Option<crate::kernel::editor::HighlightKind>,
) -> crate::kernel::editor::SemanticSegment {
    crate::kernel::editor::SemanticSegment {
        start,
        end,
        semantic_kind: kind,
    }
}

#[test]
fn hover_preview_includes_definition_and_implementation() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "fn main() {}".to_string(),
    }));
    let session = 10;

    let _ = store.dispatch(Action::LspHoverResponse {
        session,
        payload: LspHoverPayload {
            blocks: vec![LspHoverBlock::PlainText("stub hover".to_string())],
            range: None,
        },
    });
    let _ = store.dispatch(Action::LspHoverDefinitionPreview {
        session,
        payload: crate::kernel::services::ports::LspHoverPreviewPayload {
            title: String::new(),
            blocks: vec![LspHoverBlock::PlainText(
                "Definition preview: trait".to_string(),
            )],
        },
    });

    let _ = store.dispatch(Action::LspHoverImplementationPreview {
        session,
        payload: crate::kernel::services::ports::LspHoverPreviewPayload {
            title: String::new(),
            blocks: vec![LspHoverBlock::PlainText(
                "Implementation preview: impl".to_string(),
            )],
        },
    });

    let hover_text = store.state.ui.hover.display_text().expect("hover message");
    let hover = hover_text.as_str();
    assert!(hover.contains("stub hover"));
    assert!(hover.contains("Definition preview: trait"));
    assert!(hover.contains("Implementation preview: impl"));
    let def_idx = hover
        .find("Definition preview: trait")
        .expect("definition preview present");
    let impl_idx = hover
        .find("Implementation preview: impl")
        .expect("implementation preview present");
    assert!(
        def_idx < impl_idx,
        "expected definition preview before implementation preview:\n{hover}"
    );
}

#[test]
fn hover_preview_falls_back_to_definition_when_implementation_empty() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "fn main() {}".to_string(),
    }));
    let session = 11;

    let _ = store.dispatch(Action::LspHoverResponse {
        session,
        payload: LspHoverPayload {
            blocks: vec![LspHoverBlock::PlainText("stub hover".to_string())],
            range: None,
        },
    });
    let _ = store.dispatch(Action::LspHoverImplementationPreview {
        session,
        payload: crate::kernel::services::ports::LspHoverPreviewPayload::default(),
    });
    let _ = store.dispatch(Action::LspHoverDefinitionPreview {
        session,
        payload: crate::kernel::services::ports::LspHoverPreviewPayload {
            title: String::new(),
            blocks: vec![LspHoverBlock::PlainText(
                "Definition preview: trait".to_string(),
            )],
        },
    });

    let hover_text = store.state.ui.hover.display_text().expect("hover message");
    let hover = hover_text.as_str();
    assert!(hover.contains("Definition preview: trait"));
    assert!(!hover.contains("Implementation preview:"));
}

#[test]
fn explorer_dir_changed_triggers_dir_reload_when_loaded() {
    use crate::kernel::services::ports::DirEntryInfo;

    let dir = tempdir().expect("create tempdir");
    let root = dir.path().to_path_buf();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let docs_id = tree
        .insert_child_with_state(
            tree.root(),
            OsString::from("docs"),
            NodeKind::Dir,
            LoadState::Loaded,
        )
        .expect("insert docs");
    tree.expand(docs_id);

    let docs_path = root.join("docs");
    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));

    let result = store.dispatch(Action::ExplorerDirChanged {
        path: docs_path.clone(),
    });
    assert!(
        result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadDir(path) if path == &docs_path)),
        "dir changed should request load_dir effect"
    );

    let docs_row = store
        .state
        .explorer
        .rows
        .iter()
        .find(|row| row.name == "docs")
        .expect("docs row visible");
    assert_eq!(docs_row.load_state, LoadState::Loading);

    let _ = store.dispatch(Action::DirLoaded {
        path: docs_path.clone(),
        entries: vec![DirEntryInfo {
            name: "api.yaml".to_string(),
            is_dir: false,
        }],
    });

    assert!(store
        .state
        .explorer
        .node_id_for_path(docs_path.join("api.yaml").as_path())
        .is_some());

    let docs_row = store
        .state
        .explorer
        .rows
        .iter()
        .find(|row| row.name == "docs")
        .expect("docs row visible");
    assert_eq!(docs_row.load_state, LoadState::Loaded);
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

fn test_completion_record(id: u64, label: &str) -> crate::kernel::language::CompletionRecord {
    test_completion_item(id, label).into()
}

fn runtime_for_tab<'a>(
    tab: &'a crate::kernel::editor::EditorTabState,
) -> crate::kernel::language::LanguageRuntimeContext<'a> {
    let adapter = adapter_for_tab(tab);
    crate::kernel::language::LanguageRuntimeContext::new(
        tab.language(),
        tab,
        adapter.syntax().syntax_facts(tab),
    )
}

fn seed_visible_completion_for_active_tab(
    store: &mut Store,
    pane: usize,
    path: std::path::PathBuf,
    label: &str,
) {
    seed_visible_completion_item_for_active_tab(store, pane, path, test_completion_item(1, label));
}

fn seed_visible_completion_item_for_active_tab(
    store: &mut Store,
    pane: usize,
    path: std::path::PathBuf,
    item: LspCompletionItem,
) {
    let (version, normalization) = {
        let tab = store.state.editor.pane(pane).unwrap().active_tab().unwrap();
        let adapter = adapter_for_tab(tab);
        let runtime = crate::kernel::language::LanguageRuntimeContext::new(
            tab.language(),
            tab,
            adapter.syntax().syntax_facts(tab),
        );
        (tab.edit_version, runtime.completion_snapshot())
    };

    store.state.ui.completion.visible = true;
    store.state.ui.completion.selected = 0;
    store.state.ui.completion.all_items = vec![item.into()];
    store.state.ui.completion.rebuild_index_by_id();
    store.state.ui.completion.visible_indices = vec![0];
    store.state.ui.completion.request = Some(CompletionRequestContext {
        pane,
        path,
        version,
        normalization,
    });
    store.state.ui.completion.pending_request = None;
    store.state.ui.completion.is_incomplete = false;
}

type ComputeSyntaxEffect = (
    crate::kernel::editor::TabId,
    u64,
    crate::kernel::language::LanguageId,
    ropey::Rope,
    tree_sitter::Tree,
    Vec<(usize, usize)>,
);

fn first_compute_syntax_effect(effects: &[Effect]) -> Option<ComputeSyntaxEffect> {
    effects.iter().find_map(|effect| match effect {
        Effect::ComputeSyntaxHighlights {
            tab_id,
            version,
            language,
            rope,
            tree,
            segments,
        } => Some((
            *tab_id,
            *version,
            *language,
            rope.clone(),
            tree.clone(),
            segments.clone(),
        )),
        _ => None,
    })
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
fn escape_closes_command_line_first() {
    let mut store = new_store();
    store.state.ui.command_line.active = true;
    store.state.ui.command_line.input = "x".to_string();
    store.state.ui.command_line.cursor = 1;
    store.state.ui.command_line.selected = 1;
    store.state.ui.focus = FocusTarget::CommandLine;

    let result = store.dispatch(Action::RunCommand(Command::Escape));

    assert!(result.effects.is_empty());
    assert!(result.state_changed);
    assert!(!store.state.ui.command_line.active);
    assert!(store.state.ui.command_line.input.is_empty());
    assert_eq!(store.state.ui.command_line.selected, 0);
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
fn set_active_tab_syncs_explorer_selected_row_to_active_file() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let mut tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    let _a_id = tree
        .insert_child(
            tree.root(),
            OsString::from("a.java"),
            crate::models::NodeKind::File,
        )
        .unwrap();
    let _b_id = tree
        .insert_child(
            tree.root(),
            OsString::from("b.java"),
            crate::models::NodeKind::File,
        )
        .unwrap();
    let mut store = Store::new(AppState::new(root.clone(), tree, EditorConfig::default()));

    let a_path = root.join("a.java");
    let b_path = root.join("b.java");

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: a_path.clone(),
        content: "class A {}".to_string(),
    }));
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: b_path.clone(),
        content: "class B {}".to_string(),
    }));

    let b_row = store
        .state
        .explorer
        .rows
        .iter()
        .position(|row| !row.is_dir && row.name.as_os_str() == OsStr::new("b.java"))
        .expect("b row");
    let _ = store.dispatch(Action::ExplorerClickRow {
        row: b_row,
        now: Instant::now(),
    });

    let selected_before = store
        .state
        .explorer
        .selected()
        .and_then(|id| store.state.explorer.path_and_kind_for(id))
        .map(|(path, _)| path);
    assert_eq!(selected_before.as_ref(), Some(&b_path));

    let a_index = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| {
            pane.tabs
                .iter()
                .position(|tab| tab.path.as_ref() == Some(&a_path))
        })
        .expect("a tab index");
    let _ = store.dispatch(Action::Editor(EditorAction::SetActiveTab {
        pane: 0,
        index: a_index,
    }));

    let selected_after = store
        .state
        .explorer
        .selected()
        .and_then(|id| store.state.explorer.path_and_kind_for(id))
        .map(|(path, _)| path);
    assert_eq!(selected_after.as_ref(), Some(&a_path));
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
        .position(|idx| store.state.ui.completion.all_items[*idx].entry.id == 1)
        .expect("print should be visible");
    store.state.ui.completion.selected = selected;
    store.state.ui.completion.selection_locked = true;
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
fn completion_defaults_to_first_item_when_selection_unlocked() {
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

    // Seed an existing completion popup with a non-zero selection, but keep it unlocked.
    store.state.ui.completion.visible = true;
    store.state.ui.completion.all_items = vec![
        test_completion_record(1, "print"),
        test_completion_record(2, "probe"),
    ];
    store.state.ui.completion.visible_indices = vec![0, 1];
    store.state.ui.completion.selected = 1;
    store.state.ui.completion.selection_locked = false;

    let items = vec![
        test_completion_item(11, "print"),
        test_completion_item(12, "println!"),
        test_completion_item(13, "private"),
    ];
    let _ = store.dispatch(Action::LspCompletion {
        items,
        is_incomplete: false,
    });

    assert!(store.state.ui.completion.visible);
    assert_eq!(store.state.ui.completion.selected, 0);
}

#[test]
fn completion_index_map_matches_visible_indices_after_lsp_completion() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "pr\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let items = vec![
        test_completion_item(11, "println!"),
        test_completion_item(12, "print"),
        test_completion_item(13, "probe"),
    ];
    let _ = store.dispatch(Action::LspCompletion {
        items,
        is_incomplete: false,
    });

    let completion = &store.state.ui.completion;
    assert_eq!(completion.index_by_id.len(), completion.all_items.len());
    for idx in &completion.visible_indices {
        let item = completion
            .all_items
            .get(*idx)
            .expect("visible index must point to item");
        assert_eq!(
            completion.index_by_id.get(&item.entry.id).copied(),
            Some(*idx)
        );
    }
}

#[test]
fn completion_no_match_hides_popup_instead_of_falling_back_to_full_list() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "struct Abc {\n    p\n}\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorDown));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let items = vec![
        test_completion_item(11, "pub"),
        test_completion_item(12, "pub(crate)"),
        test_completion_item(13, "pub(super)"),
    ];
    let _ = store.dispatch(Action::LspCompletion {
        items,
        is_incomplete: false,
    });

    let labels = (0..store.state.ui.completion.visible_len())
        .filter_map(|i| store.state.ui.completion.visible_item(i))
        .map(|item| item.label.clone())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["pub", "pub(crate)", "pub(super)"]);
    assert_eq!(store.state.ui.completion.all_items.len(), 3);
    assert!(store.state.ui.completion.visible);

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('x')));
    assert!(!store.state.ui.completion.visible);
    assert_eq!(store.state.ui.completion.visible_len(), 0);
    assert_eq!(store.state.ui.completion.all_items.len(), 3);

    let _ = store.dispatch(Action::RunCommand(Command::DeleteBackward));
    assert!(store.state.ui.completion.visible);
    assert_eq!(store.state.ui.completion.visible_len(), 3);
}

#[test]
fn lsp_completion_resolved_repairs_stale_index_map_via_fallback() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "pr\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let _ = store.dispatch(Action::LspCompletion {
        items: vec![
            test_completion_item(101, "print"),
            test_completion_item(102, "probe"),
        ],
        is_incomplete: false,
    });

    // Force a stale mapping: id 102 points to an invalid index.
    store
        .state
        .ui
        .completion
        .index_by_id
        .insert(102, usize::MAX);

    let result = store.dispatch(Action::LspCompletionResolved {
        id: 102,
        detail: Some("resolved".to_string()),
        documentation: None,
        insert_text: None,
        insert_text_format: None,
        insert_range: None,
        replace_range: None,
        additional_text_edits: Vec::new(),
        command: None,
    });
    assert!(result.state_changed);

    let idx = store
        .state
        .ui
        .completion
        .all_items
        .iter()
        .position(|item| item.entry.id == 102)
        .expect("resolved item index");
    assert_eq!(
        store.state.ui.completion.index_by_id.get(&102).copied(),
        Some(idx)
    );
    assert_eq!(
        store.state.ui.completion.all_items[idx]
            .entry
            .detail
            .as_deref(),
        Some("resolved")
    );
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
        .selected_record()
        .expect("completion item");
    assert_eq!(item.raw.insert_text, "print(${1:value})$0");
    assert_eq!(
        item.raw.insert_text_format,
        crate::kernel::services::ports::LspInsertTextFormat::Snippet
    );
    assert!(item.raw.insert_range.is_some());
    assert!(item.raw.replace_range.is_some());
}

#[test]
fn lsp_completion_requests_resolve_for_documented_unresolved_selected_item() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "map.ins\n".to_string(),
    }));
    let _ = store.dispatch(Action::RunCommand(Command::CursorLineEnd));
    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));

    let result = store.dispatch(Action::LspCompletion {
        items: vec![LspCompletionItem {
            id: 7,
            label: "insert".to_string(),
            detail: Some("fn insert".to_string()),
            kind: Some(2),
            documentation: Some("docs already present".to_string()),
            insert_text: "insert($1, $2)$0".to_string(),
            insert_text_format: LspInsertTextFormat::Snippet,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: Some(serde_json::json!({ "id": 7 })),
        }],
        is_incomplete: false,
    });

    assert!(
        result.effects.iter().any(|effect| matches!(
            effect,
            Effect::LspCompletionResolveRequest { item }
                if item.id == 7 && item.data.is_some()
        )),
        "expected selected unresolved completion to request resolve even when docs are present"
    );
    assert_eq!(store.state.ui.completion.resolve_inflight, Some(7));
    assert!(matches!(
        store
            .state
            .ui
            .completion
            .selected_item()
            .map(|item| item.resolve_state),
        Some(crate::kernel::language::CompletionResolveState::Resolving)
    ));
}

#[test]
fn lsp_completion_resolved_uses_request_snapshot_when_tab_is_gone() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "pri".to_string(),
    }));

    let (version, normalization) = {
        let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
        let adapter = adapter_for_tab(tab);
        let runtime = crate::kernel::language::LanguageRuntimeContext::new(
            tab.language(),
            tab,
            adapter.syntax().syntax_facts(tab),
        );
        (tab.edit_version, runtime.completion_snapshot())
    };

    let _ = store.dispatch(Action::Editor(EditorAction::CloseTabAt {
        pane: 0,
        index: 0,
    }));

    store.state.ui.completion.visible = true;
    store.state.ui.completion.selected = 0;
    let mut item = test_completion_item(1, "printf");
    item.kind = Some(3);
    store.state.ui.completion.all_items = vec![item.into()];
    store.state.ui.completion.rebuild_index_by_id();
    store.state.ui.completion.visible_indices = vec![0];
    store.state.ui.completion.request = Some(CompletionRequestContext {
        pane: 0,
        path,
        version,
        normalization,
    });
    store.state.ui.completion.pending_request = None;

    let result = store.dispatch(Action::LspCompletionResolved {
        id: 1,
        detail: Some("resolved".to_string()),
        documentation: None,
        insert_text: None,
        insert_text_format: None,
        insert_range: None,
        replace_range: None,
        additional_text_edits: Vec::new(),
        command: None,
    });

    assert!(result.state_changed);
    let item = store
        .state
        .ui
        .completion
        .selected_record()
        .expect("completion item");
    assert_eq!(item.entry.detail.as_deref(), Some("resolved"));
    assert_eq!(item.entry.commit.insert.text, "printf()");
    assert_eq!(
        item.entry.commit.insert.cursor,
        Some("printf(".chars().count())
    );
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
fn cpp_insert_hash_at_line_start_triggers_completion_request() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: String::new(),
    }));

    let result = store.dispatch(Action::RunCommand(Command::InsertChar('#')));

    assert!(result.effects.iter().any(|effect| {
        matches!(
            effect,
            Effect::LspCompletionRequest { trigger, .. }
                if trigger.kind == LspCompletionTriggerKind::TriggerCharacter
                    && trigger.character == Some('#')
        )
    }));
}

#[test]
fn cpp_hash_empty_lsp_completion_falls_back_to_directives() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: String::new(),
    }));

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('#')));
    let _ = store.dispatch(Action::LspCompletion {
        items: Vec::new(),
        is_incomplete: false,
    });

    let labels = (0..store.state.ui.completion.visible_len())
        .filter_map(|i| store.state.ui.completion.visible_item(i))
        .map(|item| item.label.clone())
        .collect::<Vec<_>>();

    assert!(store.state.ui.completion.visible);
    assert!(labels.contains(&"include".to_string()));
    assert!(labels.contains(&"define".to_string()));
}

#[test]
fn cpp_hash_directive_context_allows_completion() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#".to_string();

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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);
    assert!(adapter.interaction().context_allows_completion(&runtime));
}

#[test]
fn cpp_define_macro_name_space_closes_directive_completion_popup() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#define LEFT".to_string();

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
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

    let normalization = {
        let tab = store.state.editor.pane(0).unwrap().active_tab().unwrap();
        runtime_for_tab(tab).completion_snapshot()
    };

    store.state.ui.completion.visible = true;
    store.state.ui.completion.selected = 0;
    store.state.ui.completion.all_items = vec![
        test_completion_item(1, "define").into(),
        test_completion_item(2, "line").into(),
    ];
    store.state.ui.completion.rebuild_index_by_id();
    store.state.ui.completion.visible_indices = vec![0, 1];
    store.state.ui.completion.request = Some(CompletionRequestContext {
        pane: 0,
        path,
        version: 0,
        normalization,
    });

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar(' ')));

    assert!(!store.state.ui.completion.visible);
    assert_eq!(store.state.ui.completion.visible_len(), 0);
    assert!(store.state.ui.completion.request.is_none());
}

#[test]
fn cpp_include_empty_lsp_completion_falls_back_to_delimiters() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#include ".to_string();

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

    let _ = store.dispatch(Action::RunCommand(Command::LspCompletion));
    let _ = store.dispatch(Action::LspCompletion {
        items: Vec::new(),
        is_incomplete: false,
    });

    let labels = (0..store.state.ui.completion.visible_len())
        .filter_map(|i| store.state.ui.completion.visible_item(i))
        .map(|item| item.label.clone())
        .collect::<Vec<_>>();

    assert!(store.state.ui.completion.visible);
    assert_eq!(labels, vec!["<...>".to_string(), "\"...\"".to_string()]);
}

#[test]
fn cpp_include_directive_before_delimiter_allows_completion() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let content = "#include ".to_string();

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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);
    assert!(adapter.interaction().context_allows_completion(&runtime));
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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);
    assert!(adapter.interaction().context_allows_completion(&runtime));
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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);
    assert!(!adapter.interaction().context_allows_completion(&runtime));
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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);
    assert!(!adapter.interaction().completion_should_keep_open(&runtime));
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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);

    assert!(!adapter
        .interaction()
        .should_close_on_command(&Command::InsertChar('>'), Some(&runtime)));

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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);

    assert!(adapter
        .interaction()
        .should_close_on_command(&Command::InsertChar('>'), Some(&runtime)));
    assert!(!adapter
        .interaction()
        .completion_triggered_by_insert(&runtime, '>', &[]));
}

#[test]
fn experiment_completion_filtering_scale_baseline() {
    use crate::kernel::store::intel::completion::{
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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);

    let items: Vec<LspCompletionItem> = (0..10_000)
        .map(|i| test_completion_item(i, &format!("item_{i:05}")))
        .collect();

    let item_records: Vec<crate::kernel::language::CompletionRecord> =
        items.clone().into_iter().map(Into::into).collect();
    let warm = filtered_completion_indices(&runtime, &item_records, adapter.interaction());
    assert!(!warm.is_empty());

    let start = Instant::now();
    let mut total = 0usize;
    for _ in 0..50 {
        total = total.saturating_add(
            filtered_completion_indices(&runtime, &item_records, adapter.interaction()).len(),
        );
    }
    let elapsed = start.elapsed();

    let mut popup = crate::kernel::state::CompletionPopupState {
        all_items: items.into_iter().map(Into::into).collect(),
        ..Default::default()
    };
    let start_sync = Instant::now();
    let mut changed_count = 0usize;
    for _ in 0..50 {
        if sync_completion_items_from_cache(&mut popup, &runtime, adapter.interaction()) {
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
    reset_include_context_perf_counter();

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
    let adapter = adapter_for_tab(tab);
    let runtime = runtime_for_tab(tab);

    for _ in 0..1000 {
        let _ = adapter.interaction().context_allows_completion(&runtime);
        let _ = adapter.interaction().completion_prefix_bounds(&runtime);
        let _ = adapter.interaction().completion_should_keep_open(&runtime);
    }

    let calls = include_context_perf_counter();
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
    use crate::kernel::store::intel::lsp::{
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
fn lsp_workspace_edit_requests_semantic_tokens_for_changed_open_tab() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("test.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "hello\nworld\n".to_string(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: false,
            ..Default::default()
        },
    });

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

    let result = store.dispatch(Action::LspApplyWorkspaceEdit { edit });
    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");

    assert!(result.state_changed);
    assert!(result.effects.iter().any(|effect| {
        matches!(
            effect,
            Effect::LspSemanticTokensRequest {
                path: effect_path,
                version: effect_version,
            } if effect_path == &path && *effect_version == version
        )
    }));
}

#[test]
fn semantic_tokens_response_triggers_second_pass_after_format_workspace_edit() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("test.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() {}\n".to_string(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: false,
            semantic_tokens_legend: Some(LspSemanticTokensLegend {
                token_types: vec!["keyword".to_string()],
                token_modifiers: Vec::new(),
            }),
            ..Default::default()
        },
    });

    store.state.lsp.pending_format_on_save = Some(path.clone());
    let _ = store.dispatch(Action::LspApplyWorkspaceEdit {
        edit: LspWorkspaceEdit {
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
                    new_text: "// formatted\n".to_string(),
                }],
            }],
            ..Default::default()
        },
    });

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");
    assert_eq!(
        store
            .state
            .lsp
            .pending_second_semantic_pass_by_path
            .get(&path)
            .copied(),
        Some(version)
    );

    let result = store.dispatch(Action::LspSemanticTokens {
        path: path.clone(),
        version,
        tokens: vec![LspSemanticToken {
            line: 1,
            start: 0,
            length: 2,
            token_type: 0,
            modifiers: 0,
        }],
    });

    assert!(result.effects.iter().any(|effect| {
        matches!(
            effect,
            Effect::LspSemanticTokensRequest {
                path: effect_path,
                version: effect_version,
            } if effect_path == &path && *effect_version == version
        )
    }));
    assert!(!store
        .state
        .lsp
        .pending_second_semantic_pass_by_path
        .contains_key(&path));
}

#[test]
fn direct_editor_text_edit_schedules_syntax_highlight_recompute() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("format.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "use std::sync::mpsc;\nfn main() {}\n".to_string(),
    }));

    let result = store.dispatch(Action::Editor(EditorAction::ApplyTextEditToTab {
        pane: 0,
        tab_index: 0,
        start_byte: 0,
        end_byte: 0,
        text: "// formatted\n".to_string(),
    }));

    assert!(result.state_changed);
    assert!(result
        .effects
        .iter()
        .any(|effect| { matches!(effect, Effect::ComputeSyntaxHighlights { .. }) }));
}

#[test]
fn lsp_format_workspace_edit_propagates_syntax_highlight_recompute() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("format.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "use std::sync::mpsc;\nfn main() {}\n".to_string(),
    }));

    let result = store.dispatch(Action::LspApplyWorkspaceEdit {
        edit: LspWorkspaceEdit {
            changes: vec![LspWorkspaceFileEdit {
                path,
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
                    new_text: "// formatted\n".to_string(),
                }],
            }],
            ..Default::default()
        },
    });

    assert!(result.state_changed);
    assert!(
        result
            .effects
            .iter()
            .any(|effect| { matches!(effect, Effect::ComputeSyntaxHighlights { .. }) }),
        "format-applied LSP edits should propagate editor syntax-highlight effects"
    );
}

#[test]
fn semantic_tokens_response_is_deferred_until_boundary_after_identifier_input() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("timing.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: String::new(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: false,
            semantic_tokens_legend: Some(LspSemanticTokensLegend {
                token_types: vec!["keyword".to_string()],
                token_modifiers: Vec::new(),
            }),
            ..Default::default()
        },
    });

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('u')));
    let version_after_identifier = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");

    let deferred = store.dispatch(Action::LspSemanticTokens {
        path: path.clone(),
        version: version_after_identifier,
        tokens: vec![LspSemanticToken {
            line: 0,
            start: 0,
            length: 1,
            token_type: 0,
            modifiers: 0,
        }],
    });
    assert!(
        !deferred.state_changed,
        "identifier输入期间语义响应应只缓存，不应立即落屏"
    );
    assert!(
        store
            .state
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.semantic_segments_lines(0, 1))
            .is_none(),
        "非边界输入后不应出现语义高亮"
    );

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar(' ')));
    let version_after_boundary = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");

    let flushed = store.dispatch(Action::LspSemanticTokens {
        path: path.clone(),
        version: version_after_boundary,
        tokens: vec![LspSemanticToken {
            line: 0,
            start: 0,
            length: 1,
            token_type: 0,
            modifiers: 0,
        }],
    });
    assert!(flushed.state_changed);
    assert!(
        store
            .state
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.semantic_segments_lines(0, 1))
            .is_some(),
        "边界输入后新的语义响应应可见"
    );
}

#[test]
fn cursor_move_flush_preserves_sticky_semantic_kind_on_active_line() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("sticky.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path,
        content: "cin >> value\n".to_string(),
    }));

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");
    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("open tab exists");
        tab.buffer.set_cursor(0, 6);
        let _ = tab.set_semantic_highlight(
            version,
            vec![vec![
                sem_seg(0, 3, Some(crate::kernel::editor::HighlightKind::Variable)),
                sem_seg(3, "cin >> value".len(), None),
            ]],
        );
        let _ = tab.set_pending_semantic_highlight_from_slice(
            version,
            &[vec![sem_seg(0, "cin >> value".len(), None)]],
        );
    }

    let _ = store.dispatch(Action::RunCommand(Command::CursorRight));

    let expected = vec![
        sem_seg(0, 3, Some(crate::kernel::editor::HighlightKind::Variable)),
        sem_seg(3, "cin >> value".len(), None),
    ];
    let row = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .and_then(|tab| tab.semantic_segments_line(0))
        .expect("semantic row should remain visible");
    assert_eq!(row, expected.as_slice());
}

#[test]
fn semantic_tokens_full_empty_response_preserves_sticky_kind_left_of_cursor() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("sticky.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "cin >> value\n".to_string(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: false,
            semantic_tokens_legend: Some(LspSemanticTokensLegend::default()),
            ..Default::default()
        },
    });

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");
    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("open tab exists");
        tab.buffer.set_cursor(0, 7);
        let _ = tab.set_semantic_highlight(
            version,
            vec![
                vec![
                    sem_seg(0, 3, Some(crate::kernel::editor::HighlightKind::Variable)),
                    sem_seg(3, "cin >> value".len(), None),
                ],
                Vec::new(),
            ],
        );
    }

    let _ = store.dispatch(Action::LspSemanticTokens {
        path,
        version,
        tokens: Vec::new(),
    });
    let expected = vec![
        sem_seg(0, 3, Some(crate::kernel::editor::HighlightKind::Variable)),
        sem_seg(3, "cin >> value".len(), None),
    ];
    let row = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .and_then(|tab| tab.semantic_segments_line(0))
        .expect("semantic row should remain visible");
    assert_eq!(row, expected.as_slice());
}

#[test]
fn semantic_tokens_range_empty_response_preserves_sticky_kind_left_of_cursor() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("sticky.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "cin >> value\n".to_string(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: true,
            semantic_tokens_legend: Some(LspSemanticTokensLegend::default()),
            ..Default::default()
        },
    });

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");
    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("open tab exists");
        tab.buffer.set_cursor(0, 7);
        let _ = tab.set_semantic_highlight(
            version,
            vec![vec![
                sem_seg(0, 3, Some(crate::kernel::editor::HighlightKind::Variable)),
                sem_seg(3, "cin >> value".len(), None),
            ]],
        );
    }

    let result = store.dispatch(Action::LspSemanticTokensRange {
        path,
        version,
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 0,
            },
            end: LspPosition {
                line: 1,
                character: 0,
            },
        },
        tokens: Vec::new(),
    });

    assert!(
        !result.state_changed,
        "sticky merge should keep the visible semantic row unchanged"
    );
    let expected = vec![
        sem_seg(0, 3, Some(crate::kernel::editor::HighlightKind::Variable)),
        sem_seg(3, "cin >> value".len(), None),
    ];
    let row = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .and_then(|tab| tab.semantic_segments_line(0))
        .expect("semantic row should remain visible");
    assert_eq!(row, expected.as_slice());
}

// A ranged semantic-tokens response only covers part of the document. Applying it must
// not drop the committed highlight on lines outside the range — otherwise every line the
// server didn't re-send turns plain until a full response arrives (the same wipe as the
// completion flicker, reached through the range path).
#[test]
fn semantic_tokens_range_keeps_committed_highlight_outside_range() {
    let mut store = new_store();
    let path = store.state.workspace_root.join("ranged.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "alpha\nbeta\ngamma\n".to_string(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: true,
            semantic_tokens_legend: Some(LspSemanticTokensLegend::default()),
            ..Default::default()
        },
    });

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");
    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("open tab exists");
        let _ = tab.set_semantic_highlight(
            version,
            vec![
                vec![sem_seg(
                    0,
                    5,
                    Some(crate::kernel::editor::HighlightKind::Variable),
                )],
                vec![sem_seg(
                    0,
                    4,
                    Some(crate::kernel::editor::HighlightKind::Variable),
                )],
                vec![sem_seg(
                    0,
                    5,
                    Some(crate::kernel::editor::HighlightKind::Variable),
                )],
            ],
        );
    }

    // Response covers only line 1.
    let _ = store.dispatch(Action::LspSemanticTokensRange {
        path,
        version,
        range: LspRange {
            start: LspPosition {
                line: 1,
                character: 0,
            },
            end: LspPosition {
                line: 2,
                character: 0,
            },
        },
        tokens: Vec::new(),
    });

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("open tab exists");
    assert!(
        tab.semantic_segments_line(0)
            .is_some_and(|row| row.iter().any(|seg| seg.semantic_kind.is_some())),
        "line 0 lies outside the ranged response and must keep its committed highlight"
    );
    assert!(
        tab.semantic_segments_line(2)
            .is_some_and(|row| row.iter().any(|seg| seg.semantic_kind.is_some())),
        "line 2 lies outside the ranged response and must keep its committed highlight"
    );
}

#[test]
fn completion_confirm_requests_immediate_refresh_but_clears_preexisting_pending_semantic() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: String::new(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: false,
            semantic_tokens_legend: Some(LspSemanticTokensLegend {
                token_types: vec!["function".to_string()],
                token_modifiers: Vec::new(),
            }),
            ..Default::default()
        },
    });

    seed_visible_completion_for_active_tab(&mut store, 0, path.clone(), "println");

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");
    let pending_lines = vec![vec![crate::kernel::editor::SemanticSegment {
        start: 0,
        end: "println".len(),
        semantic_kind: Some(crate::kernel::editor::HighlightKind::Function),
    }]];
    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("open tab exists");
        let _ = tab.set_pending_semantic_highlight_from_slice(version, &pending_lines);
        assert!(tab.semantic_segments_lines(0, 1).is_none());
    }

    let result = store.dispatch(Action::CompletionConfirm);

    assert!(
        result.effects.iter().any(|effect| matches!(
            effect,
            Effect::LspSemanticTokensRequest { .. } | Effect::LspSemanticTokensRangeRequest { .. }
        )),
        "completion confirm should request an immediate semantic refresh after applying the insertion"
    );

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("open tab exists");
    assert!(
        tab.semantic_segments_lines(0, 1).is_none(),
        "the preexisting pending semantic snapshot is cleared by the edit before confirm-time flush runs"
    );
}

#[test]
fn completion_confirm_keeps_followup_semantic_response_eager_until_first_visible_refresh() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: String::new(),
    }));
    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: false,
            semantic_tokens_legend: Some(LspSemanticTokensLegend {
                token_types: vec!["function".to_string()],
                token_modifiers: Vec::new(),
            }),
            ..Default::default()
        },
    });

    seed_visible_completion_for_active_tab(&mut store, 0, path.clone(), "println");

    let confirm = store.dispatch(Action::CompletionConfirm);
    assert!(
        confirm.effects.iter().any(|effect| matches!(
            effect,
            Effect::LspSemanticTokensRequest { .. } | Effect::LspSemanticTokensRangeRequest { .. }
        )),
        "completion confirm should request semantic refresh"
    );
    assert!(
        store.state.lsp.eager_semantic_refresh_paths.contains(&path),
        "completion confirm should arm eager semantic refresh for follow-up edits"
    );

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('i')));
    let version_after_identifier = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");

    let eager = store.dispatch(Action::LspSemanticTokens {
        path: path.clone(),
        version: version_after_identifier,
        tokens: vec![LspSemanticToken {
            line: 0,
            start: 0,
            length: 1,
            token_type: 0,
            modifiers: 0,
        }],
    });
    assert!(
        eager.state_changed,
        "the first semantic response after completion follow-up typing should flush immediately"
    );
    assert!(
        store
            .state
            .editor
            .pane(0)
            .and_then(|pane| pane.active_tab())
            .and_then(|tab| tab.semantic_segments_lines(0, 1))
            .is_some(),
        "the eager semantic response should become visible immediately"
    );
    assert!(
        !store.state.lsp.eager_semantic_refresh_paths.contains(&path),
        "eager semantic refresh should be consumed after the first visible semantic response"
    );

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('n')));
    let version_after_second_identifier = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");

    let deferred = store.dispatch(Action::LspSemanticTokens {
        path,
        version: version_after_second_identifier,
        tokens: vec![LspSemanticToken {
            line: 0,
            start: 0,
            length: 1,
            token_type: 0,
            modifiers: 0,
        }],
    });
    assert!(
        !deferred.state_changed,
        "after eager refresh is consumed, plain identifier typing should return to deferred semantic flush"
    );
}

#[test]
fn completion_confirm_renders_dirty_line_with_local_keyword_fallback_before_async_patch() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.rs");
    let opened = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "foo()\n".to_string(),
    }));
    let (tab_id, version, language, rope, tree, segments) =
        first_compute_syntax_effect(&opened.effects)
            .expect("open should schedule syntax highlight");
    let patches =
        crate::kernel::editor::compute_highlight_patches(language, &tree, &rope, &segments);
    let _ = store.dispatch(Action::Editor(EditorAction::ApplySyntaxHighlightPatches {
        tab_id,
        version,
        patches,
    }));
    let initial_tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("open tab exists");
    assert!(
        initial_tab
            .highlight_lines_shared(0, 1)
            .expect("syntax available")[0]
            .iter()
            .any(|span| matches!(
                span.kind,
                crate::kernel::editor::HighlightKind::Function
                    | crate::kernel::editor::HighlightKind::Method
            )),
        "test setup should start from a line with reusable non-opaque syntax spans"
    );

    seed_visible_completion_for_active_tab(&mut store, 0, path.clone(), "let ");

    let result = store.dispatch(Action::CompletionConfirm);
    assert!(
        result
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::ComputeSyntaxHighlights { .. })),
        "completion confirm should schedule syntax recompute for the edited line"
    );

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("open tab exists");
    assert_eq!(tab.buffer.text(), "let foo()\n");

    let rendered = tab.highlight_lines_shared(0, 1).expect("syntax available");
    assert!(rendered[0].iter().any(|span| {
        matches!(
            span.kind,
            crate::kernel::editor::HighlightKind::Keyword
                | crate::kernel::editor::HighlightKind::KeywordControl
        ) && span.start == 0
            && 0 < span.end
    }));
}

#[test]
fn completion_confirm_seeds_cpp_type_highlight_before_semantic_response() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "ve".to_string(),
    }));
    let _ = store.dispatch(Action::Editor(EditorAction::PlaceCursor {
        pane: 0,
        row: 0,
        col: 2,
        granularity: Granularity::Char,
    }));

    let mut item = test_completion_item(1, "vector");
    item.kind = serde_json::to_value(lsp_types::CompletionItemKind::CLASS)
        .ok()
        .and_then(|value| value.as_u64())
        .map(|value| value as u32);
    item.insert_text_format = LspInsertTextFormat::Snippet;
    item.insert_text = "vector<$0>".to_string();
    seed_visible_completion_item_for_active_tab(&mut store, 0, path, item);

    let _ = store.dispatch(Action::CompletionConfirm);
    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('i')));
    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('n')));
    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('t')));

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("open tab exists");
    assert_eq!(tab.buffer.text(), "vector<int>");

    let semantic = tab
        .semantic_segments_line(0)
        .expect("completion confirm should seed a visible semantic line");
    let line = tab.buffer.rope().line(0).to_string();
    let line = line.trim_end_matches(['\n', '\r']);
    assert!(semantic.iter().any(|token| {
        token.semantic_kind == Some(crate::kernel::editor::HighlightKind::Type)
            && line
                .get(token.start..token.end)
                .is_some_and(|text| text.contains("vector"))
    }));
}

// Reproduces the reported flicker: accepting a completion makes already-highlighted
// code turn plain (white) before the async semantic refresh repaints it. Two distinct
// wipes are exercised here:
//   1. other lines: the confirm-time flush must not replace the whole committed
//      semantic state with the single seeded line.
//   2. the edited line itself: the completed token must be colored *on top of* the
//      reconciled line, preserving every other token that was already highlighted,
//      instead of rebuilding the line as [none][token][none].
#[test]
fn completion_confirm_keeps_semantic_highlight_on_unedited_lines() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let path = store.state.workspace_root.join("main.cpp");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        // line 1 holds unrelated, already-highlighted tokens on *both* sides of the
        // completion prefix ("ve"): `x` before the cursor and `y` after it. Both must
        // survive the edit, so the sticky-before-cursor heuristic alone is not enough.
        content: "Widget w;\nx ve y".to_string(),
    }));
    let _ = store.dispatch(Action::Editor(EditorAction::PlaceCursor {
        pane: 0,
        row: 1,
        col: 4,
        granularity: Granularity::Char,
    }));

    // Commit a full-document semantic snapshot: both lines are already highlighted,
    // mirroring a clangd semantic-tokens response that arrived before the edit.
    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .expect("open tab exists");
    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("open tab exists");
        let _ = tab.set_semantic_highlight(
            version,
            vec![
                vec![crate::kernel::editor::SemanticSegment {
                    start: 0,
                    end: "Widget".len(),
                    semantic_kind: Some(crate::kernel::editor::HighlightKind::Type),
                }],
                vec![
                    crate::kernel::editor::SemanticSegment {
                        start: 0,
                        end: "x".len(),
                        semantic_kind: Some(crate::kernel::editor::HighlightKind::Variable),
                    },
                    crate::kernel::editor::SemanticSegment {
                        // `y` sits after the completion prefix (byte 5..6 of "x ve y").
                        start: "x ve ".len(),
                        end: "x ve y".len(),
                        semantic_kind: Some(crate::kernel::editor::HighlightKind::Variable),
                    },
                ],
            ],
        );
        assert!(
            tab.semantic_segments_line(0).is_some(),
            "test setup: line 0 starts highlighted"
        );
    }

    let mut item = test_completion_item(1, "vector");
    item.kind = serde_json::to_value(lsp_types::CompletionItemKind::CLASS)
        .ok()
        .and_then(|value| value.as_u64())
        .map(|value| value as u32);
    seed_visible_completion_item_for_active_tab(&mut store, 0, path, item);

    let _ = store.dispatch(Action::CompletionConfirm);

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("open tab exists");
    assert_eq!(
        tab.buffer.rope().line(0).to_string().trim_end(),
        "Widget w;"
    );
    assert_eq!(
        tab.buffer.rope().line(1).to_string().trim_end(),
        "x vector y"
    );

    // (1) the untouched line keeps its existing semantic color.
    let line0 = tab
        .semantic_segments_line(0)
        .expect("line 0 semantic highlight must survive a completion on another line");
    assert!(
        line0.iter().any(|seg| {
            seg.semantic_kind == Some(crate::kernel::editor::HighlightKind::Type)
                && seg.start == 0
                && seg.end >= "Widget".len()
        }),
        "line 0's Type highlight on `Widget` must be preserved, got {line0:?}"
    );

    // (2) the edited line keeps the unrelated tokens on both sides of the cursor *and*
    // colors the completed identifier — none should wait for the async refresh.
    let line1 = tab
        .semantic_segments_line(1)
        .expect("line 1 semantic highlight must exist after confirm");
    assert!(
        line1.iter().any(|seg| {
            seg.semantic_kind == Some(crate::kernel::editor::HighlightKind::Variable)
                && seg.start == 0
                && seg.end == "x".len()
        }),
        "the `x` token before the cursor must keep its Variable color, got {line1:?}"
    );
    let vector_start = "x ".len();
    let vector_end = "x vector".len();
    assert!(
        line1.iter().any(|seg| {
            seg.semantic_kind == Some(crate::kernel::editor::HighlightKind::Type)
                && seg.start <= vector_start
                && seg.end >= vector_end
        }),
        "the completed `vector` token must be colored as Type, got {line1:?}"
    );
    let y_start = "x vector ".len();
    let y_end = "x vector y".len();
    assert!(
        line1.iter().any(|seg| {
            seg.semantic_kind == Some(crate::kernel::editor::HighlightKind::Variable)
                && seg.start <= y_start
                && seg.end >= y_end
        }),
        "the `y` token after the cursor must keep its Variable color, got {line1:?}"
    );
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
fn completion_plain_text_keeps_server_text_literal() {
    let insertion = CompletionInsertion::from_plain_text("println!()".to_string());
    assert_eq!(insertion.text, "println!()");
    assert!(insertion.cursor.is_none());
    assert!(insertion.selection.is_none());

    let insertion = CompletionInsertion::from_plain_text("no_parens".to_string());
    assert_eq!(insertion.text, "no_parens");
    assert!(insertion.cursor.is_none());
    assert!(insertion.selection.is_none());
}

#[test]
fn snippet_tab_navigation_moves_between_placeholders() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;

    let path = store.state.workspace_root.join("snippet.rs");
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: String::new(),
    }));

    let tab_size = store.state.editor.config.tab_size;
    let insertion = CompletionInsertion::from_snippet("fn ${1:name}(${2:arg}) { $0 }");

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("tab exists");
        assert!(tab.insert_text(&insertion.text, tab_size));
        apply_completion_insertion_cursor(tab, &insertion, tab_size);
    }

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert_eq!(tab.buffer.text(), "fn name(arg) {  }");
    assert_eq!(tab.buffer.cursor(), (0, "fn name".len()));
    assert!(tab
        .buffer
        .selection()
        .is_some_and(|sel| sel.range() == ((0, 3), (0, 7))));

    let _ = store.dispatch(Action::RunCommand(Command::InsertTab));
    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert!(tab
        .buffer
        .selection()
        .is_some_and(|sel| sel.range() == ((0, 8), (0, 11))));
    assert_eq!(tab.buffer.cursor(), (0, 11));

    let _ = store.dispatch(Action::RunCommand(Command::SnippetPrevPlaceholder));
    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert!(tab
        .buffer
        .selection()
        .is_some_and(|sel| sel.range() == ((0, 3), (0, 7))));
    assert_eq!(tab.buffer.cursor(), (0, 7));

    let _ = store.dispatch(Action::RunCommand(Command::InsertTab));
    let _ = store.dispatch(Action::RunCommand(Command::InsertTab));
    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert!(tab.buffer.selection().is_none());
    assert_eq!(tab.buffer.cursor(), (0, 15));
    assert!(tab.snippet_active_range().is_some());

    let _ = store.dispatch(Action::RunCommand(Command::InsertChar('x')));
    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert_eq!(tab.buffer.text(), "fn name(arg) { x }");
    assert!(tab.snippet_active_range().is_none());
    assert!(tab.buffer.selection().is_none());
    assert_eq!(tab.buffer.cursor(), (0, 16));

    let _ = store.dispatch(Action::RunCommand(Command::InsertTab));
    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    assert_eq!(tab.buffer.text(), "fn name(arg) { x\t }");
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

    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content: "fn main() {}\n".to_string(),
    }));

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
