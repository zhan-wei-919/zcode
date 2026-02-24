use super::*;
use crate::kernel::editor::{
    EditorPaneState, EditorTabState, HighlightKind, HighlightSpan, SearchBarMode, SnippetTabstop,
    TabId,
};
use crate::kernel::services::ports::{EditorConfig, Match};
use crate::models::{Granularity, Selection};
use crate::ui::backend::test::TestBackend;
use crate::ui::backend::Backend;
use crate::ui::core::geom::Rect;
use crate::ui::core::painter::{PaintCmd, Painter};
use crate::ui::core::theme::Theme;
use std::path::PathBuf;

fn default_render_options(show_vertical_scrollbar: bool) -> crate::views::EditorPaneRenderOptions {
    crate::views::EditorPaneRenderOptions {
        show_vertical_scrollbar,
        ..Default::default()
    }
}

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
        default_render_options(true),
        None,
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
        default_render_options(true),
        None,
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
fn paint_editor_pane_search_bar_draws_nav_buttons() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    pane.search_bar.show(SearchBarMode::Search);

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 10), &pane, &config);

    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &Theme::default(),
        default_render_options(true),
        None,
    );

    let texts: Vec<&str> = painter
        .cmds()
        .iter()
        .filter_map(|cmd| match cmd {
            PaintCmd::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();

    assert!(texts.contains(&" ▲ ▼ ✕"));
}

#[test]
fn paint_editor_pane_search_matches_use_match_and_current_match_backgrounds() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    let tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "hello world hello\n",
        &config,
    );
    pane.tabs.push(tab);
    pane.active = 0;

    pane.search_bar.show(SearchBarMode::Search);
    pane.search_bar.search_text = "hello".to_string();
    pane.search_bar.matches = vec![Match::new(0, 5, 0, 0), Match::new(12, 17, 0, 12)];
    pane.search_bar.current_match_index = Some(1);

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);
    let theme = Theme::default();
    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &theme,
        default_render_options(true),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let y = layout.content_area.y;
    let first_match_cell = buf.cell(layout.content_area.x + 1, y).unwrap();
    let current_match_cell = buf.cell(layout.content_area.x + 13, y).unwrap();

    assert_eq!(first_match_cell.style.bg, Some(theme.search_match_bg));
    assert_eq!(
        current_match_cell.style.bg,
        Some(theme.search_current_match_bg)
    );
}

#[test]
fn paint_editor_pane_transient_row_highlight_applies_destination_background() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    pane.tabs.push(EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "alpha\nbeta\n",
        &config,
    ));
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);
    let theme = Theme::default();
    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &theme,
        crate::views::EditorPaneRenderOptions {
            transient_row_highlight: Some(crate::views::TransientRowHighlight { row: 0 }),
            ..default_render_options(true)
        },
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let row0 = layout.content_area.y;
    let row1 = layout.content_area.y + 1;
    let highlighted = buf.cell(layout.content_area.x + 1, row0).unwrap();
    let untouched = buf.cell(layout.content_area.x + 1, row1).unwrap();

    assert_eq!(highlighted.style.bg, Some(theme.search_current_match_bg));
    assert_ne!(untouched.style.bg, Some(theme.search_current_match_bg));
}

#[test]
fn paint_editor_pane_selection_background_overrides_search_match_background() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.txt"),
        "hello world hello\n",
        &config,
    );
    let mut selection = Selection::new((0, 12), Granularity::Char);
    selection.update_cursor((0, 17), tab.buffer.rope());
    tab.buffer.set_selection(Some(selection));
    pane.tabs.push(tab);
    pane.active = 0;

    pane.search_bar.show(SearchBarMode::Search);
    pane.search_bar.search_text = "hello".to_string();
    pane.search_bar.matches = vec![Match::new(0, 5, 0, 0), Match::new(12, 17, 0, 12)];
    pane.search_bar.current_match_index = Some(1);

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);
    let theme = Theme::default();
    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &theme,
        default_render_options(true),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let y = layout.content_area.y;
    let cell = buf.cell(layout.content_area.x + 13, y).unwrap();
    assert_eq!(cell.style.bg, Some(theme.palette_selected_bg));
}

