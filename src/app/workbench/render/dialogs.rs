use super::super::Workbench;
use super::super::paint::centered_rect_ui;
use crate::core::text_window;
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::id::IdPath;
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod, Style as UiStyle};
use crate::ui::core::widget::{Ui, Widget};
use crate::ui::widgets::menu::{Menu, MenuStyles};
use unicode_width::UnicodeWidthStr;

pub(super) fn render_confirm_dialog(workbench: &Workbench, painter: &mut Painter, area: UiRect) {
    let dialog = &workbench.store.state().ui.confirm_dialog;
    if !dialog.visible {
        return;
    }

    let width = 50.min(area.w.saturating_sub(4));
    let height = 5.min(area.h.saturating_sub(2));
    if width < 20 || height < 3 {
        return;
    }

    let x = area.x + (area.w.saturating_sub(width)) / 2;
    let y = area.y + (area.h.saturating_sub(height)) / 2;
    let dialog_area = UiRect::new(x, y, width, height);

    let base_style = UiStyle::default()
        .bg(workbench.ui_theme.palette_bg)
        .fg(workbench.ui_theme.palette_fg);
    painter.fill_rect(dialog_area, base_style);

    let inner = UiRect::new(
        dialog_area.x.saturating_add(1),
        dialog_area.y.saturating_add(1),
        dialog_area.w.saturating_sub(2),
        dialog_area.h.saturating_sub(2),
    );
    if inner.h < 2 || inner.w < 10 {
        return;
    }

    let title_style = UiStyle::default()
        .fg(workbench.ui_theme.header_fg)
        .add_mod(Mod::BOLD);
    painter.text_clipped(Pos::new(inner.x, inner.y), "Confirm", title_style, inner);

    if inner.h >= 2 {
        let mut msg = dialog.message.clone();
        let max_w = inner.w as usize;
        if msg.width() > max_w {
            let end = text_window::truncate_to_width(&msg, max_w);
            msg.truncate(end);
        }
        painter.text_clipped(Pos::new(inner.x, inner.y + 1), msg, base_style, inner);
    }

    if inner.h >= 3 {
        let mut x = inner.x;
        let y = inner.y + 2;

        let accent = UiStyle::default().fg(workbench.ui_theme.accent_fg);
        let muted = UiStyle::default().fg(workbench.ui_theme.palette_muted_fg);

        let parts: [(&str, UiStyle); 4] = [
            ("[Enter]", accent),
            (" Close  ", base_style),
            ("[Esc]", muted),
            (" Cancel", base_style),
        ];

        for (text, style) in parts {
            if x >= inner.right() {
                break;
            }
            painter.text_clipped(Pos::new(x, y), text, style, inner);
            x = x.saturating_add(text.width().min(u16::MAX as usize) as u16);
        }
    }
}

pub(super) fn render_context_menu(workbench: &mut Workbench, painter: &mut Painter, area: UiRect) {
    let menu = &workbench.store.state().ui.context_menu;
    if !menu.visible {
        return;
    }

    if menu.items.is_empty() || area.is_empty() {
        return;
    }

    let labels = menu.items.iter().map(|i| i.label()).collect::<Vec<_>>();

    let styles = MenuStyles {
        base: UiStyle::default()
            .bg(workbench.ui_theme.palette_bg)
            .fg(workbench.ui_theme.palette_fg),
        border: UiStyle::default().fg(workbench.ui_theme.focus_border).bg(workbench.ui_theme.palette_bg),
        selected: UiStyle::default()
            .bg(workbench.ui_theme.palette_selected_bg)
            .fg(workbench.ui_theme.palette_selected_fg),
    };

    let mut ui = Ui::new(area, painter, &mut workbench.ui_tree);
    let mut widget = Menu {
        id_base: IdPath::root("workbench").push_str("context_menu"),
        menu_id: 0,
        layer: 10,
        anchor: Pos::new(menu.anchor.0, menu.anchor.1),
        items: &labels,
        selected: menu.selected,
        styles,
    };
    widget.ui(&mut ui);
}

