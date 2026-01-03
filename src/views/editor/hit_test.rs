use crate::kernel::editor::EditorPaneState;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use super::layout::EditorPaneLayout;

pub fn hit_test_editor_tab(
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    column: u16,
    row: u16,
) -> Option<usize> {
    let area = layout.tab_area;
    if area.width == 0 || area.height == 0 {
        return None;
    }
    if row != area.y {
        return None;
    }
    if column < area.x || column >= area.x + area.width {
        return None;
    }

    const PADDING_LEFT: u16 = 1;
    const PADDING_RIGHT: u16 = 1;
    const DIVIDER: u16 = 1;

    let right = area.x + area.width;
    let mut x = area.x;

    for (i, tab) in pane.tabs.iter().enumerate() {
        if x >= right {
            break;
        }

        let start = x;
        x = x.saturating_add(PADDING_LEFT).min(right);

        let mut title_width = UnicodeWidthStr::width(tab.title.as_str());
        if tab.dirty {
            title_width = title_width.saturating_add(2);
        }
        x = x
            .saturating_add(title_width.min(u16::MAX as usize) as u16)
            .min(right);

        x = x.saturating_add(PADDING_RIGHT).min(right);
        let end = x;

        if column >= start && column < end {
            return Some(i);
        }

        if i + 1 == pane.tabs.len() {
            break;
        }

        x = x.saturating_add(DIVIDER).min(right);
    }

    None
}

pub fn hit_test_editor_mouse(
    layout: &EditorPaneLayout,
    column: u16,
    row: u16,
) -> Option<(u16, u16)> {
    if layout.content_area.width == 0 || layout.content_area.height == 0 {
        return None;
    }
    if !rect_contains(layout.content_area, column, row) {
        return None;
    }
    Some((
        column.saturating_sub(layout.content_area.x),
        row.saturating_sub(layout.content_area.y),
    ))
}

fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}