#[test]
fn paint_editor_pane_snippet_placeholder_background_highlights_active_tabstop() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);

    let mut tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("snippet.rs"),
        "fn name(arg) {  }\n",
        &config,
    );
    tab.buffer.clear_selection();
    tab.buffer.set_cursor(0, 0);
    tab.begin_snippet_session(
        0,
        vec![
            SnippetTabstop {
                index: 1,
                start: 3,
                end: 7,
            },
            SnippetTabstop {
                index: 2,
                start: 8,
                end: 11,
            },
            SnippetTabstop {
                index: 0,
                start: 15,
                end: 15,
            },
        ],
    );
    pane.tabs.push(tab);
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);
    let theme = Theme::default();
    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &theme,
        default_render_options(true),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let y = layout.content_area.y;
    let highlighted = buf.cell(layout.content_area.x + 3, y).unwrap();
    let untouched = buf.cell(layout.content_area.x + 1, y).unwrap();
    assert_eq!(highlighted.symbol, "n");
    assert_eq!(highlighted.style.bg, Some(theme.palette_selected_bg));
    assert_eq!(untouched.style.bg, Some(theme.editor_bg));
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
        default_render_options(true),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    // A 4-space indent draws a guide at the start of the indent level: col 0 (0-based).
    let y = layout.content_area.y;
    let x_guide = layout.content_area.x;
    let x_code = layout.content_area.x + 4;
    assert_eq!(buf.cell(x_guide, y).unwrap().symbol, "\u{250A}");
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
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &theme,
        default_render_options(true),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let y = layout.content_area.y;
    let x_guide = layout.content_area.x;
    let cell = buf.cell(x_guide, y).unwrap();
    assert_eq!(cell.symbol, "\u{250A}");
    assert_eq!(cell.style.bg, Some(theme.palette_selected_bg));
    assert_eq!(cell.style.fg, Some(theme.indent_guide_fg));
    assert!(cell.style.mods.contains(crate::ui::core::style::Mod::DIM));
}

#[test]
fn paint_editor_pane_long_file_draws_vertical_scrollbar() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    let text = (0..120)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    pane.tabs.push(EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("long.rs"),
        &text,
        &config,
    ));
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);
    let scrollbar = layout
        .v_scrollbar_area
        .expect("vertical scrollbar should be visible");

    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &Theme::default(),
        default_render_options(true),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let mut thumb_cells = 0usize;
    let mut track_cells = 0usize;
    for y in scrollbar.y..scrollbar.bottom() {
        let cell = buf.cell(scrollbar.x, y).expect("scrollbar cell");
        if cell.symbol == "█" {
            thumb_cells += 1;
        } else if cell.symbol == "│" {
            track_cells += 1;
        }
    }

    assert!(thumb_cells > 0, "scrollbar thumb should be drawn");
    assert!(track_cells > 0, "scrollbar track should be drawn");
}

#[test]
fn paint_editor_pane_long_file_hides_vertical_scrollbar_when_not_hovered() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    let text = (0..120)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    pane.tabs.push(EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("long.rs"),
        &text,
        &config,
    ));
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);
    let scrollbar = layout
        .v_scrollbar_area
        .expect("vertical scrollbar should be visible");

    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &Theme::default(),
        default_render_options(false),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    for y in scrollbar.y..scrollbar.bottom() {
        let cell = buf.cell(scrollbar.x, y).expect("scrollbar cell");
        assert_ne!(cell.symbol, "█");
        assert_ne!(cell.symbol, "│");
    }
}

#[test]
fn paint_editor_pane_vertical_scrollbar_thumb_moves_with_line_offset() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);
    let text = (0..160)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    pane.tabs.push(EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("long.rs"),
        &text,
        &config,
    ));
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 6), &pane, &config);
    let scrollbar = layout
        .v_scrollbar_area
        .expect("vertical scrollbar should be visible");

    let thumb_top_at = |pane: &EditorPaneState| -> u16 {
        let mut painter = Painter::new();
        paint_editor_pane(
            &mut painter,
            &layout,
            pane,
            &config,
            &Theme::default(),
            default_render_options(true),
            None,
        );

        let mut backend = TestBackend::new(layout.area.w, layout.area.h);
        backend.draw(layout.area, painter.cmds());
        let buf = backend.buffer();

        (scrollbar.y..scrollbar.bottom())
            .find(|&y| buf.cell(scrollbar.x, y).is_some_and(|c| c.symbol == "█"))
            .expect("thumb top")
    };

    {
        let tab = pane
            .active_tab_mut()
            .expect("tab should exist for scrollbar test");
        tab.viewport.follow_cursor = false;
    }
    let top_before = thumb_top_at(&pane);

    {
        let tab = pane
            .active_tab_mut()
            .expect("tab should exist for scrollbar test");
        tab.viewport.line_offset = 60;
    }
    let top_after = thumb_top_at(&pane);

    assert!(top_after > top_before);
}

