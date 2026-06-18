use crate::kernel::{EditorState, FocusTarget, OverlayKind, SearchViewport, UiState};
use ropey::RopeSlice;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) fn resolve_renamed_path(
    mut path: PathBuf,
    renames: &HashMap<PathBuf, PathBuf>,
) -> PathBuf {
    let mut hops = 0usize;
    while let Some(next) = renames.get(&path).cloned() {
        if next == path {
            break;
        }
        path = next;
        hops += 1;
        if hops > 16 {
            break;
        }
    }
    path
}

pub(super) fn search_viewport_for_focus(ui: &UiState) -> Option<SearchViewport> {
    match ui.focus {
        FocusTarget::Overlay if ui.overlay.active == Some(OverlayKind::Search) => {
            Some(SearchViewport::BottomPanel)
        }
        _ => None,
    }
}

pub(super) fn is_lsp_source_path(path: &Path) -> bool {
    crate::kernel::lsp_registry::is_lsp_source_path(path)
}

pub(super) fn line_len_chars(line: RopeSlice<'_>) -> usize {
    let mut len = 0usize;
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            break;
        }
        if ch == '\r' && matches!(it.peek(), Some('\n')) {
            break;
        }
        len += 1;
    }
    len
}

pub(super) fn find_open_tab(
    editor: &EditorState,
    preferred_pane: usize,
    path: &PathBuf,
) -> Option<(usize, usize)> {
    if let Some(pane_state) = editor.panes.get(preferred_pane) {
        if let Some(index) = pane_state
            .tabs
            .iter()
            .position(|t| t.path.as_ref() == Some(path))
        {
            return Some((preferred_pane, index));
        }
    }

    for (pane, pane_state) in editor.panes.iter().enumerate() {
        if pane == preferred_pane {
            continue;
        }
        if let Some(index) = pane_state
            .tabs
            .iter()
            .position(|t| t.path.as_ref() == Some(path))
        {
            return Some((pane, index));
        }
    }

    None
}

pub(super) fn open_tabs_for_path(editor: &EditorState, path: &PathBuf) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (pane, pane_state) in editor.panes.iter().enumerate() {
        for (tab_index, tab) in pane_state.tabs.iter().enumerate() {
            if tab.path.as_ref() == Some(path) {
                out.push((pane, tab_index));
            }
        }
    }
    out
}