fn input_dialog_area(area: UiRect) -> UiRect {
    super::super::util::centered_rect(60, 7, area)
}

pub(super) fn render_input_dialog(workbench: &Workbench, painter: &mut Painter, area: UiRect) {
    let dialog = &workbench.store.state().ui.input_dialog;
    if !dialog.visible {
        return;
    }

    let popup_area = centered_rect_ui(60, 7, area);
    if popup_area.w < 20 || popup_area.h < 5 {
        return;
    }

    let base_style = UiStyle::default()
        .bg(workbench.ui_theme.palette_bg)
        .fg(workbench.ui_theme.palette_fg);
    let muted_style = UiStyle::default().fg(workbench.ui_theme.palette_muted_fg);
    let title_style = UiStyle::default()
        .fg(workbench.ui_theme.header_fg)
        .add_mod(Mod::BOLD);
    let error_style = UiStyle::default().fg(workbench.ui_theme.error_fg);

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

    let title = if dialog.title.is_empty() {
        "Input"
    } else {
        dialog.title.as_str()
    };
    let prefix = "> ";
    let prefix_w = prefix.width() as u16;
    let cursor = dialog.cursor.min(dialog.value.len());
    let (v_start, v_end) = text_window::window(
        dialog.value.as_str(),
        cursor,
        inner.w.saturating_sub(prefix_w) as usize,
    );
    let visible_value = dialog.value.get(v_start..v_end).unwrap_or_default();

    let mut y = inner.y;
    if inner.h >= 1 {
        painter.text_clipped(Pos::new(inner.x, y), title, title_style, inner);
        y = y.saturating_add(1);
    }

    if inner.h >= 2 {
        painter.text_clipped(Pos::new(inner.x, y), prefix, base_style, inner);
        painter.text_clipped(
            Pos::new(inner.x.saturating_add(prefix_w), y),
            visible_value,
            base_style,
            inner,
        );
        y = y.saturating_add(1);
    }

    if inner.h >= 3 {
        if let Some(err) = dialog.error.as_deref() {
            painter.text_clipped(Pos::new(inner.x, y), err, error_style, inner);
        }
        y = y.saturating_add(1);
    }

    if inner.h >= 4 {
        let accent = UiStyle::default().fg(workbench.ui_theme.accent_fg);

        let parts: [(&str, UiStyle); 4] = [
            ("[Enter]", accent),
            (" Create  ", base_style),
            ("[Esc]", muted_style),
            (" Cancel", base_style),
        ];

        let mut x = inner.x;
        for (text, style) in parts {
            if x >= inner.right() {
                break;
            }
            painter.text_clipped(Pos::new(x, y), text, style, inner);
            x = x.saturating_add(text.width().min(u16::MAX as usize) as u16);
        }
    }
}

pub(super) fn input_dialog_cursor(workbench: &Workbench) -> Option<(u16, u16)> {
    let area = workbench.last_render_area?;
    let dialog = &workbench.store.state().ui.input_dialog;
    if !dialog.visible {
        return None;
    }

    let popup_area = input_dialog_area(area);
    if popup_area.w < 4 || popup_area.h < 3 {
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

    let cursor = dialog.cursor.min(dialog.value.len());
    let prefix_w = "> ".width() as u16;
    let (start, _end) = text_window::window(
        dialog.value.as_str(),
        cursor,
        inner.w.saturating_sub(prefix_w) as usize,
    );
    let before = dialog.value.get(start..cursor).unwrap_or_default();
    let before_w = before.width() as u16;

    let x = inner
        .x
        .saturating_add(prefix_w)
        .saturating_add(before_w)
        .min(inner.x + inner.w.saturating_sub(1));
    // Title line is at inner.y, input line is at inner.y + 1.
    let y = inner.y.saturating_add(1);

    Some((x, y))
}
