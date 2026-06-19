use crate::core::event::InputEvent;
use crate::ui::backend::Backend;
use crate::ui::core::geom::Rect;
use std::path::PathBuf;

pub trait View {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult;

    fn render(&mut self, backend: &mut dyn Backend, area: Rect);

    fn cursor_position(&self) -> Option<(u16, u16)> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventResult {
    Consumed,
    Ignored,
    Quit,
    Restart { path: PathBuf, hard: bool },
}

impl EventResult {
    pub fn is_consumed(&self) -> bool {
        matches!(self, EventResult::Consumed)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui/view.rs"]
mod tests;
