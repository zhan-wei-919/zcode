use crate::ui::core::geom::Rect as UiRect;

pub(super) fn centered_rect_ui(width_percent: u16, height: u16, area: UiRect) -> UiRect {
    let width = area.w.saturating_mul(width_percent).saturating_div(100);
    let min_width = 10.min(area.w);
    let width = width.max(min_width).min(area.w);

    let min_height = 3.min(area.h);
    let height = height.max(min_height).min(area.h);

    let x = area.x + (area.w.saturating_sub(width) / 2);
    let y = area.y + (area.h.saturating_sub(height) / 2);

    UiRect::new(x, y, width, height)
}
