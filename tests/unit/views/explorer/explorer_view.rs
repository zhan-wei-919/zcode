use super::*;
use crate::models::LoadState;
use slotmap::Key;

#[test]
fn test_explorer_view_new() {
    let view = ExplorerView::new();
    assert!(view.area.is_none());
}

#[test]
fn test_explorer_active_open_file_uses_header_bold_style() {
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

    let (_left_pad, row_style) = view.render_row_parts(&row, false, true, 20, &theme);

    assert_eq!(row_style.fg, Some(theme.header_fg));
    assert!(row_style.mods.contains(crate::ui::core::style::Mod::BOLD));
}

#[test]
fn test_explorer_selected_style_overrides_active_open_file_style() {
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

    let (_left_pad, row_style) = view.render_row_parts(&row, true, true, 20, &theme);

    assert_eq!(row_style.bg, Some(theme.palette_selected_bg));
    assert_eq!(row_style.fg, Some(theme.palette_selected_fg));
}
