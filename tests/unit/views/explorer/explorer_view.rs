use super::*;
use crate::models::LoadState;
use slotmap::Key;

#[test]
fn test_explorer_view_new() {
    let view = ExplorerView::new();
    assert!(view.area.is_none());
}

#[test]
fn test_explorer_view_renders_single_git_marker_at_row_end() {
    let view = ExplorerView::new();
    let theme = Theme::default();
    let row = FileTreeRow {
        id: NodeId::null(),
        depth: 0,
        name: "main.rs".into(),
        is_dir: false,
        is_expanded: false,
        load_state: LoadState::Loaded,
    };

    let render = |status: Option<GitFileStatus>| {
        let (_left_pad, marker, _row_style, _marker_style) =
            view.render_row_parts(&row, false, status, 20, &theme);
        marker
    };

    assert_eq!(
        render(Some(GitFileStatus {
            index: None,
            worktree: Some(GitFileStatusKind::Modified),
        })),
        'M'
    );
    assert_eq!(
        render(Some(GitFileStatus {
            index: Some(GitFileStatusKind::Added),
            worktree: None,
        })),
        'A'
    );
    assert_eq!(
        render(Some(GitFileStatus {
            index: None,
            worktree: Some(GitFileStatusKind::Untracked),
        })),
        '?'
    );
    assert_eq!(
        render(Some(GitFileStatus {
            index: Some(GitFileStatusKind::Conflict),
            worktree: None,
        })),
        'U'
    );
    assert_eq!(render(None), ' ');
}
