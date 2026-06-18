use super::*;
use crate::kernel::editor::{EditorPaneState, EditorTabState, TabId};
use crate::kernel::services::ports::EditorConfig;
use crate::ui::core::geom::Rect;
use std::path::PathBuf;

fn pane_with_text(config: &EditorConfig, text: &str) -> EditorPaneState {
    let mut pane = EditorPaneState::new(config);
    pane.tabs.push(EditorTabState::from_file(
        TabId::new(1),
        PathBuf::from("test.rs"),
        text,
        config,
    ));
    pane.active = 0;
    pane
}

#[test]
fn layout_hides_vertical_scrollbar_when_file_fits_viewport() {
    let config = EditorConfig::default();
    let pane = pane_with_text(&config, "line 1\nline 2\n");
    let layout = compute_editor_pane_layout(Rect::new(0, 0, 40, 10), &pane, &config);

    assert!(layout.v_scrollbar_area.is_none());
    assert_eq!(
        layout.content_area.w + layout.gutter_width,
        layout.editor_area.w
    );
}

#[test]
fn layout_shows_vertical_scrollbar_when_file_exceeds_viewport() {
    let config = EditorConfig::default();
    let text = (0..120)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let pane = pane_with_text(&config, &text);

    let layout = compute_editor_pane_layout(Rect::new(0, 0, 40, 10), &pane, &config);

    let scrollbar = layout
        .v_scrollbar_area
        .expect("vertical scrollbar should be visible");
    assert_eq!(scrollbar.w, 1);
    assert_eq!(scrollbar.h, layout.content_area.h);
    assert_eq!(scrollbar.x, layout.content_area.right());
    assert_eq!(
        layout.content_area.w + layout.gutter_width + scrollbar.w,
        layout.editor_area.w
    );
}

// ---- compute_pane_rects: 单编辑区，恒返回铺满 area 的单一矩形 ----

#[test]
fn pane_rects_fills_area() {
    for area in [
        Rect::new(2, 3, 80, 24),
        Rect::new(0, 0, 1, 1),
        Rect::new(5, 5, 40, 12),
    ] {
        let rects = compute_pane_rects(area);
        assert_eq!(rects.outer, vec![area]);
        assert_eq!(rects.inner, vec![area]);
        // inner 与 outer 恒等。
        assert_eq!(rects.outer, rects.inner);
    }
}
