use crate::kernel::editor::EditorPaneState;
use crate::ui::core::geom::Pos;
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
    if area.is_empty() {
        return None;
    }
    if row != area.y {
        return None;
    }
    if column < area.x || column >= area.right() {
        return None;
    }

    const PADDING_LEFT: u16 = 1;
    const PADDING_RIGHT: u16 = 1;
    const CLOSE_BUTTON_WIDTH: u16 = 2;
    const DIVIDER: u16 = 1;

    let right = area.right();
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
    if area.is_empty() {
        return None;
    }
    if row != area.y {
        return None;
    }
    if column < area.x || column >= area.right() {
        return None;
    }

    const PADDING_LEFT: u16 = 1;
    const PADDING_RIGHT: u16 = 1;
    const CLOSE_BUTTON_WIDTH: u16 = 2;
    const DIVIDER: u16 = 1;

    let right = area.right();
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

/// Returns an insertion index within the tab list for a given mouse position.
///
/// The index is relative to the current tab order (before any removal). Callers moving a tab within
/// the same pane should adjust indices after removal as needed.
pub fn tab_insertion_index(
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    column: u16,
    row: u16,
    hovered_tab: Option<usize>,
) -> Option<usize> {
    let area = layout.tab_area;
    if area.is_empty() {
        return None;
    }
    if row != area.y {
        return None;
    }
    if column < area.x || column >= area.right() {
        return None;
    }

    if pane.tabs.is_empty() {
        return Some(0);
    }

    const PADDING_LEFT: u16 = 1;
    const PADDING_RIGHT: u16 = 1;
    const CLOSE_BUTTON_WIDTH: u16 = 2;
    const DIVIDER: u16 = 1;

    let right = area.right();
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

        if hovered_tab == Some(i) {
            x = x.saturating_add(CLOSE_BUTTON_WIDTH).min(right);
        }
        let end = x;

        let width = end.saturating_sub(start);
        let mid = start.saturating_add(width / 2);
        if column < mid {
            return Some(i);
        }

        if i + 1 == pane.tabs.len() {
            break;
        }

        x = x.saturating_add(DIVIDER).min(right);
    }

    Some(pane.tabs.len())
}

/// Returns a screen x coordinate for a tab insertion marker.
///
/// The insertion index uses the same semantics as [`tab_insertion_index`]:
/// - `0` means "before the first tab"
/// - `tabs.len()` means "after the last tab"
pub fn tab_insertion_x(
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    hovered_tab: Option<usize>,
    insertion_index: usize,
) -> Option<u16> {
    let area = layout.tab_area;
    if area.is_empty() {
        return None;
    }

    if pane.tabs.is_empty() {
        return Some(area.x);
    }

    const PADDING_LEFT: u16 = 1;
    const PADDING_RIGHT: u16 = 1;
    const CLOSE_BUTTON_WIDTH: u16 = 2;
    const DIVIDER: u16 = 1;

    let right = area.right();
    let mut x = area.x;
    let mut last_end = area.x;

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

        if hovered_tab == Some(i) {
            x = x.saturating_add(CLOSE_BUTTON_WIDTH).min(right);
        }
        let end = x;
        last_end = end;

        if insertion_index <= i {
            return Some(start);
        }

        if i + 1 == pane.tabs.len() {
            break;
        }

        x = x.saturating_add(DIVIDER).min(right);
    }

    // Insert after the last tab (or after the last visible tab if truncated).
    Some(last_end.min(right.saturating_sub(1)))
}

pub fn hit_test_editor_mouse(
    layout: &EditorPaneLayout,
    column: u16,
    row: u16,
) -> Option<(u16, u16)> {
    if layout.content_area.is_empty() {
        return None;
    }
    if !layout
        .content_area
        .contains(Pos::new(column, row))
    {
        return None;
    }
    Some((
        column.saturating_sub(layout.content_area.x),
        row.saturating_sub(layout.content_area.y),
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/views/editor/hit_test.rs"]
mod tests;
