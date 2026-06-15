use super::*;
use crate::kernel::editor::{EditorPaneState, EditorTabState, TabId};
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::SplitDirection;
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

// ---- compute_pane_rects: 等价性基线（复刻 render_editor_panes 几何） ----

fn assert_single(rects: &PaneRects, area: Rect) {
    assert_eq!(rects.outer, vec![area]);
    assert_eq!(rects.inner, vec![area]);
    assert!(rects.separator.is_none());
    // inner 与 outer 当前恒等。
    assert_eq!(rects.outer, rects.inner);
}

#[test]
fn pane_rects_single_pane_fills_area() {
    let area = Rect::new(2, 3, 80, 24);
    for dir in [SplitDirection::Vertical, SplitDirection::Horizontal] {
        assert_single(&compute_pane_rects(area, 1, dir, 500), area);
    }
}

#[test]
fn pane_rects_more_than_two_panes_falls_back_to_single() {
    let area = Rect::new(0, 0, 80, 24);
    assert_single(
        &compute_pane_rects(area, 3, SplitDirection::Vertical, 500),
        area,
    );
    assert_single(
        &compute_pane_rects(area, 5, SplitDirection::Horizontal, 500),
        area,
    );
}

#[test]
fn pane_rects_vertical_degenerate_width_falls_back_to_single() {
    // available = area.w < 3 -> 单一矩形、无分隔线。
    for w in 0u16..3 {
        let area = Rect::new(1, 1, w, 10);
        assert_single(
            &compute_pane_rects(area, 2, SplitDirection::Vertical, 500),
            area,
        );
    }
}

#[test]
fn pane_rects_horizontal_degenerate_height_falls_back_to_single() {
    for h in 0u16..3 {
        let area = Rect::new(1, 1, 40, h);
        assert_single(
            &compute_pane_rects(area, 2, SplitDirection::Horizontal, 500),
            area,
        );
    }
}

#[test]
fn pane_rects_vertical_split_matches_render_math() {
    for &x in &[0u16, 5] {
        for &w in &[3u16, 4, 41, 80] {
            for &ratio in &[200u16, 500, 800] {
                let area = Rect::new(x, 2, w, 12);
                let rects = compute_pane_rects(area, 2, SplitDirection::Vertical, ratio);

                let total = w - 1;
                let left_width =
                    (((total as u32) * (ratio as u32) / 1000) as u16).clamp(1, total - 1);
                let right_width = total - left_width;

                let left = Rect::new(area.x, area.y, left_width, area.h);
                let sep = Rect::new(area.x + left_width, area.y, 1, area.h);
                let right = Rect::new(area.x + left_width + 1, area.y, right_width, area.h);

                assert_eq!(rects.outer, vec![left, right], "w={w} ratio={ratio}");
                assert_eq!(rects.inner, rects.outer);
                assert_eq!(rects.separator, Some((sep, SplitDirection::Vertical)));

                // 铺满且不重叠：left + sep + right == area.w
                assert_eq!(left.w + sep.w + right.w, area.w);
                assert_eq!(sep.x, left.right());
                assert_eq!(right.x, sep.x + 1);
            }
        }
    }
}

#[test]
fn pane_rects_horizontal_split_matches_render_math() {
    for &y in &[0u16, 3] {
        for &h in &[3u16, 4, 13, 24] {
            for &ratio in &[200u16, 500, 800] {
                let area = Rect::new(1, y, 40, h);
                let rects = compute_pane_rects(area, 2, SplitDirection::Horizontal, ratio);

                let total = h - 1;
                let top_height =
                    (((total as u32) * (ratio as u32) / 1000) as u16).clamp(1, total - 1);
                let bottom_height = total - top_height;

                let top = Rect::new(area.x, area.y, area.w, top_height);
                let sep = Rect::new(area.x, area.y + top_height, area.w, 1);
                let bottom = Rect::new(area.x, area.y + top_height + 1, area.w, bottom_height);

                assert_eq!(rects.outer, vec![top, bottom], "h={h} ratio={ratio}");
                assert_eq!(rects.inner, rects.outer);
                assert_eq!(rects.separator, Some((sep, SplitDirection::Horizontal)));

                assert_eq!(top.h + sep.h + bottom.h, area.h);
                assert_eq!(sep.y, top.bottom());
                assert_eq!(bottom.y, sep.y + 1);
            }
        }
    }
}
