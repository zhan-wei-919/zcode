use super::*;
use crate::kernel::editor::{EditorPaneState, EditorTabState, SearchBarMode, TabId};
use crate::kernel::services::ports::EditorConfig;
use crate::models::{Granularity, Selection};
use crate::ui::backend::test::TestBackend;
use crate::ui::backend::Backend;
use crate::ui::core::geom::Rect;
use crate::ui::core::painter::{PaintCmd, Painter};
use crate::ui::core::theme::Theme;
use std::path::PathBuf;

#[test]
fn paint_editor_pane_no_tab_renders_empty_message() {
    let config = EditorConfig::default();
    let pane = EditorPaneState::new(&config);
    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 10), &pane, &config);

    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &Theme::default(),
        None,
        false,
    );

    let has_message = painter
        .cmds()
        .iter()
        .any(|cmd| matches!(cmd, PaintCmd::Text { text, .. } if text.contains("No file open")));
    assert!(has_message);
}

#[test]
fn paint_editor_pane_search_bar_draws_find_label() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    pane.search_bar.show(SearchBarMode::Search);
    pane.search_bar.search_text = "abc".to_string();
    pane.search_bar.cursor_pos = pane.search_bar.search_text.len();

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 10), &pane, &config);

    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &Theme::default(),
        None,
        false,
    );

    let texts: Vec<&str> = painter
        .cmds()
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();

    assert!(texts.contains(&"Find: "));
    assert!(texts.iter().any(|t| t.contains("abc")));
}

#[test]
fn paint_editor_pane_indent_guides_do_not_overwrite_code() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    pane.tabs.push(EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "    fn main() {}\n",
        &config,
    ));
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);

    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &Theme::default(),
        None,
        false,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    // A 4-space indent draws a guide at the last whitespace cell in the level: col 3 (0-based).
    let y = layout.content_area.y;
    let x_guide = layout.content_area.x + 3;
    let x_code = layout.content_area.x + 4;
    assert_eq!(buf.cell(x_guide, y).unwrap().symbol, "│");
    assert_eq!(buf.cell(x_code, y).unwrap().symbol, "f");
}

#[test]
fn paint_editor_pane_indent_guides_respect_selection_background() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "    fn main() {}\n",
        &config,
    );
    let mut sel = Selection::new((0, 0), Granularity::Char);
    sel.update_cursor((0, 6), tab.buffer.rope());
    tab.buffer.set_selection(Some(sel));
    pane.tabs.push(tab);
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);

    let theme = Theme::default();
    let mut painter = Painter::new();
    paint_editor_pane(&mut painter, &layout, &pane, &config, &theme, None, false);

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let y = layout.content_area.y;
    let x_guide = layout.content_area.x + 3;
    let cell = buf.cell(x_guide, y).unwrap();
    assert_eq!(cell.symbol, "│");
    assert_eq!(cell.style.bg, Some(theme.palette_selected_bg));
    assert_eq!(cell.style.fg, Some(theme.indent_guide_fg));
}
