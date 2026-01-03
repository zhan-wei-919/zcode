use super::EditorView;
use crate::core::view::EventResult;
use crate::models::{Granularity, Selection};
use crate::services::EditorConfig;
use crossterm::event::{MouseButton, MouseEventKind};
use std::time::Instant;

pub(super) struct MouseState {
    last_click: Option<(u16, u16, Instant)>,
    click_count: u8,
    dragging: bool,
}

impl MouseState {
    pub(super) fn new() -> Self {
        Self {
            last_click: None,
            click_count: 0,
            dragging: false,
        }
    }

    fn on_click(&mut self, x: u16, y: u16, config: &EditorConfig) -> Granularity {
        let now = Instant::now();

        if let Some((lx, ly, lt)) = self.last_click {
            let dx = (x as i32 - lx as i32).abs();
            let dy = (y as i32 - ly as i32).abs();
            let dt = now.duration_since(lt).as_millis() as u64;

            if dx <= config.click_slop as i32
                && dy <= config.click_slop as i32
                && dt < config.triple_click_ms
            {
                self.click_count = (self.click_count % 3) + 1;
            } else {
                self.click_count = 1;
            }
        } else {
            self.click_count = 1;
        }

        self.last_click = Some((x, y, now));
        self.dragging = true;

        match self.click_count {
            1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        }
    }

    fn on_release(&mut self) {
        self.dragging = false;
    }
}

impl EditorView {
    pub(super) fn handle_mouse(&mut self, event: &crossterm::event::MouseEvent) -> EventResult {
        let area = match self.viewport.area() {
            Some(a) => a,
            None => return EventResult::Ignored,
        };

        let x = event.column.saturating_sub(area.x);
        let y = event.row.saturating_sub(area.y);

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.viewport.enable_follow_cursor();

                let granularity = self.mouse_state.on_click(x, y, &self.config);

                if let Some(pos) = self.viewport.screen_to_pos(x, y, &self.buffer) {
                    self.buffer.set_cursor(pos.0, pos.1);

                    let mut selection = Selection::new(pos, granularity);
                    if granularity != Granularity::Char {
                        selection.update_cursor(pos, self.buffer.rope());
                    }
                    self.buffer.set_selection(Some(selection));
                }
                EventResult::Consumed
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.mouse_state.dragging {
                    if let Some(pos) = self.viewport.screen_to_pos(x, y, &self.buffer) {
                        self.buffer.update_selection_cursor(pos);
                        self.buffer.set_cursor(pos.0, pos.1);
                    }
                }
                EventResult::Consumed
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_state.on_release();
                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                let step = self.config.scroll_step();
                self.viewport
                    .scroll_vertical(-(step as isize), self.buffer.len_lines());
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let step = self.config.scroll_step();
                self.viewport
                    .scroll_vertical(step as isize, self.buffer.len_lines());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }
}