#[test]
fn paint_editor_tabs_active_tab_uses_selected_palette() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);

    let mut first = EditorTabState::untitled(TabId::new(1), &config);
    first.title = "Alpha.rs".to_string();
    let mut second = EditorTabState::untitled(TabId::new(2), &config);
    second.title = "Beta.rs".to_string();

    pane.tabs.push(first);
    pane.tabs.push(second);
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 40, 8), &pane, &config);
    let theme = Theme::default();
    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &theme,
        default_render_options(true),
        None,
    );

    let mut backend = TestBackend::new(layout.area.w, layout.area.h);
    backend.draw(layout.area, painter.cmds());
    let buf = backend.buffer();

    let y = layout.tab_area.y;
    let mut alpha_style = None;
    let mut beta_style = None;
    for x in layout.tab_area.x..layout.tab_area.right() {
        let cell = buf.cell(x, y).expect("tab row cell");
        if cell.symbol == "A" && alpha_style.is_none() {
            alpha_style = Some(cell.style);
        }
        if cell.symbol == "B" && beta_style.is_none() {
            beta_style = Some(cell.style);
        }
    }

    let alpha_style = alpha_style.expect("active tab glyph style");
    let beta_style = beta_style.expect("inactive tab glyph style");

    assert_eq!(alpha_style.bg, Some(theme.palette_selected_bg));
    assert_eq!(alpha_style.fg, Some(theme.palette_selected_fg));
    assert!(alpha_style.mods.contains(crate::ui::core::style::Mod::BOLD));

    assert_ne!(beta_style.bg, Some(theme.palette_selected_bg));
}

#[test]
fn paint_editor_tabs_truncate_titles_with_ellipsis_in_narrow_width() {
    let config = EditorConfig::default();
    let mut pane = EditorPaneState::new(&config);

    let mut first = EditorTabState::untitled(TabId::new(1), &config);
    first.title = "this-is-a-very-long-file-name.rs".to_string();
    let mut second = EditorTabState::untitled(TabId::new(2), &config);
    second.title = "another-very-long-file-name.rs".to_string();

    pane.tabs.push(first);
    pane.tabs.push(second);
    pane.active = 0;

    let layout = crate::views::compute_editor_pane_layout(Rect::new(0, 0, 14, 6), &pane, &config);
    let mut painter = Painter::new();
    paint_editor_pane(
        &mut painter,
        &layout,
        &pane,
        &config,
        &Theme::default(),
        default_render_options(true),
        None,
    );

    let tab_row_clip = Rect::new(
        layout.tab_area.x,
        layout.tab_area.y,
        layout.tab_area.w,
        1.min(layout.tab_area.h),
    );
    let mut has_ellipsis = false;
    for cmd in painter.cmds() {
        let PaintCmd::Text {
            pos, text, clip, ..
        } = cmd
        else {
            continue;
        };
        if pos.y != layout.tab_area.y {
            continue;
        }
        has_ellipsis |= text.contains('…');
        assert!(pos.x < layout.tab_area.right());
        assert_eq!(*clip, Some(tab_row_clip));
    }

    assert!(
        has_ellipsis,
        "tab titles should use ellipsis when compressed"
    );
}

#[test]
fn style_for_highlight_respects_span_boundaries() {
    let theme = Theme::default();
    let spans = vec![HighlightSpan {
        start: 2,
        end: 5,
        kind: HighlightKind::Comment,
    }];
    let mut idx = 0usize;
    let expected =
        crate::ui::core::style::Style::default().fg(theme.syntax_fg(SyntaxColorGroup::Comment));

    assert_eq!(style_for_highlight(Some(&spans), &mut idx, 0, &theme), None);
    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 2, &theme),
        Some(expected)
    );
    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 4, &theme),
        Some(expected)
    );
    assert_eq!(style_for_highlight(Some(&spans), &mut idx, 5, &theme), None);
}

#[test]
fn style_for_highlight_advances_across_gaps() {
    let theme = Theme::default();
    let spans = vec![
        HighlightSpan {
            start: 0,
            end: 2,
            kind: HighlightKind::Keyword,
        },
        HighlightSpan {
            start: 4,
            end: 7,
            kind: HighlightKind::String,
        },
    ];
    let mut idx = 0usize;
    let keyword_style =
        crate::ui::core::style::Style::default().fg(theme.syntax_fg(SyntaxColorGroup::Keyword));
    let string_style =
        crate::ui::core::style::Style::default().fg(theme.syntax_fg(SyntaxColorGroup::String));

    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 0, &theme),
        Some(keyword_style)
    );
    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 1, &theme),
        Some(keyword_style)
    );
    assert_eq!(style_for_highlight(Some(&spans), &mut idx, 2, &theme), None);
    assert_eq!(style_for_highlight(Some(&spans), &mut idx, 3, &theme), None);
    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 4, &theme),
        Some(string_style)
    );
    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 6, &theme),
        Some(string_style)
    );
    assert_eq!(style_for_highlight(Some(&spans), &mut idx, 7, &theme), None);
}

