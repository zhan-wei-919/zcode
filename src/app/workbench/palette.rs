use super::paint::centered_rect_ui;
use super::util;
use super::Workbench;
use crate::core::text_window;
use crate::kernel::palette::match_items;
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod, Style as UiStyle};
use unicode_width::UnicodeWidthStr;

pub(super) fn render(workbench: &Workbench, painter: &mut Painter, area: UiRect) {
    let popup_area = centered_rect_ui(90, 10, area);
    if popup_area.is_empty() {
        return;
    }

    let base_style = UiStyle::default()
        .bg(workbench.ui_theme.popup_bg)
        .fg(workbench.ui_theme.palette_fg);
    let muted_style = UiStyle::default().fg(workbench.ui_theme.palette_muted_fg);
    let selected_style = UiStyle::default()
        .bg(workbench.ui_theme.palette_selected_bg)
        .fg(workbench.ui_theme.palette_selected_fg);
    let title_style = UiStyle::default()
        .fg(workbench.ui_theme.header_fg)
        .add_mod(Mod::BOLD);

    painter.fill_rect(popup_area, base_style);

    let inner = UiRect::new(
        popup_area.x.saturating_add(1),
        popup_area.y.saturating_add(1),
        popup_area.w.saturating_sub(2),
        popup_area.h.saturating_sub(2),
    );
    if inner.is_empty() {
        return;
    }

    let query = &workbench.store.state().ui.command_palette.query;
    let matches = match_items(query);
    let selected = workbench
        .store
        .state()
        .ui
        .command_palette
        .selected
        .min(matches.len().saturating_sub(1));

    let prefix = "> ";
    let prefix_w = prefix.width() as u16;
    let (q_start, q_end) = text_window::window(
        query,
        query.len(),
        inner.w.saturating_sub(prefix_w) as usize,
    );
    let visible_query = query.get(q_start..q_end).unwrap_or_default();

    let mut y = inner.y;
    if inner.h >= 1 {
        painter.text_clipped(Pos::new(inner.x, y), "Command Palette", title_style, inner);
        y = y.saturating_add(1);
    }

    if inner.h >= 2 {
        painter.text_clipped(Pos::new(inner.x, y), prefix, base_style, inner);
        painter.text_clipped(
            Pos::new(inner.x.saturating_add(prefix_w), y),
            visible_query,
            base_style,
            inner,
        );
        y = y.saturating_add(1);
    }

    // Keep a blank spacer line (VSCode-like) if we have room.
    if inner.h >= 3 {
        y = y.saturating_add(1);
    }

    if y >= inner.bottom() {
        return;
    }

    let max_items = inner.bottom().saturating_sub(y) as usize;
    if matches.is_empty() {
        painter.text_clipped(Pos::new(inner.x, y), "No matches", muted_style, inner);
        return;
    }

    for (pos, item) in matches.iter().take(max_items).enumerate() {
        let row_y = y.saturating_add(pos as u16);
        if row_y >= inner.bottom() {
            break;
        }
        let is_selected = pos == selected;
        let row_style = if is_selected {
            selected_style
        } else {
            base_style
        };
        let prefix = if is_selected { "▸ " } else { "  " };
        let row_rect = UiRect::new(inner.x, row_y, inner.w, 1);
        if is_selected {
            painter.fill_rect(row_rect, row_style);
        }

        painter.text_clipped(Pos::new(inner.x, row_y), prefix, row_style, inner);

        let mut label = item.label.to_string();
        let max_w = inner
            .w
            .saturating_sub(prefix.width().min(u16::MAX as usize) as u16)
            as usize;
        if label.width() > max_w {
            let end = text_window::truncate_to_width(&label, max_w);
            label.truncate(end);
        }
        painter.text_clipped(
            Pos::new(inner.x.saturating_add(prefix.width() as u16), row_y),
            label,
            row_style,
            inner,
        );
    }
}

pub(super) fn cursor(workbench: &Workbench) -> Option<(u16, u16)> {
    let area = workbench.layout_cache.render_area?;
    let popup_area = util::centered_rect(90, 10, area);

    let query = &workbench.store.state().ui.command_palette.query;
    if popup_area.w < 3 || popup_area.h < 3 {
        return None;
    }

    let inner = UiRect::new(
        popup_area.x.saturating_add(1),
        popup_area.y.saturating_add(1),
        popup_area.w.saturating_sub(2),
        popup_area.h.saturating_sub(2),
    );
    if inner.w == 0 || inner.h < 2 {
        return None;
    }

    let prefix = "> ";
    let prefix_w = prefix.width() as u16;
    let cursor = query.len();
    let (start, _end) =
        text_window::window(query, cursor, inner.w.saturating_sub(prefix_w) as usize);
    let before = query.get(start..cursor).unwrap_or_default();

    let x = inner
        .x
        .saturating_add(prefix_w)
        .saturating_add(before.width() as u16)
        .min(inner.x + inner.w.saturating_sub(1));
    let y = inner.y.saturating_add(1);

    Some((x, y))
}
