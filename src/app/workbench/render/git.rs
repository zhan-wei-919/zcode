use super::super::Workbench;
use super::layout::ThinHSeparator;
use crate::core::text_window;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

impl Workbench {
    pub(super) fn render_git_panel(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
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

        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);
        frame.render_widget(Block::default().style(base_style), area);

        let sep_area = Rect::new(area.x, area.y, area.width, 1.min(area.height));
        frame.render_widget(
            ThinHSeparator {
                fg: self.theme.separator,
                bg: self.theme.palette_bg,
            },
            sep_area,
        );

        if area.height <= 1 {
            return;
        }

        let active_style = Style::default()
            .bg(self.theme.palette_selected_bg)
            .fg(self.theme.palette_selected_fg)
            .add_modifier(Modifier::BOLD);
        let inactive_style = base_style;

        let max_items = (area.height - 1) as usize;

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
                let end = text_window::truncate_to_width(&label, area.width as usize);
                label.truncate(end);
                let line_area = Rect::new(area.x, area.y + 1, area.width, 1);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(label, inactive_style))),
                    line_area,
                );
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
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            let mut label = branch.to_string();
            let end = text_window::truncate_to_width(&label, area.width as usize);
            label.truncate(end);

            let is_active = Some(branch) == active_branch;
            let style = if is_active {
                active_style
            } else {
                inactive_style
            };
            let line_area = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label, style))),
                line_area,
            );
            self.last_git_branch_areas
                .push((branch.to_string(), line_area));
        }
    }
}
