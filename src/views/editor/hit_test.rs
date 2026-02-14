use crate::core::text_window;
use crate::kernel::editor::{EditorPaneState, SearchBarField, SearchBarState};
use crate::ui::core::geom::Pos;
use unicode_width::UnicodeWidthStr;

use super::layout::{EditorPaneLayout, VerticalScrollbarMetrics};
use super::tab_row::compute_tab_row_layout;

const SEARCH_NAV_BUTTONS_WIDTH: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabHitResult {
    Title(usize),
    CloseButton(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBarHitResult {
    PrevMatch,
    NextMatch,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorVerticalScrollbarHitResult {
    Thumb { row: u16 },
    Track { row: u16 },
}

pub fn hit_test_search_bar(
    layout: &EditorPaneLayout,
    state: &SearchBarState,
    column: u16,
    row: u16,
) -> Option<SearchBarHitResult> {
    if !state.visible {
        return None;
    }

    let area = layout.search_area?;
    if area.is_empty() || row != area.y {
        return None;
    }
    if column < area.x || column >= area.right() {
        return None;
    }

    let (nav_x, nav_y) = search_bar_nav_origin(area.x, area.y, area.w, state);
    if row != nav_y {
        return None;
    }

    if column == nav_x.saturating_add(1) {
        return Some(SearchBarHitResult::PrevMatch);
    }
    if column == nav_x.saturating_add(3) {
        return Some(SearchBarHitResult::NextMatch);
    }
    if column == nav_x.saturating_add(5) {
        return Some(SearchBarHitResult::Close);
    }

    None
}

fn search_bar_nav_origin(
    area_x: u16,
    area_y: u16,
    area_w: u16,
    state: &SearchBarState,
) -> (u16, u16) {
    let match_info = search_bar_match_info(state);
    let case_indicator = if state.case_sensitive { "[Aa]" } else { "[aa]" };
    let regex_indicator = if state.use_regex { "[.*]" } else { "[  ]" };
    let (visible_text, _start) = windowed_search_text(
        state.search_text.as_str(),
        state.cursor_pos,
        state.focused_field == SearchBarField::Search,
        area_w,
        case_indicator,
        regex_indicator,
        &match_info,
    );

    let mut x = area_x;
    x = x.saturating_add("Find: ".width() as u16);
    x = x.saturating_add(visible_text.width().min(u16::MAX as usize) as u16);
    x = x.saturating_add(1);
    x = x.saturating_add(case_indicator.width().min(u16::MAX as usize) as u16);
    x = x.saturating_add(regex_indicator.width().min(u16::MAX as usize) as u16);
    x = x.saturating_add(1);
    x = x.saturating_add(match_info.width().min(u16::MAX as usize) as u16);
    (x, area_y)
}

fn search_bar_match_info(state: &SearchBarState) -> String {
    if state.searching {
        "Searching...".to_string()
    } else if let Some(err) = state.last_error.as_deref() {
        format!("Error: {}", err)
    } else if state.matches.is_empty() {
        if state.search_text.is_empty() {
            String::new()
        } else {
            "No results".to_string()
        }
    } else {
        let current = state.current_match_index.map(|i| i + 1).unwrap_or(0);
        format!("{}/{}", current, state.matches.len())
    }
}

fn windowed_search_text<'a>(
    text: &'a str,
    cursor_pos: usize,
    focused: bool,
    area_width: u16,
    case_indicator: &str,
    regex_indicator: &str,
    match_info: &str,
) -> (&'a str, usize) {
    let prefix = "Find: ";
    let suffix_w = 1u16
        .saturating_add(case_indicator.width() as u16)
        .saturating_add(regex_indicator.width() as u16)
        .saturating_add(1)
        .saturating_add(match_info.width() as u16)
        .saturating_add(SEARCH_NAV_BUTTONS_WIDTH);
    let prefix_w = prefix.width() as u16;
    let available = area_width.saturating_sub(prefix_w).saturating_sub(suffix_w) as usize;
    let cursor = if focused { cursor_pos } else { text.len() }.min(text.len());
    let (start, end) = text_window::window(text, cursor, available);
    (&text[start..end], start)
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

    let row_layout = compute_tab_row_layout(area, pane, hovered_tab);
    for slot in row_layout.slots {
        if column < slot.start || column >= slot.end {
            continue;
        }
        if let Some(close_start) = slot.close_start {
            if column >= close_start && column < slot.close_end {
                return Some(TabHitResult::CloseButton(slot.index));
            }
        }
        if column < slot.hit_end {
            return Some(TabHitResult::Title(slot.index));
        }
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

    let row_layout = compute_tab_row_layout(area, pane, current_hovered);
    for slot in row_layout.slots {
        if column >= slot.start && column < slot.end {
            return Some(slot.index);
        }
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

    let row_layout = compute_tab_row_layout(area, pane, hovered_tab);
    for slot in row_layout.slots {
        let width = slot.end.saturating_sub(slot.start);
        let mid = slot.start.saturating_add(width / 2);
        if column < mid {
            return Some(slot.index);
        }
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
    let right = area.right();
    let row_layout = compute_tab_row_layout(area, pane, hovered_tab);
    if row_layout.slots.is_empty() {
        return Some(area.x);
    }

    let mut last_end = area.x;
    for slot in row_layout.slots {
        last_end = slot.end;
        if insertion_index <= slot.index {
            return Some(slot.start);
        }
    }

    // Insert after the last tab (or after the last visible tab if truncated).
    Some(last_end.min(right.saturating_sub(1)).max(area.x))
}

pub fn hit_test_editor_mouse(
    layout: &EditorPaneLayout,
    column: u16,
    row: u16,
) -> Option<(u16, u16)> {
    if layout.content_area.is_empty() {
        return None;
    }
    if !layout.content_area.contains(Pos::new(column, row)) {
        return None;
    }
    Some((
        column.saturating_sub(layout.content_area.x),
        row.saturating_sub(layout.content_area.y),
    ))
}

pub fn hit_test_editor_vertical_scrollbar(
    layout: &EditorPaneLayout,
    metrics: &VerticalScrollbarMetrics,
    column: u16,
    row: u16,
) -> Option<EditorVerticalScrollbarHitResult> {
    let area = layout.v_scrollbar_area?;
    let pos = Pos::new(column, row);
    if !area.contains(pos) {
        return None;
    }

    let row = row.saturating_sub(area.y);
    if metrics.thumb_area.contains(pos) {
        Some(EditorVerticalScrollbarHitResult::Thumb { row })
    } else {
        Some(EditorVerticalScrollbarHitResult::Track { row })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragHitResult {
    pub x: u16,
    pub y: u16,
    pub overflow_y: i16,
    pub past_right: bool,
}

pub fn hit_test_editor_mouse_drag(
    layout: &EditorPaneLayout,
    column: u16,
    row: u16,
) -> Option<DragHitResult> {
    let ca = layout.content_area;
    if ca.is_empty() {
        return None;
    }

    let past_right = column >= ca.right();
    let clamped_col = column.clamp(ca.x, ca.right().saturating_sub(1));
    let x = clamped_col.saturating_sub(ca.x);

    let clamped_row = row.clamp(ca.y, ca.bottom().saturating_sub(1));
    let y = clamped_row.saturating_sub(ca.y);

    let overflow_y = if row < ca.y {
        -(ca.y.saturating_sub(row) as i16)
    } else if row >= ca.bottom() {
        row.saturating_sub(ca.bottom().saturating_sub(1)) as i16
    } else {
        0
    };

    Some(DragHitResult {
        x,
        y,
        overflow_y,
        past_right,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/views/editor/hit_test.rs"]
mod tests;
