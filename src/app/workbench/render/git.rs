use super::super::Workbench;
use crate::core::text_window;
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod, Style as UiStyle};

impl Workbench {
    pub(super) fn paint_git_panel(&mut self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let state = self.store.state();
        if state.git.repo_root.is_none() {
            return;
        };
        if !state.ui.git_panel_expanded {
            return;
        }

        self.last_git_panel_area = Some(area);

        let ui_area = area;

        let base_style = UiStyle::default()
            .bg(self.ui_theme.palette_bg)
            .fg(self.ui_theme.palette_fg);
        painter.fill_rect(ui_area, base_style);

        let sep_style = UiStyle::default()
            .bg(self.ui_theme.palette_bg)
            .fg(self.ui_theme.separator);
        let sep_row = UiRect::new(area.x, area.y, area.w, 1.min(area.h));
        if sep_row.w > 0 {
            painter.hline(Pos::new(sep_row.x, sep_row.y), sep_row.w, '─', sep_style);
        }

        let active_style = UiStyle::default()
            .bg(self.ui_theme.palette_selected_bg)
            .fg(self.ui_theme.palette_selected_fg)
            .add_mod(Mod::BOLD);
        let inactive_style = base_style;

        let max_items = (area.h.saturating_sub(1)) as usize;

        let active_branch = state.git.head.as_ref().and_then(|head| {
            if head.detached {
                None
            } else {
                head.branch.as_deref()
            }
        });

        if state.git.branches.is_empty() {
            if let Some(head) = state.git.head.as_ref() {
                let mut label = head.display();
                let end = text_window::truncate_to_width(&label, area.w as usize);
                label.truncate(end);
                if area.h >= 2 {
                    let y = area.y.saturating_add(1);
                    let line_area = UiRect::new(area.x, y, area.w, 1);
                    painter.fill_rect(line_area, inactive_style);
                    painter.text_clipped(Pos::new(area.x, y), label, inactive_style, line_area);
                }
            }
            return;
        }

        let mut branches: Vec<&str> =
            Vec::with_capacity(state.git.branches.len().saturating_add(1));
        if let Some(active) = active_branch {
            branches.push(active);
        }
        for branch in &state.git.branches {
            if Some(branch.as_str()) != active_branch {
                branches.push(branch);
            }
        }

        for (idx, branch) in branches.into_iter().take(max_items).enumerate() {
            let y = area.y + 1 + idx as u16;
            if y >= area.bottom() {
                break;
            }
            let mut label = branch.to_string();
            let end = text_window::truncate_to_width(&label, area.w as usize);
            label.truncate(end);

            let is_active = Some(branch) == active_branch;
            let style = if is_active {
                active_style
            } else {
                inactive_style
            };
            let line_area = UiRect::new(area.x, y, area.w, 1);
            painter.fill_rect(line_area, style);
            painter.text_clipped(Pos::new(area.x, y), label, style, line_area);
            self.last_git_branch_areas
                .push((branch.to_string(), UiRect::new(area.x, y, area.w, 1)));
        }
    }
}
