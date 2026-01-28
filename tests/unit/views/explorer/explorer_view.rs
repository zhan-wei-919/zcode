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
    let theme = UiTheme::default();
    let row = FileTreeRow {
        id: NodeId::null(),
        depth: 0,
        name: "main.rs".into(),
        is_dir: false,
        is_expanded: false,
        load_state: LoadState::Loaded,
    };

    let render = |status: Option<GitFileStatus>| {
        let line = view.render_row(&row, false, status, 20, &theme);
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
            .chars()
            .next_back()
            .unwrap_or('\0')
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
