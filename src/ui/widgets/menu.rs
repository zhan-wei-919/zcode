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
    pub border: Option<Style>,
    pub selected: Style,
    pub disabled: Style,
    pub separator: Style,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItemKind {
    Action { enabled: bool },
    Separator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuItem<'a> {
    pub label: &'a str,
    pub kind: MenuItemKind,
}

impl<'a> MenuItem<'a> {
    pub fn action(label: &'a str) -> Self {
        Self {
            label,
            kind: MenuItemKind::Action { enabled: true },
        }
    }

    pub fn disabled_action(label: &'a str) -> Self {
        Self {
            label,
            kind: MenuItemKind::Action { enabled: false },
        }
    }

    pub fn separator() -> Self {
        Self {
            label: "",
            kind: MenuItemKind::Separator,
        }
    }

    pub fn is_selectable(&self) -> bool {
        matches!(self.kind, MenuItemKind::Action { enabled: true })
    }

    pub fn is_separator(&self) -> bool {
        matches!(self.kind, MenuItemKind::Separator)
    }
}

pub struct Menu<'a> {
    pub id_base: IdPath,
    pub menu_id: u32,
    pub layer: u8,
    pub anchor: Pos,
    pub items: &'a [MenuItem<'a>],
    pub selected: usize,
    pub styles: MenuStyles,
}

impl Widget for Menu<'_> {
    fn ui(&mut self, ui: &mut Ui) {
        let screen = ui.rect;
        if screen.is_empty() || self.items.is_empty() {
            return;
        }

        let mut max_label_w = 0usize;
        for item in self.items {
            let width = if item.is_separator() {
                1
            } else {
                item.label.width().saturating_add(2)
            };
            max_label_w = max_label_w.max(width);
        }

        let desired_inner_width = (max_label_w.saturating_add(2)).min(u16::MAX as usize) as u16;
        let desired_inner_height = (self.items.len().min(u16::MAX as usize)) as u16;
        let border_padding = if self.styles.border.is_some() { 2 } else { 0 };
        let width = desired_inner_width
            .saturating_add(border_padding)
            .min(screen.w)
            .max(1);
        let height = desired_inner_height
            .saturating_add(border_padding)
            .min(screen.h)
            .max(1);

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
        if let Some(border_style) = self.styles.border {
            ui.painter.border(popup, border_style, BorderKind::Plain);
        }

        let inner = if self.styles.border.is_some() {
            popup.inset(Insets::all(1))
        } else {
            popup
        };
        if inner.is_empty() {
            return;
        }

        let selected = self.selected.min(self.items.len().saturating_sub(1));
        for (idx, item) in self.items.iter().enumerate().take(inner.h as usize) {
            let row_y = inner.y.saturating_add(idx as u16);
            let row_rect = Rect::new(inner.x, row_y, inner.w, 1);

            if item.is_selectable() && !row_rect.is_empty() {
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

            let is_selected = idx == selected && item.is_selectable();
            let row_style = if item.is_separator() {
                self.styles.separator
            } else if is_selected {
                self.styles.selected
            } else if item.is_selectable() {
                self.styles.base
            } else {
                self.styles.disabled
            };

            let mut text = if item.is_separator() {
                "─".repeat(inner.w as usize)
            } else {
                let prefix = if is_selected { "▸ " } else { "  " };
                format!("{prefix}{}", item.label)
            };
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