#[test]
fn style_for_highlight_distinguishes_keyword_control_from_keyword() {
    let theme = Theme::default();
    assert_ne!(
        theme.syntax_fg(SyntaxColorGroup::Keyword),
        theme.syntax_fg(SyntaxColorGroup::KeywordControl)
    );

    let spans = vec![
        HighlightSpan {
            start: 0,
            end: 1,
            kind: HighlightKind::Keyword,
        },
        HighlightSpan {
            start: 1,
            end: 2,
            kind: HighlightKind::KeywordControl,
        },
    ];
    let mut idx = 0usize;
    let keyword_style =
        crate::ui::core::style::Style::default().fg(theme.syntax_fg(SyntaxColorGroup::Keyword));
    let keyword_control_style = crate::ui::core::style::Style::default()
        .fg(theme.syntax_fg(SyntaxColorGroup::KeywordControl));

    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 0, &theme),
        Some(keyword_style)
    );
    assert_eq!(
        style_for_highlight(Some(&spans), &mut idx, 1, &theme),
        Some(keyword_control_style)
    );
}

#[test]
fn semantic_keyword_should_not_override_keyword_control() {
    let theme = Theme::default();
    let semantic_spans = vec![HighlightSpan {
        start: 0,
        end: 2,
        kind: HighlightKind::Keyword,
    }];

    let syntax_kind = Some(HighlightKind::KeywordControl);
    let mut semantic_idx = 0usize;
    let mut highlight_style = None;
    let opaque = syntax_kind.is_some_and(|kind| kind.is_opaque());
    if !opaque {
        highlight_style = style_for_highlight(Some(&semantic_spans), &mut semantic_idx, 0, &theme);
    }
    if highlight_style.is_none() {
        if let Some(kind) = syntax_kind {
            highlight_style = Some(
                crate::ui::core::style::Style::default().fg(theme.syntax_fg(kind.color_group())),
            );
        }
    }

    let expected = crate::ui::core::style::Style::default()
        .fg(theme.syntax_fg(SyntaxColorGroup::KeywordControl));
    assert_eq!(highlight_style, Some(expected));
}

#[test]
fn semantic_keyword_can_override_keyword() {
    let theme = Theme::default();
    let semantic_spans = vec![HighlightSpan {
        start: 0,
        end: 2,
        kind: HighlightKind::Keyword,
    }];

    let syntax_kind = Some(HighlightKind::Keyword);
    let mut semantic_idx = 0usize;
    let mut highlight_style = None;
    let opaque = syntax_kind.is_some_and(|kind| kind.is_opaque());
    if !opaque {
        highlight_style = style_for_highlight(Some(&semantic_spans), &mut semantic_idx, 0, &theme);
    }
    if highlight_style.is_none() {
        if let Some(kind) = syntax_kind {
            highlight_style = Some(
                crate::ui::core::style::Style::default().fg(theme.syntax_fg(kind.color_group())),
            );
        }
    }

    let expected =
        crate::ui::core::style::Style::default().fg(theme.syntax_fg(SyntaxColorGroup::Keyword));
    assert_eq!(highlight_style, Some(expected));
}

#[test]
fn style_for_highlight_cached_matches_non_cached_lookup() {
    let theme = Theme::default();
    let spans = vec![
        HighlightSpan {
            start: 0,
            end: 2,
            kind: HighlightKind::Keyword,
        },
        HighlightSpan {
            start: 4,
            end: 7,
            kind: HighlightKind::String,
        },
    ];

    let mut idx = 0usize;
    let mut state = HighlightCacheState::default();
    for byte in 0..8 {
        let expected = style_for_highlight(Some(&spans), &mut idx, byte, &theme);
        let actual = highlight_kind_cached(Some(&spans), &mut state, byte).map(|kind| {
            crate::ui::core::style::Style::default().fg(theme.syntax_fg(kind.color_group()))
        });
        assert_eq!(actual, expected, "byte offset {byte}");
    }
}

#[test]
fn build_syntax_highlights_returns_shared_for_contiguous_visible_lines() {
    let config = EditorConfig::default();
    let tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n",
        &config,
    );
    let visible = vec![0usize, 1usize, 2usize];

    let syntax = build_syntax_highlights(&tab, &visible).expect("syntax available");
    assert!(matches!(syntax, SyntaxHighlightLines::Shared(_)));
}

#[test]
fn build_syntax_highlights_returns_owned_for_sparse_visible_lines() {
    let config = EditorConfig::default();
    let tab = EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n",
        &config,
    );
    let visible = vec![0usize, 2usize];

    let syntax = build_syntax_highlights(&tab, &visible).expect("syntax available");
    assert!(matches!(syntax, SyntaxHighlightLines::Owned(_)));
}
