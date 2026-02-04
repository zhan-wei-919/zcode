use crate::kernel::editor::action::EditorAction;
use crate::kernel::editor::EditorState;
use crate::kernel::services::ports::EditorConfig;
use std::path::PathBuf;

fn open(editor: &mut EditorState, pane: usize, name: &str) {
    let _ = editor.dispatch_action(EditorAction::OpenFile {
        pane,
        path: PathBuf::from(name),
        content: name.to_string(),
    });
}

fn tab_titles(editor: &EditorState, pane: usize) -> Vec<String> {
    editor
        .pane(pane)
        .unwrap()
        .tabs
        .iter()
        .map(|t| t.title.clone())
        .collect()
}

#[test]
fn move_tab_within_same_pane_reorders_tabs() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);

    open(&mut editor, 0, "a.rs");
    open(&mut editor, 0, "b.rs");
    open(&mut editor, 0, "c.rs");

    let tab_id = editor.pane(0).unwrap().tabs[0].id;
    let (changed, _effects) = editor.dispatch_action(EditorAction::MoveTab {
        tab_id,
        from_pane: 0,
        to_pane: 0,
        to_index: 3, // move to end (insertion index before removal)
    });
    assert!(changed);
    assert_eq!(tab_titles(&editor, 0), vec!["b.rs", "c.rs", "a.rs"]);
}

#[test]
fn move_tab_within_same_pane_moves_active_tab_and_keeps_it_active() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);

    open(&mut editor, 0, "a.rs");
    open(&mut editor, 0, "b.rs");
    open(&mut editor, 0, "c.rs");

    let _ = editor.dispatch_action(EditorAction::SetActiveTab { pane: 0, index: 1 });

    let tab_id = editor.pane(0).unwrap().tabs[1].id;
    let (changed, _effects) = editor.dispatch_action(EditorAction::MoveTab {
        tab_id,
        from_pane: 0,
        to_pane: 0,
        to_index: 0,
    });
    assert!(changed);
    assert_eq!(tab_titles(&editor, 0), vec!["b.rs", "a.rs", "c.rs"]);
    assert_eq!(editor.pane(0).unwrap().active, 0);
}

#[test]
fn move_tab_across_panes_removes_from_source_and_inserts_into_target() {
    let config = EditorConfig::default();
    let mut editor = EditorState::new(config);
    assert!(editor.ensure_panes(2));

    open(&mut editor, 0, "a.rs");
    open(&mut editor, 0, "b.rs");
    open(&mut editor, 1, "x.rs");

    // Make sure b.rs is active in pane 0 (OpenFile activates the last opened).
    assert_eq!(tab_titles(&editor, 0), vec!["a.rs", "b.rs"]);
    assert_eq!(editor.pane(0).unwrap().active, 1);

    let tab_id = editor.pane(0).unwrap().tabs[1].id;
    let (changed, _effects) = editor.dispatch_action(EditorAction::MoveTab {
        tab_id,
        from_pane: 0,
        to_pane: 1,
        to_index: 0,
    });
    assert!(changed);

    assert_eq!(tab_titles(&editor, 0), vec!["a.rs"]);
    assert_eq!(editor.pane(0).unwrap().active, 0);

    assert_eq!(tab_titles(&editor, 1), vec!["b.rs", "x.rs"]);
    assert_eq!(editor.pane(1).unwrap().active, 0);
}
