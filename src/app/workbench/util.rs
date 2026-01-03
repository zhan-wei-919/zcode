use ratatui::layout::Rect;

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
