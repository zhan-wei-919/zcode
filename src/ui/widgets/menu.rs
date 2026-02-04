use crate::core::text_window;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::id::IdPath;
use crate::ui::core::layout::Insets;
use crate::ui::core::painter::BorderKind;
use crate::ui::core::style::Style;
use crate::ui::core::tree::{Node, NodeKind, Sense};
use crate::ui::core::widget::{Ui, Widget};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug)]
pub struct MenuStyles {
    pub base: Style,
    pub border: Style,
    pub selected: Style,
}

pub struct Menu<'a> {
    pub id_base: IdPath,
    pub menu_id: u32,
    pub layer: u8,
    pub anchor: Pos,
    pub items: &'a [&'a str],
    pub selected: usize,
    pub styles: MenuStyles,
}

impl Widget for Menu<'_> {
    fn ui(&mut self, ui: &mut Ui) {
        let screen = ui.rect;
        if screen.is_empty() || self.items.is_empty() {
            return;
        }

        if screen.w < 3 || screen.h < 3 {
            return;
        }

        let mut max_label_w = 0usize;
        for item in self.items {
            max_label_w = max_label_w.max(item.width());
        }

        let desired_inner_width = (max_label_w.saturating_add(4)).min(u16::MAX as usize) as u16;
        let desired_inner_height = (self.items.len().min(u16::MAX as usize)) as u16;
        let width = desired_inner_width.saturating_add(2).min(screen.w).max(3);
        let height = desired_inner_height.saturating_add(2).min(screen.h).max(3);

        let right = screen.right();
        let bottom = screen.bottom();

        let mut x = self.anchor.x.max(screen.x);
        let mut y = self.anchor.y.max(screen.y);
        if x.saturating_add(width) > right {
            x = right.saturating_sub(width);
        }
        if y.saturating_add(height) > bottom {
            y = bottom.saturating_sub(height);
        }

        let popup = Rect::new(x, y, width, height);

        // Overlay nodes (hit-test priority over base UI).
        let overlay_id = self.id_base.push_str("overlay").finish();
        ui.tree.push(Node {
            id: overlay_id,
            rect: screen,
            layer: self.layer,
            z: 0,
            sense: Sense::CLICK | Sense::CONTEXT_MENU,
            kind: NodeKind::Unknown,
        });

        let container_id = self.id_base.push_str("container").finish();
        ui.tree.push(Node {
            id: container_id,
            rect: popup,
            layer: self.layer,
            z: 0,
            sense: Sense::CLICK | Sense::CONTEXT_MENU,
            kind: NodeKind::Unknown,
        });

        ui.painter.fill_rect(popup, self.styles.base);
        ui.painter
            .border(popup, self.styles.border, BorderKind::Plain);

        let inner = popup.inset(Insets::all(1));
        if inner.is_empty() {
            return;
        }

        let selected = self.selected.min(self.items.len().saturating_sub(1));
        for (idx, item) in self.items.iter().enumerate().take(inner.h as usize) {
            let row_y = inner.y.saturating_add(idx as u16);
            let row_rect = Rect::new(inner.x, row_y, inner.w, 1);
            if !row_rect.is_empty() {
                let id = self.id_base.push_str("item").push_u64(idx as u64).finish();
                ui.tree.push(Node {
                    id,
                    rect: row_rect,
                    layer: self.layer,
                    z: 0,
                    sense: Sense::HOVER | Sense::CLICK,
                    kind: NodeKind::MenuItem {
                        menu_id: self.menu_id,
                        index: idx,
                    },
                });
            }

            let is_selected = idx == selected;
            let row_style = if is_selected {
                self.styles.selected
            } else {
                self.styles.base
            };

            let prefix = if is_selected { "▸ " } else { "  " };
            let mut text = format!("{prefix}{item}");
            let pad_to = inner.w as usize;

            if text.width() > pad_to {
                let end = text_window::truncate_to_width(&text, pad_to);
                text.truncate(end);
            }

            let current_w = text.width();
            if current_w < pad_to {
                text.push_str(&" ".repeat(pad_to - current_w));
            }

            ui.painter
                .text_clipped(Pos::new(inner.x, row_y), text, row_style, inner);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/widgets/menu.rs"]
mod tests;
