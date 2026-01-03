use super::EditorGroup;
use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

impl View for EditorGroup {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        if let InputEvent::Mouse(mouse_event) = event {
            if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
                if let Some(index) = self.hit_test_tab(mouse_event.column, mouse_event.row) {
                    self.active_index = index.min(self.tabs.len().saturating_sub(1));
                    return EventResult::Consumed;
                }
            }
        }

        if self.search_bar.is_visible() {
            if let InputEvent::Key(key_event) = event {
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Enter, KeyModifiers::NONE) => {
                        self.find_next();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Enter, KeyModifiers::SHIFT) => {
                        self.find_prev();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Enter, KeyModifiers::CONTROL) => {
                        self.replace_current();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Enter, mods)
                        if mods == KeyModifiers::CONTROL | KeyModifiers::SHIFT =>
                    {
                        self.replace_all();
                        return EventResult::Consumed;
                    }
                    _ => {}
                }
            }

            let old_text = self.search_bar.search_text().to_string();
            let result = self.search_bar.handle_input(event);

            if self.search_bar.search_text() != old_text {
                self.trigger_search();
            }

            if result.is_consumed() {
                return result;
            }
        }

        if let Some(editor) = self.active_editor_mut() {
            editor.handle_input(event)
        } else {
            EventResult::Ignored
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.area = Some(area);
        let search_bar_height = self.search_bar.height();

        let total_chrome_height = super::TAB_BAR_HEIGHT + search_bar_height;
        if area.height <= total_chrome_height {
            return;
        }

        let tab_area = Rect::new(area.x, area.y, area.width, super::TAB_BAR_HEIGHT);

        let search_area = if search_bar_height > 0 {
            Rect::new(
                area.x,
                area.y + super::TAB_BAR_HEIGHT,
                area.width,
                search_bar_height,
            )
        } else {
            Rect::default()
        };

        let editor_area = Rect::new(
            area.x,
            area.y + total_chrome_height,
            area.width,
            area.height - total_chrome_height,
        );

        self.render_tabs(frame, tab_area);

        if self.search_bar.is_visible() {
            self.search_bar.render(frame, search_area);
        }

        if let Some(editor) = self.active_editor_mut() {
            editor.render(frame, editor_area);
        }
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        if self.search_bar.is_visible() {
            return self.search_bar.cursor_position();
        }
        self.active_editor().and_then(|e| e.cursor_position())
    }
}

impl EditorGroup {
    fn hit_test_tab(&self, column: u16, row: u16) -> Option<usize> {
        let area = self.area?;
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if row != area.y {
            return None;
        }
        if column < area.x || column >= area.x + area.width {
            return None;
        }

        const PADDING_LEFT: u16 = 1;
        const PADDING_RIGHT: u16 = 1;
        const DIVIDER: u16 = 1;

        let right = area.x + area.width;
        let mut x = area.x;

        for (i, tab) in self.tabs.iter().enumerate() {
            if x >= right {
                break;
            }

            let start = x;
            x = x.saturating_add(PADDING_LEFT).min(right);

            let mut title_width = UnicodeWidthStr::width(tab.title.as_str());
            if tab.editor.is_dirty() {
                title_width = title_width.saturating_add(2);
            }
            title_width = title_width.saturating_add(2);
            x = x
                .saturating_add(title_width.min(u16::MAX as usize) as u16)
                .min(right);

            x = x.saturating_add(PADDING_RIGHT).min(right);
            let end = x;

            if column >= start && column < end {
                return Some(i);
            }

            if i + 1 == self.tabs.len() {
                break;
            }

            x = x.saturating_add(DIVIDER).min(right);
        }

        None
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles: Vec<Line> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let title = tab.display_title();
                if i == self.active_index {
                    Line::from(Span::styled(
                        format!(" {} ", title),
                        Style::default()
                            .fg(self.theme.sidebar_tab_active_fg)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(
                        format!(" {} ", title),
                        Style::default().fg(self.theme.sidebar_tab_inactive_fg),
                    ))
                }
            })
            .collect();

        let tabs_widget = Tabs::new(titles)
            .select(self.active_index)
            .highlight_style(Style::default().bg(self.theme.sidebar_tab_active_bg));

        frame.render_widget(tabs_widget, area);
    }
}
