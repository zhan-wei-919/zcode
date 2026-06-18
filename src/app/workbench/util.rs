use crate::ui::core::geom::Rect;

pub(super) fn centered_rect(width_percent: u16, height: u16, area: Rect) -> Rect {
    let width = area.w.saturating_mul(width_percent).saturating_div(100);
    let min_width = 10.min(area.w);
    let width = width.max(min_width).min(area.w);

    let min_height = 3.min(area.h);
    let height = height.max(min_height).min(area.h);

    let x = area.x + (area.w.saturating_sub(width) / 2);
    let y = area.y + (area.h.saturating_sub(height) / 2);

    Rect::new(x, y, width, height)
}

pub(super) fn rect_contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h
}

pub(super) fn sidebar_width(available: u16) -> u16 {
    if available == 0 {
        return 0;
    }

    let desired = available
        .saturating_mul(super::SIDEBAR_WIDTH_PERCENT)
        .saturating_div(100);
    clamp_sidebar_width(available, desired)
}

pub(super) fn clamp_sidebar_width(available: u16, desired: u16) -> u16 {
    if available == 0 {
        return 0;
    }

    // Keep some editor space so the UI remains usable even on small terminals.
    let min_width = super::SIDEBAR_MIN_WIDTH.min(available);
    let max_width = available.saturating_sub(10).max(min_width);

    desired.max(min_width).min(max_width)
}
