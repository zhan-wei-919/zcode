use crate::kernel::editor::EditorPaneState;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use super::layout::EditorPaneLayout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabHitResult {
    Title(usize),
    CloseButton(usize),
}

pub fn hit_test_editor_tab(
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    column: u16,
    row: u16,
    hovered_tab: Option<usize>,
) -> Option<TabHitResult> {
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
    const CLOSE_BUTTON_WIDTH: u16 = 2;
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

        let is_hovered = hovered_tab == Some(i);
        let close_start = x;
        if is_hovered {
            x = x.saturating_add(CLOSE_BUTTON_WIDTH).min(right);
        }
        let end = x;

        if column >= start && column < end {
            if is_hovered && column >= close_start {
                return Some(TabHitResult::CloseButton(i));
            }
            return Some(TabHitResult::Title(i));
        }

        if i + 1 == pane.tabs.len() {
            break;
        }

        x = x.saturating_add(DIVIDER).min(right);
    }

    None
}

pub fn hit_test_tab_hover(
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    column: u16,
    row: u16,
    current_hovered: Option<usize>,
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
    const CLOSE_BUTTON_WIDTH: u16 = 2;
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

        let is_currently_hovered = current_hovered == Some(i);
        if is_currently_hovered {
            x = x.saturating_add(CLOSE_BUTTON_WIDTH).min(right);
        }
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
