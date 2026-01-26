use ratatui::layout::Rect;

use crate::kernel::BottomPanelTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ActivityItem {
    Explorer,
    Search,
    Problems,
    Results,
    Logs,
    Find,
    Replace,
    Palette,
    Settings,
}

const ACTIVITY_ITEMS: [ActivityItem; 9] = [
    ActivityItem::Explorer,
    ActivityItem::Search,
    ActivityItem::Problems,
    ActivityItem::Results,
    ActivityItem::Logs,
    ActivityItem::Find,
    ActivityItem::Replace,
    ActivityItem::Palette,
    ActivityItem::Settings,
];

impl ActivityItem {
    pub(super) fn icon(self) -> char {
        match self {
            ActivityItem::Explorer => 'E',
            ActivityItem::Search => 'S',
            ActivityItem::Problems => '!',
            ActivityItem::Results => '*',
            ActivityItem::Logs => 'L',
            ActivityItem::Find => '/',
            ActivityItem::Replace => 'H',
            ActivityItem::Palette => 'P',
            ActivityItem::Settings => ',',
        }
    }

    pub(super) fn bottom_panel_tab(self) -> Option<BottomPanelTab> {
        match self {
            ActivityItem::Problems => Some(BottomPanelTab::Problems),
            ActivityItem::Results => Some(BottomPanelTab::SearchResults),
            ActivityItem::Logs => Some(BottomPanelTab::Logs),
            _ => None,
        }
    }
}

pub(super) fn activity_items() -> &'static [ActivityItem] {
    &ACTIVITY_ITEMS
}

pub(super) fn activity_slot_height(height: u16) -> u16 {
    let items = ACTIVITY_ITEMS.len() as u16;
    if items == 0 || height == 0 {
        return 1;
    }

    if height >= items.saturating_mul(3) {
        3
    } else if height >= items.saturating_mul(2) {
        2
    } else {
        1
    }
}

pub(super) fn activity_item_at_row(index: u16) -> Option<ActivityItem> {
    ACTIVITY_ITEMS.get(index as usize).copied()
}

pub(super) fn centered_rect(width_percent: u16, height: u16, area: Rect) -> Rect {
    let width = area.width.saturating_mul(width_percent).saturating_div(100);
    let min_width = 10.min(area.width);
    let width = width.max(min_width).min(area.width);

    let min_height = 3.min(area.height);
    let height = height.max(min_height).min(area.height);

    let x = area.x + (area.width.saturating_sub(width) / 2);
    let y = area.y + (area.height.saturating_sub(height) / 2);

    Rect::new(x, y, width, height)
}

pub(super) fn rect_contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

pub(super) fn bottom_panel_height(body_height: u16) -> u16 {
    let max_height = body_height.saturating_sub(1);
    if max_height == 0 {
        return 0;
    }

    let desired = body_height.saturating_div(3);
    desired.max(6).min(max_height)
}

pub(super) fn sidebar_width(available: u16) -> u16 {
    if available == 0 {
        return 0;
    }

    let desired = available
        .saturating_mul(super::SIDEBAR_WIDTH_PERCENT)
        .saturating_div(100);
    let min_width = super::SIDEBAR_MIN_WIDTH.min(available);
    let max_width = available.saturating_sub(10).max(min_width);

    desired.max(min_width).min(max_width)
}
